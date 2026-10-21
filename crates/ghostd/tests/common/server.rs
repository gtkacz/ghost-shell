use std::fs;
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::time::Duration;

use ghostd::fake;
use ghostd::server::{self, ConfigFeed, Feed, Hub, ProviderSpecs, ServeError, SocketLock};
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::net::UnixStream;
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};
use tokio::task::JoinHandle;
use tokio::time::timeout;

pub const TICK_MS: u32 = 50;
pub const WAIT: Duration = Duration::from_secs(5);
pub const SOCKET_FILE: &str = "ghostd.sock";
pub const CONFIG_DIR: &str = "config";
pub const CONFIG_FILE: &str = "config.toml";
pub const WIDGETS_DIR: &str = "widgets";

/// A config whose only view holds one widget, so the test widget dir can satisfy the whole layout
/// and no layout errors arrive between the messages under test.
pub fn single_widget_config(widget: &str, panel_fill: u32) -> String {
    format!("[appearance]\npanel_fill = {panel_fill}\n\n[layout.views.v1]\nwidget = {widget:?}\n")
}

/// Only the providers the config lists, without the one `location.alerts` would add, so that no
/// test can start a real alerts provider from PATH.
pub fn configured_providers() -> ProviderSpecs {
    Box::new(|config| config.providers.clone())
}

pub fn install_widget(widgets: &Path, id: &str) {
    let dir = widgets.join(id);
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("manifest.toml"),
        format!("id = {id:?}\nname = {id:?}\nversion = \"1.0.0\"\napi = 1\nkinds = [\"panel\"]\n"),
    )
    .unwrap();
    fs::write(dir.join("Widget.qml"), "import QtQuick\nItem {}\n").unwrap();
}

pub struct Daemon {
    pub dir: TempDir,
    pub serve: JoinHandle<ServeError>,
    _lock: SocketLock,
}

impl Drop for Daemon {
    fn drop(&mut self) {
        self.serve.abort();
    }
}

impl Daemon {
    pub async fn start() -> Self {
        Self::start_with_config(&single_widget_config("clock", 55)).await
    }

    pub async fn start_with_config(toml: &str) -> Self {
        Self::start_with_widgets(toml, &["clock"]).await
    }

    pub async fn start_with_widgets(toml: &str, installed: &[&str]) -> Self {
        Self::launch(toml, installed, Vec::new(), configured_providers()).await
    }

    pub async fn start_with_feeds(feeds: Vec<Feed>) -> Self {
        Self::launch(
            &single_widget_config("clock", 55),
            &["clock"],
            feeds,
            configured_providers(),
        )
        .await
    }

    pub async fn start_with_providers(toml: &str, providers: ProviderSpecs) -> Self {
        Self::launch(toml, &["clock"], Vec::new(), providers).await
    }

    async fn launch(
        toml: &str,
        installed: &[&str],
        feeds: Vec<Feed>,
        providers: ProviderSpecs,
    ) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join(CONFIG_DIR).join(CONFIG_FILE);
        fs::create_dir_all(config.parent().unwrap()).unwrap();
        fs::write(&config, toml).unwrap();
        let widgets = dir.path().join(WIDGETS_DIR);
        for id in installed {
            install_widget(&widgets, id);
        }

        let socket = dir.path().join(SOCKET_FILE);
        let config = ConfigFeed::open(&config);
        let hub = Hub {
            config,
            providers,
            socket: socket.clone(),
            sampler: fake::sampler(),
            tick_ms: NonZeroU32::new(TICK_MS).unwrap(),
            // Only the temp dir, so widgets installed on this machine cannot leak into the tests.
            widget_dirs: Box::new(move |_| vec![widgets.clone()]),
            on_warning: Box::new(|_, _| {}),
            feeds,
        };
        let (listener, lock) = server::bind(&socket).unwrap();
        let serve = tokio::spawn(server::serve(listener, hub));
        Self {
            dir,
            serve,
            _lock: lock,
        }
    }

    pub fn socket(&self) -> PathBuf {
        self.dir.path().join(SOCKET_FILE)
    }

    pub fn config(&self) -> PathBuf {
        self.dir.path().join(CONFIG_DIR).join(CONFIG_FILE)
    }

    pub fn widgets(&self) -> PathBuf {
        self.dir.path().join(WIDGETS_DIR)
    }

    pub async fn connect(&self) -> Client {
        Client::connect(&self.socket()).await
    }

    /// A UI client past the handshake.
    pub async fn ui(&self) -> Client {
        let mut client = self.connect().await;
        client
            .send(json!({"t": "hello", "proto": 1, "topics": []}))
            .await;
        assert_eq!(client.recv().await["t"], "hello");
        assert_eq!(client.recv().await["t"], "config");
        assert_eq!(client.recv().await["t"], "layout");
        client
    }

    /// A UI client subscribed to `topics` plus `cpu`, returned once the History reply proves the
    /// subscription is in place.
    pub async fn subscribed_ui(&self, topics: &[&str]) -> Client {
        let mut client = self.ui().await;
        let mut all: Vec<&str> = topics.to_vec();
        all.push("cpu");
        client.send(json!({"t": "subscribe", "topics": all})).await;
        assert_eq!(client.recv().await["t"], "history");
        client
    }

    pub async fn provider(&self, name: &str, topics: &[&str]) -> Client {
        let mut client = self.connect().await;
        client
            .send(json!({"t": "provide", "proto": 1, "name": name, "topics": topics}))
            .await;
        client
    }
}

pub struct Client {
    pub lines: Lines<BufReader<OwnedReadHalf>>,
    pub writer: OwnedWriteHalf,
}

impl Client {
    pub async fn connect(socket: &Path) -> Self {
        let stream = UnixStream::connect(socket).await.unwrap();
        let (reader, writer) = stream.into_split();
        Self {
            lines: BufReader::new(reader).lines(),
            writer,
        }
    }

    pub async fn send(&mut self, message: Value) {
        self.send_raw(&message.to_string()).await;
    }

    pub async fn send_raw(&mut self, line: &str) {
        self.writer.write_all(line.as_bytes()).await.unwrap();
        self.writer.write_all(b"\n").await.unwrap();
    }

    pub async fn next_line(&mut self) -> Option<String> {
        timeout(WAIT, self.lines.next_line())
            .await
            .expect("nothing arrived in time")
            .expect("socket read failed")
    }

    /// The next message of any kind.
    pub async fn recv_any(&mut self) -> Value {
        let line = self
            .next_line()
            .await
            .expect("server closed the connection");
        serde_json::from_str(&line).unwrap()
    }

    /// The next message other than `config_error`, which only the config tests look at.
    pub async fn recv(&mut self) -> Value {
        timeout(WAIT, async {
            loop {
                let message = self.recv_any().await;
                if message["t"] != "config_error" {
                    return message;
                }
            }
        })
        .await
        .expect("only config errors arrived in time")
    }

    pub async fn recv_data(&mut self, topic: &str) -> Value {
        // One deadline for the whole search, since other topics may keep streaming meanwhile.
        timeout(WAIT, async {
            loop {
                let message = self.recv().await;
                if message["t"] == "data" && message["topic"] == topic {
                    return message;
                }
            }
        })
        .await
        .unwrap_or_else(|_| panic!("no {topic} data arrived in time"))
    }

    pub async fn recv_topic_state(&mut self, topic: &str) -> Value {
        // One deadline for the whole search, since other topics may keep streaming meanwhile.
        timeout(WAIT, async {
            loop {
                let message = self.recv().await;
                if message["t"] == "topic_state" && message["topic"] == topic {
                    return message;
                }
            }
        })
        .await
        .unwrap_or_else(|_| panic!("no {topic} topic_state arrived in time"))
    }

    /// Skips ahead to a `layout` whose single `v1` slot satisfies `wanted`.
    pub async fn recv_layout_where(&mut self, wanted: impl Fn(&Value) -> bool) -> Value {
        timeout(WAIT, async {
            loop {
                let message = self.recv().await;
                if message["t"] == "layout" && wanted(&message["data"]["views"]["v1"]) {
                    return message;
                }
            }
        })
        .await
        .expect("no matching layout arrived in time")
    }

    pub async fn expect_error(&mut self, expected: &str) {
        let message = self.recv().await;
        assert_eq!(message["t"], "error", "got {message}");
        assert_eq!(message["message"], expected);
        self.expect_closed().await;
    }

    /// Reads, discarding anything still buffered, until the server closes the connection.
    pub async fn expect_closed(&mut self) {
        // One deadline for the whole drain, since a connection that stays open may keep streaming.
        timeout(WAIT, async {
            while let Some(_line) = self.lines.next_line().await.expect("socket read failed") {}
        })
        .await
        .expect("the server kept the connection open");
    }
}
