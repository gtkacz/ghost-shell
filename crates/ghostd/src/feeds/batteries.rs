//! The `batteries` topic: the peripherals UPower knows, on the system bus, then the paired phones
//! KDE Connect reaches, on the session bus.
//!
//! Nothing runs until some client wants the topic, and everything stops once none does. While it
//! is wanted:
//! - UPower is read in full at once, and again whenever it comes or goes on the bus, adds or
//!   removes a device, or a device's properties change;
//! - KDE Connect is read in full at once, whenever it comes or goes on the bus, and every 60 s by
//!   the wall clock: it declares its battery properties without change signals. Only what the
//!   phone already reported is read; no refresh is ever asked of it, so the phone is never woken.
//!
//! Both services are D-Bus activatable, so every call asks the bus not to start its service: an
//! absent service reads as no batteries and waits for its name to gain an owner.
//!
//! A snapshot is published only when it differs from the last one, and the first one of each
//! wanted period only once both services have answered or failed to, so a phone never pops in a
//! moment after the controller.

use std::collections::BTreeMap;
use std::future::pending;
use std::ops::RangeInclusive;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use ghost_proto::{Batteries, Battery, BatteryKind, ResolvedConfig, topics};
use serde::Serialize;
use tokio::sync::watch;
use tokio::time::{sleep, timeout};
use tracing::{debug, warn};
use zbus::fdo::NameOwnerChangedStream;
use zbus::proxy::{self, CacheProperties, MethodFlags};
use zbus::wire::DynamicDeserialize;
use zbus::{Connection, DynamicType, MatchRule, MessageStream, OwnedObjectPath, OwnedVariant};
use zbus::{fdo, message};

use super::{WallClock, drain, next, unix_now_s, until_unwanted, until_wanted};
use crate::server::{Feed, FeedContext, Publisher};

const UPOWER: &str = "org.freedesktop.UPower";
const UPOWER_PATH: &str = "/org/freedesktop/UPower";
const UPOWER_DEVICES: &str = "/org/freedesktop/UPower/devices";
const UPOWER_DEVICE: &str = "org.freedesktop.UPower.Device";
/// UPower's summary of the machine's own power, which is no device.
const DISPLAY_DEVICE: &str = "/org/freedesktop/UPower/devices/DisplayDevice";
// `UpDeviceKind` and `UpDeviceState` in UPower's D-Bus API reference.
const KIND_MOUSE: u32 = 5;
const KIND_KEYBOARD: u32 = 6;
const KIND_GAMING_INPUT: u32 = 12;
const KIND_HEADSET: u32 = 17;
const STATE_CHARGING: u32 = 1;
const KDE_CONNECT: &str = "org.kde.kdeconnect";
const KDE_CONNECT_DAEMON_PATH: &str = "/modules/kdeconnect";
const KDE_CONNECT_DAEMON: &str = "org.kde.kdeconnect.daemon";
const KDE_CONNECT_DEVICES: &str = "/modules/kdeconnect/devices/";
const KDE_CONNECT_DEVICE: &str = "org.kde.kdeconnect.device";
const KDE_CONNECT_BATTERY: &str = "org.kde.kdeconnect.device.battery";
const KDE_CONNECT_BATTERY_NODE: &str = "/battery";
const KDE_CONNECT_ID_PREFIX: &str = "kdeconnect:";
const KDE_CONNECT_PHONE_TYPES: [&str; 2] = ["phone", "tablet"];
const ONLY_REACHABLE: bool = true;
const ONLY_PAIRED: bool = true;
/// A phone that has not reported reads −1 here; anything outside a percentage is no charge.
const CHARGE_PCT: RangeInclusive<i32> = 0..=100;
const KDE_CONNECT_EVERY_S: u64 = 60;
// Tokio's timers stop during a suspend and nothing reports the resume, so while the topic is
// wanted the feed looks at the wall clock at least this often.
const WALL_CLOCK_CHECK: Duration = Duration::from_secs(60);
/// Either service answers a full read in milliseconds; one that takes longer is stuck, and the
/// batteries read before stay up meanwhile.
const READ_TIMEOUT: Duration = Duration::from_secs(2);
const PROPERTIES: &str = "org.freedesktop.DBus.Properties";
const GET: &str = "Get";
const PROPERTIES_CHANGED: &str = "PropertiesChanged";

/// Reads UPower on the system bus and KDE Connect on the session bus, each connected to only once
/// the topic is first wanted.
pub fn feed() -> Feed {
    Feed::new(topics::BATTERIES, |mut context| async move {
        until_wanted(&mut context.demand, topics::BATTERIES).await;
        let system = Connection::system()
            .await
            .inspect_err(|error| {
                warn!(%error, "cannot connect to the system bus; batteries has no UPower devices");
            })
            .ok();
        let session = Connection::session()
            .await
            .inspect_err(|error| {
                warn!(%error, "cannot connect to the session bus; batteries has no phones");
            })
            .ok();
        run(system, session, Arc::new(unix_now_s), context).await;
    })
}

/// As [`feed`], reading UPower on `system`, KDE Connect on `session` and the wall clock from
/// `clock`.
pub fn feed_on(system: Connection, session: Connection, clock: WallClock) -> Feed {
    Feed::new(topics::BATTERIES, move |context| {
        run(Some(system), Some(session), clock, context)
    })
}

async fn run(
    system: Option<Connection>,
    session: Option<Connection>,
    clock: WallClock,
    mut context: FeedContext,
) {
    let batteries = Lanes::new(context.publisher.clone());
    loop {
        until_wanted(&mut context.demand, topics::BATTERIES).await;
        batteries.begin(&context.config.borrow_and_update());
        // Dropping the other branches stops them, and with them their match rules.
        tokio::select! {
            biased;
            () = until_unwanted(&mut context.demand, topics::BATTERIES) => {}
            () = follow_labels(&mut context.config, &batteries) => {}
            () = watch_upower(system.as_ref(), &batteries) => {}
            () = poll_kde_connect(session.as_ref(), &clock, &batteries) => {}
        }
    }
}

/// The batteries as the lanes read them, and the snapshot last published.
struct Lanes {
    publisher: Publisher,
    progress: Mutex<Progress>,
}

#[derive(Default)]
struct Progress {
    upower: Vec<Battery>,
    kde_connect: Vec<Battery>,
    upower_read: bool,
    kde_connect_read: bool,
    labels: BTreeMap<String, String>,
    published: Option<Batteries>,
}

impl Lanes {
    fn new(publisher: Publisher) -> Self {
        Self {
            publisher,
            progress: Mutex::default(),
        }
    }

    /// Starts a wanted period, whose first snapshot waits for both services.
    fn begin(&self, config: &ResolvedConfig) {
        let mut progress = self.lock();
        progress.upower_read = false;
        progress.kde_connect_read = false;
        progress.labels.clone_from(&config.batteries.labels);
    }

    /// `None` is a read that did not finish, which keeps the devices read before.
    fn set_upower(&self, devices: Option<Vec<Battery>>) {
        self.update(|progress| {
            progress.upower_read = true;
            if let Some(devices) = devices {
                progress.upower = devices;
            }
        });
    }

    /// As [`Self::set_upower`].
    fn set_kde_connect(&self, devices: Option<Vec<Battery>>) {
        self.update(|progress| {
            progress.kde_connect_read = true;
            if let Some(devices) = devices {
                progress.kde_connect = devices;
            }
        });
    }

    fn set_labels(&self, labels: &BTreeMap<String, String>) {
        self.update(|progress| progress.labels.clone_from(labels));
    }

    fn update(&self, change: impl FnOnce(&mut Progress)) {
        let mut progress = self.lock();
        change(&mut progress);
        if !(progress.upower_read && progress.kde_connect_read) {
            return;
        }
        let snapshot = Batteries {
            devices: progress
                .upower
                .iter()
                .chain(&progress.kde_connect)
                .map(|battery| Battery {
                    label: progress
                        .labels
                        .get(&battery.id)
                        .unwrap_or(&battery.label)
                        .clone(),
                    ..battery.clone()
                })
                .collect(),
        };
        if progress.published.as_ref() == Some(&snapshot) {
            return;
        }
        // Published under the lock, so two quick changes can never reach clients swapped.
        self.publisher.publish(topics::BATTERIES, &snapshot);
        progress.published = Some(snapshot);
    }

    fn lock(&self) -> MutexGuard<'_, Progress> {
        self.progress.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

async fn follow_labels(config: &mut watch::Receiver<ResolvedConfig>, batteries: &Lanes) {
    while config.changed().await.is_ok() {
        let labels = config.borrow_and_update().batteries.labels.clone();
        batteries.set_labels(&labels);
    }
    pending().await
}

/// Reads UPower now and after every change it signals. Without a bus there are no UPower
/// devices; without its signals they are read once a wanted period.
async fn watch_upower(bus: Option<&Connection>, batteries: &Lanes) {
    let Some(bus) = bus else {
        batteries.set_upower(Some(Vec::new()));
        return pending().await;
    };
    let mut changes = UpowerChanges::listen(bus)
        .await
        .inspect_err(|error| {
            warn!(%error, "cannot watch UPower; batteries reads it only when first wanted");
        })
        .ok();
    loop {
        if let Some(changes) = &mut changes {
            // A read covers every change queued before it starts.
            changes.drain();
        }
        let read = timeout(READ_TIMEOUT, read_upower(bus)).await;
        if read.is_err() {
            warn!("UPower did not answer within {READ_TIMEOUT:?}; keeping its last devices");
        }
        batteries.set_upower(read.ok());
        match &mut changes {
            Some(changes) => changes.next().await,
            None => return pending().await,
        }
    }
}

/// Reads KDE Connect now, whenever it comes or goes, and every [`KDE_CONNECT_EVERY_S`] by the
/// wall clock.
async fn poll_kde_connect(bus: Option<&Connection>, clock: &WallClock, batteries: &Lanes) {
    let Some(bus) = bus else {
        batteries.set_kde_connect(Some(Vec::new()));
        return pending().await;
    };
    let mut owners = owner_changes(bus, KDE_CONNECT)
        .await
        .inspect_err(|error| {
            warn!(%error, "cannot watch KDE Connect come and go; batteries polls it only");
        })
        .ok();
    loop {
        if let Some(owners) = &mut owners {
            drain(owners);
        }
        let read = timeout(READ_TIMEOUT, read_kde_connect(bus)).await;
        if read.is_err() {
            warn!("KDE Connect did not answer within {READ_TIMEOUT:?}; keeping its last phones");
        }
        batteries.set_kde_connect(read.ok());
        let read_at_s = clock();
        loop {
            let now_s = clock();
            if is_due(read_at_s, now_s) {
                break;
            }
            let wait_s = (read_at_s + KDE_CONNECT_EVERY_S).saturating_sub(now_s);
            tokio::select! {
                () = sleep(Duration::from_secs(wait_s).min(WALL_CLOCK_CHECK)) => {}
                () = next_owner_change(&mut owners) => break,
            }
        }
    }
}

/// A clock set back before the last read makes the next one due at once, rather than an hour on.
fn is_due(read_at_s: u64, now_s: u64) -> bool {
    now_s < read_at_s || now_s >= read_at_s + KDE_CONNECT_EVERY_S
}

async fn next_owner_change(owners: &mut Option<NameOwnerChangedStream>) {
    match owners {
        Some(owners) => {
            if next(owners).await.is_none() {
                pending::<()>().await;
            }
        }
        None => pending().await,
    }
}

async fn owner_changes(
    bus: &Connection,
    name: &'static str,
) -> zbus::Result<NameOwnerChangedStream> {
    fdo::DBusProxy::builder(bus)
        .cache_properties(CacheProperties::No)
        .build()
        .await?
        .receive_name_owner_changed_with_args(&[(0, name)])
        .await
}

/// The signals after which UPower is read again.
struct UpowerChanges {
    owners: NameOwnerChangedStream,
    manager: MessageStream,
    devices: MessageStream,
}

impl UpowerChanges {
    /// Listens on `bus`; UPower must be read only after this, so that no change is missed.
    async fn listen(bus: &Connection) -> zbus::Result<Self> {
        let owners = owner_changes(bus, UPOWER).await?;
        // The bus, not zbus, applies a rule's well-known sender, following the name to its owner.
        let manager = MatchRule::builder()
            .msg_type(message::Type::Signal)
            .sender(UPOWER)
            .path(UPOWER_PATH)
            .interface(UPOWER)
            .build()?;
        let devices = MatchRule::builder()
            .msg_type(message::Type::Signal)
            .sender(UPOWER)
            .path_namespace(UPOWER_DEVICES)
            .interface(PROPERTIES)
            .member(PROPERTIES_CHANGED)
            .add_arg(UPOWER_DEVICE)
            .build()?;
        Ok(Self {
            owners,
            manager: MessageStream::for_match_rule(manager, bus, None).await?,
            devices: MessageStream::for_match_rule(devices, bus, None).await?,
        })
    }

    /// Drops every change already queued.
    fn drain(&mut self) {
        drain(&mut self.owners);
        drain(&mut self.manager);
        drain(&mut self.devices);
    }

    /// Waits for UPower to come or go, add or remove a device, or report a device's change other
    /// than the DisplayDevice's. Never returns once the bus is gone.
    async fn next(&mut self) {
        loop {
            tokio::select! {
                Some(_) = next(&mut self.owners) => return,
                Some(_) = next(&mut self.manager) => return,
                Some(message) = next(&mut self.devices) => {
                    let is_display_device = message.is_ok_and(|message| {
                        message.header().path().is_some_and(|path| path.as_str() == DISPLAY_DEVICE)
                    });
                    if !is_display_device {
                        return;
                    }
                }
                else => pending::<()>().await,
            }
        }
    }
}

/// Calls `method` without letting the bus start `service` for it: an absent service must read as
/// absent, not be launched by a dashboard.
async fn call<B, R>(
    bus: &Connection,
    service: &'static str,
    path: &str,
    interface: &'static str,
    method: &'static str,
    body: &B,
) -> zbus::Result<R>
where
    B: Serialize + DynamicType,
    R: for<'d> DynamicDeserialize<'d>,
{
    let proxy: zbus::Proxy<'_> = proxy::Builder::new(bus)
        .destination(service)
        .path(path)
        .interface(interface)
        .cache_properties(CacheProperties::No)
        .build()
        .await?;
    proxy
        .call_with_flags(method, MethodFlags::NO_AUTO_START, body)
        .await?
        .ok_or_else(|| zbus::Error::Failure(format!("{method} returned no reply")))
}

/// One interface of one object, whose properties are read as [`call`] calls.
struct Object<'a> {
    bus: &'a Connection,
    service: &'static str,
    path: &'a str,
    interface: &'static str,
}

impl Object<'_> {
    async fn get<T>(&self, name: &'static str) -> zbus::Result<T>
    where
        T: TryFrom<OwnedVariant>,
        <T as TryFrom<OwnedVariant>>::Error: Into<zbus::Error>,
    {
        let value: OwnedVariant = call(
            self.bus,
            self.service,
            self.path,
            PROPERTIES,
            GET,
            &(self.interface, name),
        )
        .await?;
        T::try_from(value).map_err(Into::into)
    }
}

/// Every UPower device that runs on its own battery. An absent UPower reads as none, and a device
/// it cannot answer for is left out: it can drop devices at any time, and says so with a signal
/// that brings the next read.
async fn read_upower(bus: &Connection) -> Vec<Battery> {
    let paths: Vec<OwnedObjectPath> =
        match call(bus, UPOWER, UPOWER_PATH, UPOWER, "EnumerateDevices", &()).await {
            Ok(paths) => paths,
            Err(error) => {
                debug!(%error, "UPower is not answering; no UPower batteries");
                return Vec::new();
            }
        };
    let mut batteries = Vec::new();
    for path in paths.iter().filter(|path| path.as_str() != DISPLAY_DEVICE) {
        match read_upower_device(bus, path.as_str()).await {
            Ok(Some(battery)) => batteries.push(battery),
            Ok(None) => {}
            Err(error) => debug!(%error, %path, "UPower device not answering; left out"),
        }
    }
    batteries
}

/// The device at `path`, unless it powers the machine itself.
async fn read_upower_device(bus: &Connection, path: &str) -> zbus::Result<Option<Battery>> {
    let device = Object {
        bus,
        service: UPOWER,
        path,
        interface: UPOWER_DEVICE,
    };
    let power_supply: bool = device.get("PowerSupply").await?;
    if power_supply {
        return Ok(None);
    }
    let kind: u32 = device.get("Type").await?;
    let id: String = device.get("NativePath").await?;
    let model: String = device.get("Model").await?;
    let percentage: f64 = device.get("Percentage").await?;
    let state: u32 = device.get("State").await?;
    Ok(Some(Battery {
        id,
        label: model.to_uppercase(),
        kind: match kind {
            KIND_GAMING_INPUT => BatteryKind::Controller,
            KIND_MOUSE => BatteryKind::Mouse,
            KIND_KEYBOARD => BatteryKind::Keyboard,
            KIND_HEADSET => BatteryKind::Headset,
            _ => BatteryKind::Other,
        },
        pct: Some(percentage as f32),
        charging: state == STATE_CHARGING,
    }))
}

/// Every paired, reachable KDE Connect device with a battery plugin, as it last reported. An
/// absent KDE Connect reads as none.
async fn read_kde_connect(bus: &Connection) -> Vec<Battery> {
    let ids: Vec<String> = match call(
        bus,
        KDE_CONNECT,
        KDE_CONNECT_DAEMON_PATH,
        KDE_CONNECT_DAEMON,
        "devices",
        &(ONLY_REACHABLE, ONLY_PAIRED),
    )
    .await
    {
        Ok(ids) => ids,
        Err(error) => {
            debug!(%error, "KDE Connect is not answering; no phones");
            return Vec::new();
        }
    };
    let mut batteries = Vec::new();
    for id in ids {
        match read_kde_connect_device(bus, &id).await {
            Ok(battery) => batteries.push(battery),
            // Also a device without the battery plugin, whose battery object does not exist.
            Err(error) => debug!(%error, %id, "KDE Connect device has no battery to read"),
        }
    }
    batteries
}

async fn read_kde_connect_device(bus: &Connection, id: &str) -> zbus::Result<Battery> {
    let device_path = format!("{KDE_CONNECT_DEVICES}{id}");
    let battery_path = format!("{device_path}{KDE_CONNECT_BATTERY_NODE}");
    let device = Object {
        bus,
        service: KDE_CONNECT,
        path: &device_path,
        interface: KDE_CONNECT_DEVICE,
    };
    let battery = Object {
        bus,
        service: KDE_CONNECT,
        path: &battery_path,
        interface: KDE_CONNECT_BATTERY,
    };
    let name: String = device.get("name").await?;
    let kind: String = device.get("type").await?;
    let has_battery: bool = battery.get("hasBattery").await?;
    let charge: i32 = battery.get("charge").await?;
    let charging: bool = battery.get("isCharging").await?;
    Ok(Battery {
        id: format!("{KDE_CONNECT_ID_PREFIX}{id}"),
        label: name.to_uppercase(),
        kind: if KDE_CONNECT_PHONE_TYPES.contains(&kind.as_str()) {
            BatteryKind::Phone
        } else {
            BatteryKind::Other
        },
        pct: (has_battery && CHARGE_PCT.contains(&charge)).then_some(charge as f32),
        charging,
    })
}
