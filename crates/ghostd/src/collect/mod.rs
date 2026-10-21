mod cpu;
mod disk;
mod gpu;
mod hwmon;
mod labels;
mod mem;

pub use cpu::{CpuCollector, parse_cpu_list};
pub use disk::{DiskCollector, FsStat, StatvfsFs, attach_temps};
pub use gpu::{GpuCollector, GpuReading, GpuSource, NvmlSource};
pub use hwmon::SensorCollector;
pub use mem::MemCollector;

use crate::sysroot::SysRoot;

/// A telemetry collector that yields one snapshot per tick.
///
/// Sampling cannot fail: a collector whose inputs are unreadable reports empty or zeroed data
/// instead, so one flaky file never stalls the other collectors.
pub trait Source<T> {
    fn sample(&mut self) -> T;
}

/// The trimmed text of a file, or `None` when it cannot be read or is blank.
pub(crate) fn read_text(root: &SysRoot, rel: &str) -> Option<String> {
    let text = root.read(rel).ok()?;
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}
