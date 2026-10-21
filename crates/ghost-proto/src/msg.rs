use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

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
        series: BTreeMap<String, Vec<f32>>,
    },
    Data {
        topic: String,
        seq: u64,
        data: serde_json::Value,
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
