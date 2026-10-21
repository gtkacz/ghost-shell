mod common;

use std::num::NonZeroU32;
use std::path::Path;
use std::time::Duration;

use common::server::{CONFIG_FILE, Client, SOCKET_FILE, TICK_MS, WAIT};
use ghost_proto::{CpuSnapshot, topics};
use ghostd::collect::Source;
use ghostd::providers;
use ghostd::sampler::Sampler;
use ghostd::server::{self, ConfigFeed, Hub, ServeError};
use serde_json::json;
use tokio::time::timeout;

const SAMPLES_BEFORE_THE_PANIC: usize = 2;
const SAMPLED_TOPICS: [&str; 5] = [
    topics::CPU,
    topics::MEM,
    topics::SENSORS,
    topics::GPU,
    topics::DISKS,
];

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

fn hub(dir: &Path, sampler: Sampler) -> Hub {
    let socket = dir.join(SOCKET_FILE);
    let config = ConfigFeed::open(&dir.join(CONFIG_FILE));
    Hub {
        providers: providers::supervise(&config.current().providers, &socket),
        config,
        sampler,
        tick_ms: NonZeroU32::new(TICK_MS).unwrap(),
        widget_dirs: Box::new(|_| Vec::new()),
        on_warning: Box::new(|_, _| {}),
        feeds: Vec::new(),
    }
}

/// A UI client whose hello subscribes it to `topics`, so the collectors behind them get sampled.
async fn subscriber(socket: &Path, topics: &[&str]) -> Client {
    let mut client = Client::connect(socket).await;
    client
        .send(json!({"t": "hello", "proto": 1, "topics": topics}))
        .await;
    client
}

#[tokio::test]
async fn serving_stops_with_an_error_when_a_collector_panics() {
    let dir = tempfile::tempdir().unwrap();
    let sampler = Sampler::new(
        Box::new(PanicsAfter {
            samples_left: SAMPLES_BEFORE_THE_PANIC,
        }),
        Box::new(Idle),
        Box::new(Idle),
        Box::new(Idle),
        Box::new(Idle),
    );
    let socket = dir.path().join(SOCKET_FILE);
    let (listener, _lock) = server::bind(&socket).unwrap();
    let serving = tokio::spawn(server::serve(listener, hub(dir.path(), sampler)));
    let _ui = subscriber(&socket, &[topics::CPU]).await;

    let stopped = timeout(WAIT, serving).await;

    let error = stopped
        .expect("the daemon kept serving after a collector panicked")
        .unwrap();
    let ServeError::Sampling(failure) = &error else {
        panic!("expected a sampling failure, got {error}");
    };
    assert!(failure.is_panic(), "{error}");
    assert!(
        error.to_string().contains("the cpu collector hit a bug"),
        "{error}"
    );
}

#[tokio::test]
async fn serving_goes_on_while_every_collector_keeps_sampling() {
    let dir = tempfile::tempdir().unwrap();
    let sampler = Sampler::new(
        Box::new(Idle),
        Box::new(Idle),
        Box::new(Idle),
        Box::new(Idle),
        Box::new(Idle),
    );
    let socket = dir.path().join(SOCKET_FILE);
    let (listener, _lock) = server::bind(&socket).unwrap();
    let serving = tokio::spawn(server::serve(listener, hub(dir.path(), sampler)));
    let _ui = subscriber(&socket, &SAMPLED_TOPICS).await;

    let stopped = timeout(Duration::from_millis(u64::from(TICK_MS) * 6), serving).await;

    assert!(stopped.is_err(), "serve returned with {:?}", stopped.ok());
}
