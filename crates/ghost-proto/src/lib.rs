mod config;
mod msg;
mod snapshot;

pub use config::*;
pub use msg::*;
pub use snapshot::*;

pub const PROTO_VERSION: u32 = 1;

const MAX_TOPIC_LEN: usize = 64;

pub mod topics {
    pub const CPU: &str = "cpu";
    pub const MEM: &str = "mem";
    pub const SENSORS: &str = "sensors";
    pub const GPU: &str = "gpu";
    pub const DISKS: &str = "disks";
    pub const PERF: &str = "perf";

    pub const BUILTIN: [&str; 6] = [CPU, MEM, SENSORS, GPU, DISKS, PERF];
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
