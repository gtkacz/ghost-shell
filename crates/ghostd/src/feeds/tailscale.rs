//! Tailscale's local API, read over tailscaled's Unix socket.

use std::collections::BTreeMap;
use std::io;
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::time::Duration;

use serde::Deserialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::time::timeout;
use tracing::debug;

pub const TAILSCALED_SOCKET: &str = "/run/tailscale/tailscaled.sock";
const TIMEOUT: Duration = Duration::from_secs(2);
// HTTP/1.0, so tailscaled closes the connection after the body and sends it unchunked.
const STATUS_REQUEST: &[u8] =
    b"GET /localapi/v0/status HTTP/1.0\r\nHost: local-tailscaled.sock\r\n\r\n";
const HEADER_END: &[u8] = b"\r\n\r\n";
const STATUS_OK_PREFIXES: [&[u8]; 2] = [b"HTTP/1.0 200 ", b"HTTP/1.1 200 "];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TailscaleStatus {
    /// `Running`, `Starting`, `NeedsLogin`, `Stopped` and the like.
    pub backend_state: String,
    pub self_ips: Vec<IpAddr>,
    pub peers: Vec<TailscalePeer>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TailscalePeer {
    pub host_name: String,
    pub online: bool,
    /// The peer's endpoint while the connection to it is direct, and `None` while it is relayed.
    pub cur_addr: Option<SocketAddr>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct StatusResponse {
    backend_state: String,
    #[serde(rename = "TailscaleIPs", default)]
    tailscale_ips: Option<Vec<IpAddr>>,
    // Keyed by node key, which nothing here needs.
    #[serde(default)]
    peer: Option<BTreeMap<String, PeerResponse>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct PeerResponse {
    host_name: String,
    online: bool,
    #[serde(default)]
    cur_addr: String,
}

#[derive(Debug, thiserror::Error)]
enum StatusError {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("timed out after {TIMEOUT:?}")]
    TimedOut,
    #[error("unexpected response: {0}")]
    Response(String),
    #[error("unexpected status: {0}")]
    Json(#[from] serde_json::Error),
}

pub struct TailscaleClient {
    socket: PathBuf,
}

impl TailscaleClient {
    pub fn new(socket: PathBuf) -> Self {
        Self { socket }
    }

    /// The daemon's status, or `None` when its socket is missing or refuses, or it does not
    /// answer within 2 s.
    pub async fn status(&self) -> Option<TailscaleStatus> {
        let status = match timeout(TIMEOUT, self.request()).await {
            Ok(status) => status,
            Err(_) => Err(StatusError::TimedOut),
        };
        // Most machines have no Tailscale at all, so a failure here is ordinary.
        status
            .inspect_err(
                |error| debug!(%error, socket = %self.socket.display(), "no Tailscale status"),
            )
            .ok()
    }

    async fn request(&self) -> Result<TailscaleStatus, StatusError> {
        let mut stream = UnixStream::connect(&self.socket).await?;
        stream.write_all(STATUS_REQUEST).await?;
        let mut response = Vec::new();
        stream.read_to_end(&mut response).await?;
        parse_response(&response)
    }
}

fn parse_response(response: &[u8]) -> Result<TailscaleStatus, StatusError> {
    if !STATUS_OK_PREFIXES
        .iter()
        .any(|prefix| response.starts_with(prefix))
    {
        let status_line = response
            .split(|&byte| byte == b'\n')
            .next()
            .unwrap_or_default();
        return Err(StatusError::Response(
            String::from_utf8_lossy(status_line).trim_end().to_owned(),
        ));
    }
    let body_start = response
        .windows(HEADER_END.len())
        .position(|window| window == HEADER_END)
        .ok_or_else(|| StatusError::Response("no end to the headers".to_owned()))?
        + HEADER_END.len();
    let status: StatusResponse = serde_json::from_slice(&response[body_start..])?;
    Ok(TailscaleStatus {
        backend_state: status.backend_state,
        self_ips: status.tailscale_ips.unwrap_or_default(),
        peers: status
            .peer
            .unwrap_or_default()
            .into_values()
            .map(|peer| TailscalePeer {
                host_name: peer.host_name,
                online: peer.online,
                // `""` when the peer is reached through a relay.
                cur_addr: peer.cur_addr.parse().ok(),
            })
            .collect(),
    })
}
