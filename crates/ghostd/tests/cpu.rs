mod common;

use common::{
    CPU_COUNT, CURRENT_FREQ_KHZ, MODEL_NAME, PLAIN_L3_KIB, SMT_SIBLING_OFFSET, Ticks,
    VCACHE_L3_KIB, fake_9950x3d, proc_stat,
};
use std::time::Duration;

use ghost_proto::CoreLoad;
use ghostd::collect::{CpuCollector, Source, parse_cpu_list};

const KHZ_PER_MHZ: u32 = 1000;

fn stat_with_busy_cpu0(cpu0: Ticks) -> String {
    let mut cpus = [Ticks::default(); CPU_COUNT];
    cpus[0] = cpu0;
    proc_stat(cpu0, &cpus)
}

#[test]
fn parses_ranges() {
    assert_eq!(parse_cpu_list("0-2,16-17"), vec![0, 1, 2, 16, 17]);
}

#[test]
fn groups_ccds_and_detects_vcache() {
    let fake = fake_9950x3d();
    let mut cpu = CpuCollector::new(&fake.root()).unwrap();

    let snapshot = cpu.sample();

    assert_eq!(snapshot.model, MODEL_NAME);
    assert_eq!(snapshot.threads.len(), CPU_COUNT);
    assert_eq!(snapshot.freq_mhz, CURRENT_FREQ_KHZ / KHZ_PER_MHZ);
    assert_eq!(snapshot.ccds.len(), 2);

    let vcache_ccd = &snapshot.ccds[0];
    assert_eq!(vcache_ccd.id, 0);
    assert_eq!(
        vcache_ccd.cpus,
        (0..=7).chain(16..=23).collect::<Vec<u16>>()
    );
    assert_eq!(vcache_ccd.l3_kib, VCACHE_L3_KIB);
    assert!(vcache_ccd.vcache);

    let plain_ccd = &snapshot.ccds[1];
    assert_eq!(plain_ccd.id, 1);
    assert_eq!(
        plain_ccd.cpus,
        (8..=15).chain(24..=31).collect::<Vec<u16>>()
    );
    assert_eq!(plain_ccd.l3_kib, PLAIN_L3_KIB);
    assert!(!plain_ccd.vcache);
}

#[test]
fn computes_thread_and_ccd_load() {
    let fake = fake_9950x3d();
    let before = Ticks {
        user: 1000,
        system: 200,
        idle: 5000,
        iowait: 100,
        guest: 300,
    };
    fake.write("proc/stat", &stat_with_busy_cpu0(before));
    let mut cpu = CpuCollector::new(&fake.root()).unwrap();

    // Guest ticks are already inside user, so growing them must not widen the total.
    let after = Ticks {
        user: before.user + 40,
        system: before.system + 10,
        idle: before.idle + 40,
        iowait: before.iowait + 10,
        guest: before.guest + 10,
    };
    fake.write("proc/stat", &stat_with_busy_cpu0(after));
    let snapshot = cpu.sample();

    assert_eq!(snapshot.threads[0], 50.0);
    assert!(snapshot.threads[1..].iter().all(|load| *load == 0.0));
    assert_eq!(snapshot.ccds[0].load, 50.0 / 16.0);
    assert_eq!(snapshot.ccds[1].load, 0.0);
    assert_eq!(snapshot.load, 50.0);
}

#[test]
fn cores_average_their_smt_siblings() {
    let fake = fake_9950x3d();
    let mut cpu = CpuCollector::new(&fake.root()).unwrap();
    let busy = |user: u64| Ticks {
        user,
        idle: 100 - user,
        ..Ticks::default()
    };
    let mut cpus = [Ticks::default(); CPU_COUNT];
    cpus[0] = busy(10);
    cpus[SMT_SIBLING_OFFSET] = busy(30);
    fake.write("proc/stat", &proc_stat(Ticks::default(), &cpus));

    let snapshot = cpu.sample();

    assert_eq!(
        snapshot.cores[0],
        CoreLoad {
            cpus: vec![0, 16],
            load: 20.0
        }
    );
    assert_eq!(snapshot.cores.len(), 16);
    assert_eq!(
        snapshot.cores[15],
        CoreLoad {
            cpus: vec![15, 31],
            load: 0.0
        }
    );
}

#[test]
fn an_offline_cpu_without_topology_belongs_to_no_core() {
    let fake = fake_9950x3d();
    fake.remove("sys/devices/system/cpu/cpu31/topology");
    fake.write(
        "sys/devices/system/cpu/cpu15/topology/thread_siblings_list",
        "15\n",
    );

    let snapshot = CpuCollector::new(&fake.root()).unwrap().sample();

    assert_eq!(snapshot.cores.len(), 16);
    assert_eq!(snapshot.cores[15].cpus, [15]);
}

#[test]
fn missing_cpufreq_is_zero() {
    let fake = fake_9950x3d();
    for cpu in 0..CPU_COUNT {
        fake.remove(&format!("sys/devices/system/cpu/cpu{cpu}/cpufreq"));
    }
    let mut cpu = CpuCollector::new(&fake.root()).unwrap();

    let snapshot = cpu.sample();

    assert_eq!(snapshot.freq_mhz, 0);
    assert_eq!(snapshot.load, 0.0);
    assert!(snapshot.threads.iter().all(|load| *load == 0.0));
}

fn write_for_every_cpu(fake: &common::FakeRoot, file: &str, content: &str) {
    for cpu in 0..CPU_COUNT {
        fake.write(&format!("sys/devices/system/cpu/cpu{cpu}/{file}"), content);
    }
}

#[test]
fn out_of_range_shared_cpu_list_degrades_instead_of_panicking() {
    let fake = fake_9950x3d();
    write_for_every_cpu(&fake, "cache/index3/shared_cpu_list", "0-40\n");
    let mut cpu = CpuCollector::new(&fake.root()).unwrap();
    fake.write(
        "proc/stat",
        &stat_with_busy_cpu0(Ticks {
            user: 50,
            idle: 50,
            ..Ticks::default()
        }),
    );

    let snapshot = cpu.sample();

    assert_eq!(snapshot.ccds.len(), 1);
    assert_eq!(snapshot.ccds[0].cpus, (0..=31).collect::<Vec<u16>>());
    assert!(snapshot.ccds[0].load.is_finite());
    assert!(snapshot.threads.iter().all(|load| load.is_finite()));
}

#[test]
fn absurd_counters_and_frequencies_do_not_overflow() {
    let fake = fake_9950x3d();
    write_for_every_cpu(&fake, "cpufreq/scaling_cur_freq", "18446744073709551615\n");
    let maxed = Ticks {
        user: u64::MAX,
        system: 1,
        ..Ticks::default()
    };
    fake.write("proc/stat", &proc_stat(maxed, &[maxed; CPU_COUNT]));

    let mut cpu = CpuCollector::new(&fake.root()).unwrap();
    let snapshot = cpu.sample();

    assert_eq!(snapshot.freq_mhz, 0);
    assert!(snapshot.load.is_finite());
}

#[test]
fn missing_l3_size_skips_that_cache_entry() {
    let fake = fake_9950x3d();
    for cpu in (8..=15).chain(24..=31) {
        fake.remove(&format!(
            "sys/devices/system/cpu/cpu{cpu}/cache/index3/size"
        ));
    }

    let snapshot = CpuCollector::new(&fake.root()).unwrap().sample();

    assert_eq!(snapshot.ccds.len(), 1);
    assert_eq!(snapshot.ccds[0].l3_kib, VCACHE_L3_KIB);
}

fn set_l3(fake: &common::FakeRoot, cpus: std::ops::RangeInclusive<usize>, group: &str, l3: &str) {
    for cpu in cpus {
        let cache = format!("sys/devices/system/cpu/cpu{cpu}/cache/index3");
        fake.write(&format!("{cache}/shared_cpu_list"), &format!("{group}\n"));
        fake.write(&format!("{cache}/size"), &format!("{l3}\n"));
    }
}

#[test]
fn vcache_is_not_decided_while_a_ccd_has_no_known_l3_size() {
    let fake = fake_9950x3d();
    set_l3(&fake, 0..=7, "0-7", "32768K");
    set_l3(&fake, 8..=15, "8-15", "98304K");
    set_l3(&fake, 16..=31, "16-31", "32768K");
    // The unknown CCD could be the smallest one, so the others cannot be ranked against it.
    for cpu in 16..=31 {
        fake.remove(&format!(
            "sys/devices/system/cpu/cpu{cpu}/cache/index3/size"
        ));
    }

    let snapshot = CpuCollector::new(&fake.root()).unwrap().sample();

    assert_eq!(snapshot.ccds.len(), 2);
    assert!(snapshot.ccds.iter().all(|ccd| !ccd.vcache));
}

#[test]
fn vcache_is_decided_once_every_ccd_size_is_known() {
    let fake = fake_9950x3d();
    set_l3(&fake, 0..=7, "0-7", "32768K");
    set_l3(&fake, 8..=15, "8-15", "98304K");
    set_l3(&fake, 16..=31, "16-31", "32768K");

    let snapshot = CpuCollector::new(&fake.root()).unwrap().sample();

    let flags: Vec<bool> = snapshot.ccds.iter().map(|ccd| ccd.vcache).collect();
    assert_eq!(flags, [false, true, false]);
}

#[test]
fn unreadable_cpuinfo_gives_an_empty_model() {
    let fake = fake_9950x3d();
    fake.remove("proc/cpuinfo");

    let snapshot = CpuCollector::new(&fake.root()).unwrap().sample();

    assert_eq!(snapshot.model, "");
    assert_eq!(snapshot.threads.len(), CPU_COUNT);
}

const PACKAGE_ZONE: &str = "sys/class/powercap/intel-rapl:0";
const ONE_SECOND: Duration = Duration::from_secs(1);
// This machine's counter range, read from its `max_energy_range_uj`.
const WRAP_RANGE_UJ: u64 = 65_532_610_987;
const WATTS_FOR_45_MILLION_UJ: f32 = 45.0;
const AMD_VENDOR: &str = "0x1002";
const NVIDIA_VENDOR: &str = "0x10de";

fn write_package_zone(fake: &common::FakeRoot, energy_uj: u64) {
    fake.write(&format!("{PACKAGE_ZONE}/name"), "package-0\n");
    fake.write(
        &format!("{PACKAGE_ZONE}/max_energy_range_uj"),
        &format!("{WRAP_RANGE_UJ}\n"),
    );
    set_energy(fake, energy_uj);
}

fn set_energy(fake: &common::FakeRoot, energy_uj: u64) {
    fake.write(
        &format!("{PACKAGE_ZONE}/energy_uj"),
        &format!("{energy_uj}\n"),
    );
}

/// Takes the baseline reading, then samples after one second once the counter reads `after`.
fn package_w_between(fake: &common::FakeRoot, after: u64) -> Option<f32> {
    let mut cpu = CpuCollector::new(&fake.root()).unwrap();
    cpu.sample_after(ONE_SECOND);
    set_energy(fake, after);
    cpu.sample_after(ONE_SECOND).package_w
}

#[test]
fn package_power_from_the_energy_delta() {
    let fake = fake_9950x3d();
    write_package_zone(&fake, 1_000_000);

    let package_w = package_w_between(&fake, 46_000_000);

    assert_eq!(package_w, Some(WATTS_FOR_45_MILLION_UJ));
}

#[test]
fn package_power_follows_the_interval_not_the_tick() {
    let fake = fake_9950x3d();
    write_package_zone(&fake, 0);
    let mut cpu = CpuCollector::new(&fake.root()).unwrap();
    cpu.sample_after(ONE_SECOND);

    set_energy(&fake, 90_000_000);
    let snapshot = cpu.sample_after(2 * ONE_SECOND);

    assert_eq!(snapshot.package_w, Some(WATTS_FOR_45_MILLION_UJ));
}

#[test]
fn a_wrapped_counter_still_reads_positive() {
    let fake = fake_9950x3d();
    write_package_zone(&fake, WRAP_RANGE_UJ - 1_000_000);

    let package_w = package_w_between(&fake, 44_000_000);

    assert_eq!(package_w, Some(WATTS_FOR_45_MILLION_UJ));
}

#[test]
fn a_counter_reset_is_not_read_as_a_wrap() {
    let fake = fake_9950x3d();
    write_package_zone(&fake, 10_000_000);

    let package_w = package_w_between(&fake, 0);

    assert_eq!(package_w, None);
}

#[test]
fn package_power_reads_normally_again_after_a_reset() {
    let fake = fake_9950x3d();
    write_package_zone(&fake, 10_000_000);
    let mut cpu = CpuCollector::new(&fake.root()).unwrap();
    cpu.sample_after(ONE_SECOND);
    set_energy(&fake, 0);
    let reset = cpu.sample_after(ONE_SECOND);

    set_energy(&fake, 45_000_000);
    let recovered = cpu.sample_after(ONE_SECOND);

    assert_eq!(reset.package_w, None);
    assert_eq!(recovered.package_w, Some(WATTS_FOR_45_MILLION_UJ));
}

#[test]
fn a_counter_beyond_its_own_range_hides_package_power() {
    let fake = fake_9950x3d();
    write_package_zone(&fake, WRAP_RANGE_UJ + 1);

    let package_w = package_w_between(&fake, 44_000_000);

    assert_eq!(package_w, None);
}

#[test]
fn a_decreasing_counter_without_a_range_hides_package_power() {
    let fake = fake_9950x3d();
    write_package_zone(&fake, 50_000_000);
    fake.remove(&format!("{PACKAGE_ZONE}/max_energy_range_uj"));

    let package_w = package_w_between(&fake, 1_000_000);

    assert_eq!(package_w, None);
}

#[test]
fn a_zone_without_a_name_is_skipped() {
    let fake = fake_9950x3d();
    // The control-type entry sorts first and has an energy file of its own, but no `name`.
    fake.write("sys/class/powercap/intel-rapl/energy_uj", "999999999\n");
    write_package_zone(&fake, 1_000_000);

    let package_w = package_w_between(&fake, 46_000_000);

    assert_eq!(package_w, Some(WATTS_FOR_45_MILLION_UJ));
}

#[test]
fn only_the_package_zone_is_read() {
    let fake = fake_9950x3d();
    write_package_zone(&fake, 1_000_000);
    let core_zone = "sys/class/powercap/intel-rapl:0:0";
    fake.write(&format!("{core_zone}/name"), "core\n");
    fake.write(&format!("{core_zone}/energy_uj"), "0\n");

    let package_w = package_w_between(&fake, 46_000_000);

    assert_eq!(package_w, Some(WATTS_FOR_45_MILLION_UJ));
}

#[test]
fn an_unreadable_counter_hides_package_power() {
    let fake = fake_9950x3d();
    write_package_zone(&fake, 1_000_000);
    fake.remove(&format!("{PACKAGE_ZONE}/energy_uj"));
    let mut cpu = CpuCollector::new(&fake.root()).unwrap();

    cpu.sample_after(ONE_SECOND);
    let snapshot = cpu.sample_after(ONE_SECOND);

    assert_eq!(snapshot.package_w, None);
}

#[test]
fn package_power_returns_once_the_counter_becomes_readable() {
    let fake = fake_9950x3d();
    write_package_zone(&fake, 1_000_000);
    fake.remove(&format!("{PACKAGE_ZONE}/energy_uj"));
    let mut cpu = CpuCollector::new(&fake.root()).unwrap();
    cpu.sample_after(ONE_SECOND);

    set_energy(&fake, 10_000_000);
    let baseline = cpu.sample_after(ONE_SECOND);
    set_energy(&fake, 55_000_000);
    let measured = cpu.sample_after(ONE_SECOND);

    assert_eq!(baseline.package_w, None);
    assert_eq!(measured.package_w, Some(WATTS_FOR_45_MILLION_UJ));
}

#[test]
fn a_machine_without_powercap_has_no_package_power() {
    let fake = fake_9950x3d();
    let mut cpu = CpuCollector::new(&fake.root()).unwrap();

    cpu.sample_after(ONE_SECOND);
    let snapshot = cpu.sample_after(ONE_SECOND);

    assert_eq!(snapshot.package_w, None);
}

#[test]
fn the_first_sample_has_no_package_power() {
    let fake = fake_9950x3d();
    write_package_zone(&fake, 1_000_000);
    let mut cpu = CpuCollector::new(&fake.root()).unwrap();

    let first = cpu.sample_after(ONE_SECOND);

    assert_eq!(first.package_w, None);
}

#[test]
fn a_zero_interval_hides_package_power() {
    let fake = fake_9950x3d();
    write_package_zone(&fake, 1_000_000);
    let mut cpu = CpuCollector::new(&fake.root()).unwrap();
    cpu.sample_after(ONE_SECOND);

    set_energy(&fake, 46_000_000);
    let snapshot = cpu.sample_after(Duration::ZERO);

    assert_eq!(snapshot.package_w, None);
}

fn write_drm_card(fake: &common::FakeRoot, card: &str, vendor: &str, busy_percent: Option<&str>) {
    let device = format!("sys/class/drm/{card}/device");
    fake.write(&format!("{device}/vendor"), &format!("{vendor}\n"));
    if let Some(busy) = busy_percent {
        fake.write(&format!("{device}/gpu_busy_percent"), &format!("{busy}\n"));
    }
}

#[test]
fn no_amd_drm_device_means_no_igpu() {
    let fake = fake_9950x3d();
    write_drm_card(&fake, "card1", NVIDIA_VENDOR, None);

    let snapshot = CpuCollector::new(&fake.root()).unwrap().sample();

    assert_eq!(snapshot.igpu_busy_pct, None);
}

#[test]
fn a_machine_without_drm_has_no_igpu() {
    let fake = fake_9950x3d();

    let snapshot = CpuCollector::new(&fake.root()).unwrap().sample();

    assert_eq!(snapshot.igpu_busy_pct, None);
}

#[test]
fn an_amd_igpu_reports_its_busy_percent() {
    let fake = fake_9950x3d();
    write_drm_card(&fake, "card0", AMD_VENDOR, Some("7"));

    let snapshot = CpuCollector::new(&fake.root()).unwrap().sample();

    assert_eq!(snapshot.igpu_busy_pct, Some(7.0));
}

#[test]
fn an_amd_card_without_a_busy_counter_is_passed_over() {
    let fake = fake_9950x3d();
    write_drm_card(&fake, "card0", AMD_VENDOR, None);
    write_drm_card(&fake, "card1", AMD_VENDOR, Some("12"));
    // Connector entries carry no `device` of their own.
    fake.write("sys/class/drm/card1-DP-1/status", "disconnected\n");

    let snapshot = CpuCollector::new(&fake.root()).unwrap().sample();

    assert_eq!(snapshot.igpu_busy_pct, Some(12.0));
}

#[test]
fn a_non_amd_busy_counter_is_not_the_igpu() {
    let fake = fake_9950x3d();
    write_drm_card(&fake, "card0", NVIDIA_VENDOR, Some("80"));

    let snapshot = CpuCollector::new(&fake.root()).unwrap().sample();

    assert_eq!(snapshot.igpu_busy_pct, None);
}
