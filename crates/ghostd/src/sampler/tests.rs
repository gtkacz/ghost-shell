use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use ghost_proto::{CcdLoad, CoreLoad, Drive, DriveState, ProcTree, Reading};

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

/// Samples taken of cpu, mem, sensors, gpu, disks and procs, in that order.
#[derive(Default)]
struct SampleCounts([Arc<AtomicUsize>; 6]);

impl SampleCounts {
    fn get(&self) -> [usize; 6] {
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
    mem: MemSnapshot,
    sensors: SensorSnapshot,
    gpu: GpuSnapshot,
    disks: DiskSnapshot,
    procs: ProcSample,
}

impl Machine {
    fn sampler(self) -> Sampler {
        Sampler::new(
            Box::new(Fixed(self.cpu)),
            Box::new(Fixed(self.mem)),
            Box::new(Fixed(self.sensors)),
            Box::new(Fixed(self.gpu)),
            Box::new(Fixed(self.disks)),
            Box::new(Fixed(self.procs)),
        )
    }

    fn counted_sampler(self) -> (Sampler, SampleCounts) {
        let counts = SampleCounts::default();
        let [cpu, mem, sensors, gpu, disks, procs] = &counts.0;
        let sampler = Sampler::new(
            counted(self.cpu, cpu),
            counted(self.mem, mem),
            counted(self.sensors, sensors),
            counted(self.gpu, gpu),
            counted(self.disks, disks),
            counted(self.procs, procs),
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

/// A board sensor listed ahead of the CPU package, which reads 58 °C.
fn machine_with_a_package_temp() -> Machine {
    let reading = |id: &str, value: f32| Reading {
        id: id.to_owned(),
        label: id.to_owned(),
        value: Some(value),
    };
    Machine {
        sensors: SensorSnapshot {
            temps: vec![
                reading("nct6799/SYSTIN", 31.0),
                reading("k10temp/Tctl", 58.0),
            ],
            fans: vec![],
        },
        ..Default::default()
    }
}

/// What ticking a sampler left behind.
struct Ticked {
    history: History,
    counts: SampleCounts,
}

impl Ticked {
    fn sensors_sampled(&self) -> bool {
        let [_, _, sensors, ..] = self.counts.get();
        sensors > 0
    }

    fn history(&self, key: &str) -> Vec<Option<f32>> {
        self.history.series().remove(key).unwrap_or_default()
    }
}

/// Ticks once per demand in `demands`, on a machine whose CPU package reads 58 °C.
fn ticks_wanting(demands: &[&[&str]]) -> Ticked {
    let (mut sampler, counts) = machine_with_a_package_temp().counted_sampler();
    let mut history = History::new(10);
    for topics in demands {
        sampler.tick(&mut history, &wanted(topics));
    }
    Ticked { history, counts }
}

fn tick_wanting(topics: &[&str]) -> Ticked {
    ticks_wanting(&[topics])
}

/// Ticks once with every topic wanted, on a machine with `drives`.
fn tick_with(drives: &[Drive]) -> Ticked {
    let (mut sampler, counts) = Machine {
        disks: DiskSnapshot {
            drives: drives.to_vec(),
        },
        ..Default::default()
    }
    .counted_sampler();
    let mut history = History::new(10);
    sampler.tick(&mut history, &all());
    Ticked { history, counts }
}

fn drive(kernel_name: &str, state: DriveState, temp_c: Option<f32>) -> Drive {
    Drive {
        kernel_name: kernel_name.to_owned(),
        state,
        temp_c,
        ..Default::default()
    }
}

fn with_rates(drive: Drive, read_bps: u64, write_bps: u64) -> Drive {
    Drive {
        read_bps,
        write_bps,
        ..drive
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
    let mut sampler = Machine {
        procs: ProcSample {
            top: vec![],
            tree: Some(ProcTree::default()),
        },
        ..Default::default()
    }
    .sampler();
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
            (topics::PROCS, expected_seq),
            (topics::PROCS_TREE, expected_seq),
        ];
        assert_eq!(observed, expected);
    }
}

#[test]
fn tick_samples_only_wanted_sources() {
    let (mut sampler, counts) = Machine::default().counted_sampler();

    let msgs = sampler.tick(&mut History::new(3), &wanted(&[topics::CPU]));

    assert_eq!(counts.get(), [1, 0, 0, 0, 0, 0]);
    assert_eq!(topics_of(&msgs), [topics::CPU]);
}

#[test]
fn disks_pull_in_sensors_without_publishing_them() {
    let (mut sampler, counts) = machine_with_an_nvme_reading().counted_sampler();

    let msgs = sampler.tick(&mut History::new(3), &wanted(&[topics::DISKS]));

    assert_eq!(counts.get(), [0, 0, 1, 0, 1, 0]);
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
        Box::new(Fixed(ProcSample::default())),
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

/// Records every sample and resume it is asked for, in order.
struct Journal(Arc<Mutex<Vec<&'static str>>>);

impl Source<GpuSnapshot> for Journal {
    fn sample(&mut self) -> GpuSnapshot {
        self.0.lock().unwrap().push("sample");
        GpuSnapshot::default()
    }

    fn resume(&mut self) {
        self.0.lock().unwrap().push("resume");
    }
}

#[test]
fn a_source_hears_of_a_resume_just_before_its_first_sample_after_a_gap() {
    let journal = Arc::new(Mutex::new(Vec::new()));
    let mut sampler = Sampler::new(
        Box::new(Fixed(CpuSnapshot::default())),
        Box::new(Fixed(MemSnapshot::default())),
        Box::new(Fixed(SensorSnapshot::default())),
        Box::new(Journal(Arc::clone(&journal))),
        Box::new(Fixed(DiskSnapshot::default())),
        Box::new(Fixed(ProcSample::default())),
    );
    let mut history = History::new(3);
    let gpu = wanted(&[topics::GPU]);

    sampler.tick(&mut history, &gpu);
    sampler.tick(&mut history, &gpu);
    sampler.tick(&mut history, &wanted(&[]));
    sampler.tick(&mut history, &gpu);

    assert_eq!(
        *journal.lock().unwrap(),
        ["sample", "sample", "resume", "sample"]
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
        Box::new(Fixed(ProcSample::default())),
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
            temp_c: 61.0,
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
    assert_eq!(absent_history.series()["temp_gpu"], [None]);
    assert_eq!(present_history.series()["gpu_load"], [Some(42.0)]);
    assert_eq!(present_history.series()["temp_gpu"], [Some(61.0)]);
}

#[test]
fn every_core_gets_its_own_series() {
    let core = |cpus: [u16; 2], load: f32| CoreLoad {
        cpus: cpus.to_vec(),
        load,
    };
    let mut sampler = Machine {
        cpu: CpuSnapshot {
            cores: vec![core([0, 2], 5.0), core([1, 3], 7.0)],
            ..Default::default()
        },
        ..Default::default()
    }
    .sampler();
    let mut history = History::new(10);

    sampler.tick(&mut history, &wanted(&[topics::CPU]));

    let series = history.series();
    assert_eq!(series["core0"], [Some(5.0)]);
    assert_eq!(series["core1"], [Some(7.0)]);
    assert!(!series.contains_key("core2"));
}

#[test]
fn mem_used_is_in_gib() {
    let mut sampler = Machine {
        mem: MemSnapshot {
            used_kib: 3 * 1_048_576 + 524_288,
            ..Default::default()
        },
        ..Default::default()
    }
    .sampler();
    let mut history = History::new(10);

    sampler.tick(&mut history, &wanted(&[topics::MEM]));

    assert_eq!(history.series()["mem_used"], [Some(3.5)]);
}

#[test]
fn temp_cpu_is_recorded_only_when_sensors_is_published() {
    let disks_only = tick_wanting(&[topics::DISKS]);
    // Drive temperatures need the sensors, so they are sampled all the same.
    assert!(disks_only.sensors_sampled());
    assert_eq!(disks_only.history("temp_cpu"), Vec::<Option<f32>>::new());

    assert_eq!(
        tick_wanting(&[topics::SENSORS]).history("temp_cpu"),
        [Some(58.0)]
    );
}

#[test]
fn temp_cpu_gets_a_null_for_each_tick_sensors_went_unpublished() {
    let ticked = ticks_wanting(&[&[topics::SENSORS], &[topics::DISKS], &[topics::SENSORS]]);

    assert_eq!(ticked.history("temp_cpu"), [Some(58.0), None, Some(58.0)]);
}

#[test]
fn temp_cpu_is_null_without_a_package_sensor() {
    let ticked = tick_with(&[]);

    assert_eq!(ticked.history("temp_cpu"), [None]);
}

#[test]
fn temp_nvme_is_the_hottest_live_nvme() {
    let nvme0 = drive("nvme0n1", DriveState::Live, Some(38.0));
    let dead_nvme1 = drive("nvme1n1", DriveState::Dead, None);
    let unmounted_nvme2 = drive("nvme2n1", DriveState::Unmounted, Some(70.0));
    let sda = drive("sda", DriveState::Live, Some(30.0));

    assert_eq!(
        tick_with(&[nvme0, dead_nvme1.clone(), unmounted_nvme2, sda.clone()]).history("temp_nvme"),
        [Some(38.0)]
    );
    assert_eq!(tick_with(&[dead_nvme1, sda]).history("temp_nvme"), [None]);
}

#[test]
fn disk_rates_sum_the_live_drives() {
    let ticked = tick_with(&[
        with_rates(drive("nvme0n1", DriveState::Live, None), 1_000_000, 10),
        with_rates(drive("nvme1n1", DriveState::Live, None), 2_000_000, 20),
        with_rates(drive("nvme2n1", DriveState::Dead, None), 9_000_000, 90),
        with_rates(drive("sda", DriveState::Unmounted, None), 4_000_000, 40),
    ]);

    assert_eq!(ticked.history("disk_read"), [Some(3_000_000.0)]);
    assert_eq!(ticked.history("disk_write"), [Some(30.0)]);
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
