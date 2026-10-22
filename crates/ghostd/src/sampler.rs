use std::collections::BTreeSet;

use ghost_proto::{
    CpuSnapshot, DiskSnapshot, GpuSnapshot, MemSnapshot, ProcsSnapshot, SensorSnapshot, ServerMsg,
    data_msg, topics,
};

use crate::collect::{ProcSample, Source, attach_temps};
use crate::history::{History, record_cpu, record_disks, record_gpu, record_mem, record_sensors};

/// `Send` so a `Sampler` can be ticked from a blocking thread.
type BoxedSource<T> = Box<dyn Source<T> + Send>;

/// How a source's values relate to the time between its samples.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Measure {
    /// Each value covers the time since the previous sample, so after a gap it covers the gap.
    SincePrevious,
    /// Each value stands on its own.
    Instant,
}

/// A source with the ticks it was last sampled and last kept at. Tick 0 stands for the baseline
/// a collector takes when it is built.
struct Tracked<T> {
    source: BoxedSource<T>,
    measure: Measure,
    sampled_at: u64,
    kept_at: u64,
}

/// A value fit to publish and the ticks before it that kept none.
struct Kept<T> {
    value: T,
    missed: u64,
}

impl<T> Tracked<T> {
    fn new(source: BoxedSource<T>, measure: Measure) -> Self {
        Self {
            source,
            measure,
            sampled_at: 0,
            kept_at: 0,
        }
    }

    /// Samples at `tick`. A delta's first sample after skipped ticks would report the gap's
    /// average, so it is dropped and only serves as the next sample's baseline.
    fn sample(&mut self, tick: u64) -> Option<Kept<T>> {
        let resumed = tick - self.sampled_at > 1;
        self.sampled_at = tick;
        if resumed {
            self.source.resume();
        }
        let value = self.source.sample();
        if resumed && self.measure == Measure::SincePrevious {
            return None;
        }
        let missed = tick - self.kept_at - 1;
        self.kept_at = tick;
        Some(Kept { value, missed })
    }
}

/// Samples the built-in collectors whose topics are wanted and feeds the series of the topics it
/// publishes into a [`History`].
pub struct Sampler {
    cpu: Tracked<CpuSnapshot>,
    mem: Tracked<MemSnapshot>,
    sensors: Tracked<SensorSnapshot>,
    /// The tick `sensors` was last published at. Sensors are also sampled for disks alone, so
    /// their history gaps are counted from here rather than from their last sample.
    sensors_published_at: u64,
    gpu: Tracked<GpuSnapshot>,
    disks: Tracked<DiskSnapshot>,
    procs: Tracked<ProcSample>,
    seq: u64,
}

impl Sampler {
    pub fn new(
        cpu: BoxedSource<CpuSnapshot>,
        mem: BoxedSource<MemSnapshot>,
        sensors: BoxedSource<SensorSnapshot>,
        gpu: BoxedSource<GpuSnapshot>,
        disks: BoxedSource<DiskSnapshot>,
        procs: BoxedSource<ProcSample>,
    ) -> Self {
        Self {
            cpu: Tracked::new(cpu, Measure::SincePrevious),
            mem: Tracked::new(mem, Measure::SincePrevious),
            sensors: Tracked::new(sensors, Measure::Instant),
            sensors_published_at: 0,
            gpu: Tracked::new(gpu, Measure::Instant),
            disks: Tracked::new(disks, Measure::SincePrevious),
            procs: Tracked::new(procs, Measure::SincePrevious),
            seq: 0,
        }
    }

    /// Samples the sources of the `wanted` topics and returns one `Data` message per wanted topic
    /// that has a value to publish, in the order `cpu`, `mem`, `sensors`, `gpu`, `disks`, `procs`,
    /// `procs.tree`.
    ///
    /// The messages share a `seq` that is 1 on the first tick and rises by 1 on every tick after
    /// it, whether or not anything is sampled, so a jump in `seq` shows the ticks a topic missed.
    /// Those ticks reach the history as `None` samples once its source is sampled again.
    pub fn tick(&mut self, hist: &mut History, wanted: &BTreeSet<String>) -> Vec<ServerMsg> {
        self.seq += 1;
        let seq = self.seq;
        let is_wanted = |topic: &str| wanted.contains(topic);
        let mut messages = Vec::new();

        if is_wanted(topics::CPU)
            && let Some(cpu) = self.cpu.sample(seq)
        {
            record_cpu(hist, &cpu.value, cpu.missed);
            messages.push(data_msg(topics::CPU, seq, &cpu.value));
        }
        if is_wanted(topics::MEM)
            && let Some(mem) = self.mem.sample(seq)
        {
            record_mem(hist, &mem.value, mem.missed);
            messages.push(data_msg(topics::MEM, seq, &mem.value));
        }
        // Drive temperatures come from the sensor readings, hence sensors are sampled for disks.
        let sensors = if is_wanted(topics::SENSORS) || is_wanted(topics::DISKS) {
            self.sensors.sample(seq)
        } else {
            None
        };
        if is_wanted(topics::SENSORS)
            && let Some(sensors) = &sensors
        {
            let missed = seq - self.sensors_published_at - 1;
            self.sensors_published_at = seq;
            record_sensors(hist, &sensors.value, missed);
            messages.push(data_msg(topics::SENSORS, seq, &sensors.value));
        }
        if is_wanted(topics::GPU)
            && let Some(gpu) = self.gpu.sample(seq)
        {
            record_gpu(hist, &gpu.value, gpu.missed);
            messages.push(data_msg(topics::GPU, seq, &gpu.value));
        }
        if is_wanted(topics::DISKS)
            && let Some(mut disks) = self.disks.sample(seq)
        {
            if let Some(sensors) = &sensors {
                attach_temps(&mut disks.value, &sensors.value);
            }
            record_disks(hist, &disks.value, disks.missed);
            messages.push(data_msg(topics::DISKS, seq, &disks.value));
        }
        // The tree comes from the same scans as the top list, hence one source for both topics.
        if (is_wanted(topics::PROCS) || is_wanted(topics::PROCS_TREE))
            && let Some(procs) = self.procs.sample(seq)
        {
            let ProcSample { top, tree } = procs.value;
            if is_wanted(topics::PROCS) {
                messages.push(data_msg(topics::PROCS, seq, &ProcsSnapshot { top }));
            }
            if is_wanted(topics::PROCS_TREE)
                && let Some(tree) = &tree
            {
                messages.push(data_msg(topics::PROCS_TREE, seq, tree));
            }
        }
        messages
    }
}

#[cfg(test)]
mod tests;
