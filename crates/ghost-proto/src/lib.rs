mod config;
mod layout;
mod msg;
mod node_de;
mod snapshot;

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
    /// Published by an alerts provider, so it is well known without being built in.
    pub const WEATHER_ALERTS: &str = "weather.alerts";

    // Feed topics count as built in too, so no provider can claim one, even before its feed exists.
    pub const BUILTIN: [&str; 15] = [
        CPU, MEM, SENSORS, GPU, DISKS, PERF, HOST, DESKTOP, WEATHER, SKY, SPECTRUM, PROCS,
        PROCS_TREE, NET_LINKS, NET,
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
