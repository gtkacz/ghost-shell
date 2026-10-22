mod common;

use common::{FakeRoot, meminfo};
use ghost_proto::MemSnapshot;
use ghostd::collect::{MemCollector, Source};

const TOTAL_KIB: u64 = 96_388_184;
const AVAILABLE_KIB: u64 = 77_000_000;
const SWAP_TOTAL_KIB: u64 = 8_388_604;
const SWAP_FREE_KIB: u64 = 8_000_000;

fn machine(total_kib: u64) -> FakeRoot {
    let fake = FakeRoot::new();
    fake.write(
        "proc/meminfo",
        &meminfo(total_kib, AVAILABLE_KIB, SWAP_TOTAL_KIB, SWAP_FREE_KIB),
    );
    fake
}

#[test]
fn used_is_total_minus_available() {
    let fake = machine(TOTAL_KIB);
    let mut mem = MemCollector::new(&fake.root());

    let snapshot = mem.sample();

    assert_eq!(snapshot.total_kib, TOTAL_KIB);
    assert_eq!(snapshot.used_kib, 19_388_184);
}

#[test]
fn swap_used_is_total_minus_free() {
    let fake = machine(TOTAL_KIB);
    let mut mem = MemCollector::new(&fake.root());

    let snapshot = mem.sample();

    assert_eq!(snapshot.swap_total_kib, SWAP_TOTAL_KIB);
    assert_eq!(snapshot.swap_used_kib, 388_604);
}

#[test]
fn available_above_total_saturates_used_to_zero() {
    let fake = FakeRoot::new();
    fake.write("proc/meminfo", &meminfo(100, 200, 50, 60));
    let mut mem = MemCollector::new(&fake.root());

    let snapshot = mem.sample();

    assert_eq!(snapshot.used_kib, 0);
    assert_eq!(snapshot.swap_used_kib, 0);
}

#[test]
fn missing_meminfo_keys_read_as_zero() {
    let fake = FakeRoot::new();
    fake.write("proc/meminfo", "MemFree:        1000 kB\n");
    let mut mem = MemCollector::new(&fake.root());

    let snapshot = mem.sample();

    assert_eq!(snapshot.total_kib, 0);
    assert_eq!(snapshot.used_kib, 0);
    assert_eq!(snapshot.swap_total_kib, 0);
    assert_eq!(snapshot.swap_used_kib, 0);
}

#[test]
fn unreadable_root_yields_empty_snapshot() {
    let fake = FakeRoot::new();
    let mut mem = MemCollector::new(&fake.root());

    assert_eq!(mem.sample(), MemSnapshot::default());
}

#[test]
fn memory_totals_are_read_afresh_on_every_sample() {
    let fake = machine(TOTAL_KIB);
    let mut mem = MemCollector::new(&fake.root());
    let first = mem.sample();

    let reclaimed = meminfo(
        TOTAL_KIB,
        AVAILABLE_KIB - 1_000,
        SWAP_TOTAL_KIB,
        SWAP_FREE_KIB,
    );
    fake.write("proc/meminfo", &reclaimed);

    assert_eq!(mem.sample().used_kib, first.used_kib + 1_000);
}
