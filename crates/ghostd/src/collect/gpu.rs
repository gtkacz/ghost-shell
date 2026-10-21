use ghost_proto::GpuSnapshot;
use nvml_wrapper::Nvml;
use nvml_wrapper::enum_wrappers::device::TemperatureSensor;

use super::Source;

const FIRST_DEVICE_INDEX: u32 = 0;
const FIRST_FAN_INDEX: u32 = 0;
const BYTES_PER_MIB: u64 = 1024 * 1024;
const MILLIWATTS_PER_WATT: f32 = 1000.0;
// The fan query is most of a read's cost, and fans ramp over seconds with no alert threshold or
// graph, so a fan percentage a few seconds old is as good as a fresh one.
const FAN_READ_EVERY: u32 = 5;

/// One successful read of a GPU: every `GpuSnapshot` field except `present`.
#[derive(Debug, Clone, PartialEq)]
pub struct GpuReading {
    pub name: String,
    pub load: f32,
    pub vram_used_mib: u32,
    pub vram_total_mib: u32,
    pub temp_c: f32,
    pub power_w: f32,
    pub power_limit_w: f32,
    pub fan_pct: f32,
}

/// Where a `GpuCollector` gets its readings.
///
/// `Send` because collectors are sampled on a blocking thread.
pub trait GpuSource: Send {
    /// `None` when the GPU cannot be read right now; the next call may succeed again.
    fn read(&mut self) -> Option<GpuReading>;
}

/// Reads the first NVIDIA GPU through NVML.
pub struct NvmlSource {
    nvml: Nvml,
    fan: FanCache,
}

impl NvmlSource {
    /// `None` when `libnvidia-ml` cannot be loaded or NVML will not initialise, as on a machine
    /// without an NVIDIA driver.
    pub fn open() -> Option<Self> {
        Nvml::init().ok().map(|nvml| Self {
            nvml,
            fan: FanCache::new(FAN_READ_EVERY),
        })
    }
}

impl GpuSource for NvmlSource {
    fn read(&mut self) -> Option<GpuReading> {
        // A `Device` borrows the `Nvml` handle, so it is looked up per read rather than stored.
        let device = self.nvml.device_by_index(FIRST_DEVICE_INDEX).ok()?;
        let memory = device.memory_info().ok()?;
        Some(GpuReading {
            name: device.name().ok()?,
            load: device.utilization_rates().ok()?.gpu as f32,
            vram_used_mib: bytes_to_mib(memory.used)?,
            vram_total_mib: bytes_to_mib(memory.total)?,
            temp_c: device.temperature(TemperatureSensor::Gpu).ok()? as f32,
            power_w: milliwatts_to_watts(device.power_usage().ok()?),
            power_limit_w: milliwatts_to_watts(device.enforced_power_limit().ok()?),
            fan_pct: self
                .fan
                .next(|| device.fan_speed(FIRST_FAN_INDEX).ok().map(|pct| pct as f32))?,
        })
    }
}

/// Repeats a slow-moving reading between real reads of it.
///
/// A failed read is cached like any other, so the reading stays `None` until the next real read
/// succeeds instead of flickering between present and absent.
struct FanCache {
    every: u32,
    countdown: u32,
    last: Option<f32>,
}

impl FanCache {
    fn new(every: u32) -> Self {
        Self {
            every,
            countdown: 0,
            last: None,
        }
    }

    /// Runs `read` on the first call and on every `every`th call after it, and repeats its result
    /// on the calls in between.
    fn next(&mut self, read: impl FnOnce() -> Option<f32>) -> Option<f32> {
        if self.countdown == 0 {
            self.last = read();
            self.countdown = self.every.saturating_sub(1);
        } else {
            self.countdown -= 1;
        }
        self.last
    }
}

fn bytes_to_mib(bytes: u64) -> Option<u32> {
    u32::try_from(bytes / BYTES_PER_MIB).ok()
}

fn milliwatts_to_watts(milliwatts: u32) -> f32 {
    milliwatts as f32 / MILLIWATTS_PER_WATT
}

pub struct GpuCollector {
    source: Option<Box<dyn GpuSource>>,
}

impl GpuCollector {
    /// A collector without a source reports an absent GPU on every sample.
    pub fn new(source: Option<Box<dyn GpuSource>>) -> Self {
        Self { source }
    }
}

impl Source<GpuSnapshot> for GpuCollector {
    fn sample(&mut self) -> GpuSnapshot {
        // An unreadable GPU yields the default snapshot, so no stale values outlive the failure.
        let Some(reading) = self.source.as_mut().and_then(|source| source.read()) else {
            return GpuSnapshot::default();
        };
        GpuSnapshot {
            present: true,
            name: reading.name,
            load: reading.load,
            vram_used_mib: reading.vram_used_mib,
            vram_total_mib: reading.vram_total_mib,
            temp_c: reading.temp_c,
            power_w: reading.power_w,
            power_limit_w: reading.power_limit_w,
            fan_pct: reading.fan_pct,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::collections::VecDeque;

    use ghost_proto::GpuSnapshot;

    use super::*;

    struct Scripted(VecDeque<Option<GpuReading>>);

    impl GpuSource for Scripted {
        fn read(&mut self) -> Option<GpuReading> {
            self.0.pop_front().flatten()
        }
    }

    fn reading() -> GpuReading {
        GpuReading {
            name: "NVIDIA GeForce RTX 4080 SUPER".to_owned(),
            load: 12.0,
            vram_used_mib: 3214,
            vram_total_mib: 16376,
            temp_c: 48.0,
            power_w: 54.5,
            power_limit_w: 320.0,
            fan_pct: 35.0,
        }
    }

    fn collector(script: impl IntoIterator<Item = Option<GpuReading>>) -> GpuCollector {
        GpuCollector::new(Some(Box::new(Scripted(script.into_iter().collect()))))
    }

    #[test]
    fn maps_reading() {
        let expected = reading();
        let snapshot = collector([Some(reading())]).sample();

        assert_eq!(
            snapshot,
            GpuSnapshot {
                present: true,
                name: expected.name,
                load: expected.load,
                vram_used_mib: expected.vram_used_mib,
                vram_total_mib: expected.vram_total_mib,
                temp_c: expected.temp_c,
                power_w: expected.power_w,
                power_limit_w: expected.power_limit_w,
                fan_pct: expected.fan_pct,
            }
        );
    }

    #[test]
    fn missing_nvml_is_absent() {
        assert!(!GpuCollector::new(None).sample().present);
    }

    #[test]
    fn error_mid_run_degrades() {
        let mut gpu = collector([Some(reading()), None, Some(reading())]);

        let before = gpu.sample();
        let during = gpu.sample();
        let after = gpu.sample();

        assert!(before.present);
        assert_eq!(during, GpuSnapshot::default());
        assert!(after.present);
        assert_eq!(after.name, reading().name);
    }

    #[test]
    fn fan_cache_reads_on_the_first_call_and_every_nth_after() {
        let reads = Cell::new(0);
        let mut fan = FanCache::new(5);

        for _ in 0..11 {
            fan.next(|| {
                reads.set(reads.get() + 1);
                Some(40.0)
            });
        }

        // Calls 1, 6 and 11.
        assert_eq!(reads.get(), 3);
    }

    #[test]
    fn fan_cache_repeats_the_last_read_in_between() {
        let mut fan = FanCache::new(3);

        let seen: Vec<_> = (1..=7).map(|call| fan.next(|| Some(call as f32))).collect();

        assert_eq!(seen, [1.0, 1.0, 1.0, 4.0, 4.0, 4.0, 7.0].map(Some).to_vec());
    }

    #[test]
    fn fan_cache_stays_empty_after_a_failed_read_until_a_later_read_succeeds() {
        let mut fan = FanCache::new(3);

        assert_eq!(fan.next(|| Some(30.0)), Some(30.0));
        assert_eq!(fan.next(|| unreachable!()), Some(30.0));
        assert_eq!(fan.next(|| unreachable!()), Some(30.0));
        assert_eq!(fan.next(|| None), None);
        assert_eq!(fan.next(|| unreachable!()), None);
        assert_eq!(fan.next(|| unreachable!()), None);
        assert_eq!(fan.next(|| Some(35.0)), Some(35.0));
    }

    #[test]
    fn bytes_to_mib_rounds_down_to_whole_mebibytes() {
        assert_eq!(bytes_to_mib(BYTES_PER_MIB - 1), Some(0));
        assert_eq!(bytes_to_mib(BYTES_PER_MIB), Some(1));
    }

    #[test]
    fn bytes_to_mib_is_none_past_what_a_u32_of_mebibytes_holds() {
        let largest_mib = u64::from(u32::MAX);

        assert_eq!(bytes_to_mib(largest_mib * BYTES_PER_MIB), Some(u32::MAX));
        assert_eq!(bytes_to_mib((largest_mib + 1) * BYTES_PER_MIB), None);
        assert_eq!(bytes_to_mib(u64::MAX), None);
    }

    #[test]
    fn milliwatts_to_watts_keeps_the_fraction() {
        assert_eq!(milliwatts_to_watts(55_728), 55.728);
        assert_eq!(milliwatts_to_watts(0), 0.0);
    }

    #[test]
    fn fan_cache_reads_every_call_when_the_period_is_one() {
        let mut fan = FanCache::new(1);

        assert_eq!(fan.next(|| Some(1.0)), Some(1.0));
        assert_eq!(fan.next(|| Some(2.0)), Some(2.0));
    }
}
