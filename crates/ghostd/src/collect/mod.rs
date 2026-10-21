mod cpu;
mod hwmon;
mod labels;
mod mem;

pub use cpu::{CpuCollector, parse_cpu_list};
pub use hwmon::SensorCollector;
pub use mem::MemCollector;

/// A telemetry collector that yields one snapshot per tick.
///
/// Sampling cannot fail: a collector whose inputs are unreadable reports empty or zeroed data
/// instead, so one flaky file never stalls the other collectors.
pub trait Source<T> {
    fn sample(&mut self) -> T;
}
