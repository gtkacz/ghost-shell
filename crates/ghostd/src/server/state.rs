use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::iter;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use ghost_proto::{PROTO_VERSION, ServerMsg, data_msg, is_valid_topic, topics};
use serde_json::Value;
use tokio::sync::{mpsc, oneshot, watch};
use tracing::{debug, warn};

use super::sampling::Tick;
use super::{Demand, invalid_topic, line};
use crate::sampler::HISTORY_KEYS;

/// The output a UI client may leave unread before it is disconnected.
const MAX_UNSENT_BYTES: usize = 256 * 1024;

/// The encoded `Layout` message and one `ConfigError` per layout problem.
pub(super) struct LayoutLines {
    pub(super) layout: Arc<str>,
    pub(super) errors: Vec<Arc<str>>,
}

#[derive(Default)]
struct Topic {
    last: Option<Arc<str>>,
    seq: u64,
    claim: Claim,
}

/// Where a topic stands with providers, which decides whether it has a live state at all.
#[derive(Default)]
enum Claim {
    /// No provider has claimed it: a built-in, or a topic only fed or subscribed to so far.
    #[default]
    Never,
    Held(Provider),
    /// Its provider is gone, so its last value is no longer current.
    Released,
}

impl Claim {
    /// Whether a provider publishes the topic, or `None` for one no provider ever claimed.
    fn live(&self) -> Option<bool> {
        match self {
            Self::Never => None,
            Self::Held(_) => Some(true),
            Self::Released => Some(false),
        }
    }
}

struct Provider {
    id: u64,
    name: String,
}

fn topic_state(topic: &str, live: bool) -> Arc<str> {
    line(&ServerMsg::TopicState {
        topic: topic.to_owned(),
        live,
    })
}

struct UiClient {
    outbox: Outbox,
    topics: HashSet<String>,
    pid: Option<u32>,
    // Dropped with the client, which tells its session the hub let it go.
    _registration: oneshot::Sender<()>,
}

/// The hub's shared state. Its lock is never held across an await.
pub(super) struct State {
    hello: Arc<str>,
    step_ms: u32,
    config: Arc<str>,
    layout: LayoutLines,
    config_error: Option<Arc<str>>,
    series: BTreeMap<String, Vec<Option<f32>>>,
    topics: HashMap<String, Topic>,
    clients: BTreeMap<u64, UiClient>,
    next_id: u64,
    demand: watch::Sender<BTreeSet<String>>,
}

impl State {
    pub(super) fn new(host: String, step_ms: u32, config: Arc<str>, layout: LayoutLines) -> Self {
        Self {
            hello: line(&ServerMsg::Hello {
                proto: PROTO_VERSION,
                host,
            }),
            step_ms,
            config,
            layout,
            config_error: None,
            series: BTreeMap::new(),
            topics: HashMap::new(),
            clients: BTreeMap::new(),
            next_id: 0,
            demand: watch::Sender::new(BTreeSet::new()),
        }
    }

    fn allocate_id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }

    /// Registers a UI client and queues its greeting; the receiver resolves once the hub drops it.
    pub(super) fn register_ui(
        &mut self,
        outbox: Outbox,
        pid: Option<u32>,
    ) -> (u64, oneshot::Receiver<()>) {
        let id = self.allocate_id();
        let (registration, released) = oneshot::channel();
        self.clients.insert(
            id,
            UiClient {
                outbox,
                topics: HashSet::new(),
                pid,
                _registration: registration,
            },
        );
        let greeting: Vec<Arc<str>> = [&self.hello, &self.config, &self.layout.layout]
            .into_iter()
            .chain(&self.config_error)
            .chain(&self.layout.errors)
            .cloned()
            .collect();
        for line in &greeting {
            self.send(id, line);
        }
        (id, released)
    }

    pub(super) fn remove_ui(&mut self, id: u64) {
        self.clients.remove(&id);
        self.update_demand();
    }

    /// A receiver of the union of every UI client's subscriptions.
    pub(super) fn demand(&self) -> Demand {
        self.demand.subscribe()
    }

    /// Recomputes the union of every client's subscriptions, notifying only when it changed.
    fn update_demand(&mut self) {
        let union: BTreeSet<String> = self
            .clients
            .values()
            .flat_map(|client| client.topics.iter().cloned())
            .collect();
        self.demand.send_if_modified(|current| {
            let changed = *current != union;
            if changed {
                *current = union;
            }
            changed
        });
    }

    /// Replaces the client's subscriptions, then queues the history of the subscribed built-ins
    /// and the last value of each subscribed topic, followed by its state if a provider ever
    /// claimed it.
    pub(super) fn subscribe(&mut self, id: u64, topics: Vec<String>) {
        let mut wanted = HashSet::new();
        let mut replies = Vec::new();
        let with_history: Vec<&[&str]> = HISTORY_KEYS
            .iter()
            .filter(|(topic, _)| topics.iter().any(|wanted| wanted == topic))
            .map(|(_, keys)| *keys)
            .collect();
        if !with_history.is_empty() {
            let series = with_history
                .into_iter()
                .flatten()
                .filter_map(|&key| Some((key.to_owned(), self.series.get(key)?.clone())))
                .collect();
            replies.push(line(&ServerMsg::History {
                step_ms: self.step_ms,
                series,
            }));
        }
        for topic in topics {
            if !wanted.contains(&topic)
                && let Some(entry) = self.topics.get(&topic)
            {
                replies.extend(entry.last.clone());
                replies.extend(entry.claim.live().map(|live| topic_state(&topic, live)));
            }
            wanted.insert(topic);
        }
        let Some(client) = self.clients.get_mut(&id) else {
            return;
        };
        client.topics = wanted;
        self.update_demand();
        for reply in &replies {
            self.send(id, reply);
        }
    }

    /// The pid of the longest-connected UI client subscribed to `perf`, taken to be the shell.
    pub(super) fn perf_pid(&self) -> Option<u32> {
        self.clients
            .values()
            .filter(|client| client.topics.contains(topics::PERF))
            .find_map(|client| client.pid)
    }

    pub(super) fn claim(&mut self, name: &str, wanted: &[String]) -> Result<u64, String> {
        for topic in wanted {
            if !is_valid_topic(topic) {
                return Err(invalid_topic(topic));
            }
            if topics::BUILTIN.contains(&topic.as_str()) {
                return Err(format!("topic {topic:?} is built in"));
            }
            if let Some(Claim::Held(owner)) = self.topics.get(topic).map(|t| &t.claim) {
                return Err(format!(
                    "topic {topic:?} is already provided by {:?}",
                    owner.name
                ));
            }
        }
        let id = self.allocate_id();
        for topic in wanted {
            self.topics.entry(topic.clone()).or_default().claim = Claim::Held(Provider {
                id,
                name: name.to_owned(),
            });
            self.announce(topic, true);
        }
        Ok(id)
    }

    /// Frees the provider's topics, which keep their last value for later subscribers but are no
    /// longer live.
    pub(super) fn release(&mut self, id: u64, claimed: &[String]) {
        for topic in claimed {
            if let Some(entry) = self.topics.get_mut(topic)
                && matches!(&entry.claim, Claim::Held(owner) if owner.id == id)
            {
                entry.claim = Claim::Released;
                self.announce(topic, false);
            }
        }
    }

    /// Tells the topic's subscribers whether a provider publishes it now.
    fn announce(&mut self, topic: &str, live: bool) {
        let line = topic_state(topic, live);
        self.deliver(&line, |client| client.topics.contains(topic));
    }

    /// Wraps `data` in the topic's next `Data` message, then caches and relays it.
    pub(super) fn publish(&mut self, topic: &str, data: Value) {
        let seq = self.next_seq(topic);
        self.relay(&ServerMsg::Data {
            topic: topic.to_owned(),
            seq,
            data,
        });
    }

    fn next_seq(&mut self, topic: &str) -> u64 {
        let entry = self.topics.entry(topic.to_owned()).or_default();
        entry.seq += 1;
        entry.seq
    }

    /// Caches a `Data` message as its topic's last value and queues it for the topic's subscribers.
    fn relay(&mut self, message: &ServerMsg) {
        let ServerMsg::Data { topic, .. } = message else {
            return;
        };
        let line = line(message);
        self.topics.entry(topic.clone()).or_default().last = Some(Arc::clone(&line));
        self.deliver(&line, |client| client.topics.contains(topic));
    }

    pub(super) fn record_tick(&mut self, tick: Tick) {
        self.series = tick.series;
        for message in &tick.messages {
            self.relay(message);
        }
        if let Some(perf) = &tick.perf {
            let seq = self.next_seq(topics::PERF);
            self.relay(&data_msg(topics::PERF, seq, perf));
        }
    }

    /// Sends a newly loaded config and its layout to every UI client.
    pub(super) fn set_config(&mut self, config: Arc<str>, layout: LayoutLines) {
        self.config_error = None;
        self.deliver(&config, |_| true);
        self.config = config;
        self.set_layout(layout);
    }

    /// Sends a layout resolved anew, after a config load or a change in the widget dirs.
    pub(super) fn set_layout(&mut self, layout: LayoutLines) {
        let outgoing: Vec<Arc<str>> = iter::once(&layout.layout)
            .chain(&layout.errors)
            .cloned()
            .collect();
        self.layout = layout;
        for line in &outgoing {
            self.deliver(line, |_| true);
        }
    }

    pub(super) fn set_config_error(&mut self, line: Arc<str>) {
        self.deliver(&line, |_| true);
        self.config_error = Some(line);
    }

    fn send(&mut self, id: u64, line: &Arc<str>) {
        let undelivered = self
            .clients
            .get(&id)
            .and_then(|client| client.outbox.push(line).err());
        if let Some(reason) = undelivered {
            self.drop_client(id, reason);
        }
    }

    /// Queues `line` for every client `wants` picks, dropping any client it cannot reach.
    fn deliver(&mut self, line: &Arc<str>, wants: impl Fn(&UiClient) -> bool) {
        let undelivered: Vec<(u64, Undelivered)> = self
            .clients
            .iter()
            .filter(|(_, client)| wants(client))
            .filter_map(|(id, client)| Some((*id, client.outbox.push(line).err()?)))
            .collect();
        for (id, reason) in undelivered {
            self.drop_client(id, reason);
        }
    }

    fn drop_client(&mut self, id: u64, reason: Undelivered) {
        match reason {
            Undelivered::Behind => warn!(
                client = id,
                "UI client stopped reading with over {MAX_UNSENT_BYTES} bytes unsent; disconnecting"
            ),
            Undelivered::Gone => debug!(client = id, "UI client connection already closed"),
        }
        self.clients.remove(&id);
        self.update_demand();
    }
}

/// Why a line could not be queued for a UI client.
#[derive(Debug, Clone, Copy)]
pub(super) enum Undelivered {
    /// More than [`MAX_UNSENT_BYTES`] would be waiting to be written.
    Behind,
    /// The writer has stopped, because the socket failed or the session ended.
    Gone,
}

/// A UI client's outgoing queue, which counts the bytes not yet written to its socket.
#[derive(Clone)]
pub(super) struct Outbox {
    lines: mpsc::UnboundedSender<Arc<str>>,
    pub(super) unsent: Arc<AtomicUsize>,
}

impl Outbox {
    pub(super) fn new() -> (Self, mpsc::UnboundedReceiver<Arc<str>>) {
        let (lines, queued) = mpsc::unbounded_channel();
        let outbox = Self {
            lines,
            unsent: Arc::new(AtomicUsize::new(0)),
        };
        (outbox, queued)
    }

    /// Queues `line` unless the client is too far behind or its writer has stopped.
    pub(super) fn push(&self, line: &Arc<str>) -> Result<(), Undelivered> {
        let unsent = self.unsent.fetch_add(line.len(), Ordering::Relaxed) + line.len();
        if unsent > MAX_UNSENT_BYTES {
            return Err(Undelivered::Behind);
        }
        self.lines
            .send(Arc::clone(line))
            .map_err(|_closed| Undelivered::Gone)
    }
}
