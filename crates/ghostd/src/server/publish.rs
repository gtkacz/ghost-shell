//! What a feed runs with: a way to publish, the topics clients want, and the live config.

use std::collections::BTreeSet;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use ghost_proto::ResolvedConfig;
use serde::Serialize;
use serde_json::Value;
use tokio::sync::watch;

type PublishFn = dyn Fn(&str, Value) + Send + Sync;

/// Publishes values under a topic. The hub's publisher caches, sequences and relays them exactly
/// as it does a provider's.
#[derive(Clone)]
pub struct Publisher(Arc<PublishFn>);

impl Publisher {
    pub fn new(publish: impl Fn(&str, Value) + Send + Sync + 'static) -> Self {
        Self(Arc::new(publish))
    }

    /// Panics if `value` has no JSON representation (e.g. a map with non-string keys); the
    /// snapshot types always do.
    pub fn publish<T: Serialize>(&self, topic: &str, value: &T) {
        let value = serde_json::to_value(value).expect("value has a JSON representation");
        (self.0)(topic, value);
    }
}

/// The union of every UI client's subscriptions, sent only when it changes.
pub type Demand = watch::Receiver<BTreeSet<String>>;

pub struct FeedContext {
    pub publisher: Publisher,
    pub demand: Demand,
    pub config: watch::Receiver<ResolvedConfig>,
}

/// A built-in source that publishes from an async task instead of being sampled every tick.
///
/// Its future must never complete: one that has nothing to do publishes its fallback and waits
/// forever, because `serve` stops with [`super::ServeError::Feed`] when a feed ends or panics.
pub type Feed = Box<dyn FnOnce(FeedContext) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send>;
