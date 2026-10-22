use std::collections::{HashMap, HashSet};
use std::ops::Range;

use ghost_proto::{ProcNode, ProcTree, ProcUsage};
use nix::unistd::{SysconfVar, sysconf};
use serde::Serialize;

use super::Source;
use super::cpu::parse_proc_stat;
use super::mem::{MEM_TOTAL_KEY, PROC_MEMINFO, kib_field};
use crate::sysroot::SysRoot;

const PROC_DIR: &str = "proc";
const PROC_STAT: &str = "proc/stat";
const PID_STAT_FILE: &str = "stat";

// 1-based positions in /proc/<pid>/stat, counting the pid as field 1.
const PPID_FIELD: usize = 4;
const FLAGS_FIELD: usize = 9;
const UTIME_FIELD: usize = 14;
const STIME_FIELD: usize = 15;
const START_TIME_FIELD: usize = 22;
const RSS_FIELD: usize = 24;
// comm is field 2 and may itself contain spaces, so parsing resumes after its closing parenthesis.
const FIRST_FIELD_AFTER_COMM: usize = 3;

// Set in a task's flags when it is a kernel thread (include/linux/sched.h).
const PF_KTHREAD: u64 = 0x0020_0000;

// Walking every /proc/<pid> is most of a sample's cost, so the process list is refreshed every
// second sample and repeated in between.
const PROCESS_SCAN_EVERY: u32 = 2;
// The tree is the whole table, which nobody reads at 1 Hz, so it is built on every fifth scan: every
// 10 s (spec: 5–20 s; user-confirmed 2026-10-21).
const TREE_EVERY_SCANS: u32 = 5;

const FULL_SHARE_PCT: f64 = 100.0;
const BYTES_PER_KIB: u64 = 1024;
// Holds a typical /proc/<pid>/stat line in one read; larger files grow the buffer once and keep it.
const FILE_BUFFER_BYTES: usize = 4096;

/// The busiest processes, and on every fifth scan the whole process table.
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
pub struct ProcSample {
    pub top: Vec<ProcUsage>,
    pub tree: Option<ProcTree>,
}

/// A process identity that survives pid reuse: a recycled pid starts at a later `start_time`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct ProcId {
    pid: u32,
    start_time: u64,
}

struct ProcRow {
    id: ProcId,
    ppid: u32,
    cpu_ticks: u64,
    rss_pages: u64,
    /// Where this process's comm sits in `ProcCollector::names`.
    name: Range<usize>,
    cpu: f32,
}

struct StatFields<'a> {
    name: &'a [u8],
    ppid: u32,
    flags: u64,
    cpu_ticks: u64,
    start_time: u64,
    rss_pages: u64,
}

pub struct ProcCollector {
    root: SysRoot,
    top_n: usize,
    page_size_bytes: u64,
    samples_until_scan: u32,
    scans_until_tree: u32,
    top: Vec<ProcUsage>,
    previous_all_cpu_ticks: Option<u64>,
    previous_proc_ticks: HashMap<ProcId, u64>,
    // A kernel thread stays one for as long as its pid lives, so it is read once and then skipped.
    // Keyed by pid alone, since the start time is unknowable without the read being avoided: an
    // entry survives only while its pid is listed on every scan, so a recycled pid is read afresh.
    known_kernel_threads: HashSet<u32>,
    // Everything below is per-scan scratch, kept so a scan does not reallocate it.
    current_proc_ticks: HashMap<ProcId, u64>,
    current_kernel_threads: HashSet<u32>,
    rows: Vec<ProcRow>,
    names: Vec<u8>,
    file: Vec<u8>,
    path: String,
}

impl ProcCollector {
    /// Reads a baseline of CPU ticks, so the first `sample` already has deltas for live processes.
    pub fn new(root: &SysRoot, top_n: usize) -> Self {
        let mut collector = Self {
            root: root.clone(),
            top_n,
            // A page size sysconf cannot report leaves every `mem` at 0 rather than a guess.
            page_size_bytes: sysconf(SysconfVar::PAGE_SIZE)
                .ok()
                .flatten()
                .and_then(|bytes| u64::try_from(bytes).ok())
                .unwrap_or_default(),
            samples_until_scan: 0,
            scans_until_tree: 0,
            top: Vec::new(),
            previous_all_cpu_ticks: None,
            previous_proc_ticks: HashMap::new(),
            known_kernel_threads: HashSet::new(),
            current_proc_ticks: HashMap::new(),
            current_kernel_threads: HashSet::new(),
            rows: Vec::new(),
            names: Vec::new(),
            file: Vec::with_capacity(FILE_BUFFER_BYTES),
            path: String::new(),
        };
        collector.take_baseline();
        collector
    }

    /// Scans `/proc` as the baseline the next scan measures process shares from.
    fn take_baseline(&mut self) {
        let all_cpu_ticks = self.read_all_cpu_ticks();
        self.scan_processes();
        self.commit_baseline(all_cpu_ticks);
    }

    fn read_text(&mut self, rel: &str) -> Option<&str> {
        self.root.read_into(rel, &mut self.file).ok()?;
        std::str::from_utf8(&self.file).ok()
    }

    fn read_mem_total_kib(&mut self) -> u64 {
        self.read_text(PROC_MEMINFO)
            .and_then(|text| kib_field(text, MEM_TOTAL_KEY))
            .unwrap_or_default()
    }

    fn read_all_cpu_ticks(&mut self) -> Option<u64> {
        let stat = parse_proc_stat(self.read_text(PROC_STAT)?).ok()?;
        Some(stat.aggregate.total)
    }

    /// Refills `rows` and `current_proc_ticks` from `/proc`, skipping kernel threads and any
    /// process that exits while its file is being read. Also rolls `known_kernel_threads` forward.
    fn scan_processes(&mut self) {
        self.rows.clear();
        self.names.clear();
        self.current_proc_ticks.clear();
        self.current_kernel_threads.clear();
        let Ok(entries) = self.root.entries(PROC_DIR) else {
            // The skip set only stays valid while every scan sees its pids, so a scan that sees
            // none leaves nothing to carry over to a recovered /proc, where a pid may be recycled.
            self.known_kernel_threads.clear();
            return;
        };
        for entry in entries {
            let Some(dir_name) = entry.to_str() else {
                continue;
            };
            let Ok(pid) = dir_name.parse() else {
                continue;
            };
            if self.known_kernel_threads.contains(&pid) {
                self.current_kernel_threads.insert(pid);
                continue;
            }
            self.path.clear();
            self.path
                .extend([PROC_DIR, "/", dir_name, "/", PID_STAT_FILE]);
            if self.root.read_into(&self.path, &mut self.file).is_err() {
                continue;
            }
            let Some(stat) = parse_stat(&self.file) else {
                continue;
            };
            // Kernel threads have no address space of their own, so the kernel reports an RSS of 0
            // for most of them, but the flag also catches the few that borrow a user address space.
            if stat.flags & PF_KTHREAD != 0 {
                self.current_kernel_threads.insert(pid);
                continue;
            }
            let id = ProcId {
                pid,
                start_time: stat.start_time,
            };
            let name_start = self.names.len();
            self.names.extend_from_slice(stat.name);
            self.current_proc_ticks.insert(id, stat.cpu_ticks);
            self.rows.push(ProcRow {
                id,
                ppid: stat.ppid,
                cpu_ticks: stat.cpu_ticks,
                rss_pages: stat.rss_pages,
                name: name_start..self.names.len(),
                cpu: 0.0,
            });
        }
        std::mem::swap(
            &mut self.known_kernel_threads,
            &mut self.current_kernel_threads,
        );
    }

    /// Sets each row's cpu to its share of the ticks that passed since the previous scan.
    fn measure_shares(&mut self, elapsed_ticks: u64) {
        for row in &mut self.rows {
            row.cpu = self.previous_proc_ticks.get(&row.id).map_or(0.0, |before| {
                percent_of(row.cpu_ticks.saturating_sub(*before), elapsed_ticks)
            });
        }
    }

    fn name_of(&self, row: &ProcRow) -> String {
        // Lossily, so that a comm which is not UTF-8 (any process can set one) still shows up.
        String::from_utf8_lossy(&self.names[row.name.clone()]).into_owned()
    }

    fn rss_bytes(&self, row: &ProcRow) -> u64 {
        row.rss_pages.saturating_mul(self.page_size_bytes)
    }

    /// Every scanned process, by pid.
    fn build_tree(&mut self) -> ProcTree {
        self.rows.sort_unstable_by_key(|row| row.id.pid);
        ProcTree {
            kernel_threads: u32::try_from(self.known_kernel_threads.len()).unwrap_or(u32::MAX),
            procs: self
                .rows
                .iter()
                .map(|row| ProcNode {
                    pid: row.id.pid,
                    ppid: row.ppid,
                    name: self.name_of(row),
                    cpu: row.cpu,
                    rss_kib: self.rss_bytes(row) / BYTES_PER_KIB,
                })
                .collect(),
        }
    }

    /// Replaces `top` with the busiest `top_n` processes that hold memory of their own.
    fn rank_top(&mut self, total_kib: u64) {
        // A process without resident memory (a zombie, or a sandbox stub) is no one's memory hog.
        self.rows.retain(|row| row.rss_pages != 0);
        self.rows.sort_unstable_by(|a, b| {
            b.cpu
                .total_cmp(&a.cpu)
                .then_with(|| a.id.pid.cmp(&b.id.pid))
        });
        self.rows.truncate(self.top_n);
        let total_bytes = total_kib.saturating_mul(BYTES_PER_KIB);
        // Names are copied out only for the survivors.
        self.top = self
            .rows
            .iter()
            .map(|row| ProcUsage {
                pid: row.id.pid,
                name: self.name_of(row),
                cpu: row.cpu,
                mem: percent_of(self.rss_bytes(row), total_bytes),
            })
            .collect();
    }

    /// Rescans `/proc`, replaces `top` and, when `with_tree`, returns the whole table. Cpu is each
    /// process's share of the ticks that passed since the previous scan, so it reads the same
    /// however many samples that took.
    fn scan(&mut self, with_tree: bool) -> Option<ProcTree> {
        let total_kib = self.read_mem_total_kib();
        let all_cpu_ticks = self.read_all_cpu_ticks();
        let elapsed_ticks = match (all_cpu_ticks, self.previous_all_cpu_ticks) {
            (Some(now), Some(before)) => now.saturating_sub(before),
            _ => 0,
        };

        self.scan_processes();
        self.measure_shares(elapsed_ticks);
        let tree = with_tree.then(|| self.build_tree());
        self.rank_top(total_kib);
        self.commit_baseline(all_cpu_ticks);
        tree
    }

    fn commit_baseline(&mut self, all_cpu_ticks: Option<u64>) {
        // Process ticks only mean something against the /proc/stat total read alongside them, so
        // a scan without that total leaves the old pair in place as the baseline.
        let Some(ticks) = all_cpu_ticks else {
            return;
        };
        self.previous_all_cpu_ticks = Some(ticks);
        std::mem::swap(&mut self.previous_proc_ticks, &mut self.current_proc_ticks);
    }
}

impl Source<ProcSample> for ProcCollector {
    fn sample(&mut self) -> ProcSample {
        if self.samples_until_scan > 0 {
            self.samples_until_scan -= 1;
            return ProcSample {
                top: self.top.clone(),
                tree: None,
            };
        }
        self.samples_until_scan = PROCESS_SCAN_EVERY - 1;
        let with_tree = self.scans_until_tree == 0;
        self.scans_until_tree = if with_tree {
            TREE_EVERY_SCANS - 1
        } else {
            self.scans_until_tree - 1
        };
        let tree = self.scan(with_tree);
        ProcSample {
            top: self.top.clone(),
            tree,
        }
    }

    fn resume(&mut self) {
        // Shares against the last scan would span the gap. The sampler drops the next sample, so the
        // one after it is the first to scan, against a baseline taken now. That scan builds the tree,
        // as a new collector's first does, so a tree wanted again is not up to 10 s away.
        self.take_baseline();
        self.samples_until_scan = 1;
        self.scans_until_tree = 0;
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

fn parse_stat(stat: &[u8]) -> Option<StatFields<'_>> {
    // The first '(' opens comm because only the pid's digits precede it, and the last ')' closes it
    // because comm may contain parentheses of its own.
    let name_start = stat.iter().position(|byte| *byte == b'(')? + 1;
    let name_end = stat.iter().rposition(|byte| *byte == b')')?;
    let name = stat.get(name_start..name_end)?;
    let after_comm = std::str::from_utf8(&stat[name_end + 1..]).ok()?;

    let mut ppid: Option<u32> = None;
    let mut flags: Option<u64> = None;
    let mut utime: Option<u64> = None;
    let mut stime: Option<u64> = None;
    let mut start_time: Option<u64> = None;
    let mut rss_pages: Option<u64> = None;
    for (offset, token) in after_comm.split_ascii_whitespace().enumerate() {
        match FIRST_FIELD_AFTER_COMM + offset {
            PPID_FIELD => ppid = token.parse().ok(),
            FLAGS_FIELD => flags = token.parse().ok(),
            UTIME_FIELD => utime = token.parse().ok(),
            STIME_FIELD => stime = token.parse().ok(),
            START_TIME_FIELD => start_time = token.parse().ok(),
            RSS_FIELD => {
                rss_pages = token.parse().ok();
                break;
            }
            _ => {}
        }
    }
    Some(StatFields {
        name,
        ppid: ppid?,
        flags: flags?,
        cpu_ticks: utime?.saturating_add(stime?),
        start_time: start_time?,
        rss_pages: rss_pages?,
    })
}
