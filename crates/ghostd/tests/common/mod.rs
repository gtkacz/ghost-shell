// Every test binary compiles this module but uses only part of it.
#![allow(dead_code)]

pub mod actions;
pub mod bus;
pub mod calls;
pub mod disk;
pub mod kdeconnect;
pub mod nm;
pub mod server;
pub mod session;
pub mod tailscale;
pub mod udisks;
pub mod upower;

use std::fs;
use std::os::unix::fs::symlink;

use ghostd::sysroot::SysRoot;
use nix::unistd::{SysconfVar, sysconf};
use tempfile::TempDir;

pub const CPU_COUNT: usize = 32;
pub const MODEL_NAME: &str = "AMD Ryzen 9 9950X3D 16-Core Processor";
pub const VCACHE_L3_KIB: u32 = 98304;
pub const PLAIN_L3_KIB: u32 = 32768;
pub const CURRENT_FREQ_KHZ: u32 = 4_500_000;

const CCD_WIDTH: usize = 8;
// Logical cpu n and n + 16 are the two threads of one core.
pub const SMT_SIBLING_OFFSET: usize = CPU_COUNT / 2;
const VCACHE_CPU_LIST: &str = "0-7,16-23";
const PLAIN_CPU_LIST: &str = "8-15,24-31";
// Fields 5 to 8 of /proc/<pid>/stat sit between the ppid and flags.
const PROC_STAT_FIELDS_BEFORE_FLAGS: usize = 4;
// Fields 10 to 13 sit between flags and utime.
const PROC_STAT_FIELDS_BEFORE_UTIME: usize = 4;
// Fields 16 to 21 sit between stime and starttime.
const PROC_STAT_FIELDS_BEFORE_START_TIME: usize = 6;
const DEFAULT_START_TIME: u64 = 1000;
pub const DEFAULT_RSS_KIB: u64 = 4096;
pub const PF_KTHREAD: u64 = 0x0020_0000;
const BYTES_PER_KIB: u64 = 1024;

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

    /// Points `rel` at the directory `target_rel`, as sysfs does with `device` links.
    pub fn link_dir(&self, rel: &str, target_rel: &str) {
        let path = self.dir.path().join(rel);
        let target = self.dir.path().join(target_rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::create_dir_all(&target).unwrap();
        symlink(target, path).unwrap();
    }

    pub fn write_bytes(&self, rel: &str, content: &[u8]) {
        let path = self.dir.path().join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    pub fn write_process(&self, process: &FakeProcess) {
        let page_size_kib = page_size_kib();
        assert_eq!(
            process.rss_kib % page_size_kib,
            0,
            "rss_kib must be a whole number of {page_size_kib} KiB pages"
        );
        let rss_pages = process.rss_kib / page_size_kib;
        let before_flags = ["0"; PROC_STAT_FIELDS_BEFORE_FLAGS].join(" ");
        let before_utime = ["0"; PROC_STAT_FIELDS_BEFORE_UTIME].join(" ");
        let before_start_time = ["0"; PROC_STAT_FIELDS_BEFORE_START_TIME].join(" ");
        let mut stat = format!("{} (", process.pid).into_bytes();
        stat.extend_from_slice(&process.comm);
        stat.extend_from_slice(
            format!(
                ") S {} {before_flags} {} {before_utime} {} {} {before_start_time} {} 0 {rss_pages} 0\n",
                process.ppid, process.flags, process.utime, process.stime, process.start_time
            )
            .as_bytes(),
        );
        self.write_bytes(&format!("proc/{}/stat", process.pid), &stat);
    }
}

/// A `/proc/<pid>/stat` entry.
pub struct FakeProcess {
    pid: u32,
    ppid: u32,
    comm: Vec<u8>,
    flags: u64,
    utime: u64,
    stime: u64,
    start_time: u64,
    rss_kib: u64,
}

impl FakeProcess {
    pub fn new(pid: u32, comm: impl AsRef<[u8]>) -> Self {
        Self {
            pid,
            ppid: 0,
            comm: comm.as_ref().to_vec(),
            flags: 0,
            utime: 0,
            stime: 0,
            start_time: DEFAULT_START_TIME,
            rss_kib: DEFAULT_RSS_KIB,
        }
    }

    /// Flagged `PF_KTHREAD` with no address space, as the kernel reports its own threads.
    pub fn kernel_thread(pid: u32, comm: impl AsRef<[u8]>) -> Self {
        Self {
            flags: PF_KTHREAD,
            rss_kib: 0,
            ..Self::new(pid, comm)
        }
    }

    pub fn ticks(self, utime: u64, stime: u64) -> Self {
        Self {
            utime,
            stime,
            ..self
        }
    }

    pub fn start_time(self, start_time: u64) -> Self {
        Self { start_time, ..self }
    }

    pub fn rss_kib(self, rss_kib: u64) -> Self {
        Self { rss_kib, ..self }
    }

    pub fn ppid(self, ppid: u32) -> Self {
        Self { ppid, ..self }
    }
}

/// Fake RSS values must be multiples of this so they are whole pages on 4 KiB, 16 KiB and 64 KiB hosts.
pub fn page_size_kib() -> u64 {
    let bytes = sysconf(SysconfVar::PAGE_SIZE).unwrap().unwrap();
    u64::try_from(bytes).unwrap() / BYTES_PER_KIB
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

pub fn meminfo(
    total_kib: u64,
    available_kib: u64,
    swap_total_kib: u64,
    swap_free_kib: u64,
) -> String {
    format!(
        "MemTotal:       {total_kib} kB\nMemFree:        1000 kB\nMemAvailable:   {available_kib} kB\n\
         SwapCached:     2000 kB\nSwapTotal:      {swap_total_kib} kB\nSwapFree:       {swap_free_kib} kB\n"
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
        let first_sibling = cpu % SMT_SIBLING_OFFSET;
        fake.write(
            &format!("{cpu_dir}/topology/thread_siblings_list"),
            &format!("{first_sibling},{}\n", first_sibling + SMT_SIBLING_OFFSET),
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
