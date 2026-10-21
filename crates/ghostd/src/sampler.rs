use std::collections::BTreeSet;
use std::iter;

use ghost_proto::{
    CpuSnapshot, DiskSnapshot, GpuSnapshot, MemSnapshot, SensorSnapshot, ServerMsg, data_msg,
    topics,
};

use crate::collect::{Source, attach_temps};
use crate::history::History;

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
        let value = self.source.sample();
        if resumed && self.measure == Measure::SincePrevious {
            return None;
        }
        let missed = tick - self.kept_at - 1;
        self.kept_at = tick;
        Some(Kept { value, missed })
    }
}

/// Samples the built-in collectors whose topics are wanted and feeds the load series into a
/// [`History`].
pub struct Sampler {
    cpu: Tracked<CpuSnapshot>,
    mem: Tracked<MemSnapshot>,
    sensors: Tracked<SensorSnapshot>,
    gpu: Tracked<GpuSnapshot>,
    disks: Tracked<DiskSnapshot>,
    seq: u64,
}

impl Sampler {
    pub fn new(
        cpu: BoxedSource<CpuSnapshot>,
        mem: BoxedSource<MemSnapshot>,
        sensors: BoxedSource<SensorSnapshot>,
        gpu: BoxedSource<GpuSnapshot>,
        disks: BoxedSource<DiskSnapshot>,
    ) -> Self {
        Self {
            cpu: Tracked::new(cpu, Measure::SincePrevious),
            mem: Tracked::new(mem, Measure::SincePrevious),
            sensors: Tracked::new(sensors, Measure::Instant),
            gpu: Tracked::new(gpu, Measure::Instant),
            disks: Tracked::new(disks, Measure::SincePrevious),
            seq: 0,
        }
    }

    /// Samples the sources of the `wanted` topics and returns one `Data` message per wanted topic
    /// that has a value to publish, in the order `cpu`, `mem`, `sensors`, `gpu`, `disks`.
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
            record_cpu(hist, &cpu);
            messages.push(data_msg(topics::CPU, seq, &cpu.value));
        }
        if is_wanted(topics::MEM)
            && let Some(mem) = self.mem.sample(seq)
        {
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
            messages.push(data_msg(topics::SENSORS, seq, &sensors.value));
        }
        if is_wanted(topics::GPU)
            && let Some(gpu) = self.gpu.sample(seq)
        {
            hist.push_gap(GPU_LOAD_KEY, gpu.missed);
            hist.push(GPU_LOAD_KEY, gpu.value.present.then_some(gpu.value.load));
            messages.push(data_msg(topics::GPU, seq, &gpu.value));
        }
        if is_wanted(topics::DISKS)
            && let Some(mut disks) = self.disks.sample(seq)
        {
            if let Some(sensors) = &sensors {
                attach_temps(&mut disks.value, &sensors.value);
            }
            messages.push(data_msg(topics::DISKS, seq, &disks.value));
        }
        messages
    }
}

fn record_cpu(hist: &mut History, cpu: &Kept<CpuSnapshot>) {
    let ccd_loads = CCD_LOAD_KEYS
        .into_iter()
        .zip(cpu.value.ccds.iter().map(|ccd| ccd.load));
    for (key, load) in iter::once((CPU_LOAD_KEY, cpu.value.load)).chain(ccd_loads) {
        hist.push_gap(key, cpu.missed);
        hist.push(key, Some(load));
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use ghost_proto::{CcdLoad, Drive, Reading};

    use super::*;

    fn assert_send<T: Send>() {}

    fn wanted(names: &[&str]) -> BTreeSet<String> {
        names.iter().map(|&name| name.to_owned()).collect()
    }

    fn all() -> BTreeSet<String> {
        wanted(&topics::BUILTIN)
    }

    struct Fixed<T>(T);

    impl<T: Clone> Source<T> for Fixed<T> {
        fn sample(&mut self) -> T {
            self.0.clone()
        }
    }

    struct RisingLoad {
        last: f32,
    }

    impl Source<CpuSnapshot> for RisingLoad {
        fn sample(&mut self) -> CpuSnapshot {
            self.last += 1.0;
            CpuSnapshot {
                load: self.last,
                ..Default::default()
            }
        }
    }

    /// A fixed value that counts how often it is sampled.
    struct Counted<T> {
        value: T,
        samples: Arc<AtomicUsize>,
    }

    impl<T: Clone> Source<T> for Counted<T> {
        fn sample(&mut self) -> T {
            self.samples.fetch_add(1, Ordering::Relaxed);
            self.value.clone()
        }
    }

    /// Samples taken of cpu, mem, sensors, gpu and disks, in that order.
    #[derive(Default)]
    struct SampleCounts([Arc<AtomicUsize>; 5]);

    impl SampleCounts {
        fn get(&self) -> [usize; 5] {
            self.0
                .each_ref()
                .map(|samples| samples.load(Ordering::Relaxed))
        }
    }

    fn counted<T>(value: T, samples: &Arc<AtomicUsize>) -> Box<Counted<T>> {
        Box::new(Counted {
            value,
            samples: Arc::clone(samples),
        })
    }

    #[derive(Default)]
    struct Machine {
        cpu: CpuSnapshot,
        sensors: SensorSnapshot,
        gpu: GpuSnapshot,
        disks: DiskSnapshot,
    }

    impl Machine {
        fn sampler(self) -> Sampler {
            Sampler::new(
                Box::new(Fixed(self.cpu)),
                Box::new(Fixed(MemSnapshot::default())),
                Box::new(Fixed(self.sensors)),
                Box::new(Fixed(self.gpu)),
                Box::new(Fixed(self.disks)),
            )
        }

        fn counted_sampler(self) -> (Sampler, SampleCounts) {
            let counts = SampleCounts::default();
            let [cpu, mem, sensors, gpu, disks] = &counts.0;
            let sampler = Sampler::new(
                counted(self.cpu, cpu),
                counted(MemSnapshot::default(), mem),
                counted(self.sensors, sensors),
                counted(self.gpu, gpu),
                counted(self.disks, disks),
            );
            (sampler, counts)
        }
    }

    /// One NVMe drive whose temperature the sensors report, and one they do not.
    fn machine_with_an_nvme_reading() -> Machine {
        let drive = |model: &str, serial: &str| Drive {
            id: format!("{model}#{serial}"),
            model: model.to_owned(),
            ..Default::default()
        };
        Machine {
            sensors: SensorSnapshot {
                temps: vec![Reading {
                    id: "nvme/SERIAL".to_owned(),
                    label: "Composite".to_owned(),
                    value: Some(41.0),
                }],
                fans: vec![],
            },
            disks: DiskSnapshot {
                drives: vec![drive("Model", "SERIAL"), drive("Other", "UNMATCHED")],
            },
            ..Default::default()
        }
    }

    fn ccd(load: f32) -> CcdLoad {
        CcdLoad {
            load,
            ..Default::default()
        }
    }

    fn topic_and_seq(msg: &ServerMsg) -> (&str, u64) {
        match msg {
            ServerMsg::Data { topic, seq, .. } => (topic, *seq),
            other => panic!("expected a data message, got {other:?}"),
        }
    }

    fn topics_of(msgs: &[ServerMsg]) -> Vec<&str> {
        msgs.iter().map(|msg| topic_and_seq(msg).0).collect()
    }

    fn data_for<'a>(msgs: &'a [ServerMsg], wanted: &str) -> &'a serde_json::Value {
        msgs.iter()
            .find_map(|msg| match msg {
                ServerMsg::Data { topic, data, .. } if topic == wanted => Some(data),
                _ => None,
            })
            .unwrap_or_else(|| panic!("no {wanted} message"))
    }

    #[test]
    fn sampler_is_send() {
        assert_send::<Sampler>();
    }

    #[test]
    fn tick_order_and_seq() {
        let mut sampler = Machine::default().sampler();
        let mut history = History::new(3);

        for expected_seq in 1..=3 {
            let msgs = sampler.tick(&mut history, &all());

            let observed: Vec<_> = msgs.iter().map(topic_and_seq).collect();
            let expected = [
                (topics::CPU, expected_seq),
                (topics::MEM, expected_seq),
                (topics::SENSORS, expected_seq),
                (topics::GPU, expected_seq),
                (topics::DISKS, expected_seq),
            ];
            assert_eq!(observed, expected);
        }
    }

    #[test]
    fn tick_samples_only_wanted_sources() {
        let (mut sampler, counts) = Machine::default().counted_sampler();

        let msgs = sampler.tick(&mut History::new(3), &wanted(&[topics::CPU]));

        assert_eq!(counts.get(), [1, 0, 0, 0, 0]);
        assert_eq!(topics_of(&msgs), [topics::CPU]);
    }

    #[test]
    fn disks_pull_in_sensors_without_publishing_them() {
        let (mut sampler, counts) = machine_with_an_nvme_reading().counted_sampler();

        let msgs = sampler.tick(&mut History::new(3), &wanted(&[topics::DISKS]));

        assert_eq!(counts.get(), [0, 0, 1, 0, 1]);
        assert_eq!(topics_of(&msgs), [topics::DISKS]);
        let disks: DiskSnapshot = serde_json::from_value(data_for(&msgs, topics::DISKS).clone())
            .expect("disks message holds a DiskSnapshot");
        assert_eq!(disks.drives[0].temp_c, Some(41.0));
    }

    #[test]
    fn seq_rises_on_ticks_that_publish_nothing() {
        let mut sampler = Machine::default().sampler();
        let mut history = History::new(3);

        sampler.tick(&mut history, &wanted(&[]));
        let msgs = sampler.tick(&mut history, &wanted(&[topics::GPU]));

        assert_eq!(topic_and_seq(&msgs[0]), (topics::GPU, 2));
    }

    #[test]
    fn a_resumed_source_skips_a_baseline_then_pushes_nulls_for_the_gap() {
        let mut sampler = Sampler::new(
            Box::new(RisingLoad { last: 0.0 }),
            Box::new(Fixed(MemSnapshot::default())),
            Box::new(Fixed(SensorSnapshot::default())),
            Box::new(Fixed(GpuSnapshot {
                present: true,
                load: 50.0,
                ..Default::default()
            })),
            Box::new(Fixed(DiskSnapshot::default())),
        );
        let mut history = History::new(10);
        let watched = wanted(&[topics::CPU, topics::MEM, topics::GPU, topics::DISKS]);

        for _ in 1..=2 {
            sampler.tick(&mut history, &watched);
        }
        for _ in 3..=5 {
            sampler.tick(&mut history, &wanted(&[]));
        }
        let rebaseline = sampler.tick(&mut history, &watched);
        let resumed = sampler.tick(&mut history, &watched);

        // The gpu reading stands on its own, while the deltas' first samples span the whole gap.
        assert_eq!(topics_of(&rebaseline), [topics::GPU]);
        assert_eq!(
            topics_of(&resumed),
            [topics::CPU, topics::MEM, topics::GPU, topics::DISKS]
        );
        assert_eq!(data_for(&resumed, topics::CPU)["load"], 4.0);
        let series = history.series();
        assert_eq!(
            series["load"],
            [Some(1.0), Some(2.0), None, None, None, None, Some(4.0)]
        );
        assert_eq!(
            series["gpu_load"],
            [
                Some(50.0),
                Some(50.0),
                None,
                None,
                None,
                Some(50.0),
                Some(50.0)
            ]
        );
    }

    #[test]
    fn history_caps() {
        let mut sampler = Sampler::new(
            Box::new(RisingLoad { last: 0.0 }),
            Box::new(Fixed(MemSnapshot::default())),
            Box::new(Fixed(SensorSnapshot::default())),
            Box::new(Fixed(GpuSnapshot::default())),
            Box::new(Fixed(DiskSnapshot::default())),
        );
        let mut history = History::new(3);

        for _ in 0..5 {
            sampler.tick(&mut history, &all());
        }

        assert_eq!(history.series()["load"], [Some(3.0), Some(4.0), Some(5.0)]);
    }

    #[test]
    fn ccd_keys_present() {
        let mut sampler = Machine {
            cpu: CpuSnapshot {
                load: 15.0,
                ccds: vec![ccd(10.0), ccd(20.0)],
                ..Default::default()
            },
            ..Default::default()
        }
        .sampler();
        let mut history = History::new(10);

        sampler.tick(&mut history, &all());
        sampler.tick(&mut history, &all());

        let series = history.series();
        assert_eq!(series["load"], [Some(15.0), Some(15.0)]);
        assert_eq!(series["ccd0"], [Some(10.0), Some(10.0)]);
        assert_eq!(series["ccd1"], [Some(20.0), Some(20.0)]);
    }

    #[test]
    fn single_ccd_machine_has_no_ccd1_series() {
        let mut sampler = Machine {
            cpu: CpuSnapshot {
                ccds: vec![ccd(10.0)],
                ..Default::default()
            },
            ..Default::default()
        }
        .sampler();
        let mut history = History::new(10);

        sampler.tick(&mut history, &all());

        let series = history.series();
        assert!(series.contains_key("ccd0"));
        assert!(!series.contains_key("ccd1"));
    }

    #[test]
    fn an_absent_gpu_pushes_null() {
        let mut absent = Machine {
            gpu: GpuSnapshot {
                present: false,
                load: 99.0,
                ..Default::default()
            },
            ..Default::default()
        }
        .sampler();
        let mut present = Machine {
            gpu: GpuSnapshot {
                present: true,
                load: 42.0,
                ..Default::default()
            },
            ..Default::default()
        }
        .sampler();
        let mut absent_history = History::new(10);
        let mut present_history = History::new(10);

        absent.tick(&mut absent_history, &all());
        present.tick(&mut present_history, &all());

        assert_eq!(absent_history.series()["gpu_load"], [None]);
        assert_eq!(present_history.series()["gpu_load"], [Some(42.0)]);
    }

    #[test]
    fn drive_temperature_comes_from_the_matching_nvme_reading() {
        let mut sampler = machine_with_an_nvme_reading().sampler();

        let msgs = sampler.tick(&mut History::new(3), &all());

        let disks: DiskSnapshot = serde_json::from_value(data_for(&msgs, topics::DISKS).clone())
            .expect("disks message holds a DiskSnapshot");
        assert_eq!(disks.drives[0].temp_c, Some(41.0));
        assert_eq!(disks.drives[1].temp_c, None);
    }
}
