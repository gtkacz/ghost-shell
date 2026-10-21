use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{ResolvedConfig, ResolvedLayout};

#[derive(Debug, thiserror::Error)]
pub enum WireError {
    #[error("invalid message: {0}")]
    Invalid(#[from] serde_json::Error),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum ClientMsg {
    /// A UI client.
    Hello { proto: u32, topics: Vec<String> },
    /// Replaces the client's subscription set.
    Subscribe { topics: Vec<String> },
    /// An external provider.
    Provide {
        proto: u32,
        name: String,
        topics: Vec<String>,
    },
    Publish {
        topic: String,
        data: serde_json::Value,
    },
}

// Config is sent only when the file changes, so a larger enum on the stack costs less than
// boxing it at every construction site.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum ServerMsg {
    Hello {
        proto: u32,
        host: String,
    },
    Error {
        message: String,
    },
    History {
        step_ms: u32,
        /// `None` (`null` on the wire) marks a tick without a reading: the source was not sampled,
        /// or had nothing to measure, as with an absent GPU.
        series: BTreeMap<String, Vec<Option<f32>>>,
    },
    Data {
        topic: String,
        seq: u64,
        data: serde_json::Value,
    },
    /// Whether a provider is publishing the topic. Sent only for topics a provider has claimed, so
    /// a topic without one is as live as the daemon.
    TopicState {
        topic: String,
        live: bool,
    },
    Config {
        data: ResolvedConfig,
    },
    Layout {
        data: ResolvedLayout,
    },
    ConfigError {
        message: String,
    },
}

/// Wraps `v` in a [`ServerMsg::Data`] envelope.
///
/// Panics if `v` has no JSON representation (e.g. a map with non-string keys); the built-in
/// snapshot types always do.
pub fn data_msg<T: Serialize>(topic: &str, seq: u64, v: &T) -> ServerMsg {
    ServerMsg::Data {
        topic: topic.to_owned(),
        seq,
        data: serde_json::to_value(v).expect("value has a JSON representation"),
    }
}

/// Returns the message as compact JSON followed by a newline.
pub fn encode_line(m: &ServerMsg) -> String {
    // ServerMsg holds only string-keyed maps and JSON values, so serialization cannot fail.
    let mut line = serde_json::to_string(m).expect("ServerMsg serializes to JSON");
    line.push('\n');
    line
}

pub fn decode_client_line(l: &str) -> Result<ClientMsg, WireError> {
    Ok(serde_json::from_str(l)?)
}
