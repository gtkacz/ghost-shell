//! Runs what the launcher and the bar ask for through `call`: the session actions, KRunner
//! queries, and KDE's own popups. The feed publishes nothing and only waits for calls.
//!
//! Every call but the KRunner query asks the bus not to start the service for it: a lock,
//! logout or suspend that launched its service would act on a machine nobody asked it to.
//! KRunner is D-Bus activated and idle until asked, so its query may start it.

use std::env;
use std::ffi::OsString;
use std::fs;
use std::future::pending;
use std::path::{Path, PathBuf};
use std::time::Duration;

use ghost_proto::calls;
use serde::Serialize;
use serde_json::Value;
use tokio::sync::broadcast;
use tokio::time::timeout;
use tracing::warn;
use zbus::proxy::{self, CacheProperties, MethodFlags};
use zbus::{Connection, DynamicType};

use crate::server::{Call, Feed};

const SCREEN_SAVER: Target = Target::session(
    "org.freedesktop.ScreenSaver",
    "/ScreenSaver",
    "org.freedesktop.ScreenSaver",
    "Lock",
);
const SHUTDOWN_SERVICE: &str = "org.kde.Shutdown";
const SHUTDOWN_PATH: &str = "/Shutdown";
const LOGOUT: Target = Target::session(SHUTDOWN_SERVICE, SHUTDOWN_PATH, SHUTDOWN_SERVICE, "logout");
const REBOOT: Target = Target::session(
    SHUTDOWN_SERVICE,
    SHUTDOWN_PATH,
    SHUTDOWN_SERVICE,
    "logoutAndReboot",
);
const SHUTDOWN: Target = Target::session(
    SHUTDOWN_SERVICE,
    SHUTDOWN_PATH,
    SHUTDOWN_SERVICE,
    "logoutAndShutdown",
);
const SUSPEND: Target = Target {
    bus: Bus::System,
    service: "org.freedesktop.login1",
    path: "/org/freedesktop/login1",
    interface: "org.freedesktop.login1.Manager",
    method: "Suspend",
};
const KRUNNER: Target = Target::session("org.kde.krunner", "/App", "org.kde.krunner.App", "query");
const INVOKE_SHORTCUT: Target = Target::session(
    "org.kde.kglobalaccel",
    "/component/plasmashell",
    "org.kde.kglobalaccel.Component",
    "invokeShortcut",
);

/// A shortcut Plasma registers only for a widget that has a global key, which
/// `scripts/plasma-panel.sh retire` gives each popup's widget.
const ACTIVATE_WIDGET: &str = "activate widget";
const POPUP_APPLETS: [&str; 7] = [
    "org.kde.plasma.networkmanagement",
    "org.kde.plasma.bluetooth",
    "org.kde.plasma.volume",
    "org.kde.kdeconnect",
    "org.kde.plasma.clipboard",
    "org.kde.plasma.systemtray",
    "org.kde.plasma.digitalclock",
];
const POPUP_MAP: &str = "ghost-shell/plasma-popups";
const MAP_SEPARATOR: char = '\t';
const STATE_HOME_ENV: &str = "XDG_STATE_HOME";
const HOME_ENV: &str = "HOME";
const STATE_HOME_SUBDIR: &str = ".local/state";

const TEXT_ARG: &str = "text";
const APPLET_ARG: &str = "applet";
/// Longer than anything a person types into a launcher, and short enough that no one can hand
/// KRunner a megabyte.
const MAX_QUERY_CHARS: usize = 256;
/// These services answer in milliseconds; one that takes longer is stuck, and must not hold up
/// the calls queued behind it.
const CALL_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Clone, Copy)]
enum Bus {
    Session,
    System,
}

#[derive(Clone, Copy)]
struct Target {
    bus: Bus,
    service: &'static str,
    path: &'static str,
    interface: &'static str,
    method: &'static str,
}

impl Target {
    const fn session(
        service: &'static str,
        path: &'static str,
        interface: &'static str,
        method: &'static str,
    ) -> Self {
        Self {
            bus: Bus::Session,
            service,
            path,
            interface,
            method,
        }
    }
}

struct Buses {
    session: Option<Connection>,
    system: Option<Connection>,
}

/// Forwards the calls of [`calls::ALL`] that move the session or open a KDE popup, on the real
/// session and system buses, and reads the popup map under `$XDG_STATE_HOME`.
pub fn feed() -> Feed {
    Feed::new("actions", |context| async move {
        let session = Connection::session()
            .await
            .inspect_err(|error| {
                warn!(%error, "cannot connect to the session bus; session, KRunner and popup actions do nothing");
            })
            .ok();
        let system = Connection::system()
            .await
            .inspect_err(|error| {
                warn!(%error, "cannot connect to the system bus; suspend does nothing");
            })
            .ok();
        let map = popup_map_path(env::var_os(STATE_HOME_ENV), env::var_os(HOME_ENV));
        run(Buses { session, system }, map, context.calls).await;
    })
}

/// As [`feed`], on the given connections and reading the popup map from `popup_map`.
pub fn feed_on(
    session: Option<Connection>,
    system: Option<Connection>,
    popup_map: PathBuf,
) -> Feed {
    Feed::new("actions", move |context| {
        run(Buses { session, system }, popup_map, context.calls)
    })
}

fn popup_map_path(state_home: Option<OsString>, home: Option<OsString>) -> PathBuf {
    // The XDG spec has relative values ignored.
    let state_home = state_home
        .map(PathBuf::from)
        .filter(|path| path.is_absolute());
    let base = state_home
        .unwrap_or_else(|| PathBuf::from(home.unwrap_or_default()).join(STATE_HOME_SUBDIR));
    base.join(POPUP_MAP)
}

/// Never returns: calls keep coming for as long as the daemon runs. They are handled one at a
/// time, in arrival order, so a lock followed by a logout cannot swap.
async fn run(buses: Buses, popup_map: PathBuf, mut incoming: broadcast::Receiver<Call>) {
    loop {
        match incoming.recv().await {
            Ok(call) => handle(&buses, &popup_map, call).await,
            Err(broadcast::error::RecvError::Lagged(missed)) => {
                warn!(missed, "actions were dropped: the feed fell behind");
            }
            Err(broadcast::error::RecvError::Closed) => pending().await,
        }
    }
}

async fn handle(buses: &Buses, popup_map: &Path, call: Call) {
    match call.method.as_str() {
        calls::SESSION_LOCK => invoke(buses, SCREEN_SAVER, &(), false).await,
        calls::SESSION_LOGOUT => invoke(buses, LOGOUT, &(), false).await,
        calls::SESSION_REBOOT => invoke(buses, REBOOT, &(), false).await,
        calls::SESSION_SHUTDOWN => invoke(buses, SHUTDOWN, &(), false).await,
        // Not interactive: nothing on this machine would show the authentication prompt.
        calls::SESSION_SUSPEND => invoke(buses, SUSPEND, &(false,), false).await,
        calls::KRUNNER_QUERY => query(buses, &call).await,
        calls::PLASMA_POPUP => popup(buses, popup_map, &call).await,
        _ => {}
    }
}

async fn query(buses: &Buses, call: &Call) {
    let text = call.args.get(TEXT_ARG).and_then(Value::as_str);
    let text: String = text
        .map_or("", str::trim)
        .chars()
        .take(MAX_QUERY_CHARS)
        .collect();
    if text.is_empty() {
        warn!(method = call.method, "refused a KRunner query without text");
        return;
    }
    invoke(buses, KRUNNER, &(text,), true).await;
}

async fn popup(buses: &Buses, popup_map: &Path, call: &Call) {
    let applet = call.args.get(APPLET_ARG).and_then(Value::as_str);
    let Some(applet) = applet.filter(|applet| POPUP_APPLETS.contains(applet)) else {
        warn!(method = call.method, applet = ?applet, "refused a popup for an unknown applet");
        return;
    };
    let Some(widget) = widget_id(popup_map, applet) else {
        warn!(
            applet,
            map = %popup_map.display(),
            "no popup route: the Plasma panel isn't retired (`scripts/plasma-panel.sh retire` writes the map)"
        );
        return;
    };
    invoke(
        buses,
        INVOKE_SHORTCUT,
        &(format!("{ACTIVATE_WIDGET} {widget}"),),
        false,
    )
    .await;
}

/// The widget id `applet` has in the map, read afresh since `retire` can run while the daemon
/// does. Lines that aren't an allowed applet and a run of digits are skipped.
fn widget_id(popup_map: &Path, applet: &str) -> Option<String> {
    let map = fs::read_to_string(popup_map).ok()?;
    map.lines()
        .filter_map(|line| line.split_once(MAP_SEPARATOR))
        .filter(|(name, id)| {
            POPUP_APPLETS.contains(name)
                && !id.is_empty()
                && id.bytes().all(|byte| byte.is_ascii_digit())
        })
        .find(|(name, _)| *name == applet)
        .map(|(_, id)| id.to_owned())
}

async fn invoke<B: Serialize + DynamicType>(
    buses: &Buses,
    target: Target,
    body: &B,
    may_start_service: bool,
) {
    let bus = match target.bus {
        Bus::Session => buses.session.as_ref(),
        Bus::System => buses.system.as_ref(),
    };
    let Some(bus) = bus else {
        warn!(
            method = target.method,
            "no bus connection; the action did nothing"
        );
        return;
    };
    match timeout(CALL_TIMEOUT, send(bus, target, body, may_start_service)).await {
        Ok(Ok(())) => {}
        Ok(Err(error)) => warn!(method = target.method, %error, "the action failed"),
        Err(_) => warn!(
            method = target.method,
            "the action did not answer within {CALL_TIMEOUT:?}"
        ),
    }
}

async fn send<B: Serialize + DynamicType>(
    bus: &Connection,
    target: Target,
    body: &B,
    may_start_service: bool,
) -> zbus::Result<()> {
    let proxy: zbus::Proxy<'_> = proxy::Builder::new(bus)
        .destination(target.service)
        .path(target.path)
        .interface(target.interface)
        .cache_properties(CacheProperties::No)
        .build()
        .await?;
    let flags = if may_start_service {
        None
    } else {
        Some(MethodFlags::NO_AUTO_START)
    };
    match flags {
        Some(flags) => proxy
            .call_with_flags::<_, _, ()>(target.method, flags, body)
            .await
            .map(|_| ()),
        None => proxy.call::<_, _, ()>(target.method, body).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(state: Option<&str>, home: Option<&str>) -> PathBuf {
        popup_map_path(state.map(OsString::from), home.map(OsString::from))
    }

    #[test]
    fn map_lives_under_the_state_home() {
        assert_eq!(
            path(Some("/s"), Some("/h")),
            Path::new("/s/ghost-shell/plasma-popups")
        );
    }

    #[test]
    fn relative_or_missing_state_home_falls_back_to_home() {
        let expected = Path::new("/h/.local/state/ghost-shell/plasma-popups");
        assert_eq!(path(Some("rel"), Some("/h")), expected);
        assert_eq!(path(None, Some("/h")), expected);
    }
}
