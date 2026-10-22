use ghost_proto::MemSnapshot;

use super::Source;
use crate::sysroot::SysRoot;

pub(super) const PROC_MEMINFO: &str = "proc/meminfo";

pub(super) const MEM_TOTAL_KEY: &str = "MemTotal";
const MEM_AVAILABLE_KEY: &str = "MemAvailable";
const SWAP_TOTAL_KEY: &str = "SwapTotal";
const SWAP_FREE_KEY: &str = "SwapFree";

// Holds /proc/meminfo in one read; a larger file grows the buffer once and keeps it.
const FILE_BUFFER_BYTES: usize = 4096;

#[derive(Default)]
struct MemInfo {
    total_kib: u64,
    available_kib: u64,
    swap_total_kib: u64,
    swap_free_kib: u64,
}

pub struct MemCollector {
    root: SysRoot,
    file: Vec<u8>,
}

impl MemCollector {
    pub fn new(root: &SysRoot) -> Self {
        Self {
            root: root.clone(),
            file: Vec::with_capacity(FILE_BUFFER_BYTES),
        }
    }

    fn read_text(&mut self, rel: &str) -> Option<&str> {
        self.root.read_into(rel, &mut self.file).ok()?;
        std::str::from_utf8(&self.file).ok()
    }

    fn read_meminfo(&mut self) -> MemInfo {
        let Some(text) = self.read_text(PROC_MEMINFO) else {
            return MemInfo::default();
        };
        let kib = |key| kib_field(text, key).unwrap_or_default();
        MemInfo {
            total_kib: kib(MEM_TOTAL_KEY),
            available_kib: kib(MEM_AVAILABLE_KEY),
            swap_total_kib: kib(SWAP_TOTAL_KEY),
            swap_free_kib: kib(SWAP_FREE_KEY),
        }
    }
}

impl Source<MemSnapshot> for MemCollector {
    fn sample(&mut self) -> MemSnapshot {
        let meminfo = self.read_meminfo();
        MemSnapshot {
            total_kib: meminfo.total_kib,
            used_kib: meminfo.total_kib.saturating_sub(meminfo.available_kib),
            swap_total_kib: meminfo.swap_total_kib,
            swap_used_kib: meminfo.swap_total_kib.saturating_sub(meminfo.swap_free_kib),
        }
    }
}

/// The value of a `Key:   1234 kB` line in `/proc/meminfo`.
pub(super) fn kib_field(text: &str, key: &str) -> Option<u64> {
    text.lines()
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| *name == key)
        .and_then(|(_, value)| value.split_whitespace().next()?.parse().ok())
}
