mod common;

use common::{FakeRoot, Ticks, meminfo, proc_stat};
use ghost_proto::MemSnapshot;
use ghostd::collect::{MemCollector, Source};

const TOP_N: usize = 5;
const TOTAL_KIB: u64 = 96_388_184;
const AVAILABLE_KIB: u64 = 77_000_000;
const SWAP_TOTAL_KIB: u64 = 8_388_604;
const SWAP_FREE_KIB: u64 = 8_000_000;
const SMALL_TOTAL_KIB: u64 = 16_000_000;

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
fn malformed_meminfo_saturates_instead_of_panicking() {
    let fake = FakeRoot::new();
    fake.write("proc/meminfo", &meminfo(100, 200, 50, 60));
    let mut mem = MemCollector::new(&fake.root(), TOP_N);

    let snapshot = mem.sample();

    assert_eq!(snapshot.used_kib, 0);
    assert_eq!(snapshot.swap_used_kib, 0);
}

#[test]
fn process_cpu_is_machine_share() {
    let fake = machine(SMALL_TOTAL_KIB, 10_000);
    fake.write_process(42, "firefox", 100, 50, Some(1_600_000));
    let mut mem = MemCollector::new(&fake.root(), TOP_N);

    advance_clock(&fake, 13_200);
    fake.write_process(42, "firefox", 140, 74, Some(1_600_000));
    let snapshot = mem.sample();

    let process = &snapshot.top[0];
    assert_eq!(process.pid, 42);
    assert_eq!(process.name, "firefox");
    assert_eq!(process.cpu, 2.0);
    assert_eq!(process.mem, 10.0);
}

#[test]
fn process_first_seen_this_tick_reports_zero_cpu() {
    let fake = machine(TOTAL_KIB, 10_000);
    let mut mem = MemCollector::new(&fake.root(), TOP_N);

    advance_clock(&fake, 13_200);
    fake.write_process(7, "latecomer", 3_000, 0, Some(1_000));
    let snapshot = mem.sample();

    assert_eq!(snapshot.top.len(), 1);
    assert_eq!(snapshot.top[0].cpu, 0.0);
}

#[test]
fn no_elapsed_cpu_time_reports_zero_cpu() {
    let fake = machine(TOTAL_KIB, 10_000);
    fake.write_process(1, "init", 0, 0, Some(1_000));
    let mut mem = MemCollector::new(&fake.root(), TOP_N);

    fake.write_process(1, "init", 500, 0, Some(1_000));
    let snapshot = mem.sample();

    assert_eq!(snapshot.top[0].cpu, 0.0);
}

#[test]
fn comm_with_spaces_and_parentheses_is_parsed() {
    let fake = machine(TOTAL_KIB, 10_000);
    let comm = "Web Content (x) y)";
    fake.write_process(9, comm, 0, 0, Some(1_000));
    let mut mem = MemCollector::new(&fake.root(), TOP_N);

    advance_clock(&fake, 13_200);
    fake.write_process(9, comm, 60, 4, Some(1_000));
    let snapshot = mem.sample();

    assert_eq!(snapshot.top[0].name, comm);
    assert_eq!(snapshot.top[0].cpu, 2.0);
}

#[test]
fn vanished_process_skipped() {
    let fake = machine(TOTAL_KIB, 10_000);
    for pid in 1..=3 {
        fake.write_process(pid, "worker", 0, 0, Some(1_000));
    }
    let mut mem = MemCollector::new(&fake.root(), TOP_N);

    advance_clock(&fake, 13_200);
    fake.remove("proc/2");
    fake.remove("proc/3/stat");
    let snapshot = mem.sample();

    let pids: Vec<u32> = snapshot.top.iter().map(|process| process.pid).collect();
    assert_eq!(pids, vec![1]);
}

#[test]
fn kernel_threads_excluded() {
    let fake = machine(TOTAL_KIB, 10_000);
    fake.write_process(2, "kthreadd", 0, 0, None);
    fake.write_process(3, "idle_inject", 0, 0, Some(0));
    fake.write_process(4, "bash", 0, 0, Some(4_000));
    let mut mem = MemCollector::new(&fake.root(), TOP_N);

    let snapshot = mem.sample();

    let pids: Vec<u32> = snapshot.top.iter().map(|process| process.pid).collect();
    assert_eq!(pids, vec![4]);
}

#[test]
fn top_sorted_and_truncated() {
    let fake = machine(TOTAL_KIB, 10_000);
    let busyness = [(10, 10), (11, 50), (12, 30), (13, 50), (14, 20), (15, 5)];
    for (pid, _) in busyness {
        fake.write_process(pid, "worker", 0, 0, Some(1_000));
    }
    let mut mem = MemCollector::new(&fake.root(), TOP_N);

    advance_clock(&fake, 13_200);
    for (pid, ticks) in busyness {
        fake.write_process(pid, "worker", ticks, 0, Some(1_000));
    }
    let snapshot = mem.sample();

    let pids: Vec<u32> = snapshot.top.iter().map(|process| process.pid).collect();
    assert_eq!(pids, vec![11, 13, 12, 14, 10]);
}

#[test]
fn unreadable_root_yields_empty_snapshot() {
    let fake = FakeRoot::new();
    let mut mem = MemCollector::new(&fake.root(), TOP_N);

    assert_eq!(mem.sample(), MemSnapshot::default());
}
