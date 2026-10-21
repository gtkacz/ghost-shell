use std::collections::{BTreeMap, VecDeque};
use std::num::NonZeroU32;

const MILLIS_PER_MINUTE: u64 = 60_000;

/// Samples to keep so that `history_minutes` of history fit at one sample every `tick_ms`, and
/// never fewer than one.
pub fn history_len(history_minutes: u32, tick_ms: NonZeroU32) -> usize {
    let samples = u64::from(history_minutes) * MILLIS_PER_MINUTE / u64::from(tick_ms.get());
    usize::try_from(samples).unwrap_or(usize::MAX).max(1)
}

/// Fixed-capacity sample buffers keyed by series name, newest samples winning once full.
pub struct History {
    len: usize,
    series: BTreeMap<String, VecDeque<f32>>,
}

impl History {
    pub fn new(len: usize) -> Self {
        Self {
            len,
            series: BTreeMap::new(),
        }
    }

    pub fn push(&mut self, key: &str, v: f32) {
        let len = self.len;
        let buffer = self
            .series
            .entry(key.to_owned())
            .or_insert_with(|| VecDeque::with_capacity(len));
        // Evict before pushing so a full buffer never grows past its allocation.
        if buffer.len() >= len {
            buffer.pop_front();
        }
        buffer.push_back(v);
    }

    /// Every series, oldest sample first.
    pub fn series(&self) -> BTreeMap<String, Vec<f32>> {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn filled(len: usize, values: impl IntoIterator<Item = f32>) -> History {
        let mut history = History::new(len);
        for value in values {
            history.push("load", value);
        }
        history
    }

    #[test]
    fn resize_keeps_newest() {
        let mut history = filled(5, [1.0, 2.0, 3.0, 4.0, 5.0]);

        history.resize(2);
        assert_eq!(history.series()["load"], [4.0, 5.0]);

        history.push("load", 6.0);
        assert_eq!(history.series()["load"], [5.0, 6.0]);
    }

    #[test]
    fn resize_larger_keeps_everything_and_grows() {
        let mut history = filled(2, [1.0, 2.0]);

        history.resize(4);
        history.push("load", 3.0);
        history.push("load", 4.0);
        history.push("load", 5.0);

        assert_eq!(history.series()["load"], [2.0, 3.0, 4.0, 5.0]);
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
