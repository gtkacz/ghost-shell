use std::collections::HashMap;

use ghost_proto::{MemSnapshot, ProcUsage};

use super::Source;
use super::cpu::parse_proc_stat;
use crate::sysroot::SysRoot;

const PROC_DIR: &str = "proc";
const PROC_STAT: &str = "proc/stat";
const PROC_MEMINFO: &str = "proc/meminfo";

const MEM_TOTAL_KEY: &str = "MemTotal";
const MEM_AVAILABLE_KEY: &str = "MemAvailable";
const SWAP_TOTAL_KEY: &str = "SwapTotal";
const SWAP_FREE_KEY: &str = "SwapFree";
const VM_RSS_KEY: &str = "VmRSS";

// 1-based positions in /proc/<pid>/stat, counting the pid as field 1.
const UTIME_FIELD: usize = 14;
const STIME_FIELD: usize = 15;
// comm is field 2 and may itself contain spaces, so parsing resumes after its closing parenthesis.
const FIRST_FIELD_AFTER_COMM: usize = 3;
const FULL_SHARE_PCT: f64 = 100.0;

struct ProcReading {
    pid: u32,
    name: String,
    rss_kib: u64,
    cpu_ticks: u64,
}

pub struct MemCollector {
    root: SysRoot,
    top_n: usize,
    previous_all_cpu_ticks: u64,
    previous_proc_ticks: HashMap<u32, u64>,
}

impl MemCollector {
    /// Reads a baseline of CPU ticks, so the first `sample` already has deltas for live processes.
    pub fn new(root: &SysRoot, top_n: usize) -> Self {
        Self {
            previous_all_cpu_ticks: read_all_cpu_ticks(root).unwrap_or_default(),
            previous_proc_ticks: ticks_by_pid(&read_processes(root)),
            root: root.clone(),
            top_n,
        }
    }
}

impl Source<MemSnapshot> for MemCollector {
    fn sample(&mut self) -> MemSnapshot {
        let meminfo = self.root.read(PROC_MEMINFO).unwrap_or_default();
        let kib = |key| kib_field(&meminfo, key).unwrap_or_default();
        let total_kib = kib(MEM_TOTAL_KEY);
        let swap_total_kib = kib(SWAP_TOTAL_KEY);

        // An unreadable /proc/stat reads as no elapsed time, since sampling has no way to fail.
        let all_cpu_ticks = read_all_cpu_ticks(&self.root).unwrap_or(self.previous_all_cpu_ticks);
        let elapsed_ticks = all_cpu_ticks.saturating_sub(self.previous_all_cpu_ticks);

        let processes = read_processes(&self.root);
        let current_proc_ticks = ticks_by_pid(&processes);
        let mut top: Vec<ProcUsage> = processes
            .into_iter()
            .map(|process| {
                let cpu = self
                    .previous_proc_ticks
                    .get(&process.pid)
                    .map_or(0.0, |before| {
                        let used_ticks = process.cpu_ticks.saturating_sub(*before);
                        percent_of(used_ticks, elapsed_ticks)
                    });
                ProcUsage {
                    pid: process.pid,
                    name: process.name,
                    cpu,
                    mem: percent_of(process.rss_kib, total_kib),
                }
            })
            .collect();
        top.sort_by(|a, b| b.cpu.total_cmp(&a.cpu).then_with(|| a.pid.cmp(&b.pid)));
        top.truncate(self.top_n);

        self.previous_all_cpu_ticks = all_cpu_ticks;
        self.previous_proc_ticks = current_proc_ticks;

        MemSnapshot {
            total_kib,
            used_kib: total_kib.saturating_sub(kib(MEM_AVAILABLE_KEY)),
            swap_total_kib,
            swap_used_kib: swap_total_kib.saturating_sub(kib(SWAP_FREE_KEY)),
            top,
        }
    }
}

/// `part` as a percentage of `whole`, or 0 when `whole` is 0.
fn percent_of(part: u64, whole: u64) -> f32 {
    if whole == 0 {
        return 0.0;
    }
    // The inputs are read at slightly different instants, so a part can briefly outgrow its whole.
    (FULL_SHARE_PCT * part as f64 / whole as f64).min(FULL_SHARE_PCT) as f32
}

/// The value of a `Key:   1234 kB` line in `/proc/meminfo` or `/proc/<pid>/status`.
fn kib_field(text: &str, key: &str) -> Option<u64> {
    text.lines()
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| *name == key)
        .and_then(|(_, value)| value.split_whitespace().next()?.parse().ok())
}

fn read_all_cpu_ticks(root: &SysRoot) -> Option<u64> {
    let stat = parse_proc_stat(&root.read(PROC_STAT).ok()?).ok()?;
    Some(stat.aggregate.total)
}

fn ticks_by_pid(processes: &[ProcReading]) -> HashMap<u32, u64> {
    processes
        .iter()
        .map(|process| (process.pid, process.cpu_ticks))
        .collect()
}

fn read_processes(root: &SysRoot) -> Vec<ProcReading> {
    let Ok(entries) = root.list(PROC_DIR) else {
        return Vec::new();
    };
    entries
        .iter()
        .filter_map(|entry| read_process(root, entry.parse().ok()?))
        .collect()
}

/// `None` for kernel threads and for processes that exit while their files are being read.
fn read_process(root: &SysRoot, pid: u32) -> Option<ProcReading> {
    let dir = format!("{PROC_DIR}/{pid}");
    // Kernel threads have no address space, so the kernel reports no VmRSS or a zero one.
    let rss_kib = kib_field(&root.read(&format!("{dir}/status")).ok()?, VM_RSS_KEY)
        .filter(|rss_kib| *rss_kib > 0)?;
    let cpu_ticks = parse_cpu_ticks(&root.read(&format!("{dir}/stat")).ok()?)?;
    let comm = root.read(&format!("{dir}/comm")).ok()?;
    Some(ProcReading {
        pid,
        name: comm.trim_end_matches('\n').to_owned(),
        rss_kib,
        cpu_ticks,
    })
}

fn parse_cpu_ticks(stat: &str) -> Option<u64> {
    // The last ')' ends comm, which may contain parentheses of its own.
    let after_comm = &stat[stat.rfind(')')? + 1..];
    let fields: Vec<&str> = after_comm.split_whitespace().collect();
    let field = |number: usize| -> Option<u64> {
        fields.get(number - FIRST_FIELD_AFTER_COMM)?.parse().ok()
    };
    Some(field(UTIME_FIELD)?.saturating_add(field(STIME_FIELD)?))
}
