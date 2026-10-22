mod common;

use std::collections::BTreeSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use common::{DEFAULT_RSS_KIB, FakeProcess, FakeRoot, Ticks, fake_9950x3d, meminfo, proc_stat};
use ghost_proto::{ProcNode, ProcTree, ProcsSnapshot, ServerMsg, topics};
use ghostd::collect::{ProcCollector, ProcSample, Source};
use ghostd::history::History;
use ghostd::sampler::Sampler;

const TOP_N: usize = 5;
const TOTAL_KIB: u64 = 96_388_184;
const AVAILABLE_KIB: u64 = 77_000_000;
const SWAP_TOTAL_KIB: u64 = 8_388_604;
const SWAP_FREE_KIB: u64 = 8_000_000;
const SMALL_TOTAL_KIB: u64 = 16_000_000;
const BASELINE_TICKS: u64 = 10_000;
// One second on a 32-thread machine at 100 ticks per second.
const INTERVAL_TICKS: u64 = 3_200;
// A gap of 100 seconds, through which one process keeps half the machine busy.
const GAP_TICKS: u64 = 100 * INTERVAL_TICKS;
const BUSY_PROCESS_TICKS: u64 = GAP_TICKS / 2;
// Scans fall on every second sample and the tree on every fifth scan, so eleven samples span two
// trees: the first scan's and the sixth's.
const SAMPLES_SPANNING_TWO_TREES: usize = 11;

/// A machine whose /proc/stat has accumulated `all_cpu_ticks` jiffies, all of them idle.
fn machine(total_kib: u64, all_cpu_ticks: u64) -> FakeRoot {
    let fake = FakeRoot::new();
    fake.write(
        "proc/meminfo",
        &meminfo(total_kib, AVAILABLE_KIB, SWAP_TOTAL_KIB, SWAP_FREE_KIB),
    );
    advance_clock(&fake, all_cpu_ticks);
    fake
}

fn advance_clock(fake: &FakeRoot, all_cpu_ticks: u64) {
    let idle = Ticks {
        idle: all_cpu_ticks,
        ..Ticks::default()
    };
    fake.write("proc/stat", &proc_stat(idle, &[]));
}

/// One scan window: the sample that rescans /proc and the in-between one that repeats its list.
/// Returns the rescan's sample.
fn scan_window(procs: &mut ProcCollector) -> ProcSample {
    let scan = procs.sample();
    procs.sample();
    scan
}

fn pids(sample: &ProcSample) -> Vec<u32> {
    sample.top.iter().map(|process| process.pid).collect()
}

fn tree_pids(tree: &ProcTree) -> Vec<u32> {
    tree.procs.iter().map(|process| process.pid).collect()
}

fn node(tree: &ProcTree, pid: u32) -> &ProcNode {
    tree.procs
        .iter()
        .find(|process| process.pid == pid)
        .unwrap_or_else(|| panic!("pid {pid} is not in the tree"))
}

/// The tree of the collector's first sample, which is its first scan.
fn first_tree(fake: &FakeRoot) -> ProcTree {
    ProcCollector::new(&fake.root(), TOP_N)
        .sample()
        .tree
        .expect("the first scan builds the tree")
}

/// Systemd, a kernel thread, and a user process with no resident memory of its own.
fn machine_with_an_rss_free_process() -> FakeRoot {
    let fake = fake_9950x3d();
    fake.write_process(&FakeProcess::new(1, "systemd").rss_kib(100));
    fake.write_process(&FakeProcess::kernel_thread(2, "kthreadd"));
    fake.write_process(&FakeProcess::new(5, "zypak-sandbox").rss_kib(0).ppid(1));
    fake
}

#[test]
fn process_cpu_is_machine_share() {
    let fake = machine(SMALL_TOTAL_KIB, BASELINE_TICKS);
    let firefox = |utime, stime| {
        FakeProcess::new(42, "firefox")
            .rss_kib(1_600_000)
            .ticks(utime, stime)
    };
    fake.write_process(&firefox(100, 50));
    let mut procs = ProcCollector::new(&fake.root(), TOP_N);

    advance_clock(&fake, BASELINE_TICKS + INTERVAL_TICKS);
    fake.write_process(&firefox(140, 74));
    let snapshot = procs.sample();

    let process = &snapshot.top[0];
    assert_eq!(process.pid, 42);
    assert_eq!(process.name, "firefox");
    assert_eq!(process.cpu, 2.0);
    assert_eq!(process.mem, 10.0);
}

#[test]
fn process_share_is_clamped_to_the_whole_machine() {
    let fake = machine(SMALL_TOTAL_KIB, BASELINE_TICKS);
    fake.write_process(&FakeProcess::new(1, "hog").rss_kib(32_000_000));
    let mut procs = ProcCollector::new(&fake.root(), TOP_N);

    advance_clock(&fake, BASELINE_TICKS + INTERVAL_TICKS);
    fake.write_process(
        &FakeProcess::new(1, "hog")
            .rss_kib(32_000_000)
            .ticks(2 * INTERVAL_TICKS, 0),
    );
    let snapshot = procs.sample();

    assert_eq!(snapshot.top[0].cpu, 100.0);
    assert_eq!(snapshot.top[0].mem, 100.0);
}

#[test]
fn process_first_seen_this_tick_reports_zero_cpu() {
    let fake = machine(TOTAL_KIB, BASELINE_TICKS);
    let mut procs = ProcCollector::new(&fake.root(), TOP_N);

    advance_clock(&fake, BASELINE_TICKS + INTERVAL_TICKS);
    fake.write_process(&FakeProcess::new(7, "latecomer").ticks(3_000, 0));
    let snapshot = procs.sample();

    assert_eq!(pids(&snapshot), vec![7]);
    assert_eq!(snapshot.top[0].cpu, 0.0);
}

#[test]
fn recycled_pid_does_not_inherit_the_previous_baseline() {
    let fake = machine(TOTAL_KIB, BASELINE_TICKS);
    fake.write_process(&FakeProcess::new(5, "old").start_time(1_000).ticks(100, 0));
    let mut procs = ProcCollector::new(&fake.root(), TOP_N);

    advance_clock(&fake, BASELINE_TICKS + INTERVAL_TICKS);
    fake.write_process(&FakeProcess::new(5, "new").start_time(2_000).ticks(500, 0));
    let snapshot = procs.sample();

    assert_eq!(snapshot.top[0].name, "new");
    assert_eq!(snapshot.top[0].cpu, 0.0);
}

#[test]
fn no_elapsed_cpu_time_reports_zero_cpu() {
    let fake = machine(TOTAL_KIB, BASELINE_TICKS);
    fake.write_process(&FakeProcess::new(1, "init"));
    let mut procs = ProcCollector::new(&fake.root(), TOP_N);

    fake.write_process(&FakeProcess::new(1, "init").ticks(500, 0));
    let snapshot = procs.sample();

    assert_eq!(snapshot.top[0].cpu, 0.0);
}

#[test]
fn unreadable_proc_stat_keeps_the_previous_baseline() {
    let fake = machine(TOTAL_KIB, BASELINE_TICKS);
    fake.write_process(&FakeProcess::new(1, "init"));
    let mut procs = ProcCollector::new(&fake.root(), TOP_N);

    fake.remove("proc/stat");
    fake.write_process(&FakeProcess::new(1, "init").ticks(32, 0));
    assert_eq!(scan_window(&mut procs).top[0].cpu, 0.0);

    advance_clock(&fake, BASELINE_TICKS + INTERVAL_TICKS);
    fake.write_process(&FakeProcess::new(1, "init").ticks(64, 0));
    let snapshot = scan_window(&mut procs);

    // Both intervals of process time against both intervals of total time, not one against two.
    assert_eq!(snapshot.top[0].cpu, 2.0);
}

#[test]
fn comm_with_spaces_and_parentheses_is_parsed() {
    let fake = machine(TOTAL_KIB, BASELINE_TICKS);
    let comm = "Web Content (x) y)";
    fake.write_process(&FakeProcess::new(9, comm));
    let mut procs = ProcCollector::new(&fake.root(), TOP_N);

    advance_clock(&fake, BASELINE_TICKS + INTERVAL_TICKS);
    fake.write_process(&FakeProcess::new(9, comm).ticks(60, 4));
    let snapshot = procs.sample();

    assert_eq!(snapshot.top[0].name, comm);
    assert_eq!(snapshot.top[0].cpu, 2.0);
}

#[test]
fn non_utf8_comm_is_kept_with_replacement_characters() {
    let fake = machine(TOTAL_KIB, BASELINE_TICKS);
    fake.write_process(&FakeProcess::new(3, b"bad\xFFname"));
    let mut procs = ProcCollector::new(&fake.root(), TOP_N);

    let snapshot = procs.sample();

    assert_eq!(snapshot.top[0].name, "bad\u{FFFD}name");
}

#[test]
fn truncated_stat_line_skips_the_process() {
    let fake = machine(TOTAL_KIB, BASELINE_TICKS);
    fake.write_bytes("proc/8/stat", b"8 (short) S 1 2 3\n");
    fake.write_process(&FakeProcess::new(9, "whole"));
    let mut procs = ProcCollector::new(&fake.root(), TOP_N);

    assert_eq!(pids(&procs.sample()), vec![9]);
}

#[test]
fn vanished_process_skipped() {
    let fake = machine(TOTAL_KIB, BASELINE_TICKS);
    for pid in 1..=3 {
        fake.write_process(&FakeProcess::new(pid, "worker"));
    }
    let mut procs = ProcCollector::new(&fake.root(), TOP_N);

    advance_clock(&fake, BASELINE_TICKS + INTERVAL_TICKS);
    fake.remove("proc/2");
    fake.remove("proc/3/stat");
    assert_eq!(pids(&scan_window(&mut procs)), vec![1]);

    // Back with ticks far above anything they had at the baseline: a baseline that outlived
    // the missing tick would turn that gap into a cpu share.
    advance_clock(&fake, BASELINE_TICKS + 2 * INTERVAL_TICKS);
    for pid in 2..=3 {
        fake.write_process(&FakeProcess::new(pid, "worker").ticks(640, 0));
    }
    let snapshot = scan_window(&mut procs);

    assert_eq!(pids(&snapshot), vec![1, 2, 3]);
    assert!(snapshot.top.iter().all(|process| process.cpu == 0.0));
}

#[test]
fn kernel_threads_excluded() {
    let fake = machine(TOTAL_KIB, BASELINE_TICKS);
    fake.write_process(&FakeProcess::kernel_thread(2, "kthreadd"));
    fake.write_process(&FakeProcess::new(5, "zombie").rss_kib(0));
    fake.write_process(&FakeProcess::new(4, "bash"));
    let mut procs = ProcCollector::new(&fake.root(), TOP_N);

    assert_eq!(pids(&procs.sample()), vec![4]);
}

#[test]
fn kernel_thread_with_borrowed_address_space_excluded() {
    let fake = machine(TOTAL_KIB, BASELINE_TICKS);
    fake.write_process(&FakeProcess::new(4, "bash"));
    let mut procs = ProcCollector::new(&fake.root(), TOP_N);

    fake.write_process(&FakeProcess::kernel_thread(3, "vhost-worker").rss_kib(DEFAULT_RSS_KIB));

    assert_eq!(pids(&procs.sample()), vec![4]);
}

#[test]
fn kernel_thread_is_read_once_then_skipped() {
    let fake = machine(TOTAL_KIB, BASELINE_TICKS);
    fake.write_process(&FakeProcess::kernel_thread(2, "kthreadd"));
    fake.write_process(&FakeProcess::new(4, "bash"));
    let mut procs = ProcCollector::new(&fake.root(), TOP_N);

    // Were pid 2 read again, this stat would list it as a user process.
    fake.write_process(&FakeProcess::new(2, "impostor"));
    assert_eq!(pids(&scan_window(&mut procs)), vec![4]);
    fake.write_bytes("proc/2/stat", b"not a stat line\n");
    assert_eq!(pids(&scan_window(&mut procs)), vec![4]);
}

#[test]
fn pid_recycled_after_a_kernel_thread_is_read_again() {
    let fake = machine(TOTAL_KIB, BASELINE_TICKS);
    fake.write_process(&FakeProcess::kernel_thread(2, "kworker").start_time(1_000));
    let mut procs = ProcCollector::new(&fake.root(), TOP_N);

    fake.remove("proc/2");
    assert!(scan_window(&mut procs).top.is_empty());
    fake.write_process(&FakeProcess::new(2, "reborn").start_time(2_000));
    let snapshot = scan_window(&mut procs);

    assert_eq!(pids(&snapshot), vec![2]);
    assert_eq!(snapshot.top[0].name, "reborn");
}

#[test]
fn kernel_thread_skip_set_does_not_survive_an_unlistable_proc() {
    let fake = machine(TOTAL_KIB, BASELINE_TICKS);
    fake.write_process(&FakeProcess::kernel_thread(2, "kthreadd"));
    let mut procs = ProcCollector::new(&fake.root(), TOP_N);

    fake.remove("proc");
    assert!(scan_window(&mut procs).top.is_empty());
    // Nothing says pid 2 is the same task after /proc comes back, so it must be read afresh.
    fake.write(
        "proc/meminfo",
        &meminfo(TOTAL_KIB, AVAILABLE_KIB, SWAP_TOTAL_KIB, SWAP_FREE_KIB),
    );
    advance_clock(&fake, BASELINE_TICKS);
    fake.write_process(&FakeProcess::new(2, "reborn"));

    assert_eq!(pids(&scan_window(&mut procs)), vec![2]);
}

#[test]
fn process_share_is_measured_over_the_scan_window() {
    let fake = machine(TOTAL_KIB, BASELINE_TICKS);
    fake.write_process(&FakeProcess::new(1, "worker"));
    let mut procs = ProcCollector::new(&fake.root(), TOP_N);

    advance_clock(&fake, BASELINE_TICKS + INTERVAL_TICKS);
    fake.write_process(&FakeProcess::new(1, "worker").ticks(100, 0));
    let first_scan = procs.sample();
    advance_clock(&fake, BASELINE_TICKS + 2 * INTERVAL_TICKS);
    procs.sample();
    advance_clock(&fake, BASELINE_TICKS + 3 * INTERVAL_TICKS);
    fake.write_process(&FakeProcess::new(1, "worker").ticks(300, 0));
    let second_scan = procs.sample();

    assert_eq!(first_scan.top[0].cpu, 3.125);
    // 200 of the 6400 ticks in the two seconds since the last scan, not 200 of the last 3200.
    assert_eq!(second_scan.top[0].cpu, 3.125);
}

#[test]
fn top_sorted_and_truncated() {
    let fake = machine(TOTAL_KIB, BASELINE_TICKS);
    // Pids 9 and 10 tie, and sort differently as numbers than as strings.
    let busyness = [(14, 5), (13, 10), (12, 20), (11, 30), (10, 50), (9, 50)];
    for (pid, _) in busyness {
        fake.write_process(&FakeProcess::new(pid, "worker"));
    }
    let mut procs = ProcCollector::new(&fake.root(), TOP_N);

    advance_clock(&fake, BASELINE_TICKS + INTERVAL_TICKS);
    for (pid, ticks) in busyness {
        fake.write_process(&FakeProcess::new(pid, "worker").ticks(ticks, 0));
    }
    let snapshot = procs.sample();

    assert_eq!(pids(&snapshot), vec![9, 10, 11, 12, 13]);
}

#[test]
fn the_tree_comes_on_every_fifth_scan() {
    let root = fake_9950x3d();
    root.write_process(&FakeProcess::new(1, "systemd").rss_kib(100));
    let mut c = ProcCollector::new(&root.root(), 6);

    let with_tree: Vec<usize> = (1..=SAMPLES_SPANNING_TWO_TREES)
        .filter_map(|i| c.sample().tree.map(|_| i))
        .collect();

    assert_eq!(with_tree, [1, 11]);
}

#[test]
fn the_tree_keeps_rss_free_processes_and_counts_kernel_threads() {
    let t = first_tree(&machine_with_an_rss_free_process());

    assert_eq!(t.kernel_threads, 1);
    assert_eq!(tree_pids(&t), [1, 5]);
    assert_eq!(node(&t, 5).ppid, 1);
    assert_eq!(node(&t, 5).name, "zypak-sandbox");
    assert_eq!(node(&t, 5).rss_kib, 0);
    assert_eq!(node(&t, 1).rss_kib, 100);
}

#[test]
fn top_leaves_out_rss_free_processes() {
    let fake = machine_with_an_rss_free_process();
    let mut c = ProcCollector::new(&fake.root(), TOP_N);

    advance_clock(&fake, INTERVAL_TICKS);
    fake.write_process(
        &FakeProcess::new(5, "zypak-sandbox")
            .rss_kib(0)
            .ppid(1)
            .ticks(1_600, 0),
    );
    let sample = c.sample();

    assert!(sample.top.iter().all(|p| p.pid != 5));
    assert_eq!(pids(&sample), [1]);
    // It is the busiest process, so only its missing RSS keeps it out.
    let tree = sample.tree.expect("the first scan builds the tree");
    assert_eq!(node(&tree, 5).cpu, 50.0);
}

#[test]
fn a_recycled_pid_starts_its_share_from_zero() {
    let fake = machine(TOTAL_KIB, BASELINE_TICKS);
    fake.write_process(&FakeProcess::new(7, "old").start_time(1_000).ticks(100, 0));
    fake.write_process(&FakeProcess::new(8, "steady").ticks(100, 0));
    let mut c = ProcCollector::new(&fake.root(), TOP_N);

    fake.remove("proc/7");
    fake.write_process(
        &FakeProcess::new(7, "reborn")
            .start_time(2_000)
            .ticks(3_000, 0),
    );
    fake.write_process(&FakeProcess::new(8, "steady").ticks(132, 0));
    advance_clock(&fake, BASELINE_TICKS + INTERVAL_TICKS);
    let tree = c.sample().tree.expect("the first scan builds the tree");

    assert_eq!(node(&tree, 7).name, "reborn");
    assert_eq!(node(&tree, 7).cpu, 0.0);
    // The same interval does give a pid that lived through it a share.
    assert_eq!(node(&tree, 8).cpu, 1.0);
}

#[test]
fn missing_mem_total_reads_process_memory_as_zero() {
    let fake = FakeRoot::new();
    fake.write("proc/meminfo", "MemFree:        1000 kB\n");
    advance_clock(&fake, BASELINE_TICKS);
    fake.write_process(&FakeProcess::new(1, "init"));
    let mut procs = ProcCollector::new(&fake.root(), TOP_N);

    let sample = procs.sample();

    assert_eq!(sample.top[0].mem, 0.0);
}

#[test]
fn top_repeats_between_scans_without_a_tree() {
    let fake = machine(TOTAL_KIB, BASELINE_TICKS);
    fake.write_process(&FakeProcess::new(1, "init"));
    let mut procs = ProcCollector::new(&fake.root(), TOP_N);
    let scan = procs.sample();
    assert_eq!(pids(&scan), vec![1]);

    fake.remove("proc/1");
    fake.write_process(&FakeProcess::new(2, "latecomer"));
    let between = procs.sample();

    assert_eq!(between.top, scan.top);
    assert_eq!(between.tree, None);
    assert_eq!(pids(&procs.sample()), vec![2]);
}

#[test]
fn unreadable_root_yields_an_empty_top_and_tree() {
    let fake = FakeRoot::new();
    let mut procs = ProcCollector::new(&fake.root(), TOP_N);

    let sample = procs.sample();

    assert!(sample.top.is_empty());
    assert_eq!(sample.tree, Some(ProcTree::default()));
}

#[test]
fn a_resumed_collector_builds_the_tree_on_its_first_scan() {
    let fake = machine(TOTAL_KIB, BASELINE_TICKS);
    fake.write_process(&FakeProcess::new(1, "systemd"));
    let mut c = ProcCollector::new(&fake.root(), TOP_N);
    for _ in 0..3 {
        c.sample();
    }

    c.resume();
    let dropped_by_the_sampler = c.sample();
    let first_scan = c.sample();

    assert_eq!(dropped_by_the_sampler.tree, None);
    assert_eq!(tree_pids(&first_scan.tree.expect("the tree is due")), [1]);
}

/// Stands in for every collector but the process scan.
struct Idle;

impl<T: Default> Source<T> for Idle {
    fn sample(&mut self) -> T {
        T::default()
    }
}

/// Counts how often the sampler asks the collector it wraps for a sample.
struct Counting {
    inner: ProcCollector,
    samples: Arc<AtomicUsize>,
}

impl Source<ProcSample> for Counting {
    fn sample(&mut self) -> ProcSample {
        self.samples.fetch_add(1, Ordering::Relaxed);
        self.inner.sample()
    }

    fn resume(&mut self) {
        self.inner.resume();
    }
}

fn procs_only_sampler(fake: &FakeRoot, samples: &Arc<AtomicUsize>) -> Sampler {
    Sampler::new(
        Box::new(Idle),
        Box::new(Idle),
        Box::new(Idle),
        Box::new(Idle),
        Box::new(Idle),
        Box::new(Idle),
        Box::new(Counting {
            inner: ProcCollector::new(&fake.root(), TOP_N),
            samples: Arc::clone(samples),
        }),
    )
}

/// The messages of eleven ticks under one demand, and how often the process scan was sampled.
struct Run {
    messages: Vec<ServerMsg>,
    samples: usize,
}

impl Run {
    fn published(&self, wanted: &str) -> usize {
        self.messages
            .iter()
            .filter(|msg| matches!(msg, ServerMsg::Data { topic, .. } if topic == wanted))
            .count()
    }
}

fn run(demand: &[&str]) -> Run {
    let fake = machine(TOTAL_KIB, BASELINE_TICKS);
    fake.write_process(&FakeProcess::new(1, "systemd"));
    let samples = Arc::new(AtomicUsize::new(0));
    let mut sampler = procs_only_sampler(&fake, &samples);
    let wanted: BTreeSet<String> = demand.iter().map(|&topic| topic.to_owned()).collect();
    let mut history = History::new(1);
    let messages = (0..SAMPLES_SPANNING_TWO_TREES)
        .flat_map(|_| sampler.tick(&mut history, &wanted))
        .collect();
    Run {
        messages,
        samples: samples.load(Ordering::Relaxed),
    }
}

#[test]
fn procs_and_tree_publish_only_when_wanted() {
    let procs = run(&[topics::PROCS]);
    assert_eq!(procs.published(topics::PROCS_TREE), 0);
    assert_eq!(procs.published(topics::PROCS), SAMPLES_SPANNING_TWO_TREES);

    let tree = run(&[topics::PROCS_TREE]);
    assert_eq!(tree.published(topics::PROCS), 0);
    // Scans 1 and 11.
    assert_eq!(tree.published(topics::PROCS_TREE), 2);

    // Neither wanted: the source is never sampled.
    assert_eq!(run(&[]).samples, 0);
}

fn procs_data(messages: &[ServerMsg]) -> ProcsSnapshot {
    match messages {
        [ServerMsg::Data { topic, data, .. }] if topic == topics::PROCS => {
            serde_json::from_value(data.clone()).unwrap()
        }
        other => panic!("expected one procs message, got {other:?}"),
    }
}

#[test]
fn a_resumed_procs_topic_measures_its_top_from_after_the_gap() {
    // The scan cadence may stand at either phase when the gap starts.
    for kept_before_the_gap in 1..=2 {
        let fake = machine(TOTAL_KIB, BASELINE_TICKS);
        let worker = |ticks| FakeProcess::new(42, "worker").ticks(ticks, 0);
        fake.write_process(&worker(0));
        let mut sampler = procs_only_sampler(&fake, &Arc::default());
        let mut history = History::new(1);
        let procs = BTreeSet::from([topics::PROCS.to_owned()]);
        let nothing = BTreeSet::new();
        let mut clock = BASELINE_TICKS;
        for _ in 0..kept_before_the_gap {
            clock += INTERVAL_TICKS;
            advance_clock(&fake, clock);
            sampler.tick(&mut history, &procs);
        }

        clock += GAP_TICKS;
        advance_clock(&fake, clock);
        fake.write_process(&worker(BUSY_PROCESS_TICKS));
        for _ in 0..GAP_TICKS / INTERVAL_TICKS {
            sampler.tick(&mut history, &nothing);
        }
        let rebaseline = sampler.tick(&mut history, &procs);
        clock += INTERVAL_TICKS;
        advance_clock(&fake, clock);
        let resumed = procs_data(&sampler.tick(&mut history, &procs));

        assert!(rebaseline.is_empty());
        assert_eq!(resumed.top[0].pid, 42);
        assert_eq!(
            resumed.top[0].cpu, 0.0,
            "the worker's share spans the gap after {kept_before_the_gap} kept samples"
        );
    }
}
