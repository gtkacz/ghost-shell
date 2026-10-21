mod common;

use common::{FakeProcess, FakeRoot, Ticks, meminfo, proc_stat};
use ghost_proto::MemSnapshot;
use ghostd::collect::{MemCollector, Source};

const TOP_N: usize = 5;
const TOTAL_KIB: u64 = 96_388_184;
const AVAILABLE_KIB: u64 = 77_000_000;
const SWAP_TOTAL_KIB: u64 = 8_388_604;
const SWAP_FREE_KIB: u64 = 8_000_000;
const SMALL_TOTAL_KIB: u64 = 16_000_000;
const BASELINE_TICKS: u64 = 10_000;
// One second on a 32-thread machine at 100 ticks per second.
const INTERVAL_TICKS: u64 = 3_200;

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

fn pids(snapshot: &MemSnapshot) -> Vec<u32> {
    snapshot.top.iter().map(|process| process.pid).collect()
}

#[test]
fn used_is_total_minus_available() {
    let fake = machine(TOTAL_KIB, 0);
    let mut mem = MemCollector::new(&fake.root(), TOP_N);

    let snapshot = mem.sample();

    assert_eq!(snapshot.total_kib, TOTAL_KIB);
    assert_eq!(snapshot.used_kib, 19_388_184);
}

#[test]
fn swap_used_is_total_minus_free() {
    let fake = machine(TOTAL_KIB, 0);
    let mut mem = MemCollector::new(&fake.root(), TOP_N);

    let snapshot = mem.sample();

    assert_eq!(snapshot.swap_total_kib, SWAP_TOTAL_KIB);
    assert_eq!(snapshot.swap_used_kib, 388_604);
}

#[test]
fn available_above_total_saturates_used_to_zero() {
    let fake = FakeRoot::new();
    fake.write("proc/meminfo", &meminfo(100, 200, 50, 60));
    let mut mem = MemCollector::new(&fake.root(), TOP_N);

    let snapshot = mem.sample();

    assert_eq!(snapshot.used_kib, 0);
    assert_eq!(snapshot.swap_used_kib, 0);
}

#[test]
fn missing_meminfo_keys_read_as_zero() {
    let fake = FakeRoot::new();
    fake.write("proc/meminfo", "MemFree:        1000 kB\n");
    advance_clock(&fake, BASELINE_TICKS);
    fake.write_process(&FakeProcess::new(1, "init"));
    let mut mem = MemCollector::new(&fake.root(), TOP_N);

    let snapshot = mem.sample();

    assert_eq!(snapshot.total_kib, 0);
    assert_eq!(snapshot.used_kib, 0);
    assert_eq!(snapshot.swap_total_kib, 0);
    assert_eq!(snapshot.swap_used_kib, 0);
    assert_eq!(snapshot.top[0].mem, 0.0);
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
    let mut mem = MemCollector::new(&fake.root(), TOP_N);

    advance_clock(&fake, BASELINE_TICKS + INTERVAL_TICKS);
    fake.write_process(&firefox(140, 74));
    let snapshot = mem.sample();

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
    let mut mem = MemCollector::new(&fake.root(), TOP_N);

    advance_clock(&fake, BASELINE_TICKS + INTERVAL_TICKS);
    fake.write_process(
        &FakeProcess::new(1, "hog")
            .rss_kib(32_000_000)
            .ticks(2 * INTERVAL_TICKS, 0),
    );
    let snapshot = mem.sample();

    assert_eq!(snapshot.top[0].cpu, 100.0);
    assert_eq!(snapshot.top[0].mem, 100.0);
}

#[test]
fn process_first_seen_this_tick_reports_zero_cpu() {
    let fake = machine(TOTAL_KIB, BASELINE_TICKS);
    let mut mem = MemCollector::new(&fake.root(), TOP_N);

    advance_clock(&fake, BASELINE_TICKS + INTERVAL_TICKS);
    fake.write_process(&FakeProcess::new(7, "latecomer").ticks(3_000, 0));
    let snapshot = mem.sample();

    assert_eq!(pids(&snapshot), vec![7]);
    assert_eq!(snapshot.top[0].cpu, 0.0);
}

#[test]
fn recycled_pid_does_not_inherit_the_previous_baseline() {
    let fake = machine(TOTAL_KIB, BASELINE_TICKS);
    fake.write_process(&FakeProcess::new(5, "old").start_time(1_000).ticks(100, 0));
    let mut mem = MemCollector::new(&fake.root(), TOP_N);

    advance_clock(&fake, BASELINE_TICKS + INTERVAL_TICKS);
    fake.write_process(&FakeProcess::new(5, "new").start_time(2_000).ticks(500, 0));
    let snapshot = mem.sample();

    assert_eq!(snapshot.top[0].name, "new");
    assert_eq!(snapshot.top[0].cpu, 0.0);
}

#[test]
fn no_elapsed_cpu_time_reports_zero_cpu() {
    let fake = machine(TOTAL_KIB, BASELINE_TICKS);
    fake.write_process(&FakeProcess::new(1, "init"));
    let mut mem = MemCollector::new(&fake.root(), TOP_N);

    fake.write_process(&FakeProcess::new(1, "init").ticks(500, 0));
    let snapshot = mem.sample();

    assert_eq!(snapshot.top[0].cpu, 0.0);
}

#[test]
fn unreadable_proc_stat_keeps_the_previous_baseline() {
    let fake = machine(TOTAL_KIB, BASELINE_TICKS);
    fake.write_process(&FakeProcess::new(1, "init"));
    let mut mem = MemCollector::new(&fake.root(), TOP_N);

    fake.remove("proc/stat");
    fake.write_process(&FakeProcess::new(1, "init").ticks(32, 0));
    assert_eq!(mem.sample().top[0].cpu, 0.0);

    advance_clock(&fake, BASELINE_TICKS + INTERVAL_TICKS);
    fake.write_process(&FakeProcess::new(1, "init").ticks(64, 0));
    let snapshot = mem.sample();

    // Both intervals of process time against both intervals of total time, not one against two.
    assert_eq!(snapshot.top[0].cpu, 2.0);
}

#[test]
fn comm_with_spaces_and_parentheses_is_parsed() {
    let fake = machine(TOTAL_KIB, BASELINE_TICKS);
    let comm = "Web Content (x) y)";
    fake.write_process(&FakeProcess::new(9, comm));
    let mut mem = MemCollector::new(&fake.root(), TOP_N);

    advance_clock(&fake, BASELINE_TICKS + INTERVAL_TICKS);
    fake.write_process(&FakeProcess::new(9, comm).ticks(60, 4));
    let snapshot = mem.sample();

    assert_eq!(snapshot.top[0].name, comm);
    assert_eq!(snapshot.top[0].cpu, 2.0);
}

#[test]
fn non_utf8_comm_is_kept_with_replacement_characters() {
    let fake = machine(TOTAL_KIB, BASELINE_TICKS);
    fake.write_process(&FakeProcess::new(3, b"bad\xFFname"));
    let mut mem = MemCollector::new(&fake.root(), TOP_N);

    let snapshot = mem.sample();

    assert_eq!(snapshot.top[0].name, "bad\u{FFFD}name");
}

#[test]
fn truncated_stat_line_skips_the_process() {
    let fake = machine(TOTAL_KIB, BASELINE_TICKS);
    fake.write_bytes("proc/8/stat", b"8 (short) S 1 2 3\n");
    fake.write_process(&FakeProcess::new(9, "whole"));
    let mut mem = MemCollector::new(&fake.root(), TOP_N);

    assert_eq!(pids(&mem.sample()), vec![9]);
}

#[test]
fn vanished_process_skipped() {
    let fake = machine(TOTAL_KIB, BASELINE_TICKS);
    for pid in 1..=3 {
        fake.write_process(&FakeProcess::new(pid, "worker"));
    }
    let mut mem = MemCollector::new(&fake.root(), TOP_N);

    advance_clock(&fake, BASELINE_TICKS + INTERVAL_TICKS);
    fake.remove("proc/2");
    fake.remove("proc/3/stat");
    assert_eq!(pids(&mem.sample()), vec![1]);

    // Back with ticks far above anything they had at the baseline: a baseline that outlived
    // the missing tick would turn that gap into a cpu share.
    advance_clock(&fake, BASELINE_TICKS + 2 * INTERVAL_TICKS);
    for pid in 2..=3 {
        fake.write_process(&FakeProcess::new(pid, "worker").ticks(640, 0));
    }
    let snapshot = mem.sample();

    assert_eq!(pids(&snapshot), vec![1, 2, 3]);
    assert!(snapshot.top.iter().all(|process| process.cpu == 0.0));
}

#[test]
fn kernel_threads_excluded() {
    let fake = machine(TOTAL_KIB, BASELINE_TICKS);
    fake.write_process(&FakeProcess::new(2, "kthreadd").rss_kib(0));
    fake.write_process(&FakeProcess::new(4, "bash"));
    let mut mem = MemCollector::new(&fake.root(), TOP_N);

    assert_eq!(pids(&mem.sample()), vec![4]);
}

#[test]
fn top_sorted_and_truncated() {
    let fake = machine(TOTAL_KIB, BASELINE_TICKS);
    // Pids 9 and 10 tie, and sort differently as numbers than as strings.
    let busyness = [(14, 5), (13, 10), (12, 20), (11, 30), (10, 50), (9, 50)];
    for (pid, _) in busyness {
        fake.write_process(&FakeProcess::new(pid, "worker"));
    }
    let mut mem = MemCollector::new(&fake.root(), TOP_N);

    advance_clock(&fake, BASELINE_TICKS + INTERVAL_TICKS);
    for (pid, ticks) in busyness {
        fake.write_process(&FakeProcess::new(pid, "worker").ticks(ticks, 0));
    }
    let snapshot = mem.sample();

    assert_eq!(pids(&snapshot), vec![9, 10, 11, 12, 13]);
}

#[test]
fn unreadable_root_yields_empty_snapshot() {
    let fake = FakeRoot::new();
    let mut mem = MemCollector::new(&fake.root(), TOP_N);

    assert_eq!(mem.sample(), MemSnapshot::default());
}
