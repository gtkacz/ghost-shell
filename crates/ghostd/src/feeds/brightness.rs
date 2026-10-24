//! The `brightness` topic and its calls: the displays powerdevil can set and KWin's Night Light,
//! on the session bus.
//!
//! The topic is read only while some client wants it: in full at once, and again whenever
//! powerdevil or KWin's Night Light signals a change or either comes or goes on the bus. Nothing
//! is polled. Both services are D-Bus activatable, so every call asks the bus not to start its
//! service: an absent one reads as no displays and an unavailable Night Light.
//!
//! The calls are handled whether or not the topic is wanted, one at a time in arrival order:
//! - `brightness.set` checks the display against the ones powerdevil lists now, so no client can
//!   name a path of its own, and clamps the value to `DARK_PANEL_FLOOR_PCT` of the display's range up to its
//!   maximum;
//! - `nightlight.toggle` runs KWin's own "Toggle Night Color" global action, the one its keyboard
//!   shortcut runs, through KGlobalAccel. KWin's Night Light has no D-Bus method or writable
//!   property for the switch (only `inhibit`, whose cookie dies with the caller, and `enabled`,
//!   which is read-only), and the action suspends or resumes it, so a second toggle undoes the
//!   first. It is left alone while Night Light is unavailable or disabled in the settings: it
//!   would suspend nothing visible, and would leave it suspended when it is enabled later. A toggle
//!   within `TOGGLE_DEBOUNCE` of the last one is dropped, so a double click is one toggle.

use std::collections::HashMap;
use std::future::pending;
use std::time::Duration;

use ghost_proto::{Brightness, DisplayBrightness, NightLight, calls, topics};
use serde_json::Value;
use tokio::sync::broadcast;
use tokio::time::{Instant, timeout};
use tracing::{debug, warn};
use zbus::fdo::NameOwnerChangedStream;
use zbus::{Connection, MatchRule, Message, MessageStream, OwnedVariant, message};

use super::batteries::{Object, PROPERTIES, PROPERTIES_CHANGED, call, owner_changes};
use super::{drain, next, until_unwanted, until_wanted};
use crate::server::{Call, Demand, Feed, FeedContext, Publisher};

const SCREEN_BRIGHTNESS: &str = "org.kde.ScreenBrightness";
const SCREEN_BRIGHTNESS_PATH: &str = "/org/kde/ScreenBrightness";
const DISPLAY_INTERFACE: &str = "org.kde.ScreenBrightness.Display";
const KWIN: &str = "org.kde.KWin";
const NIGHT_LIGHT_PATH: &str = "/org/kde/KWin/NightLight";
const NIGHT_LIGHT: &str = "org.kde.KWin.NightLight";
const KGLOBALACCEL: &str = "org.kde.kglobalaccel";
const KWIN_COMPONENT_PATH: &str = "/component/kwin";
const COMPONENT_INTERFACE: &str = "org.kde.kglobalaccel.Component";
const TOGGLE_NIGHT_LIGHT: &str = "Toggle Night Color";
const NIGHT_LIGHT_READ: [&str; 4] = ["available", "enabled", "inhibited", "running"];
const DISPLAY_ARG: &str = "display";
const VALUE_ARG: &str = "value";
/// A backlight at zero can be fully dark, with nothing on screen to turn it back up, and one raw
/// unit is as dark on a display whose range has thousands; the floor the user chose is a share of
/// the range. The shell mirrors it in `tray.mjs`.
const DARK_PANEL_FLOOR_PCT: i32 = 1;
const PERCENT: i32 = 100;
/// Long enough to take a double click for one toggle, short enough that the next deliberate one
/// goes through; KWin's action flips a state, so two in a row would undo each other.
const TOGGLE_DEBOUNCE: Duration = Duration::from_millis(400);
/// powerdevil's `SetBrightness` flags; none asks for its defaults.
const NO_FLAGS: u32 = 0;
/// Both services answer in milliseconds; one that takes longer is stuck, and must not hold up
/// the calls queued behind it, or keep the last snapshot from standing.
const BUS_TIMEOUT: Duration = Duration::from_secs(2);

/// Reads and calls the session bus, connected to once the daemon starts: a call can come before
/// the topic is ever wanted.
pub fn feed() -> Feed {
    Feed::new(topics::BRIGHTNESS, |context| async move {
        let bus = Connection::session()
            .await
            .inspect_err(|error| {
                warn!(%error, "cannot connect to the session bus; brightness has no displays and its calls do nothing");
            })
            .ok();
        run(bus, context).await;
    })
}

/// As [`feed`], on `bus`.
pub fn feed_on(bus: Connection) -> Feed {
    Feed::new(topics::BRIGHTNESS, move |context| run(Some(bus), context))
}

async fn run(bus: Option<Connection>, context: FeedContext) {
    let FeedContext {
        publisher,
        mut demand,
        calls,
        ..
    } = context;
    tokio::select! {
        () = serve_calls(bus.as_ref(), calls) => {}
        () = publish_while_wanted(bus.as_ref(), &publisher, &mut demand) => {}
    }
}

async fn publish_while_wanted(
    bus: Option<&Connection>,
    publisher: &Publisher,
    demand: &mut Demand,
) {
    loop {
        until_wanted(demand, topics::BRIGHTNESS).await;
        // Dropping the other branch stops it, and with it its match rules.
        tokio::select! {
            biased;
            () = until_unwanted(demand, topics::BRIGHTNESS) => {}
            () = watch(bus, publisher) => {}
        }
    }
}

/// Publishes the topic now and after every change signalled, when it differs from the last.
async fn watch(bus: Option<&Connection>, publisher: &Publisher) {
    let Some(bus) = bus else {
        publisher.publish(topics::BRIGHTNESS, &Brightness::default());
        return pending().await;
    };
    let mut changes = Changes::listen(bus)
        .await
        .inspect_err(|error| {
            warn!(%error, "cannot watch powerdevil and KWin; brightness reads them only when first wanted");
        })
        .ok();
    let mut published: Option<Brightness> = None;
    loop {
        if let Some(changes) = &mut changes {
            // A read covers every change queued before it starts.
            changes.drain();
        }
        match timeout(BUS_TIMEOUT, read(bus)).await {
            Ok(snapshot) if published.as_ref() != Some(&snapshot) => {
                publisher.publish(topics::BRIGHTNESS, &snapshot);
                published = Some(snapshot);
            }
            Ok(_) => {}
            Err(_) => warn!(
                "powerdevil or KWin did not answer within {BUS_TIMEOUT:?}; keeping the last brightness"
            ),
        }
        match &mut changes {
            Some(changes) => changes.next().await,
            None => return pending().await,
        }
    }
}

/// The signals after which the topic is read again.
struct Changes {
    screen_owners: NameOwnerChangedStream,
    kwin_owners: NameOwnerChangedStream,
    screen: MessageStream,
    night_light: MessageStream,
}

impl Changes {
    /// Listens on `bus`; the topic must be read only after this, so that no change is missed.
    async fn listen(bus: &Connection) -> zbus::Result<Self> {
        // The bus, not zbus, applies a rule's well-known sender, following the name to its owner.
        let screen = MatchRule::builder()
            .msg_type(message::Type::Signal)
            .sender(SCREEN_BRIGHTNESS)
            .path_namespace(SCREEN_BRIGHTNESS_PATH)
            .build()?;
        let night_light = MatchRule::builder()
            .msg_type(message::Type::Signal)
            .sender(KWIN)
            .path(NIGHT_LIGHT_PATH)
            .interface(PROPERTIES)
            .member(PROPERTIES_CHANGED)
            .build()?;
        Ok(Self {
            screen_owners: owner_changes(bus, SCREEN_BRIGHTNESS).await?,
            kwin_owners: owner_changes(bus, KWIN).await?,
            screen: MessageStream::for_match_rule(screen, bus, None).await?,
            night_light: MessageStream::for_match_rule(night_light, bus, None).await?,
        })
    }

    /// Drops every change already queued.
    fn drain(&mut self) {
        drain(&mut self.screen_owners);
        drain(&mut self.kwin_owners);
        drain(&mut self.screen);
        drain(&mut self.night_light);
    }

    /// Waits for a change. Never returns once the bus is gone.
    async fn next(&mut self) {
        loop {
            tokio::select! {
                Some(_) = next(&mut self.screen_owners) => return,
                Some(_) = next(&mut self.kwin_owners) => return,
                Some(_) = next(&mut self.screen) => return,
                Some(message) = next(&mut self.night_light) => {
                    if message.is_ok_and(|message| changes_what_is_read(&message)) {
                        return;
                    }
                }
                else => pending::<()>().await,
            }
        }
    }
}

/// Whether a Night Light `PropertiesChanged` touches a property the topic carries: KWin also
/// signals `currentTemperature` every few tens of milliseconds through a transition.
fn changes_what_is_read(message: &Message) -> bool {
    let body = message
        .body()
        .deserialize::<(String, HashMap<String, OwnedVariant>, Vec<String>)>();
    body.is_ok_and(|(_, changed, invalidated)| {
        NIGHT_LIGHT_READ.iter().any(|name| {
            changed.contains_key(*name) || invalidated.iter().any(|invalid| invalid == name)
        })
    })
}

async fn read(bus: &Connection) -> Brightness {
    Brightness {
        displays: read_displays(bus).await,
        night_light: read_night_light(bus).await,
    }
}

/// The lowest brightness a client may set on a display whose range ends at `max`: a share of the
/// range, rounded up so a coarse range keeps at least one unit.
fn dark_floor(max: i32) -> i32 {
    let floor = (i64::from(max.max(1)) * i64::from(DARK_PANEL_FLOOR_PCT) + i64::from(PERCENT) - 1)
        / i64::from(PERCENT);
    i32::try_from(floor).unwrap_or(i32::MAX)
}

async fn display_names(bus: &Connection) -> zbus::Result<Vec<String>> {
    Object {
        bus,
        service: SCREEN_BRIGHTNESS,
        path: SCREEN_BRIGHTNESS_PATH,
        interface: SCREEN_BRIGHTNESS,
    }
    .get("DisplaysDBusNames")
    .await
}

fn display_path(id: &str) -> String {
    format!("{SCREEN_BRIGHTNESS_PATH}/{id}")
}

/// Every display powerdevil lists. An absent powerdevil reads as none, and a display it cannot
/// answer for is left out: displays come and go, and powerdevil says so with a signal that
/// brings the next read.
async fn read_displays(bus: &Connection) -> Vec<DisplayBrightness> {
    let names = match display_names(bus).await {
        Ok(names) => names,
        Err(error) => {
            debug!(%error, "powerdevil is not answering; no displays");
            return Vec::new();
        }
    };
    let mut displays = Vec::new();
    for id in names {
        match read_display(bus, &id).await {
            Ok(display) => displays.push(display),
            Err(error) => debug!(%error, %id, "display not answering; left out"),
        }
    }
    displays
}

async fn read_display(bus: &Connection, id: &str) -> zbus::Result<DisplayBrightness> {
    let path = display_path(id);
    let display = Object {
        bus,
        service: SCREEN_BRIGHTNESS,
        path: &path,
        interface: DISPLAY_INTERFACE,
    };
    Ok(DisplayBrightness {
        id: id.to_owned(),
        label: display.get("Label").await?,
        brightness: display.get("Brightness").await?,
        max: display.get("MaxBrightness").await?,
    })
}

/// Night Light as KWin reports it, or the unavailable one while KWin does not answer.
async fn read_night_light(bus: &Connection) -> NightLight {
    let night_light = Object {
        bus,
        service: KWIN,
        path: NIGHT_LIGHT_PATH,
        interface: NIGHT_LIGHT,
    };
    let read = async {
        Ok::<_, zbus::Error>(NightLight {
            available: night_light.get("available").await?,
            enabled: night_light.get("enabled").await?,
            inhibited: night_light.get("inhibited").await?,
            running: night_light.get("running").await?,
        })
    };
    read.await.unwrap_or_else(|error| {
        debug!(%error, "KWin's Night Light is not answering; reading it as unavailable");
        NightLight::default()
    })
}

/// Never returns: calls keep coming for as long as the daemon runs. They are handled one at a
/// time, in arrival order.
async fn serve_calls(bus: Option<&Connection>, mut incoming: broadcast::Receiver<Call>) {
    let mut last_toggle: Option<Instant> = None;
    loop {
        match incoming.recv().await {
            Ok(call) => {
                if call.method == calls::NIGHTLIGHT_TOGGLE {
                    if last_toggle.is_some_and(|at| at.elapsed() < TOGGLE_DEBOUNCE) {
                        debug!(
                            "a Night Light toggle came within {TOGGLE_DEBOUNCE:?} of the last; dropped"
                        );
                        continue;
                    }
                    last_toggle = Some(Instant::now());
                }
                handle(bus, &call).await;
            }
            Err(broadcast::error::RecvError::Lagged(missed)) => {
                warn!(
                    missed,
                    "brightness calls were dropped: the feed fell behind"
                );
            }
            Err(broadcast::error::RecvError::Closed) => pending().await,
        }
    }
}

async fn handle(bus: Option<&Connection>, call: &Call) {
    if !matches!(
        call.method.as_str(),
        calls::BRIGHTNESS_SET | calls::NIGHTLIGHT_TOGGLE
    ) {
        return;
    }
    let Some(bus) = bus else {
        warn!(
            method = call.method,
            "no bus connection; the call did nothing"
        );
        return;
    };
    let done = if call.method == calls::BRIGHTNESS_SET {
        timeout(BUS_TIMEOUT, set_brightness(bus, &call.args)).await
    } else {
        timeout(BUS_TIMEOUT, toggle_night_light(bus)).await
    };
    match done {
        Ok(Ok(())) => {}
        Ok(Err(error)) => warn!(method = call.method, %error, "the call failed"),
        Err(_) => warn!(
            method = call.method,
            "the call did not answer within {BUS_TIMEOUT:?}"
        ),
    }
}

async fn set_brightness(bus: &Connection, args: &Value) -> Result<(), CallError> {
    let display = args.get(DISPLAY_ARG).and_then(Value::as_str);
    let value = args.get(VALUE_ARG).and_then(Value::as_f64);
    let (Some(display), Some(value)) = (display, value.filter(|value| value.is_finite())) else {
        return Err(CallError::Refused("needs a display name and a number"));
    };
    if !display_names(bus).await?.iter().any(|name| name == display) {
        return Err(CallError::Refused("no such display"));
    }
    let path = display_path(display);
    let max: i32 = Object {
        bus,
        service: SCREEN_BRIGHTNESS,
        path: &path,
        interface: DISPLAY_INTERFACE,
    }
    .get("MaxBrightness")
    .await?;
    let value = value.round().clamp(
        f64::from(dark_floor(max)),
        f64::from(max.max(dark_floor(max))),
    );
    let _: () = call(
        bus,
        SCREEN_BRIGHTNESS,
        &path,
        DISPLAY_INTERFACE,
        "SetBrightness",
        &(value as i32, NO_FLAGS),
    )
    .await?;
    Ok(())
}

async fn toggle_night_light(bus: &Connection) -> Result<(), CallError> {
    let night_light = read_night_light(bus).await;
    if !(night_light.available && night_light.enabled) {
        return Err(CallError::Refused("Night Light is unavailable or disabled"));
    }
    let _: () = call(
        bus,
        KGLOBALACCEL,
        KWIN_COMPONENT_PATH,
        COMPONENT_INTERFACE,
        "invokeShortcut",
        &(TOGGLE_NIGHT_LIGHT,),
    )
    .await?;
    Ok(())
}

#[derive(Debug, thiserror::Error)]
enum CallError {
    #[error("refused: {0}")]
    Refused(&'static str),
    #[error(transparent)]
    Bus(#[from] zbus::Error),
}
