use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroU32;
use std::time::Duration;

use ghost_proto::{PerfSnapshot, ResolvedConfig, ServerMsg, topics};
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio::time::{MissedTickBehavior, interval};

use super::{Demand, ServeError, Shared};
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
    /// Whether `selfmon` was sampled on the previous tick; building it counts as a sample.
    perf_sampled: bool,
}

pub(super) struct Tick {
    pub(super) messages: Vec<ServerMsg>,
    /// `None` while no client wants `perf`, and on the first tick after a gap.
    pub(super) perf: Option<PerfSnapshot>,
    pub(super) series: BTreeMap<String, Vec<Option<f32>>>,
}

impl Collectors {
    pub(super) fn new(sampler: Sampler, config: &ResolvedConfig, tick_ms: NonZeroU32) -> Self {
        let history_len = history_len(config.dashboard.history_minutes, tick_ms);
        Self {
            sampler,
            history: History::new(history_len),
            history_len,
            selfmon: SelfMon::new(SysRoot::host()),
            perf_sampled: true,
        }
    }

    fn tick(
        &mut self,
        shell_pid: Option<u32>,
        history_len: usize,
        wanted: &BTreeSet<String>,
    ) -> Tick {
        if history_len != self.history_len {
            self.history.resize(history_len);
            self.history_len = history_len;
        }
        let messages = self.sampler.tick(&mut self.history, wanted);
        let perf_wanted = wanted.contains(topics::PERF);
        let perf = perf_wanted
            .then(|| {
                self.selfmon.watch_shell(shell_pid);
                self.selfmon.sample()
            })
            // CPU use is measured since the previous sample, so the first one after a gap spans it
            // and only serves as the next one's baseline.
            .filter(|_| self.perf_sampled);
        self.perf_sampled = perf_wanted;
        Tick {
            messages,
            perf,
            series: self.history.series(),
        }
    }
}

pub(super) async fn sample_forever(
    shared: Shared,
    mut collectors: Collectors,
    tick_ms: NonZeroU32,
    config: watch::Receiver<ResolvedConfig>,
    demand: Demand,
) -> ServeError {
    let mut ticks = interval(Duration::from_millis(tick_ms.get().into()));
    ticks.set_missed_tick_behavior(MissedTickBehavior::Skip);
    loop {
        ticks.tick().await;
        let shell_pid = shared.lock().perf_pid();
        let history_len = history_len(config.borrow().dashboard.history_minutes, tick_ms);
        let wanted = demand.borrow().clone();
        // Collectors block on sysfs and NVML reads; off the reactor, a slow read delays the
        // built-in topics but never the socket.
        let sampled: JoinHandle<(Collectors, Tick)> = tokio::task::spawn_blocking(move || {
            let tick = collectors.tick(shell_pid, history_len, &wanted);
            (collectors, tick)
        });
        match sampled.await {
            Ok((returned, tick)) => {
                collectors = returned;
                shared.lock().record_tick(tick);
            }
            Err(error) => return error.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake;

    #[test]
    fn perf_skips_its_first_sample_after_a_gap() {
        let tick_ms = NonZeroU32::new(1000).unwrap();
        let mut collectors = Collectors::new(fake::sampler(), &ResolvedConfig::default(), tick_ms);
        let len = collectors.history_len;
        let perf = BTreeSet::from([topics::PERF.to_owned()]);
        let nothing = BTreeSet::new();

        let right_after_the_build = collectors.tick(None, len, &perf);
        collectors.tick(None, len, &nothing);
        let rebaseline = collectors.tick(None, len, &perf);
        let resumed = collectors.tick(None, len, &perf);

        assert!(right_after_the_build.perf.is_some());
        assert!(rebaseline.perf.is_none());
        assert!(resumed.perf.is_some());
    }
}
