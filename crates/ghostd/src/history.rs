use std::collections::{BTreeMap, VecDeque};
use std::iter;
use std::num::NonZeroU32;

use ghost_proto::{CpuSnapshot, GpuSnapshot, topics};

const MILLIS_PER_MINUTE: u64 = 60_000;

const CPU_LOAD_KEY: &str = "load";
const GPU_LOAD_KEY: &str = "gpu_load";
// The shell's history keys stop at ccd1, so CCDs past the second are not recorded.
const CCD_LOAD_KEYS: [&str; 2] = ["ccd0", "ccd1"];

/// The history series each built-in topic records, keyed by topic.
pub const HISTORY_KEYS: [(&str, &[&str]); 2] = [
    (
        topics::CPU,
        &[CPU_LOAD_KEY, CCD_LOAD_KEYS[0], CCD_LOAD_KEYS[1]],
    ),
    (topics::GPU, &[GPU_LOAD_KEY]),
];

/// Samples to keep so that `history_minutes` of history fit at one sample every `tick_ms`, and
/// never fewer than one.
pub fn history_len(history_minutes: u32, tick_ms: NonZeroU32) -> usize {
    let samples = u64::from(history_minutes) * MILLIS_PER_MINUTE / u64::from(tick_ms.get());
    usize::try_from(samples).unwrap_or(usize::MAX).max(1)
}

/// Fixed-capacity sample buffers keyed by series name, newest samples winning once full.
///
/// A `None` sample marks a tick without a reading.
pub struct History {
    len: usize,
    series: BTreeMap<String, VecDeque<Option<f32>>>,
}

impl History {
    pub fn new(len: usize) -> Self {
        Self {
            len,
            series: BTreeMap::new(),
        }
    }

    pub fn push(&mut self, key: &str, v: Option<f32>) {
        let len = self.len;
        // Pushing happens every tick for every series, and only the first push of a series needs
        // an owned key.
        if let Some(buffer) = self.series.get_mut(key) {
            push_bounded(buffer, len, v);
            return;
        }
        let mut buffer = VecDeque::with_capacity(len);
        push_bounded(&mut buffer, len, v);
        self.series.insert(key.to_owned(), buffer);
    }

    /// Pushes one `None` per tick in `ticks`; more than the capacity would only evict each other.
    pub fn push_gap(&mut self, key: &str, ticks: u64) {
        let nulls = usize::try_from(ticks).map_or(self.len, |ticks| ticks.min(self.len));
        for _ in 0..nulls {
            self.push(key, None);
        }
    }

    /// Every series, oldest sample first.
    pub fn series(&self) -> BTreeMap<String, Vec<Option<f32>>> {
        self.series
            .iter()
            .map(|(key, buffer)| (key.clone(), buffer.iter().copied().collect()))
            .collect()
    }

    /// Changes the capacity; shrinking drops the oldest samples.
    pub fn resize(&mut self, len: usize) {
        self.len = len;
        for buffer in self.series.values_mut() {
            let excess = buffer.len().saturating_sub(len);
            buffer.drain(..excess);
        }
    }
}

/// Records a cpu sample that followed `missed` ticks without one.
pub fn record_cpu(hist: &mut History, cpu: &CpuSnapshot, missed: u64) {
    let ccd_loads = CCD_LOAD_KEYS
        .into_iter()
        .zip(cpu.ccds.iter().map(|ccd| ccd.load));
    for (key, load) in iter::once((CPU_LOAD_KEY, cpu.load)).chain(ccd_loads) {
        hist.push_gap(key, missed);
        hist.push(key, Some(load));
    }
}

/// Records a gpu sample that followed `missed` ticks without one.
pub fn record_gpu(hist: &mut History, gpu: &GpuSnapshot, missed: u64) {
    hist.push_gap(GPU_LOAD_KEY, missed);
    hist.push(GPU_LOAD_KEY, gpu.present.then_some(gpu.load));
}

fn push_bounded(buffer: &mut VecDeque<Option<f32>>, len: usize, v: Option<f32>) {
    // Evict before pushing so a full buffer never grows past its allocation.
    if buffer.len() >= len {
        buffer.pop_front();
    }
    buffer.push_back(v);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn filled(len: usize, values: impl IntoIterator<Item = f32>) -> History {
        let mut history = History::new(len);
        for value in values {
            history.push("load", Some(value));
        }
        history
    }

    #[test]
    fn resize_keeps_newest() {
        let mut history = filled(5, [1.0, 2.0, 3.0, 4.0, 5.0]);

        history.resize(2);
        assert_eq!(history.series()["load"], [Some(4.0), Some(5.0)]);

        history.push("load", Some(6.0));
        assert_eq!(history.series()["load"], [Some(5.0), Some(6.0)]);
    }

    #[test]
    fn resize_larger_keeps_everything_and_grows() {
        let mut history = filled(2, [1.0, 2.0]);

        history.resize(4);
        history.push("load", Some(3.0));
        history.push("load", Some(4.0));
        history.push("load", Some(5.0));

        assert_eq!(
            history.series()["load"],
            [Some(2.0), Some(3.0), Some(4.0), Some(5.0)]
        );
    }

    #[test]
    fn series_fill_and_evict_independently() {
        let mut history = History::new(2);

        for (key, value) in [("a", 1.0), ("b", 10.0), ("a", 2.0), ("a", 3.0), ("b", 20.0)] {
            history.push(key, Some(value));
        }

        assert_eq!(history.series()["a"], [Some(2.0), Some(3.0)]);
        assert_eq!(history.series()["b"], [Some(10.0), Some(20.0)]);
    }

    #[test]
    fn a_gap_is_one_null_per_tick() {
        let mut history = filled(5, [1.0]);

        history.push_gap("load", 2);
        history.push("load", Some(2.0));

        assert_eq!(history.series()["load"], [Some(1.0), None, None, Some(2.0)]);
    }

    #[test]
    fn a_gap_longer_than_the_history_leaves_only_nulls() {
        let mut history = filled(3, [1.0, 2.0, 3.0]);

        history.push_gap("load", u64::MAX);

        assert_eq!(history.series()["load"], [None, None, None]);
    }

    fn tick(ms: u32) -> NonZeroU32 {
        NonZeroU32::new(ms).unwrap()
    }

    #[test]
    fn history_len_covers_the_minutes_at_the_tick_rate() {
        assert_eq!(history_len(5, tick(1000)), 300);
        assert_eq!(history_len(5, tick(50)), 6000);
    }

    #[test]
    fn history_len_keeps_at_least_one_sample() {
        assert_eq!(history_len(1, tick(120_000)), 1);
    }
}
