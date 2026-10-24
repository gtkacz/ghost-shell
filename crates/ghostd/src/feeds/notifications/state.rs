//! What the feed knows (the history, the records passing through, who serves, what suppresses
//! popups) and how each request and call changes it.
//!
//! Nothing here waits on the disk or on a reply: a `Notify` is answered from the first line of
//! its handler, because the server's dispatcher waits for that answer before it serves anything.

use std::time::Duration;

use ghost_proto::{
    DiskSnapshot, Notification, NotificationServer, Notifications, NotificationsSnapshot,
    WeatherAlerts, WindowsSnapshot, calls, topics,
};
use serde::Deserialize;
use serde_json::Value;
use tokio::sync::{mpsc, oneshot, watch};
use tokio::time::Instant;
use tracing::{debug, warn};
use zbus::Connection;

use super::history::{Excess, History, Incoming, record};
use super::images::ImageData;
use super::server::{self, CLOSED_BY_CALL, DEFAULT_ACTION, DISMISSED, Request};
use super::sources::{Sent, Sources};
use super::store::{Job, Written};
use crate::server::{Call, Publisher, TopicUpdate};

/// Saves are spaced at least this far apart, so a burst of changes is one write.
const SAVE_INTERVAL: Duration = Duration::from_secs(1);
const MS_PER_S: u64 = 1_000;
const SUPPRESSED_DND: &str = "dnd";
const SUPPRESSED_INHIBITED: &str = "inhibited";
const SUPPRESSED_FULLSCREEN: &str = "fullscreen";
const SUPPRESSED_LOCKED: &str = "locked";
const NOT_SUPPRESSED: &str = "";

pub type Clock = Box<dyn Fn() -> u64 + Send>;

/// Where the state sends what it decides.
pub struct Outputs {
    pub publisher: Publisher,
    /// The count of notifications since the lock, while the session is locked.
    pub signals: watch::Sender<Option<u32>>,
}

pub struct State {
    history: History,
    /// Transient records with the moment they stop passing; newest first, in front of the history.
    passing: Vec<(Notification, Instant)>,
    server: NotificationServer,
    inhibited: bool,
    fullscreen: bool,
    locked: bool,
    /// The wall-clock second at which `locked` last became true.
    lock_s: u64,
    published: Option<NotificationsSnapshot>,
    save_at: Option<Instant>,
    last_save: Option<Instant>,
    /// Where the server's signals go; none without a session bus.
    bus: Option<Connection>,
    writer: mpsc::Sender<Job>,
    images: bool,
    publisher: Publisher,
    signals: watch::Sender<Option<u32>>,
    sources: Sources,
    now: Clock,
}

fn id_arg(args: &Value) -> Option<u32> {
    args.get("id")
        .and_then(Value::as_u64)
        .and_then(|id| u32::try_from(id).ok())
}

impl State {
    pub fn new(
        history: History,
        sources: Sources,
        bus: Option<Connection>,
        writer: mpsc::Sender<Job>,
        images: bool,
        outputs: Outputs,
        now: Clock,
    ) -> Self {
        let Outputs { publisher, signals } = outputs;
        Self {
            history,
            passing: Vec::new(),
            server: NotificationServer::None,
            inhibited: false,
            fullscreen: false,
            locked: false,
            lock_s: 0,
            published: None,
            save_at: None,
            last_save: None,
            bus,
            writer,
            images,
            publisher,
            signals,
            sources,
            now,
        }
    }

    fn suppressed(&self) -> &'static str {
        if self.history.dnd() {
            SUPPRESSED_DND
        } else if self.inhibited {
            SUPPRESSED_INHIBITED
        } else if self.fullscreen {
            SUPPRESSED_FULLSCREEN
        } else if self.locked {
            SUPPRESSED_LOCKED
        } else {
            NOT_SUPPRESSED
        }
    }

    fn snapshot(&self) -> NotificationsSnapshot {
        let counts = self.history.counts();
        NotificationsSnapshot {
            server: self.server,
            items: self
                .passing
                .iter()
                .map(|(record, _)| record.clone())
                .chain(self.history.items().iter().cloned())
                .collect(),
            unread: counts.unread,
            unread_crit: counts.unread_crit,
            dnd: self.history.dnd(),
            inhibited: self.inhibited,
            suppressed: self.suppressed().to_owned(),
        }
    }

    /// Publishes the snapshot unless it is the one already out.
    pub fn publish(&mut self) {
        let signals = self.locked.then(|| self.history.unread_since(self.lock_s));
        self.signals.send_if_modified(|current| {
            let changed = *current != signals;
            *current = signals;
            changed
        });
        let snapshot = self.snapshot();
        if self.published.as_ref() != Some(&snapshot) {
            self.publisher.publish(topics::NOTIFICATIONS, &snapshot);
            self.published = Some(snapshot);
        }
    }

    pub fn set_server(&mut self, server: NotificationServer) {
        self.server = server;
        self.publish();
    }

    /// Takes the answer to a source's send.
    pub fn sent(&mut self, sent: Sent) {
        if self.sources.sent(&mut self.history, sent) {
            self.mark_dirty();
        }
    }

    pub fn set_inhibited(&mut self, inhibited: bool) {
        self.inhibited = inhibited;
        self.publish();
    }

    /// Takes the topics that decide whether popups are held back.
    pub fn topic(&mut self, update: &TopicUpdate) {
        match &*update.topic {
            topics::WINDOWS => match WindowsSnapshot::deserialize(&*update.value) {
                Ok(snapshot) => {
                    self.fullscreen = snapshot
                        .windows
                        .iter()
                        .any(|window| window.active && window.fullscreen);
                }
                Err(error) => debug!(%error, "the windows topic is unreadable; keeping fullscreen"),
            },
            topics::SESSION => match update.value.get("locked").and_then(Value::as_bool) {
                Some(locked) => {
                    if locked && !self.locked {
                        self.lock_s = (self.now)() / MS_PER_S;
                    }
                    self.locked = locked;
                }
                None => debug!("the session topic has no `locked`; keeping it"),
            },
            topics::WEATHER_ALERTS => match WeatherAlerts::deserialize(&*update.value) {
                Ok(alerts) => {
                    if self.sources.weather(&mut self.history, &alerts) {
                        self.mark_dirty();
                    }
                }
                Err(error) => debug!(%error, "the weather alerts are unreadable; raising none"),
            },
            topics::DISKS => match DiskSnapshot::deserialize(&*update.value) {
                Ok(disks) => {
                    if self.sources.disks(&mut self.history, &disks) {
                        self.mark_dirty();
                    }
                }
                Err(error) => debug!(%error, "the disks topic is unreadable; raising none"),
            },
            _ => return,
        }
        self.publish();
    }
    pub async fn request(&mut self, request: Request, settings: &Notifications) {
        match request {
            Request::Notify {
                incoming,
                image_data,
                reply,
            } => self.notify(incoming, image_data, reply, settings),
            Request::Close { id } => {
                debug!(id, "CloseNotification");
                self.close(id, CLOSED_BY_CALL).await;
            }
        }
    }

    fn notify(
        &mut self,
        incoming: Incoming,
        image_data: Option<ImageData>,
        reply: oneshot::Sender<Result<u32, Excess>>,
        settings: &Notifications,
    ) {
        let held = !self.suppressed().is_empty();
        let passing = incoming.transient.then(|| incoming.clone());
        let now_ms = (self.now)();
        let limit = usize::try_from(settings.history_limit).unwrap_or(usize::MAX);
        let outcome = self.history.notify(incoming, now_ms, held, limit);
        // The caller is gone only if it left while waiting, and then there is nobody to tell.
        let _ = reply.send(outcome.map(|outcome| outcome.id));
        let Ok(outcome) = outcome else {
            debug!("a repeated notification was refused");
            return;
        };
        if outcome.kept {
            self.mark_dirty();
        } else if let Some(incoming) = passing {
            let lifetime = Duration::from_secs(settings.popup_timeout_s.into());
            let record = record(outcome.id, incoming, now_ms / MS_PER_S, held);
            self.passing.insert(0, (record, Instant::now() + lifetime));
        }
        if let Some(image) = image_data {
            self.queue_image(outcome.id, image);
        }
        self.publish();
    }

    fn queue_image(&mut self, id: u32, image: ImageData) {
        if self.images && self.writer.try_send(Job::Image { id, image }).is_err() {
            warn!(
                id,
                "too many notification images are waiting; this one is dropped"
            );
        }
    }

    /// A picture that is now a file replaces the record's `image`.
    pub fn image_written(&mut self, written: Written) {
        let path = written.path.to_string_lossy().into_owned();
        if self.history.set_image(written.id, path.clone()) {
            self.mark_dirty();
        }
        for (record, _) in self.passing.iter_mut().filter(|(r, _)| r.id == written.id) {
            record.image.clone_from(&path);
        }
        self.publish();
    }

    fn find(&self, id: u32) -> Option<&Notification> {
        self.history
            .get(id)
            .or_else(|| self.passing.iter().map(|(r, _)| r).find(|r| r.id == id))
    }

    /// Removes the record wherever it is and tells the sender why; whether it was there.
    async fn close(&mut self, id: u32, reason: u32) -> bool {
        let in_history = self.history.close(id);
        let before = self.passing.len();
        self.passing.retain(|(record, _)| record.id != id);
        if !in_history && self.passing.len() == before {
            debug!(id, "close of an unknown notification");
            return false;
        }
        if in_history {
            self.mark_dirty();
        }
        self.publish();
        emit_closed(self.signal_bus(), id, reason).await;
        true
    }
}

impl State {
    pub async fn call(&mut self, call: Call) {
        if !call.method.starts_with("notifications.") {
            return;
        }
        debug!("call {}", call.method);
        let args = &call.args;
        match call.method.as_str() {
            calls::NOTIFICATIONS_INVOKE => self.invoke(args).await,
            calls::NOTIFICATIONS_DISMISS => match id_arg(args) {
                Some(id) => {
                    self.close(id, DISMISSED).await;
                }
                None => debug!("notifications.dismiss needs an id"),
            },
            calls::NOTIFICATIONS_CLEAR => self.clear().await,
            calls::NOTIFICATIONS_READ => self.read(args),
            calls::NOTIFICATIONS_DND => match args.get("on").and_then(Value::as_bool) {
                Some(on) => self.set_dnd(on),
                None => debug!("notifications.dnd needs `on`"),
            },
            _ => {}
        }
    }
}

impl State {
    /// Only for a key the record offered, as an app that has gone cannot be told of anything else.
    async fn invoke(&mut self, args: &Value) {
        let (Some(id), Some(key)) = (id_arg(args), args.get("key").and_then(Value::as_str)) else {
            debug!("notifications.invoke needs an id and a key");
            return;
        };
        let Some(record) = self.find(id) else {
            debug!(id, "invoke of an unknown notification");
            return;
        };
        let offered = if key == DEFAULT_ACTION {
            record.has_default
        } else {
            record.actions.iter().any(|action| action.key == key)
        };
        if !offered {
            debug!(
                id,
                key, "invoke of an action the notification never offered"
            );
            return;
        }
        let resident = record.resident;
        emit_action_invoked(self.signal_bus(), id, key).await;
        if !resident {
            self.close(id, DISMISSED).await;
        }
    }

    async fn clear(&mut self) {
        let before: Vec<u32> = self.history.items().iter().map(|item| item.id).collect();
        self.history.clear();
        let removed: Vec<u32> = before
            .into_iter()
            .filter(|&id| self.history.get(id).is_none())
            .collect();
        if removed.is_empty() {
            return;
        }
        self.mark_dirty();
        self.publish();
        for id in removed {
            emit_closed(self.signal_bus(), id, DISMISSED).await;
        }
    }

    fn read(&mut self, args: &Value) {
        let before = self.history.counts();
        if args.get("all").and_then(Value::as_bool) == Some(true) {
            self.history.mark_read(None);
        } else {
            let ids = args.get("ids").and_then(Value::as_array).and_then(|ids| {
                ids.iter()
                    .map(|id| id.as_u64().and_then(|id| u32::try_from(id).ok()))
                    .collect::<Option<Vec<u32>>>()
            });
            let Some(ids) = ids else {
                debug!("notifications.read needs `ids` or `all`");
                return;
            };
            self.history.mark_read(Some(&ids));
        }
        if self.history.counts() != before {
            self.mark_dirty();
            self.publish();
        }
    }

    fn set_dnd(&mut self, on: bool) {
        if self.history.dnd() != on {
            self.history.set_dnd(on);
            self.mark_dirty();
            self.publish();
        }
    }

    /// Signals are the server's: with another server on the name they would reach clients that
    /// never sent the notification.
    fn signal_bus(&self) -> Option<Connection> {
        self.bus
            .clone()
            .filter(|_| self.server == NotificationServer::GhostShell)
    }
}

// Free functions over an owned connection: a method would keep `&State` across the await, and the
// clock inside it is not `Sync`.
async fn emit_closed(bus: Option<Connection>, id: u32, reason: u32) {
    let Some(bus) = bus else {
        return;
    };
    if let Err(error) = server::emit_closed(&bus, id, reason).await {
        debug!(%error, id, "cannot send NotificationClosed");
    }
}

async fn emit_action_invoked(bus: Option<Connection>, id: u32, key: &str) {
    let Some(bus) = bus else {
        return;
    };
    if let Err(error) = server::emit_action_invoked(&bus, id, key).await {
        debug!(%error, id, "cannot send ActionInvoked");
    }
}

impl State {
    fn mark_dirty(&mut self) {
        if self.save_at.is_none() {
            let now = Instant::now();
            self.save_at = Some(
                self.last_save
                    .map_or(now, |at| (at + SAVE_INTERVAL).max(now)),
            );
        }
    }

    pub fn save_deadline(&self) -> Option<Instant> {
        self.save_at
    }

    /// Hands the history to the writer; the pictures of records still passing are kept too.
    pub fn save(&mut self) {
        let now = Instant::now();
        self.save_at = None;
        self.last_save = Some(now);
        let keep = self
            .history
            .items()
            .iter()
            .chain(self.passing.iter().map(|(record, _)| record))
            .map(|record| record.id)
            .collect();
        let job = Job::Save {
            json: self.history.to_json(),
            keep,
        };
        if self.writer.try_send(job).is_err() {
            warn!("the writer is behind; the history is saved again in a moment");
            self.save_at = Some(now + SAVE_INTERVAL);
        }
    }

    pub fn next_expiry(&self) -> Option<Instant> {
        self.passing.iter().map(|(_, until)| *until).min()
    }

    /// Drops the records whose time of passing is over.
    pub fn expire(&mut self) {
        let now = Instant::now();
        let before = self.passing.len();
        self.passing.retain(|(_, until)| *until > now);
        if self.passing.len() != before {
            self.publish();
        }
    }
}
