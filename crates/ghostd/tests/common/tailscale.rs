//! A fake of Tailscale's local API on a Unix socket of its own.

use std::sync::{Arc, Mutex};

use ghostd::feeds::tailscale::TailscaleClient;
use tempfile::TempDir;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tokio::task::JoinHandle;

pub const TAILSCALE_STATUS: &str = include_str!("../fixtures/tailscale-status.json");

/// Tailscale's local API on a socket of its own, answering every request with the fixture.
pub struct FakeTailscale {
    dir: TempDir,
    requests: Arc<Mutex<Vec<String>>>,
    _server: JoinHandle<()>,
}

impl FakeTailscale {
    pub const SOCKET: &str = "tailscaled.sock";

    pub fn start() -> Self {
        Self::start_with(TAILSCALE_STATUS.to_owned())
    }

    /// As [`FakeTailscale::start`], answering with `status` instead of the fixture.
    pub fn start_with(status: String) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let listener = UnixListener::bind(dir.path().join(Self::SOCKET)).unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let seen = Arc::clone(&requests);
        let server = tokio::spawn(async move {
            loop {
                let (stream, _) = listener.accept().await.unwrap();
                let (reader, mut writer) = stream.into_split();
                let mut reader = BufReader::new(reader);
                let mut request = String::new();
                while !request.ends_with("\r\n\r\n") {
                    if reader.read_line(&mut request).await.unwrap() == 0 {
                        break;
                    }
                }
                seen.lock().unwrap().push(request);
                let response =
                    format!("HTTP/1.0 200 OK\r\nContent-Type: application/json\r\n\r\n{status}");
                writer.write_all(response.as_bytes()).await.unwrap();
                writer.shutdown().await.unwrap();
            }
        });
        Self {
            dir,
            requests,
            _server: server,
        }
    }

    pub fn client(&self) -> TailscaleClient {
        TailscaleClient::new(self.dir.path().join(Self::SOCKET))
    }

    pub fn requests(&self) -> Vec<String> {
        self.requests.lock().unwrap().clone()
    }
}

/// A client for a socket that does not exist, kept with the directory it would be in.
pub fn missing_tailscale() -> (TempDir, TailscaleClient) {
    let dir = tempfile::tempdir().unwrap();
    let client = TailscaleClient::new(dir.path().join(FakeTailscale::SOCKET));
    (dir, client)
}
