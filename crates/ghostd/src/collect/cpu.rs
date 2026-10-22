use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::time::{Duration, Instant};

use ghost_proto::{CcdLoad, CoreLoad, CpuSnapshot};

use super::rapl::PackagePower;
use super::{Source, read_text};
use crate::sysroot::SysRoot;

const CPU_SYSFS_DIR: &str = "sys/devices/system/cpu";
const SHARED_CPU_LIST: &str = "cache/index3/shared_cpu_list";
const L3_SIZE: &str = "cache/index3/size";
const SCALING_CUR_FREQ: &str = "cpufreq/scaling_cur_freq";
const THREAD_SIBLINGS_LIST: &str = "topology/thread_siblings_list";
const PROC_STAT: &str = "proc/stat";
const PROC_CPUINFO: &str = "proc/cpuinfo";
const DRM_DIR: &str = "sys/class/drm";
const DRM_DEVICE_DIR: &str = "device";
const DRM_VENDOR: &str = "vendor";
const DRM_GPU_BUSY: &str = "gpu_busy_percent";
const DRM_CARD_PREFIX: &str = "card";
const AMD_PCI_VENDOR: &str = "0x1002";

const CPU_LABEL: &str = "cpu";
const MODEL_NAME_KEY: &str = "model name";
const L3_SIZE_UNIT: &str = "K";
const KHZ_PER_MHZ: u64 = 1000;
const FULL_LOAD_PCT: f64 = 100.0;

// user, nice, system, idle, iowait, irq, softirq, steal. Guest columns follow but the kernel
// already folds them into user/nice, so counting them would double-count.
const COUNTED_STAT_FIELDS: usize = 8;
const IDLE_FIELD: usize = 3;
const IOWAIT_FIELD: usize = 4;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct CpuTimes {
    pub busy: u64,
    pub total: u64,
}

impl CpuTimes {
    /// Percentage of non-idle time since `previous`, or 0 when no time has elapsed.
    pub fn load_since(&self, previous: &CpuTimes) -> f32 {
        let total = self.total.saturating_sub(previous.total);
        if total == 0 {
            return 0.0;
        }
        let busy = self.busy.saturating_sub(previous.busy);
        // iowait can run backwards between reads, which would push busy past total.
        let load = (FULL_LOAD_PCT * busy as f64 / total as f64).min(FULL_LOAD_PCT);
        load as f32
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ProcStat {
    pub aggregate: CpuTimes,
    pub cpus: BTreeMap<u16, CpuTimes>,
}

pub struct CpuCollector {
    root: SysRoot,
    model: String,
    cpus: Vec<u16>,
    thread_count: usize,
    topology: Vec<CcdLoad>,
    cores: Vec<CoreLoad>,
    previous: ProcStat,
    package: Option<PackagePower>,
    last_sampled: Option<Instant>,
}

impl CpuCollector {
    /// Reads topology and a baseline of CPU times, so the first `sample` is already a real delta.
    pub fn new(root: &SysRoot) -> io::Result<Self> {
        let cpus = enumerate_cpus(root)?;
        let thread_count = cpus
            .iter()
            .max()
            .map_or(0, |highest| usize::from(*highest) + 1);
        Ok(Self {
            model: read_model(root),
            topology: read_topology(root, &cpus)?,
            cores: read_cores(root, &cpus)?,
            previous: read_stat(root)?,
            package: PackagePower::find(root),
            last_sampled: None,
            root: root.clone(),
            cpus,
            thread_count,
        })
    }

    fn mean_freq_mhz(&self) -> u32 {
        // Parsed as u32 so a bogus value is skipped and the sum cannot overflow.
        let khz: Vec<u64> = self
            .cpus
            .iter()
            .filter_map(|cpu| {
                let rel = format!("{CPU_SYSFS_DIR}/cpu{cpu}/{SCALING_CUR_FREQ}");
                self.root.read(&rel).ok()?.trim().parse::<u32>().ok()
            })
            .map(u64::from)
            .collect();
        let Some(mean_khz) = khz.iter().sum::<u64>().checked_div(khz.len() as u64) else {
            return 0;
        };
        u32::try_from(mean_khz / KHZ_PER_MHZ).unwrap_or(u32::MAX)
    }

    /// Scanned every sample, because a driver can bind after the daemon starts.
    fn igpu_busy_pct(&self) -> Option<f32> {
        let mut cards: Vec<(u32, String)> = self
            .root
            .list(DRM_DIR)
            .ok()?
            .into_iter()
            // Connector entries such as `card1-DP-1` are not cards, and their numbers do not parse.
            .filter_map(|name| Some((name.strip_prefix(DRM_CARD_PREFIX)?.parse().ok()?, name)))
            .collect();
        // Entry names sort lexically (card10 before card2), so restore numeric order.
        cards.sort_unstable();
        cards.into_iter().find_map(|(_, card)| {
            let device = format!("{DRM_DIR}/{card}/{DRM_DEVICE_DIR}");
            let vendor = read_text(&self.root, &format!("{device}/{DRM_VENDOR}"))?;
            if vendor != AMD_PCI_VENDOR {
                return None;
            }
            read_text(&self.root, &format!("{device}/{DRM_GPU_BUSY}"))?
                .parse()
                .ok()
        })
    }

    /// Samples with the time since the previous sample supplied by the caller, so that a test can
    /// fix the interval that package power is measured over.
    pub fn sample_after(&mut self, elapsed: Duration) -> CpuSnapshot {
        // An unreadable /proc/stat reads as an idle tick, since sampling has no way to fail.
        let current = read_stat(&self.root).unwrap_or_else(|_| self.previous.clone());

        let mut threads = vec![0.0; self.thread_count];
        for cpu in &self.cpus {
            if let (Some(now), Some(then)) = (current.cpus.get(cpu), self.previous.cpus.get(cpu)) {
                threads[usize::from(*cpu)] = now.load_since(then);
            }
        }
        let ccds = self
            .topology
            .iter()
            .map(|ccd| CcdLoad {
                load: mean_load(&ccd.cpus, &threads),
                ..ccd.clone()
            })
            .collect();
        let cores = self
            .cores
            .iter()
            .map(|core| CoreLoad {
                load: mean_load(&core.cpus, &threads),
                cpus: core.cpus.clone(),
            })
            .collect();
        let load = current.aggregate.load_since(&self.previous.aggregate);
        self.previous = current;

        CpuSnapshot {
            model: self.model.clone(),
            threads,
            ccds,
            cores,
            load,
            freq_mhz: self.mean_freq_mhz(),
            package_w: self
                .package
                .as_mut()
                .and_then(|package| package.sample(elapsed)),
            igpu_busy_pct: self.igpu_busy_pct(),
        }
    }
}

impl Source<CpuSnapshot> for CpuCollector {
    fn sample(&mut self) -> CpuSnapshot {
        let now = Instant::now();
        let elapsed = self
            .last_sampled
            .replace(now)
            .map_or(Duration::ZERO, |before| now.duration_since(before));
        self.sample_after(elapsed)
    }
}

/// The mean of the `threads` loads of `cpus`, which is never empty.
fn mean_load(cpus: &[u16], threads: &[f32]) -> f32 {
    let summed: f32 = cpus.iter().map(|cpu| threads[usize::from(*cpu)]).sum();
    summed / cpus.len() as f32
}

/// Expands a kernel CPU list such as `0-7,16-23`; tokens that are not numbers are dropped.
pub fn parse_cpu_list(list: &str) -> Vec<u16> {
    list.trim()
        .split(',')
        .flat_map(|token| match token.split_once('-') {
            Some((first, last)) => first.parse().ok().zip(last.parse().ok()),
            None => token.parse().ok().map(|cpu| (cpu, cpu)),
        })
        .flat_map(|(first, last)| first..=last)
        .collect()
}

pub(crate) fn parse_proc_stat(text: &str) -> io::Result<ProcStat> {
    let mut aggregate = None;
    let mut cpus = BTreeMap::new();
    for line in text.lines() {
        let mut columns = line.split_whitespace();
        let Some(suffix) = columns
            .next()
            .and_then(|label| label.strip_prefix(CPU_LABEL))
        else {
            continue;
        };
        if suffix.is_empty() {
            aggregate = Some(parse_cpu_times(columns)?);
        } else if let Ok(cpu) = suffix.parse() {
            cpus.insert(cpu, parse_cpu_times(columns)?);
        }
    }
    let aggregate = aggregate.ok_or_else(|| invalid_data("no aggregate cpu line in /proc/stat"))?;
    Ok(ProcStat { aggregate, cpus })
}

fn parse_cpu_times<'a>(columns: impl Iterator<Item = &'a str>) -> io::Result<CpuTimes> {
    let ticks = columns
        .take(COUNTED_STAT_FIELDS)
        .map(str::parse)
        .collect::<Result<Vec<u64>, _>>()
        .map_err(|e| invalid_data(format!("bad /proc/stat field: {e}")))?;
    if ticks.len() < COUNTED_STAT_FIELDS {
        return Err(invalid_data("truncated cpu line in /proc/stat"));
    }
    // Saturating, because a corrupt counter must degrade the load rather than overflow-panic.
    let total = ticks
        .iter()
        .fold(0, |sum: u64, field| sum.saturating_add(*field));
    Ok(CpuTimes {
        busy: total
            .saturating_sub(ticks[IDLE_FIELD])
            .saturating_sub(ticks[IOWAIT_FIELD]),
        total,
    })
}

fn read_stat(root: &SysRoot) -> io::Result<ProcStat> {
    parse_proc_stat(&root.read(PROC_STAT)?)
}

/// The model is cosmetic, so an unreadable `/proc/cpuinfo` must not block telemetry.
fn read_model(root: &SysRoot) -> String {
    let Ok(cpuinfo) = root.read(PROC_CPUINFO) else {
        return String::new();
    };
    cpuinfo
        .lines()
        .filter_map(|line| line.split_once(':'))
        .find(|(key, _)| key.trim() == MODEL_NAME_KEY)
        .map(|(_, model)| model.trim().to_owned())
        .unwrap_or_default()
}

fn enumerate_cpus(root: &SysRoot) -> io::Result<Vec<u16>> {
    let mut cpus: Vec<u16> = root
        .list(CPU_SYSFS_DIR)?
        .iter()
        .filter_map(|name| name.strip_prefix(CPU_LABEL)?.parse().ok())
        .collect();
    // Entry names sort lexically (cpu10 before cpu2), so restore numeric order.
    cpus.sort_unstable();
    Ok(cpus)
}

/// Groups CPUs by the set that shares their L3; on Zen each such set is one CCD.
fn read_topology(root: &SysRoot, cpus: &[u16]) -> io::Result<Vec<CcdLoad>> {
    // Keyed by CPU list so iteration yields CCDs ordered by their lowest CPU.
    let mut l3_by_group: BTreeMap<Vec<u16>, Option<u32>> = BTreeMap::new();
    for cpu in cpus {
        let cache_dir = format!("{CPU_SYSFS_DIR}/cpu{cpu}");
        // Offline CPUs have no cache directory, so they belong to no CCD.
        let Some(shared) = read_if_present(root, &format!("{cache_dir}/{SHARED_CPU_LIST}"))? else {
            continue;
        };
        let mut shared = parse_cpu_list(&shared);
        // Thread loads are indexed by enumerated CPU, so unknown CPUs in the list must not leak in.
        shared.retain(|member| cpus.binary_search(member).is_ok());
        if shared.is_empty() {
            continue;
        }
        let size = read_if_present(root, &format!("{cache_dir}/{L3_SIZE}"))?
            .map(|size| parse_l3_kib(&size))
            .transpose()?;
        // A member without a size file must not erase the size another member of its group gave.
        let known = l3_by_group.entry(shared).or_default();
        *known = known.or(size);
    }

    // The smallest cache marks the plain CCDs, so it is only known once every CCD has reported.
    let every_size_known = l3_by_group.values().all(Option::is_some);
    let smallest = l3_by_group
        .values()
        .flatten()
        .min()
        .copied()
        .unwrap_or_default();
    l3_by_group
        .into_iter()
        .filter_map(|(members, l3_kib)| Some((members, l3_kib?)))
        .enumerate()
        .map(|(index, (members, l3_kib))| {
            Ok(CcdLoad {
                id: u8::try_from(index).map_err(|_| invalid_data("more CCDs than fit in a u8"))?,
                cpus: members,
                l3_kib,
                vcache: every_size_known && l3_kib > smallest,
                load: 0.0,
            })
        })
        .collect()
}

/// Groups CPUs by the core whose SMT threads they are.
fn read_cores(root: &SysRoot, cpus: &[u16]) -> io::Result<Vec<CoreLoad>> {
    // Sibling lists never overlap, so ordering them orders the cores by their lowest CPU.
    let mut cores: BTreeSet<Vec<u16>> = BTreeSet::new();
    for cpu in cpus {
        let rel = format!("{CPU_SYSFS_DIR}/cpu{cpu}/{THREAD_SIBLINGS_LIST}");
        // Offline CPUs have no topology directory, so they belong to no core.
        let Some(siblings) = read_if_present(root, &rel)? else {
            continue;
        };
        let mut siblings = parse_cpu_list(&siblings);
        // Thread loads are indexed by enumerated CPU, so unknown CPUs in the list must not leak in.
        siblings.retain(|sibling| cpus.binary_search(sibling).is_ok());
        if !siblings.is_empty() {
            cores.insert(siblings);
        }
    }
    Ok(cores
        .into_iter()
        .map(|cpus| CoreLoad { cpus, load: 0.0 })
        .collect())
}

fn read_if_present(root: &SysRoot, rel: &str) -> io::Result<Option<String>> {
    match root.read(rel) {
        Ok(content) => Ok(Some(content)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

fn parse_l3_kib(size: &str) -> io::Result<u32> {
    size.trim()
        .strip_suffix(L3_SIZE_UNIT)
        .and_then(|kib| kib.parse().ok())
        .ok_or_else(|| invalid_data(format!("unrecognised L3 size {:?}", size.trim())))
}

fn invalid_data(message: impl Into<Box<dyn std::error::Error + Send + Sync>>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
