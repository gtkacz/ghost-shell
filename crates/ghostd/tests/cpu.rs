mod common;

use common::{
    CPU_COUNT, CURRENT_FREQ_KHZ, MODEL_NAME, PLAIN_L3_KIB, Ticks, VCACHE_L3_KIB, fake_9950x3d,
    proc_stat,
};
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
