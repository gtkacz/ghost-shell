mod cpu;
mod disk;
mod gpu;
mod hwmon;
mod labels;
mod mem;
mod net;
mod procs;
mod rapl;

pub use cpu::{CpuCollector, parse_cpu_list};
pub use disk::{DiskCollector, FsStat, StatvfsFs, attach_temps};
pub use gpu::{GpuCollector, GpuReading, GpuSource, NvmlSource};
pub use hwmon::SensorCollector;
pub use mem::MemCollector;
pub use net::NetCollector;
pub use procs::{ProcCollector, ProcIdentity, ProcSample, identity};

use std::time::Duration;

use crate::sysroot::SysRoot;

/// A telemetry collector that yields one snapshot per tick.
///
/// Sampling cannot fail: a collector whose inputs are unreadable reports empty or zeroed data
/// instead, so one flaky file never stalls the other collectors.
pub trait Source<T> {
    fn sample(&mut self) -> T;

    /// Called just before the first sample after ticks that took none. From then on no value may
    /// date from before the gap: readings repeated between real reads are read afresh, and values
    /// measured since the previous sample are measured from the next one, which the sampler drops
    /// for such a source because it spans the gap.
    fn resume(&mut self) {}
}

/// Zero when the counter went backwards, as it does when a device is reset or re-enumerated.
pub(crate) fn bytes_per_second(now: u64, before: u64, elapsed: Duration) -> u64 {
    if elapsed.is_zero() {
        return 0;
    }
    let bytes = now.saturating_sub(before);
    (bytes as f64 / elapsed.as_secs_f64()) as u64
}

/// The trimmed text of a file, or `None` when it cannot be read or is blank.
pub(crate) fn read_text(root: &SysRoot, rel: &str) -> Option<String> {
    let text = root.read(rel).ok()?;
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}
