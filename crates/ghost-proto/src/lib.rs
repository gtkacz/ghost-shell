mod bar;
mod config;
mod layout;
mod msg;
mod node_de;
mod snapshot;

pub use bar::*;
pub use config::*;
pub use layout::*;
pub use msg::*;
pub use snapshot::*;

pub const PROTO_VERSION: u32 = 1;

const MAX_TOPIC_LEN: usize = 64;

pub mod topics {
    pub const CPU: &str = "cpu";
    pub const MEM: &str = "mem";
    pub const PROCS: &str = "procs";
    pub const PROCS_TREE: &str = "procs.tree";
    pub const SENSORS: &str = "sensors";
    pub const GPU: &str = "gpu";
    pub const DISKS: &str = "disks";
    pub const NET: &str = "net";
    pub const PERF: &str = "perf";
    pub const HOST: &str = "host";
    pub const DESKTOP: &str = "desktop";
    pub const WEATHER: &str = "weather";
    pub const SKY: &str = "sky";
    pub const SPECTRUM: &str = "audio.spectrum";
    pub const NET_LINKS: &str = "net.links";
    pub const DISKS_HEALTH: &str = "disks.health";
    pub const BATTERIES: &str = "batteries";
    pub const UPLINK: &str = "uplink";
    pub const SESSION: &str = "session";
    pub const WINDOWS: &str = "windows";
    pub const LAUNCHER_ENTRIES: &str = "launcher.entries";
    pub const TICKER: &str = "ticker";
    pub const RECENT: &str = "recent";
    pub const AGENTS_USAGE: &str = "agents.usage";
    pub const AGENTS_SESSIONS: &str = "agents.sessions";
    pub const BRIGHTNESS: &str = "brightness";
    /// Published by an alerts provider, so it is well known without being built in.
    pub const WEATHER_ALERTS: &str = "weather.alerts";

    // Feed topics count as built in too, so no provider can claim one, even before its feed exists.
    pub const BUILTIN: [&str; 26] = [
        CPU,
        MEM,
        SENSORS,
        GPU,
        DISKS,
        PERF,
        HOST,
        DESKTOP,
        WEATHER,
        SKY,
        SPECTRUM,
        PROCS,
        PROCS_TREE,
        NET_LINKS,
        NET,
        DISKS_HEALTH,
        BATTERIES,
        UPLINK,
        SESSION,
        WINDOWS,
        LAUNCHER_ENTRIES,
        TICKER,
        RECENT,
        AGENTS_USAGE,
        AGENTS_SESSIONS,
        BRIGHTNESS,
    ];
}

/// Accepts names matching `^[a-z][a-z0-9_]*(\.[a-z][a-z0-9_]*)*$` of at most 64 characters.
pub fn is_valid_topic(name: &str) -> bool {
    name.len() <= MAX_TOPIC_LEN && name.split('.').all(is_valid_topic_segment)
}

fn is_valid_topic_segment(segment: &str) -> bool {
    let mut chars = segment.chars();
    matches!(chars.next(), Some('a'..='z'))
        && chars.all(|c| matches!(c, 'a'..='z' | '0'..='9' | '_'))
}

/// The methods a UI client may `call`, and the arguments each takes.
pub mod calls {
    /// `{"id": "<internalId>"}`
    pub const WINDOWS_ACTIVATE: &str = "windows.activate";
    pub const WINDOWS_MINIMIZE: &str = "windows.minimize";
    pub const WINDOWS_CLOSE: &str = "windows.close";
    /// `{}`
    pub const SESSION_LOCK: &str = "session.lock";
    pub const SESSION_LOGOUT: &str = "session.logout";
    pub const SESSION_REBOOT: &str = "session.reboot";
    pub const SESSION_SHUTDOWN: &str = "session.shutdown";
    pub const SESSION_SUSPEND: &str = "session.suspend";
    /// `{"text": "<query>"}`
    pub const KRUNNER_QUERY: &str = "krunner.query";
    /// `{"applet": "<name>"}`
    pub const PLASMA_POPUP: &str = "plasma.popup";
    /// `{"display": "<id from the brightness topic>", "value": <raw brightness>}`
    pub const BRIGHTNESS_SET: &str = "brightness.set";
    /// `{}`
    pub const NIGHTLIGHT_TOGGLE: &str = "nightlight.toggle";
    /// `{"app": "<desktop id>"}`
    pub const BAR_PIN: &str = "bar.pin";
    pub const BAR_UNPIN: &str = "bar.unpin";

    pub const ALL: [&str; 14] = [
        WINDOWS_ACTIVATE,
        WINDOWS_MINIMIZE,
        WINDOWS_CLOSE,
        SESSION_LOCK,
        SESSION_LOGOUT,
        SESSION_REBOOT,
        SESSION_SHUTDOWN,
        SESSION_SUSPEND,
        KRUNNER_QUERY,
        PLASMA_POPUP,
        BRIGHTNESS_SET,
        NIGHTLIGHT_TOGGLE,
        BAR_PIN,
        BAR_UNPIN,
    ];
}
