use std::time::{Duration, Instant};

use ghost_proto::PerfSnapshot;
use nix::unistd::{SysconfVar, sysconf};

use crate::collect::Source;
use crate::sysroot::SysRoot;

const SELF_STAT: &str = "proc/self/stat";

// 1-based positions in /proc/<pid>/stat, counting the pid as field 1.
const UTIME_FIELD: usize = 14;
const STIME_FIELD: usize = 15;
const RSS_FIELD: usize = 24;
// comm is field 2 and may itself contain spaces, so parsing resumes after its closing parenthesis.
const FIRST_FIELD_AFTER_COMM: usize = 3;

const FULL_CORE_PCT: f64 = 100.0;
const BYTES_PER_KIB: u64 = 1024;

/// CPU and memory use of the daemon itself and, once [`SelfMon::watch_shell`] names it, the shell.
///
/// CPU is a percentage of one core over the time since the previous sample, so it can exceed 100
/// while several threads are busy.
pub struct SelfMon {
    root: SysRoot,
    ticks_per_second: u64,
    page_size_kib: u64,
    daemon: Tracked,
    shell: Option<(u32, Tracked)>,
    file: Vec<u8>,
}

impl SelfMon {
    /// Reads a baseline for the daemon, so the first `sample` already covers a real interval.
    pub fn new(root: SysRoot) -> Self {
        // A clock rate or page size sysconf cannot report leaves the affected values at 0 rather
        // than a guess.
        let ticks_per_second = sysconf_u64(SysconfVar::CLK_TCK);
        let page_size_kib = sysconf_u64(SysconfVar::PAGE_SIZE) / BYTES_PER_KIB;
        let mut monitor = Self {
            root,
            ticks_per_second,
            page_size_kib,
            daemon: Tracked::new(SELF_STAT.to_owned()),
            shell: None,
            file: Vec::new(),
        };
        monitor.daemon.read(
            &monitor.root,
            &mut monitor.file,
            monitor.page_size_kib,
            ticks_per_second,
        );
        monitor
    }

    /// Follows the process `pid` as the shell from the next sample on; `None` stops following one.
    pub fn watch_shell(&mut self, pid: Option<u32>) {
        if self.shell.as_ref().map(|(watched, _)| *watched) != pid {
            self.shell = pid.map(|pid| (pid, Tracked::new(format!("proc/{pid}/stat"))));
        }
    }
}

impl Source<PerfSnapshot> for SelfMon {
    fn sample(&mut self) -> PerfSnapshot {
        let daemon = self.daemon.read(
            &self.root,
            &mut self.file,
            self.page_size_kib,
            self.ticks_per_second,
        );
        let shell = self.shell.as_mut().and_then(|(_, shell)| {
            shell.read(
                &self.root,
                &mut self.file,
                self.page_size_kib,
                self.ticks_per_second,
            )
        });
        PerfSnapshot {
            daemon_cpu: daemon
                .as_ref()
                .and_then(|usage| usage.cpu_pct)
                .unwrap_or_default(),
            daemon_rss_kib: daemon.map(|usage| usage.rss_kib).unwrap_or_default(),
            shell_cpu: shell.as_ref().and_then(|usage| usage.cpu_pct),
            shell_rss_kib: shell.map(|usage| usage.rss_kib),
        }
    }
}

fn sysconf_u64(variable: SysconfVar) -> u64 {
    sysconf(variable)
        .ok()
        .flatten()
        .and_then(|value| u64::try_from(value).ok())
        .unwrap_or_default()
}

struct Usage {
    /// `None` on the first read of a process, which has no earlier read to measure from.
    cpu_pct: Option<f32>,
    rss_kib: u64,
}

/// One process whose CPU time is compared from read to read.
struct Tracked {
    stat_path: String,
    previous: Option<(u64, Instant)>,
}

impl Tracked {
    fn new(stat_path: String) -> Self {
        Self {
            stat_path,
            previous: None,
        }
    }

    /// `None` when the process cannot be read, as once it has exited.
    fn read(
        &mut self,
        root: &SysRoot,
        file: &mut Vec<u8>,
        page_size_kib: u64,
        ticks_per_second: u64,
    ) -> Option<Usage> {
        root.read_into(&self.stat_path, file).ok()?;
        let stat = parse_stat(file)?;
        let now = Instant::now();
        let cpu_pct = self
            .previous
            .replace((stat.cpu_ticks, now))
            .map(|(ticks, at)| {
                cpu_percent(
                    stat.cpu_ticks.saturating_sub(ticks),
                    ticks_per_second,
                    now.duration_since(at),
                )
            });
        Some(Usage {
            cpu_pct,
            rss_kib: stat.rss_pages * page_size_kib,
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
struct Stat {
    cpu_ticks: u64,
    rss_pages: u64,
}

fn parse_stat(file: &[u8]) -> Option<Stat> {
    let after_comm = file.iter().rposition(|&byte| byte == b')')? + 1;
    let text = std::str::from_utf8(&file[after_comm..]).ok()?;
    let mut fields = text.split_ascii_whitespace();
    let utime: u64 = fields
        .nth(UTIME_FIELD - FIRST_FIELD_AFTER_COMM)?
        .parse()
        .ok()?;
    let stime: u64 = fields.nth(STIME_FIELD - UTIME_FIELD - 1)?.parse().ok()?;
    let rss_pages = fields.nth(RSS_FIELD - STIME_FIELD - 1)?.parse().ok()?;
    Some(Stat {
        cpu_ticks: utime + stime,
        rss_pages,
    })
}

/// Percentage of one core that `ticks` of CPU time make over `elapsed`, or 0 when nothing can be
/// measured.
fn cpu_percent(ticks: u64, ticks_per_second: u64, elapsed: Duration) -> f32 {
    let seconds = elapsed.as_secs_f64();
    if ticks_per_second == 0 || seconds == 0.0 {
        return 0.0;
    }
    (FULL_CORE_PCT * ticks as f64 / ticks_per_second as f64 / seconds) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stat_fields_are_read_after_a_comm_with_spaces_and_parentheses() {
        let line =
            b"4242 (Web (Content) 1) S 1 2 3 4 5 6 7 8 9 10 70 30 11 12 13 14 15 16 17 18 512 19\n";

        assert_eq!(
            parse_stat(line),
            Some(Stat {
                cpu_ticks: 100,
                rss_pages: 512,
            })
        );
    }

    #[test]
    fn truncated_stat_is_unreadable() {
        assert_eq!(parse_stat(b"4242 (ghostd) S 1 2 3"), None);
    }

    #[test]
    fn cpu_percent_is_relative_to_one_core() {
        assert_eq!(cpu_percent(50, 100, Duration::from_secs(1)), 50.0);
        assert_eq!(cpu_percent(300, 100, Duration::from_secs(2)), 150.0);
    }

    #[test]
    fn cpu_percent_is_zero_without_a_clock_or_interval() {
        assert_eq!(cpu_percent(50, 0, Duration::from_secs(1)), 0.0);
        assert_eq!(cpu_percent(50, 100, Duration::ZERO), 0.0);
    }
}
