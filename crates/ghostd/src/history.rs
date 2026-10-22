use std::collections::{BTreeMap, VecDeque};
use std::iter;
use std::num::NonZeroU32;

use ghost_proto::{
    CpuSnapshot, DiskSnapshot, DriveState, GpuSnapshot, MemSnapshot, NetSnapshot, SensorSnapshot,
    topics,
};

const MILLIS_PER_MINUTE: u64 = 60_000;
const KIB_PER_GIB: f64 = 1_048_576.0;

const CPU_LOAD_KEY: &str = "load";
// The shell's history keys stop at ccd1, so CCDs past the second are not recorded.
const CCD_LOAD_KEYS: [&str; 2] = ["ccd0", "ccd1"];
/// Followed by the core's index in `CpuSnapshot::cores`.
const CORE_LOAD_KEY_PREFIX: &str = "core";
const GPU_LOAD_KEY: &str = "gpu_load";
const GPU_TEMP_KEY: &str = "temp_gpu";
const MEM_USED_KEY: &str = "mem_used";
const CPU_TEMP_KEY: &str = "temp_cpu";
const DISK_READ_KEY: &str = "disk_read";
const DISK_WRITE_KEY: &str = "disk_write";
const NVME_TEMP_KEY: &str = "temp_nvme";
const NET_DOWN_KEY: &str = "net_down";
const NET_UP_KEY: &str = "net_up";

/// The hwmon readings of the CPU package's temperature, on AMD and on Intel.
const CPU_PACKAGE_SENSORS: [&str; 2] = ["k10temp/Tctl", "coretemp/Package id 0"];
const NVME_KERNEL_NAME_PREFIX: &str = "nvme";

/// Every history key but the per-core ones, with the topic that records it.
const KEY_TOPICS: [(&str, &str); 12] = [
    (CPU_LOAD_KEY, topics::CPU),
    (CCD_LOAD_KEYS[0], topics::CPU),
    (CCD_LOAD_KEYS[1], topics::CPU),
    (GPU_LOAD_KEY, topics::GPU),
    (GPU_TEMP_KEY, topics::GPU),
    (MEM_USED_KEY, topics::MEM),
    (CPU_TEMP_KEY, topics::SENSORS),
    (DISK_READ_KEY, topics::DISKS),
    (DISK_WRITE_KEY, topics::DISKS),
    (NVME_TEMP_KEY, topics::DISKS),
    (NET_DOWN_KEY, topics::NET),
    (NET_UP_KEY, topics::NET),
];

/// The topic whose messages a history series is recorded from, or `None` for a key no topic
/// records.
pub fn history_topic(key: &str) -> Option<&'static str> {
    if is_core_key(key) {
        return Some(topics::CPU);
    }
    KEY_TOPICS
        .iter()
        .find(|(named, _)| *named == key)
        .map(|(_, topic)| *topic)
}

/// Whether `topic` records any history series.
pub fn has_history(topic: &str) -> bool {
    KEY_TOPICS.iter().any(|(_, recorder)| *recorder == topic)
}

fn is_core_key(key: &str) -> bool {
    key.strip_prefix(CORE_LOAD_KEY_PREFIX)
        .is_some_and(|index| !index.is_empty() && index.bytes().all(|byte| byte.is_ascii_digit()))
}

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

    /// Pushes `missed` nulls for the ticks before `v` that had no reading, then `v`.
    fn record(&mut self, key: &str, missed: u64, v: Option<f32>) {
        self.push_gap(key, missed);
        self.push(key, v);
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
        hist.record(key, missed, Some(load));
    }
    for (index, core) in cpu.cores.iter().enumerate() {
        hist.record(
            &format!("{CORE_LOAD_KEY_PREFIX}{index}"),
            missed,
            Some(core.load),
        );
    }
}

/// Records a gpu sample that followed `missed` ticks without one; an absent GPU reads as null.
pub fn record_gpu(hist: &mut History, gpu: &GpuSnapshot, missed: u64) {
    hist.record(GPU_LOAD_KEY, missed, gpu.present.then_some(gpu.load));
    hist.record(GPU_TEMP_KEY, missed, gpu.present.then_some(gpu.temp_c));
}

/// Records a mem sample that followed `missed` ticks without one.
pub fn record_mem(hist: &mut History, mem: &MemSnapshot, missed: u64) {
    let used_gib = mem.used_kib as f64 / KIB_PER_GIB;
    hist.record(MEM_USED_KEY, missed, Some(used_gib as f32));
}

/// Records the first CPU package reading of a published sensors sample that followed `missed`
/// ticks without one.
pub fn record_sensors(hist: &mut History, sensors: &SensorSnapshot, missed: u64) {
    let package = sensors
        .temps
        .iter()
        .find(|reading| CPU_PACKAGE_SENSORS.contains(&reading.id.as_str()))
        .and_then(|reading| reading.value);
    hist.record(CPU_TEMP_KEY, missed, package);
}

/// Records a disks sample that followed `missed` ticks without one: the rates summed over the
/// live drives, and the hottest live NVMe drive.
pub fn record_disks(hist: &mut History, disks: &DiskSnapshot, missed: u64) {
    let live = || {
        disks
            .drives
            .iter()
            .filter(|drive| drive.state == DriveState::Live)
    };
    // Summed as f64, as the shell sums them, so a rate cannot overflow and both sums agree.
    let read_bps: f64 = live().map(|drive| drive.read_bps as f64).sum();
    let write_bps: f64 = live().map(|drive| drive.write_bps as f64).sum();
    let hottest_nvme = live()
        .filter(|drive| drive.kernel_name.starts_with(NVME_KERNEL_NAME_PREFIX))
        .filter_map(|drive| drive.temp_c)
        .reduce(f32::max);
    hist.record(DISK_READ_KEY, missed, Some(read_bps as f32));
    hist.record(DISK_WRITE_KEY, missed, Some(write_bps as f32));
    hist.record(NVME_TEMP_KEY, missed, hottest_nvme);
}

/// Records a net sample that followed `missed` ticks without one: the default-route interface's
/// rates.
pub fn record_net(hist: &mut History, net: &NetSnapshot, missed: u64) {
    // Through f64, as the shell's JSON parse goes, so both series round a rate alike.
    hist.record(NET_DOWN_KEY, missed, Some(net.rx_bps as f64 as f32));
    hist.record(NET_UP_KEY, missed, Some(net.tx_bps as f64 as f32));
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

    #[test]
    fn every_key_has_one_topic() {
        for (key, topic) in [
            ("load", "cpu"),
            ("ccd1", "cpu"),
            ("core15", "cpu"),
            ("gpu_load", "gpu"),
            ("temp_gpu", "gpu"),
            ("mem_used", "mem"),
            ("temp_cpu", "sensors"),
            ("disk_read", "disks"),
            ("disk_write", "disks"),
            ("temp_nvme", "disks"),
            ("net_down", "net"),
            ("net_up", "net"),
        ] {
            assert_eq!(history_topic(key), Some(topic), "{key}");
        }
        for unknown in ["corex", "core", "core-1", "ccd2", "nope"] {
            assert_eq!(history_topic(unknown), None, "{unknown}");
        }
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
