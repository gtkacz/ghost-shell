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

/// `Send` so a `Sampler` can be ticked from a blocking thread.
type BoxedSource<T> = Box<dyn Source<T> + Send>;

/// Samples every built-in collector once per tick and feeds the load series into a [`History`].
pub struct Sampler {
    cpu: BoxedSource<CpuSnapshot>,
    mem: BoxedSource<MemSnapshot>,
    sensors: BoxedSource<SensorSnapshot>,
    gpu: BoxedSource<GpuSnapshot>,
    disks: BoxedSource<DiskSnapshot>,
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
            cpu,
            mem,
            sensors,
            gpu,
            disks,
            seq: 0,
        }
    }

    /// Samples every source and returns one `Data` message per topic, in the order `cpu`, `mem`,
    /// `sensors`, `gpu`, `disks`.
    ///
    /// All five messages share a `seq` that is 1 on the first tick and rises by 1 on each tick
    /// after it.
    pub fn tick(&mut self, hist: &mut History) -> Vec<ServerMsg> {
        self.seq += 1;
        let seq = self.seq;

        let cpu = self.cpu.sample();
        let mem = self.mem.sample();
        let sensors = self.sensors.sample();
        let gpu = self.gpu.sample();
        let mut disks = self.disks.sample();
        // Drive temperatures come from the sensor readings, hence sensors are sampled before disks.
        attach_temps(&mut disks, &sensors);

        hist.push(CPU_LOAD_KEY, cpu.load);
        for (key, ccd) in CCD_LOAD_KEYS.iter().zip(&cpu.ccds) {
            hist.push(key, ccd.load);
        }
        if gpu.present {
            hist.push(GPU_LOAD_KEY, gpu.load);
        }

        vec![
            data_msg(topics::CPU, seq, &cpu),
            data_msg(topics::MEM, seq, &mem),
            data_msg(topics::SENSORS, seq, &sensors),
            data_msg(topics::GPU, seq, &gpu),
            data_msg(topics::DISKS, seq, &disks),
        ]
    }
}

#[cfg(test)]
mod tests {
    use ghost_proto::{CcdLoad, Drive, Reading};

    use super::*;

    fn assert_send<T: Send>() {}

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
            let msgs = sampler.tick(&mut history);

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
            sampler.tick(&mut history);
        }

        assert_eq!(history.series()["load"], [3.0, 4.0, 5.0]);
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

        sampler.tick(&mut history);
        sampler.tick(&mut history);

        let series = history.series();
        assert_eq!(series["load"], [15.0, 15.0]);
        assert_eq!(series["ccd0"], [10.0, 10.0]);
        assert_eq!(series["ccd1"], [20.0, 20.0]);
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

        sampler.tick(&mut history);

        let series = history.series();
        assert!(series.contains_key("ccd0"));
        assert!(!series.contains_key("ccd1"));
    }

    #[test]
    fn gpu_load_is_recorded_only_when_a_gpu_is_present() {
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

        absent.tick(&mut absent_history);
        present.tick(&mut present_history);

        assert!(!absent_history.series().contains_key("gpu_load"));
        assert_eq!(present_history.series()["gpu_load"], [42.0]);
    }

    #[test]
    fn drive_temperature_comes_from_the_matching_nvme_reading() {
        let drive = |id: &str| Drive {
            id: id.to_owned(),
            ..Default::default()
        };
        let mut sampler = Machine {
            sensors: SensorSnapshot {
                temps: vec![Reading {
                    id: "nvme/SERIAL".to_owned(),
                    label: "Composite".to_owned(),
                    value: Some(41.0),
                }],
                fans: vec![],
            },
            disks: DiskSnapshot {
                drives: vec![drive("Model#SERIAL"), drive("Other#UNMATCHED")],
            },
            ..Default::default()
        }
        .sampler();

        let msgs = sampler.tick(&mut History::new(3));

        let disks: DiskSnapshot = serde_json::from_value(data_for(&msgs, topics::DISKS).clone())
            .expect("disks message holds a DiskSnapshot");
        assert_eq!(disks.drives[0].temp_c, Some(41.0));
        assert_eq!(disks.drives[1].temp_c, None);
    }
}
