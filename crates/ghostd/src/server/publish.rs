//! What a feed runs with: a way to publish, the topics clients want, and the live config.

use std::collections::BTreeSet;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use ghost_proto::ResolvedConfig;
use serde::Serialize;
use serde_json::Value;
use tokio::sync::{broadcast, watch};

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

/// A client's request, already checked to name a method in [`ghost_proto::calls::ALL`]; its
/// arguments are for the feed that handles it to check.
#[derive(Debug, Clone)]
pub struct Call {
    pub method: String,
    pub args: Value,
}

/// How many calls a feed may leave unread before it loses the oldest.
pub(super) const CALL_BACKLOG: usize = 64;

/// The union of every UI client's subscriptions, sent only when it changes.
///
/// Never hold a `borrow()` of it across [`Publisher::publish`]: the hub updates the demand while
/// holding the lock that publishing takes, so the two would deadlock.
pub type Demand = watch::Receiver<BTreeSet<String>>;

pub struct FeedContext {
    pub publisher: Publisher,
    pub demand: Demand,
    pub config: watch::Receiver<ResolvedConfig>,
    /// What UI clients ask the daemon to do. A feed that lags behind loses the oldest calls.
    pub calls: broadcast::Receiver<Call>,
}

type FeedTask = Pin<Box<dyn Future<Output = ()> + Send>>;

/// A built-in source that publishes from an async task instead of being sampled every tick.
///
/// Its task must never complete: one that has nothing to do publishes its fallback and waits
/// forever, because `serve` stops with [`super::ServeError::Feed`] when a feed ends or panics.
pub struct Feed {
    name: &'static str,
    start: Box<dyn FnOnce(FeedContext) -> FeedTask + Send>,
}

impl Feed {
    /// `name` identifies the feed in the error `serve` stops with should it end.
    pub fn new<Task>(
        name: &'static str,
        start: impl FnOnce(FeedContext) -> Task + Send + 'static,
    ) -> Self
    where
        Task: Future<Output = ()> + Send + 'static,
    {
        Self {
            name,
            start: Box::new(move |context| Box::pin(start(context))),
        }
    }

    pub(super) fn name(&self) -> &'static str {
        self.name
    }

    /// The feed's task, started with `context`; `serve` runs it once.
    pub fn run(self, context: FeedContext) -> impl Future<Output = ()> + Send + 'static {
        (self.start)(context)
    }
}
