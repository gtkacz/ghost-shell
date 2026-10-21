use std::collections::BTreeMap;
use std::num::NonZeroU32;
use std::time::Duration;

use ghost_proto::{PerfSnapshot, ResolvedConfig, ServerMsg};
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio::time::{MissedTickBehavior, interval};
use tracing::error;

use super::Shared;
use crate::collect::Source;
use crate::history::{History, history_len};
use crate::sampler::Sampler;
use crate::selfmon::SelfMon;
use crate::sysroot::SysRoot;

/// The sampler and its companions, moved onto a blocking thread for each tick and back.
pub(super) struct Collectors {
    sampler: Sampler,
    history: History,
    history_len: usize,
    selfmon: SelfMon,
}

pub(super) struct Tick {
    pub(super) messages: Vec<ServerMsg>,
    pub(super) perf: PerfSnapshot,
    pub(super) series: BTreeMap<String, Vec<f32>>,
}

impl Collectors {
    pub(super) fn new(sampler: Sampler, config: &ResolvedConfig) -> Self {
        let history_len = history_len(config.dashboard.history_minutes);
        Self {
            sampler,
            history: History::new(history_len),
            history_len,
            selfmon: SelfMon::new(SysRoot::host()),
        }
    }

    fn tick(&mut self, shell_pid: Option<u32>, history_len: usize) -> Tick {
        if history_len != self.history_len {
            self.history.resize(history_len);
            self.history_len = history_len;
        }
        let messages = self.sampler.tick(&mut self.history);
        self.selfmon.watch_shell(shell_pid);
        Tick {
            messages,
            perf: self.selfmon.sample(),
            series: self.history.series(),
        }
    }
}

pub(super) async fn sample_forever(
    shared: Shared,
    mut collectors: Collectors,
    tick_ms: NonZeroU32,
    config: watch::Receiver<ResolvedConfig>,
) {
    let mut ticks = interval(Duration::from_millis(tick_ms.get().into()));
    ticks.set_missed_tick_behavior(MissedTickBehavior::Skip);
    loop {
        ticks.tick().await;
        let shell_pid = shared.lock().perf_pid();
        let history_len = history_len(config.borrow().dashboard.history_minutes);
        // Collectors block on sysfs and NVML reads; off the reactor, a slow read delays the
        // built-in topics but never the socket.
        let sampled: JoinHandle<(Collectors, Tick)> = tokio::task::spawn_blocking(move || {
            let tick = collectors.tick(shell_pid, history_len);
            (collectors, tick)
        });
        match sampled.await {
            Ok((returned, tick)) => {
                collectors = returned;
                shared.lock().record_tick(tick);
            }
            Err(error) => {
                error!(%error, "a collector panicked; the built-in topics stop updating");
                return;
            }
        }
    }
}
