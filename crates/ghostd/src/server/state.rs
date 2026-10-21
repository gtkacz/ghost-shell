use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use ghost_proto::{PROTO_VERSION, ServerMsg, data_msg, is_valid_topic, topics};
use serde_json::Value;
use tokio::sync::{mpsc, oneshot};
use tracing::warn;

use super::sampling::Tick;
use super::{invalid_topic, line};

/// The output a UI client may leave unread before it is disconnected.
const MAX_UNSENT_BYTES: usize = 256 * 1024;
// The keys `Sampler` records into its history for each built-in topic that has one.
const HISTORY_KEYS: [(&str, &[&str]); 2] = [
    (topics::CPU, &["load", "ccd0", "ccd1"]),
    (topics::GPU, &["gpu_load"]),
];

/// The encoded config messages every UI client is greeted with.
pub(super) struct ConfigLines {
    pub(super) config: Arc<str>,
    pub(super) layout: Arc<str>,
    pub(super) layout_errors: Vec<Arc<str>>,
}

#[derive(Default)]
struct Topic {
    last: Option<Arc<str>>,
    seq: u64,
    provider: Option<Provider>,
}

struct Provider {
    id: u64,
    name: String,
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
    config: ConfigLines,
    config_error: Option<Arc<str>>,
    series: BTreeMap<String, Vec<f32>>,
    topics: HashMap<String, Topic>,
    clients: BTreeMap<u64, UiClient>,
    next_id: u64,
}

impl State {
    pub(super) fn new(host: String, step_ms: u32, config: ConfigLines) -> Self {
        Self {
            hello: line(&ServerMsg::Hello {
                proto: PROTO_VERSION,
                host,
            }),
            step_ms,
            config,
            config_error: None,
            series: BTreeMap::new(),
            topics: HashMap::new(),
            clients: BTreeMap::new(),
            next_id: 0,
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
        let greeting: Vec<Arc<str>> = [&self.hello, &self.config.config, &self.config.layout]
            .into_iter()
            .chain(&self.config_error)
            .chain(&self.config.layout_errors)
            .cloned()
            .collect();
        for line in &greeting {
            self.send(id, line);
        }
        (id, released)
    }

    pub(super) fn remove_ui(&mut self, id: u64) {
        self.clients.remove(&id);
    }

    /// Replaces the client's subscriptions, then queues the history of the subscribed built-ins
    /// and the last value of each subscribed topic.
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
            if let Some(last) = self.topics.get(&topic).and_then(|t| t.last.clone())
                && !wanted.contains(&topic)
            {
                replies.push(last);
            }
            wanted.insert(topic);
        }
        let Some(client) = self.clients.get_mut(&id) else {
            return;
        };
        client.topics = wanted;
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
            if let Some(owner) = self.topics.get(topic).and_then(|t| t.provider.as_ref()) {
                return Err(format!(
                    "topic {topic:?} is already provided by {:?}",
                    owner.name
                ));
            }
        }
        let id = self.allocate_id();
        for topic in wanted {
            self.topics.entry(topic.clone()).or_default().provider = Some(Provider {
                id,
                name: name.to_owned(),
            });
        }
        Ok(id)
    }

    /// Frees the provider's topics, which keep their last value for later subscribers.
    pub(super) fn release(&mut self, id: u64, claimed: &[String]) {
        for topic in claimed {
            if let Some(entry) = self.topics.get_mut(topic)
                && entry.provider.as_ref().is_some_and(|owner| owner.id == id)
            {
                entry.provider = None;
            }
        }
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
        let seq = self.next_seq(topics::PERF);
        self.relay(&data_msg(topics::PERF, seq, &tick.perf));
    }

    pub(super) fn set_config(&mut self, lines: ConfigLines) {
        self.config_error = None;
        let outgoing: Vec<Arc<str>> = [&lines.config, &lines.layout]
            .into_iter()
            .chain(&lines.layout_errors)
            .cloned()
            .collect();
        self.config = lines;
        for line in &outgoing {
            self.deliver(line, |_| true);
        }
    }

    pub(super) fn set_config_error(&mut self, line: Arc<str>) {
        self.deliver(&line, |_| true);
        self.config_error = Some(line);
    }

    fn send(&mut self, id: u64, line: &Arc<str>) {
        if let Some(client) = self.clients.get(&id)
            && !client.outbox.push(line)
        {
            self.drop_lagging(id);
        }
    }

    /// Queues `line` for every client `wants` picks, dropping any client that stopped reading.
    fn deliver(&mut self, line: &Arc<str>, wants: impl Fn(&UiClient) -> bool) {
        let lagging: Vec<u64> = self
            .clients
            .iter()
            .filter(|(_, client)| wants(client))
            .filter(|(_, client)| !client.outbox.push(line))
            .map(|(id, _)| *id)
            .collect();
        for id in lagging {
            self.drop_lagging(id);
        }
    }

    fn drop_lagging(&mut self, id: u64) {
        warn!(
            client = id,
            "UI client stopped reading with over {MAX_UNSENT_BYTES} bytes unsent; disconnecting"
        );
        self.clients.remove(&id);
    }
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

    /// Queues `line`, or returns `false` once the client is too far behind or gone.
    #[must_use]
    pub(super) fn push(&self, line: &Arc<str>) -> bool {
        let unsent = self.unsent.fetch_add(line.len(), Ordering::Relaxed) + line.len();
        unsent <= MAX_UNSENT_BYTES && self.lines.send(Arc::clone(line)).is_ok()
    }
}
