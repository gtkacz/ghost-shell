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
use crate::history::{has_history, history_topic};

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
    /// Its provider is gone, or is configured but has not claimed it yet, so its last value is
    /// no longer current.
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

    /// Replaces the client's subscriptions, then queues the history of the subscribed topics that
    /// record one and the last value of each subscribed topic, followed by its state if a provider
    /// ever claimed it.
    pub(super) fn subscribe(&mut self, id: u64, topics: Vec<String>) {
        let mut wanted = HashSet::new();
        let mut replies = Vec::new();
        let history = topics.iter().any(|topic| has_history(topic)).then(|| {
            let series = self
                .series
                .iter()
                .filter(|(key, _)| {
                    history_topic(key).is_some_and(|topic| topics.iter().any(|t| t == topic))
                })
                .map(|(key, samples)| (key.clone(), samples.clone()))
                .collect();
            line(&ServerMsg::History {
                step_ms: self.step_ms,
                series,
            })
        });
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
        match history {
            Some(history) => {
                let reply: Vec<Arc<str>> = iter::once(history).chain(replies).collect();
                self.send_by(id, |outbox| outbox.push_history_reply(&reply));
            }
            None => {
                for reply in &replies {
                    self.send(id, reply);
                }
            }
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

    /// Marks topics a configured provider is known to publish as not live until it claims them,
    /// leaving those a provider holds alone. Without this, a client that kept a topic's value
    /// across a daemon restart would take it for live while the provider has not yet come back.
    ///
    /// There is no undoing it: once a config that drops the provider is loaded, the topic stays
    /// not live, since going back to no state would let a kept value read as live again.
    pub(super) fn expect(&mut self, expected: &[String]) {
        for topic in expected {
            let entry = self.topics.entry(topic.clone()).or_default();
            if matches!(entry.claim, Claim::Never) {
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
        self.send_by(id, |outbox| outbox.push(line));
    }

    /// Queues a line for one client through `push`, dropping the client if it cannot be reached.
    fn send_by(&mut self, id: u64, push: impl FnOnce(&Outbox) -> Result<(), Undelivered>) {
        let undelivered = self
            .clients
            .get(&id)
            .and_then(|client| push(&client.outbox).err());
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
        self.send_line(line)
    }

    /// Queues a subscription's reply, its `History` line first, unless the bytes already waiting
    /// are over the cap or the writer has stopped. A long history makes the line longer than the
    /// cap on its own, and the topics' last values are queued behind it before the writer can
    /// drain any of it, so only what waits before the reply is held against the cap. The reply's
    /// own bytes still count toward later pushes.
    pub(super) fn push_history_reply(&self, lines: &[Arc<str>]) -> Result<(), Undelivered> {
        let reply_len = lines.iter().map(|line| line.len()).sum();
        let unsent_before = self.unsent.fetch_add(reply_len, Ordering::Relaxed);
        if unsent_before > MAX_UNSENT_BYTES {
            return Err(Undelivered::Behind);
        }
        for line in lines {
            self.send_line(line)?;
        }
        Ok(())
    }

    fn send_line(&self, line: &Arc<str>) -> Result<(), Undelivered> {
        self.lines
            .send(Arc::clone(line))
            .map_err(|_closed| Undelivered::Gone)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use tokio::sync::oneshot::error::TryRecvError;

    use super::*;

    const GREETING_LINES: usize = 3;
    // `dashboard.history_minutes` at its maximum of 60, at the default one sample a second.
    const FULL_HOUR_SAMPLES: usize = 3600;
    const CORE_COUNT: usize = 16;
    // An f32 reading at the full length it is printed with.
    const TYPICAL_SAMPLE: f32 = 49.999_996;
    const HISTORY_TOPICS: [&str; 6] = [
        topics::CPU,
        topics::GPU,
        topics::MEM,
        topics::SENSORS,
        topics::DISKS,
        "net",
    ];

    fn state() -> State {
        State::new(
            "host".to_owned(),
            1000,
            "config\n".into(),
            LayoutLines {
                layout: "layout\n".into(),
                errors: Vec::new(),
            },
        )
    }

    /// The lines a client subscribing to `topics::WEATHER_ALERTS` now is sent after its greeting.
    fn subscribe_reply(state: &mut State) -> Vec<String> {
        let (outbox, mut queued) = Outbox::new();
        let (id, _released) = state.register_ui(outbox, None);
        state.subscribe(id, vec![topics::WEATHER_ALERTS.to_owned()]);
        let mut lines = Vec::new();
        while let Ok(line) = queued.try_recv() {
            lines.push(line.to_string());
        }
        lines.split_off(GREETING_LINES)
    }

    fn state_line(live: bool) -> String {
        topic_state(topics::WEATHER_ALERTS, live).to_string()
    }

    /// A full hour of every series the daemon records, the network's two among them.
    fn full_hour_of_every_series() -> BTreeMap<String, Vec<Option<f32>>> {
        let named = [
            "load",
            "ccd0",
            "ccd1",
            "gpu_load",
            "temp_gpu",
            "mem_used",
            "temp_cpu",
            "disk_read",
            "disk_write",
            "temp_nvme",
            "net_down",
            "net_up",
        ];
        named
            .into_iter()
            .map(str::to_owned)
            .chain((0..CORE_COUNT).map(|core| format!("core{core}")))
            .map(|key| (key, vec![Some(TYPICAL_SAMPLE); FULL_HOUR_SAMPLES]))
            .collect()
    }

    /// A full hour of history, and a last value for each topic that records one, which a
    /// subscriber is sent right after the history.
    fn state_with_a_full_hour() -> State {
        let mut state = state();
        state.record_tick(Tick {
            messages: HISTORY_TOPICS
                .iter()
                .map(|&topic| data_msg(topic, 1, &json!({"value": TYPICAL_SAMPLE})))
                .collect(),
            perf: None,
            series: full_hour_of_every_series(),
        });
        state
    }

    /// A client whose lines are queued but never written, as when it stops reading.
    struct Unread {
        id: u64,
        queued: mpsc::UnboundedReceiver<Arc<str>>,
        released: oneshot::Receiver<()>,
    }

    impl Unread {
        fn register(state: &mut State) -> Self {
            let (outbox, queued) = Outbox::new();
            let (id, released) = state.register_ui(outbox, None);
            Self {
                id,
                queued,
                released,
            }
        }

        fn subscribe_to_every_history(&self, state: &mut State) {
            let topics = HISTORY_TOPICS
                .iter()
                .map(|&topic| topic.to_owned())
                .collect();
            state.subscribe(self.id, topics);
        }

        fn is_connected(&mut self, state: &State) -> bool {
            let held = state.clients.contains_key(&self.id);
            assert_eq!(
                held,
                matches!(self.released.try_recv(), Err(TryRecvError::Empty)),
                "the hub and the session disagree on whether the client is connected"
            );
            held
        }

        fn lines_after_greeting(&mut self) -> Vec<Arc<str>> {
            let mut lines = Vec::new();
            while let Ok(line) = self.queued.try_recv() {
                lines.push(line);
            }
            lines.split_off(GREETING_LINES)
        }
    }

    #[test]
    fn a_full_hour_of_history_reaches_a_client_with_room_in_its_outbox() {
        let mut state = state_with_a_full_hour();
        let mut client = Unread::register(&mut state);

        client.subscribe_to_every_history(&mut state);

        assert!(client.is_connected(&state));
        let lines = client.lines_after_greeting();
        assert_eq!(
            lines.len(),
            1 + HISTORY_TOPICS.len(),
            "the history and each last value"
        );
        assert!(
            lines[0].len() > MAX_UNSENT_BYTES,
            "the history must not fit the cap on its own, or this proves nothing: {} bytes",
            lines[0].len()
        );
        let history: Value = serde_json::from_str(&lines[0]).unwrap();
        assert_eq!(history["t"], "history");
        let series = history["series"].as_object().unwrap();
        assert_eq!(series.len(), full_hour_of_every_series().len());
        assert!(
            series
                .values()
                .all(|samples| samples.as_array().unwrap().len() == FULL_HOUR_SAMPLES)
        );
    }

    #[test]
    fn a_client_that_stops_reading_after_a_history_is_dropped_on_the_next_push() {
        let mut state = state_with_a_full_hour();
        let mut client = Unread::register(&mut state);
        client.subscribe_to_every_history(&mut state);
        assert!(client.is_connected(&state));

        state.record_tick(Tick {
            messages: vec![data_msg(topics::CPU, 1, &json!({"load": 1.0}))],
            perf: None,
            series: full_hour_of_every_series(),
        });

        assert!(!client.is_connected(&state));
    }

    #[test]
    fn a_history_is_refused_to_an_outbox_already_over_the_cap() {
        let mut state = state_with_a_full_hour();
        let mut client = Unread::register(&mut state);
        client.subscribe_to_every_history(&mut state);
        assert!(client.is_connected(&state));

        client.subscribe_to_every_history(&mut state);

        assert!(!client.is_connected(&state));
    }

    #[test]
    fn an_expected_topic_is_not_live_until_a_provider_claims_it() {
        let mut state = state();

        state.expect(&[topics::WEATHER_ALERTS.to_owned()]);
        assert_eq!(subscribe_reply(&mut state), [state_line(false)]);

        state
            .claim("inmet", &[topics::WEATHER_ALERTS.to_owned()])
            .unwrap();
        assert_eq!(subscribe_reply(&mut state), [state_line(true)]);
    }

    #[test]
    fn expecting_a_claimed_topic_leaves_it_live() {
        let mut state = state();
        state
            .claim("inmet", &[topics::WEATHER_ALERTS.to_owned()])
            .unwrap();

        state.expect(&[topics::WEATHER_ALERTS.to_owned()]);

        assert_eq!(subscribe_reply(&mut state), [state_line(true)]);
    }
}
