//! `org.freedesktop.Notifications`, served from ghostd: the calls become [`Request`]s for the feed,
//! and the bus name is claimed in a way that queues behind a running plasmashell.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use ghost_proto::{NotificationAction, NotificationLevel, Urgency};
use tokio::sync::{mpsc, oneshot, watch};
use tokio::time::timeout;
use tracing::debug;
use zbus::OwnedVariant;
use zbus::fdo::{self, NameOwnerChangedStream, RequestNameFlags, RequestNameReply};
use zbus::message::Header;
use zbus::names::BusName;
use zbus::object_server::SignalEmitter;
use zbus::proxy::CacheProperties;
use zbus::{Connection, interface};

use super::history::{Excess, Incoming};
use super::images::ImageData;
use super::inhibit::Inhibitors;
use crate::feeds::next;

pub const NAME: &str = "org.freedesktop.Notifications";
pub const PATH: &str = "/org/freedesktop/Notifications";
pub const EXPIRED: u32 = 1;
pub const DISMISSED: u32 = 2;
pub const CLOSED_BY_CALL: u32 = 3;

const CAPABILITIES: [&str; 7] = [
    "body",
    "body-markup",
    "body-hyperlinks",
    "actions",
    "persistence",
    "icon-static",
    "inhibitions",
];
const SERVER_NAME: &str = "ghost-shell";
const SPEC_VERSION: &str = "1.2";
pub const DEFAULT_ACTION: &str = "default";
const URGENCY_LOW: u8 = 0;
const URGENCY_CRITICAL: u8 = 2;
const WARN_LEVEL: &str = "warn";
const IMAGE_DATA_HINTS: [&str; 3] = ["image-data", "image_data", "icon_data"];
const IMAGE_PATH_HINTS: [&str; 2] = ["image-path", "image_path"];
const UNIQUE_NAME_PREFIX: char = ':';
/// The bus answers these in microseconds; the limit only keeps a wedged bus from stalling every call.
const BUS_QUERY_TIMEOUT: Duration = Duration::from_millis(500);

/// What the server asks of the feed, which owns the history.
// A call is a rare event and the queue is short, so boxing the one big variant buys nothing.
#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum Request {
    /// `image_data` is not yet a file: the id the PNG is named after comes from the reply.
    Notify {
        incoming: Incoming,
        image_data: Option<ImageData>,
        reply: oneshot::Sender<Result<u32, Excess>>,
    },
    Close {
        id: u32,
    },
}

#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "org.freedesktop.Notifications.Error")]
pub enum NotifyError {
    #[zbus(error)]
    ZBus(zbus::Error),
    ExcessNotificationGeneration(String),
}

/// The feed is gone, which only happens while the daemon stops.
fn unavailable<E>(_: E) -> fdo::Error {
    fdo::Error::Failed("ghostd is shutting down".to_owned())
}

pub struct Server {
    inbox: mpsc::Sender<Request>,
    inhibitors: Arc<Mutex<Inhibitors>>,
    // On a connection of its own: calls are served in order on the server's connection, whose
    // handler would otherwise wait for a reply that only its own dispatcher can read.
    bus: fdo::DBusProxy<'static>,
}

impl Server {
    fn inhibitors(&self) -> MutexGuard<'_, Inhibitors> {
        lock(&self.inhibitors)
    }

    /// Whether the caller is still on the bus; true when the bus cannot say, since the departure
    /// watcher is then the one to release it.
    async fn caller_present(&self, who: &str) -> bool {
        let Ok(name) = BusName::try_from(who) else {
            return true;
        };
        timeout(BUS_QUERY_TIMEOUT, self.bus.name_has_owner(name))
            .await
            .map_or(true, |reply| reply.unwrap_or(true))
    }
}

fn lock(inhibitors: &Mutex<Inhibitors>) -> MutexGuard<'_, Inhibitors> {
    inhibitors.lock().unwrap_or_else(PoisonError::into_inner)
}

fn hint_str<'a>(hints: &'a HashMap<String, OwnedVariant>, key: &str) -> Option<&'a str> {
    hints.get(key)?.downcast_ref::<&str>().ok()
}

fn hint_bool(hints: &HashMap<String, OwnedVariant>, key: &str) -> bool {
    hints
        .get(key)
        .and_then(|value| value.downcast_ref::<bool>().ok())
        .unwrap_or(false)
}

fn urgency(hints: &HashMap<String, OwnedVariant>) -> Urgency {
    match hints
        .get("urgency")
        .and_then(|value| value.downcast_ref::<u8>().ok())
    {
        Some(URGENCY_LOW) => Urgency::Low,
        Some(URGENCY_CRITICAL) => Urgency::Critical,
        _ => Urgency::Normal,
    }
}

struct Call<'a> {
    app_name: &'a str,
    replaces_id: u32,
    app_icon: &'a str,
    summary: &'a str,
    body: &'a str,
    actions: &'a [String],
    hints: &'a HashMap<String, OwnedVariant>,
}

/// Everything about a call that needs no bus; the pid and the picture's file come later.
fn parse(call: &Call<'_>) -> (Incoming, Option<ImageData>) {
    let hints = call.hints;
    let desktop_entry = hint_str(hints, "desktop-entry").unwrap_or_default();
    let urgency = urgency(hints);
    let level = if urgency == Urgency::Critical {
        NotificationLevel::Crit
    } else if hint_str(hints, "x-ghost-shell-level") == Some(WARN_LEVEL) {
        NotificationLevel::Warn
    } else {
        NotificationLevel::None
    };
    let mut has_default = false;
    let mut actions = Vec::new();
    let (pairs, _dangling) = call.actions.as_chunks::<2>();
    for pair in pairs {
        if pair[0] == DEFAULT_ACTION {
            has_default = true;
        } else {
            actions.push(NotificationAction {
                key: pair[0].clone(),
                label: pair[1].clone(),
            });
        }
    }
    let (body, body_text) = super::markup::sanitise(call.body);
    let image_data = IMAGE_DATA_HINTS
        .iter()
        .find_map(|key| ImageData::from_variant(hints.get(*key)?));
    let image = IMAGE_PATH_HINTS
        .iter()
        .find_map(|key| hint_str(hints, key))
        .unwrap_or_default();
    let incoming = Incoming {
        app: if call.app_name.is_empty() {
            desktop_entry.to_owned()
        } else {
            call.app_name.to_owned()
        },
        desktop_entry: desktop_entry.to_owned(),
        app_icon: call.app_icon.to_owned(),
        image: image.to_owned(),
        summary: call.summary.to_owned(),
        body,
        body_text,
        urgency,
        level,
        actions,
        has_default,
        pid: 0,
        resident: hint_bool(hints, "resident"),
        transient: hint_bool(hints, "transient"),
        replaces_id: call.replaces_id,
    };
    (incoming, image_data)
}

/// The caller's process id, or 0 when the bus cannot say.
async fn sender_pid(bus: &fdo::DBusProxy<'_>, header: &Header<'_>) -> u32 {
    let Some(sender) = header.sender() else {
        return 0;
    };
    let lookup = bus.get_connection_unix_process_id(BusName::from(sender.to_owned()));
    timeout(BUS_QUERY_TIMEOUT, lookup)
        .await
        .map_or(0, |pid| pid.unwrap_or(0))
}

// Calls are served one at a time in the order the bus delivered them: a client that replaces a
// notification without awaiting each reply (a progress bar) must see its last state win.
#[interface(name = "org.freedesktop.Notifications", spawn = false)]
impl Server {
    #[allow(clippy::too_many_arguments)]
    async fn notify(
        &self,
        app_name: String,
        replaces_id: u32,
        app_icon: String,
        summary: String,
        body: String,
        actions: Vec<String>,
        hints: HashMap<String, OwnedVariant>,
        // The app's own timeout is ignored: the shell decides how long a toast stays.
        _expire_timeout: i32,
        #[zbus(header)] header: Header<'_>,
    ) -> Result<u32, NotifyError> {
        let (mut incoming, image_data) = parse(&Call {
            app_name: &app_name,
            replaces_id,
            app_icon: &app_icon,
            summary: &summary,
            body: &body,
            actions: &actions,
            hints: &hints,
        });
        incoming.pid = sender_pid(&self.bus, &header).await;
        let (reply, answer) = oneshot::channel();
        self.inbox
            .send(Request::Notify {
                incoming,
                image_data,
                reply,
            })
            .await
            .map_err(|error| NotifyError::ZBus(unavailable(error).into()))?;
        match answer.await {
            Ok(Ok(id)) => Ok(id),
            Ok(Err(Excess)) => Err(NotifyError::ExcessNotificationGeneration(
                "the same notification was sent again within a second".to_owned(),
            )),
            Err(error) => Err(NotifyError::ZBus(unavailable(error).into())),
        }
    }

    async fn close_notification(&self, id: u32) -> fdo::Result<()> {
        self.inbox
            .send(Request::Close { id })
            .await
            .map_err(unavailable)
    }

    fn get_capabilities(&self) -> Vec<String> {
        CAPABILITIES.iter().map(|&name| name.to_owned()).collect()
    }

    fn get_server_information(&self) -> (String, String, String, String) {
        (
            SERVER_NAME.to_owned(),
            SERVER_NAME.to_owned(),
            env!("CARGO_PKG_VERSION").to_owned(),
            SPEC_VERSION.to_owned(),
        )
    }

    async fn inhibit(
        &self,
        _desktop_entry: String,
        _reason: String,
        _hints: HashMap<String, OwnedVariant>,
        #[zbus(header)] header: Header<'_>,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> fdo::Result<u32> {
        let who = header.sender().map(ToString::to_string).unwrap_or_default();
        let (cookie, was_active) = {
            let mut inhibitors = self.inhibitors();
            let was_active = inhibitors.active();
            (inhibitors.add(&who), was_active)
        };
        // After the insert, so a departure the watcher already handled cannot be missed.
        if !self.caller_present(&who).await {
            self.inhibitors().release(&who);
        }
        if self.inhibitors().active() != was_active {
            self.inhibited_changed(&emitter).await?;
        }
        Ok(cookie)
    }

    async fn un_inhibit(
        &self,
        cookie: u32,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> fdo::Result<()> {
        let (removed, now_active) = {
            let mut inhibitors = self.inhibitors();
            (inhibitors.remove(cookie), inhibitors.active())
        };
        if !removed {
            debug!(cookie, "UnInhibit of an unknown cookie");
            return Ok(());
        }
        if !now_active {
            self.inhibited_changed(&emitter).await?;
        }
        Ok(())
    }

    #[zbus(property)]
    fn inhibited(&self) -> bool {
        self.inhibitors().active()
    }

    #[zbus(signal)]
    async fn notification_closed(
        emitter: &SignalEmitter<'_>,
        id: u32,
        reason: u32,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn action_invoked(emitter: &SignalEmitter<'_>, id: u32, key: &str) -> zbus::Result<()>;
}

pub async fn emit_closed(bus: &Connection, id: u32, reason: u32) -> zbus::Result<()> {
    Server::notification_closed(&SignalEmitter::new(bus, PATH)?, id, reason).await
}

pub async fn emit_action_invoked(bus: &Connection, id: u32, key: &str) -> zbus::Result<()> {
    Server::action_invoked(&SignalEmitter::new(bus, PATH)?, id, key).await
}

/// Registers the interface at its path, before any name request, so that no call arrives at a
/// name that serves nothing. Also releases the cookies of callers that leave the bus. The receiver
/// reads whether any inhibition is held. `helper` is a second connection to the same bus, for the
/// questions the handlers ask the bus.
pub async fn serve(
    bus: &Connection,
    helper: &Connection,
    inbox: mpsc::Sender<Request>,
) -> zbus::Result<watch::Receiver<bool>> {
    let (inhibitors, inhibited) = Inhibitors::new();
    let inhibitors = Arc::new(Mutex::new(inhibitors));
    let server = Server {
        inbox,
        inhibitors: Arc::clone(&inhibitors),
        bus: fdo::DBusProxy::builder(helper)
            .cache_properties(CacheProperties::No)
            .build()
            .await?,
    };
    bus.object_server().at(PATH, server).await?;
    let departures = fdo::DBusProxy::builder(bus)
        .cache_properties(CacheProperties::No)
        .build()
        .await?
        .receive_name_owner_changed()
        .await?;
    tokio::spawn(release_departed(bus.clone(), departures, inhibitors));
    Ok(inhibited)
}

async fn release_departed(
    bus: Connection,
    mut changes: NameOwnerChangedStream,
    inhibitors: Arc<Mutex<Inhibitors>>,
) {
    while let Some(change) = next(&mut changes).await {
        let Ok(args) = change.args() else {
            continue;
        };
        let unique_name = args.name().to_string();
        if args.new_owner().is_some() || !unique_name.starts_with(UNIQUE_NAME_PREFIX) {
            continue;
        }
        let (released, now_active) = {
            let mut inhibitors = lock(&inhibitors);
            (inhibitors.release(&unique_name), inhibitors.active())
        };
        if !released {
            continue;
        }
        if !now_active && let Ok(interface) = bus.object_server().interface::<_, Server>(PATH).await
        {
            let server = interface.get().await;
            if let Err(error) = server.inhibited_changed(interface.signal_emitter()).await {
                debug!(%error, "cannot announce that inhibition ended");
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Claim {
    Owned,
    /// Another connection holds the name; ghostd takes it over when that one leaves.
    Queued {
        owner: String,
    },
    /// No marker, so ghostd leaves the name to whoever serves it.
    NotRequested,
}

/// Asks for the name with `REPLACE_EXISTING` alone: a holder that does not allow replacement keeps
/// it, ghostd waits in its queue, and inherits the name as soon as the holder's connection drops.
pub async fn claim(bus: &Connection) -> zbus::Result<Claim> {
    match bus
        .request_name_with_flags(NAME, RequestNameFlags::REPLACE_EXISTING)
        .await?
    {
        RequestNameReply::PrimaryOwner | RequestNameReply::AlreadyOwner => Ok(Claim::Owned),
        RequestNameReply::InQueue => {
            // Best effort: the holder may have left between the reply and this question.
            let owner = match fdo::DBusProxy::new(bus).await {
                Ok(proxy) => proxy
                    .get_name_owner(NAME.try_into()?)
                    .await
                    .map(|owner| owner.to_string())
                    .unwrap_or_default(),
                Err(_) => String::new(),
            };
            Ok(Claim::Queued { owner })
        }
        RequestNameReply::Exists => Err(zbus::Error::NameTaken),
    }
}

pub async fn release(bus: &Connection) -> zbus::Result<()> {
    bus.release_name(NAME).await.map(|_| ())
}
