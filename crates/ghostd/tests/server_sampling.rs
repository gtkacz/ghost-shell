mod common;

use std::num::NonZeroU32;
use std::time::Duration;

use common::server::{CONFIG_FILE, SOCKET_FILE, TICK_MS, WAIT};
use ghost_proto::CpuSnapshot;
use ghostd::collect::Source;
use ghostd::providers;
use ghostd::sampler::Sampler;
use ghostd::server::{self, ConfigFeed, Hub, ServeError};
use tokio::time::timeout;

const SAMPLES_BEFORE_THE_PANIC: usize = 2;

/// Samples normally a fixed number of times, then panics as a buggy collector would.
struct PanicsAfter {
    samples_left: usize,
}

impl Source<CpuSnapshot> for PanicsAfter {
    fn sample(&mut self) -> CpuSnapshot {
        assert!(self.samples_left > 0, "the cpu collector hit a bug");
        self.samples_left -= 1;
        CpuSnapshot::default()
    }
}

struct Idle;

impl<T: Default> Source<T> for Idle {
    fn sample(&mut self) -> T {
        T::default()
    }
}

#[tokio::test]
async fn serving_stops_with_an_error_when_a_collector_panics() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join(SOCKET_FILE);
    let config = ConfigFeed::open(&dir.path().join(CONFIG_FILE));
    let hub = Hub {
        providers: providers::supervise(&config.current().providers, &socket),
        config,
        sampler: Sampler::new(
            Box::new(PanicsAfter {
                samples_left: SAMPLES_BEFORE_THE_PANIC,
            }),
            Box::new(Idle),
            Box::new(Idle),
            Box::new(Idle),
            Box::new(Idle),
        ),
        tick_ms: NonZeroU32::new(TICK_MS).unwrap(),
        widget_dirs: Box::new(|_| Vec::new()),
        on_warning: Box::new(|_, _| {}),
    };
    let (listener, _lock) = server::bind(&socket).unwrap();

    let stopped = timeout(WAIT, server::serve(listener, hub)).await;

    let error = stopped.expect("the daemon kept serving after a collector panicked");
    let ServeError::Sampling(failure) = &error;
    assert!(failure.is_panic(), "{error}");
    assert!(
        error.to_string().contains("the cpu collector hit a bug"),
        "{error}"
    );
}

#[tokio::test]
async fn serving_goes_on_while_every_collector_keeps_sampling() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join(SOCKET_FILE);
    let config = ConfigFeed::open(&dir.path().join(CONFIG_FILE));
    let hub = Hub {
        providers: providers::supervise(&config.current().providers, &socket),
        config,
        sampler: Sampler::new(
            Box::new(Idle),
            Box::new(Idle),
            Box::new(Idle),
            Box::new(Idle),
            Box::new(Idle),
        ),
        tick_ms: NonZeroU32::new(TICK_MS).unwrap(),
        widget_dirs: Box::new(|_| Vec::new()),
        on_warning: Box::new(|_, _| {}),
    };
    let (listener, _lock) = server::bind(&socket).unwrap();

    let stopped = timeout(
        Duration::from_millis(u64::from(TICK_MS) * 6),
        server::serve(listener, hub),
    )
    .await;

    assert!(stopped.is_err(), "serve returned with {:?}", stopped.ok());
}
