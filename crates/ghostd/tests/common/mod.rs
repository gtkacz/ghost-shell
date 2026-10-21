// Every test binary compiles this module but uses only part of it.
#![allow(dead_code)]

use std::fs;

use ghostd::sysroot::SysRoot;
use tempfile::TempDir;

pub const CPU_COUNT: usize = 32;
pub const MODEL_NAME: &str = "AMD Ryzen 9 9950X3D 16-Core Processor";
pub const VCACHE_L3_KIB: u32 = 98304;
pub const PLAIN_L3_KIB: u32 = 32768;
pub const CURRENT_FREQ_KHZ: u32 = 4_500_000;

const CCD_WIDTH: usize = 8;
const VCACHE_CPU_LIST: &str = "0-7,16-23";
const PLAIN_CPU_LIST: &str = "8-15,24-31";

pub struct FakeRoot {
    dir: TempDir,
}

impl FakeRoot {
    pub fn new() -> Self {
        Self {
            dir: TempDir::new().unwrap(),
        }
    }

    pub fn write(&self, rel: &str, content: &str) {
        let path = self.dir.path().join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    pub fn remove(&self, rel: &str) {
        let path = self.dir.path().join(rel);
        if path.is_dir() {
            fs::remove_dir_all(path).unwrap();
        } else {
            fs::remove_file(path).unwrap();
        }
    }

    pub fn root(&self) -> SysRoot {
        SysRoot::new(self.dir.path())
    }
}

/// The `/proc/stat` columns the collectors care about; the rest are written as zero.
#[derive(Debug, Clone, Copy, Default)]
pub struct Ticks {
    pub user: u64,
    pub system: u64,
    pub idle: u64,
    pub iowait: u64,
    pub guest: u64,
}

impl Ticks {
    fn line(&self, label: &str) -> String {
        format!(
            "{label} {} 0 {} {} {} 0 0 0 {} 0\n",
            self.user, self.system, self.idle, self.iowait, self.guest
        )
    }
}

pub fn proc_stat(aggregate: Ticks, cpus: &[Ticks]) -> String {
    let per_cpu: String = cpus
        .iter()
        .enumerate()
        .map(|(index, ticks)| ticks.line(&format!("cpu{index}")))
        .collect();
    format!(
        "{}{per_cpu}intr 0\nctxt 0\nbtime 0\n",
        aggregate.line("cpu")
    )
}

pub fn fake_9950x3d() -> FakeRoot {
    let fake = FakeRoot::new();
    for cpu in 0..CPU_COUNT {
        let on_vcache_ccd = cpu % (CCD_WIDTH * 2) < CCD_WIDTH;
        let (shared_cpus, l3_kib) = if on_vcache_ccd {
            (VCACHE_CPU_LIST, VCACHE_L3_KIB)
        } else {
            (PLAIN_CPU_LIST, PLAIN_L3_KIB)
        };
        let cpu_dir = format!("sys/devices/system/cpu/cpu{cpu}");
        fake.write(
            &format!("{cpu_dir}/cache/index3/shared_cpu_list"),
            &format!("{shared_cpus}\n"),
        );
        fake.write(
            &format!("{cpu_dir}/cache/index3/size"),
            &format!("{l3_kib}K\n"),
        );
        fake.write(
            &format!("{cpu_dir}/cpufreq/scaling_cur_freq"),
            &format!("{CURRENT_FREQ_KHZ}\n"),
        );
    }
    fake.write(
        "proc/stat",
        &proc_stat(Ticks::default(), &[Ticks::default(); CPU_COUNT]),
    );
    let cpuinfo: String = (0..CPU_COUNT)
        .map(|cpu| {
            format!("processor\t: {cpu}\nvendor_id\t: AuthenticAMD\nmodel name\t: {MODEL_NAME}\n\n")
        })
        .collect();
    fake.write("proc/cpuinfo", &cpuinfo);
    fake
}
