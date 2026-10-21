use std::fs;
use std::num::NonZeroU32;
use std::os::unix::fs::{FileTypeExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use ghost_proto::ProviderSpec;
use ghostd::server::{self, BindError, ConfigFeed, Hub, SocketLock};
use ghostd::{fake, providers};
use nix::fcntl::{Flock, FlockArg};
use nix::sys::stat::{Mode, SFlag, mknod};
use nix::unistd::mkfifo;
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::net::UnixStream;
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf, pipe};
use tokio::task::JoinHandle;
use tokio::time::timeout;

const TICK_MS: u32 = 50;
const WAIT: Duration = Duration::from_secs(5);
const HANDSHAKE_DEADLINE: Duration = Duration::from_millis(2500);
const RESTART_WINDOW: Duration = Duration::from_secs(3);
const QUIET_WINDOW: Duration = Duration::from_millis(600);
// Half the first restart delay, so a supervisor that restarts without waiting is caught while
// scheduler jitter is not.
const MIN_RESTART_GAP: Duration = Duration::from_millis(500);
// Three grace periods of two seconds do not fit in WAIT; one does.
const STUBBORN_PROVIDERS: usize = 3;
const HANDSHAKE_ERROR: &str = "expected hello or provide with proto 1";
const SOCKET_FILE: &str = "ghostd.sock";
const CONFIG_DIR: &str = "config";
const CONFIG_FILE: &str = "config.toml";
const WIDGETS_DIR: &str = "widgets";
const PRIVATE_SOCKET_MODE: u32 = 0o600;
const PERMISSION_BITS: u32 = 0o777;
const MAX_LINE_BYTES: usize = 64 * 1024;
const BULK_PAYLOAD_BYTES: usize = 60 * 1024;
// Well past the 256 KiB queue limit plus the kernel's socket buffer, so the client must be dropped.
const BULK_PUBLISHES: usize = 20;
const CPU_HISTORY_KEYS: [&str; 3] = ["load", "ccd0", "ccd1"];

/// A config whose only view holds one widget, so the test widget dir can satisfy the whole layout
/// and no layout errors arrive between the messages under test.
fn single_widget_config(widget: &str, panel_fill: u32) -> String {
    format!("[appearance]\npanel_fill = {panel_fill}\n\n[layout.views.v1]\nwidget = {widget:?}\n")
}

fn install_widget(widgets: &Path, id: &str) {
    let dir = widgets.join(id);
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("manifest.toml"),
        format!("id = {id:?}\nname = {id:?}\nversion = \"1.0.0\"\napi = 1\nkinds = [\"panel\"]\n"),
    )
    .unwrap();
    fs::write(dir.join("Widget.qml"), "import QtQuick\nItem {}\n").unwrap();
}

struct Daemon {
    dir: TempDir,
    serve: JoinHandle<()>,
    _lock: SocketLock,
}

impl Drop for Daemon {
    fn drop(&mut self) {
        self.serve.abort();
    }
}

impl Daemon {
    async fn start() -> Self {
        Self::start_with_config(&single_widget_config("clock", 55)).await
    }

    async fn start_with_config(toml: &str) -> Self {
        Self::start_with_widgets(toml, &["clock"]).await
    }

    async fn start_with_widgets(toml: &str, installed: &[&str]) -> Self {
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
            providers: providers::supervise(&config.current().providers, &socket),
            config,
            sampler: fake::sampler(),
            tick_ms: NonZeroU32::new(TICK_MS).unwrap(),
            // Only the temp dir, so widgets installed on this machine cannot leak into the tests.
            widget_dirs: Box::new(move |_| vec![widgets.clone()]),
            on_warning: Box::new(|_, _| {}),
        };
        let (listener, lock) = server::bind(&socket).unwrap();
        let serve = tokio::spawn(server::serve(listener, hub));
        Self {
            dir,
            serve,
            _lock: lock,
        }
    }

    fn socket(&self) -> PathBuf {
        self.dir.path().join(SOCKET_FILE)
    }

    fn config(&self) -> PathBuf {
        self.dir.path().join(CONFIG_DIR).join(CONFIG_FILE)
    }

    fn widgets(&self) -> PathBuf {
        self.dir.path().join(WIDGETS_DIR)
    }

    async fn connect(&self) -> Client {
        Client::connect(&self.socket()).await
    }

    /// A UI client past the handshake.
    async fn ui(&self) -> Client {
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
    async fn subscribed_ui(&self, topics: &[&str]) -> Client {
        let mut client = self.ui().await;
        let mut all: Vec<&str> = topics.to_vec();
        all.push("cpu");
        client.send(json!({"t": "subscribe", "topics": all})).await;
        assert_eq!(client.recv().await["t"], "history");
        client
    }

    async fn provider(&self, name: &str, topics: &[&str]) -> Client {
        let mut client = self.connect().await;
        client
            .send(json!({"t": "provide", "proto": 1, "name": name, "topics": topics}))
            .await;
        client
    }
}

struct Client {
    lines: Lines<BufReader<OwnedReadHalf>>,
    writer: OwnedWriteHalf,
}

impl Client {
    async fn connect(socket: &Path) -> Self {
        let stream = UnixStream::connect(socket).await.unwrap();
        let (reader, writer) = stream.into_split();
        Self {
            lines: BufReader::new(reader).lines(),
            writer,
        }
    }

    async fn send(&mut self, message: Value) {
        self.send_raw(&message.to_string()).await;
    }

    async fn send_raw(&mut self, line: &str) {
        self.writer.write_all(line.as_bytes()).await.unwrap();
        self.writer.write_all(b"\n").await.unwrap();
    }

    async fn next_line(&mut self) -> Option<String> {
        timeout(WAIT, self.lines.next_line())
            .await
            .expect("nothing arrived in time")
            .expect("socket read failed")
    }

    /// The next message of any kind.
    async fn recv_any(&mut self) -> Value {
        let line = self
            .next_line()
            .await
            .expect("server closed the connection");
        serde_json::from_str(&line).unwrap()
    }

    /// The next message other than `config_error`, which only the config tests look at.
    async fn recv(&mut self) -> Value {
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

    async fn recv_data(&mut self, topic: &str) -> Value {
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

    /// Skips ahead to a `layout` whose single `v1` slot satisfies `wanted`.
    async fn recv_layout_where(&mut self, wanted: impl Fn(&Value) -> bool) -> Value {
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

    async fn expect_error(&mut self, expected: &str) {
        let message = self.recv().await;
        assert_eq!(message["t"], "error", "got {message}");
        assert_eq!(message["message"], expected);
        self.expect_closed().await;
    }

    /// Reads, discarding anything still buffered, until the server closes the connection.
    async fn expect_closed(&mut self) {
        // One deadline for the whole drain, since a connection that stays open may keep streaming.
        timeout(WAIT, async {
            while let Some(_line) = self.lines.next_line().await.expect("socket read failed") {}
        })
        .await
        .expect("the server kept the connection open");
    }
}

#[tokio::test]
async fn ui_handshake_then_subscribed_topics_only() {
    let daemon = Daemon::start().await;
    let mut ui = daemon.connect().await;

    ui.send(json!({"t": "hello", "proto": 1, "topics": []}))
        .await;
    let hello = ui.recv().await;
    assert_eq!(hello["t"], "hello");
    assert_eq!(hello["proto"], 1);
    assert!(hello["host"].is_string());
    let config = ui.recv().await;
    assert_eq!(config["t"], "config");
    assert_eq!(config["data"]["appearance"]["panel_fill"], 55);
    let layout = ui.recv().await;
    assert_eq!(layout["t"], "layout");
    assert_eq!(layout["data"]["views"]["v1"]["widget"], "clock");

    ui.send(json!({"t": "subscribe", "topics": ["cpu"]})).await;
    let history = ui.recv().await;
    assert_eq!(history["t"], "history");
    assert_eq!(history["step_ms"], TICK_MS);
    let series = history["series"].as_object().unwrap();
    assert!(
        series
            .keys()
            .all(|key| CPU_HISTORY_KEYS.contains(&key.as_str())),
        "history carries series of unsubscribed topics: {series:?}"
    );

    let mut cpu_lines = 0;
    while cpu_lines < 2 {
        let message = ui.recv().await;
        assert_eq!(message["t"], "data", "got {message}");
        assert_eq!(message["topic"], "cpu", "got {message}");
        assert_eq!(message["data"]["threads"].as_array().unwrap().len(), 32);
        cpu_lines += 1;
    }

    // The ticks so far recorded every series, `gpu_load` included, so a history that leaks
    // unsubscribed series would show it now.
    ui.send(json!({"t": "subscribe", "topics": ["cpu"]})).await;
    let history = loop {
        let message = ui.recv().await;
        if message["t"] == "history" {
            break message;
        }
        assert_eq!(message["topic"], "cpu", "got {message}");
    };
    let series = history["series"].as_object().unwrap();
    let mut keys: Vec<&str> = series.keys().map(String::as_str).collect();
    keys.sort_unstable();
    let mut expected = CPU_HISTORY_KEYS;
    expected.sort_unstable();
    assert_eq!(keys, expected);
    assert!(
        series
            .values()
            .all(|samples| !samples.as_array().unwrap().is_empty())
    );
}

#[tokio::test]
async fn wrong_proto_and_silence_get_errors() {
    let daemon = Daemon::start().await;

    let mut wrong_proto = daemon.connect().await;
    wrong_proto
        .send(json!({"t": "hello", "proto": 9, "topics": []}))
        .await;
    wrong_proto.expect_error(HANDSHAKE_ERROR).await;

    let mut wrong_provider_proto = daemon.connect().await;
    wrong_provider_proto
        .send(json!({"t": "provide", "proto": 2, "name": "p", "topics": ["a.b"]}))
        .await;
    wrong_provider_proto.expect_error(HANDSHAKE_ERROR).await;

    let started = Instant::now();
    let mut silent = daemon.connect().await;
    silent.expect_error(HANDSHAKE_ERROR).await;
    assert!(
        started.elapsed() < HANDSHAKE_DEADLINE,
        "the silent client was dropped only after {:?}",
        started.elapsed()
    );
}

#[tokio::test]
async fn provider_publish_reaches_subscriber() {
    let daemon = Daemon::start().await;
    let mut ui = daemon.subscribed_ui(&["weather.alerts"]).await;

    let mut provider = daemon.provider("inmet", &["weather.alerts"]).await;
    provider
        .send(json!({"t": "publish", "topic": "weather.alerts", "data": {"n": 1}}))
        .await;

    let data = ui.recv_data("weather.alerts").await;
    assert_eq!(data["data"]["n"], 1);
    assert_eq!(data["seq"], 1);
}

#[tokio::test]
async fn late_subscriber_gets_last_value() {
    let daemon = Daemon::start().await;
    let mut witness = daemon.subscribed_ui(&["weather.alerts"]).await;
    let mut provider = daemon.provider("inmet", &["weather.alerts"]).await;
    provider
        .send(json!({"t": "publish", "topic": "weather.alerts", "data": {"n": 1}}))
        .await;
    witness.recv_data("weather.alerts").await;

    let mut late = daemon.ui().await;
    late.send(json!({"t": "subscribe", "topics": ["weather.alerts"]}))
        .await;

    let cached = late.recv().await;
    assert_eq!(cached["t"], "data");
    assert_eq!(cached["topic"], "weather.alerts");
    assert_eq!(cached["data"]["n"], 1);
}

#[tokio::test]
async fn provider_rules() {
    let daemon = Daemon::start().await;

    let mut builtin = daemon.provider("thief", &["cpu"]).await;
    builtin.expect_error("topic \"cpu\" is built in").await;

    let mut invalid = daemon.provider("shouty", &["Weather"]).await;
    invalid.expect_error("invalid topic name \"Weather\"").await;

    let mut witness = daemon.subscribed_ui(&["x.y"]).await;
    let mut owner = daemon.provider("other", &["x.y"]).await;
    owner
        .send(json!({"t": "publish", "topic": "x.y", "data": 1}))
        .await;
    witness.recv_data("x.y").await;

    let mut duplicate = daemon.provider("copycat", &["x.y"]).await;
    duplicate
        .expect_error("topic \"x.y\" is already provided by \"other\"")
        .await;

    owner
        .send(json!({"t": "publish", "topic": "x.z", "data": 1}))
        .await;
    owner
        .expect_error("topic \"x.z\" is not declared by \"other\"")
        .await;
}

#[tokio::test]
async fn provider_crash_does_not_affect_others() {
    let daemon = Daemon::start().await;
    let mut ui = daemon.subscribed_ui(&["w.t"]).await;
    let mut provider = daemon.provider("first", &["w.t"]).await;
    provider
        .send(json!({"t": "publish", "topic": "w.t", "data": {"n": 1}}))
        .await;
    ui.recv_data("w.t").await;

    drop(provider);
    ui.recv_data("cpu").await;
    ui.recv_data("cpu").await;

    let mut stale = daemon.ui().await;
    stale
        .send(json!({"t": "subscribe", "topics": ["w.t"]}))
        .await;
    assert_eq!(stale.recv_data("w.t").await["data"]["n"], 1);

    let mut replacement = daemon.provider("second", &["w.t"]).await;
    replacement
        .send(json!({"t": "publish", "topic": "w.t", "data": {"n": 2}}))
        .await;
    let relayed = ui.recv_data("w.t").await;
    assert_eq!(relayed["data"]["n"], 2);
    assert_eq!(relayed["seq"], 2);
}

#[tokio::test]
async fn reconnect_works() {
    let daemon = Daemon::start().await;
    let mut first = daemon.ui().await;
    first
        .send(json!({"t": "subscribe", "topics": ["cpu"]}))
        .await;
    first.recv_data("cpu").await;
    drop(first);

    let mut second = daemon.ui().await;
    second
        .send(json!({"t": "subscribe", "topics": ["cpu"]}))
        .await;
    let history = second.recv().await;
    assert_eq!(history["t"], "history");
    assert!(!history["series"]["load"].as_array().unwrap().is_empty());
    second.recv_data("cpu").await;
}

#[tokio::test]
async fn config_change_resends_config_and_layout() {
    let daemon =
        Daemon::start_with_widgets(&single_widget_config("clock", 55), &["clock", "gauge"]).await;
    let mut ui = daemon.ui().await;

    fs::write(daemon.config(), single_widget_config("gauge", 40)).unwrap();

    let config = ui.recv().await;
    assert_eq!(config["t"], "config");
    assert_eq!(config["data"]["appearance"]["panel_fill"], 40);
    let layout = ui.recv().await;
    assert_eq!(layout["t"], "layout");
    let slot = &layout["data"]["views"]["v1"];
    assert_eq!(slot["widget"], "gauge");
    assert_eq!(slot["error"], Value::Null);

    fs::write(daemon.config(), "[appearance\npanel_fi").unwrap();

    let problem = ui.recv_any().await;
    assert_eq!(problem["t"], "config_error", "got {problem}");
    assert!(!problem["message"].as_str().unwrap().is_empty());
}

/// The `GHOST_SOCKET` value the next provider run wrote into the FIFO.
async fn next_start(started: &mut Lines<BufReader<pipe::Receiver>>) -> String {
    timeout(RESTART_WINDOW, started.next_line())
        .await
        .expect("provider not started in time")
        .unwrap()
        .expect("the FIFO closed")
}

#[tokio::test]
async fn widget_changes_resend_the_layout() {
    let daemon = Daemon::start_with_config(&single_widget_config("gauge", 55)).await;
    let mut ui = daemon.connect().await;
    ui.send(json!({"t": "hello", "proto": 1, "topics": []}))
        .await;
    assert_eq!(ui.recv().await["t"], "hello");
    assert_eq!(ui.recv().await["t"], "config");
    let layout = ui.recv().await;
    assert!(layout["data"]["views"]["v1"]["error"].is_string());

    // No config change follows, so only the widget-dir watch can bring the new layout.
    install_widget(&daemon.widgets(), "gauge");
    ui.recv_layout_where(|slot| slot["error"].is_null()).await;

    // Discovery reads every manifest, which inotify reports too, so a watch that reacted to its own
    // reads would resend the layout within a debounce period or two. Absence can only be shown by
    // waiting, so the window spans several debounce periods.
    assert!(
        timeout(QUIET_WINDOW, ui.lines.next_line()).await.is_err(),
        "the layout was resent without any widget change"
    );

    fs::remove_dir_all(daemon.widgets().join("gauge")).unwrap();
    ui.recv_layout_where(|slot| slot["error"].is_string()).await;
}

#[tokio::test]
async fn providers_follow_config_reloads() {
    let daemon = Daemon::start().await;
    let starts = daemon.dir.path().join("starts");
    let held = daemon.dir.path().join("held");
    mkfifo(&starts, Mode::S_IRWXU).unwrap();
    mkfifo(&held, Mode::S_IRWXU).unwrap();
    let mut started = BufReader::new(
        pipe::OpenOptions::new()
            .read_write(true)
            .open_receiver(&starts)
            .unwrap(),
    )
    .lines();
    // Read-only, so once the provider holds the write end, reading ends only when it dies.
    let mut lifeline = pipe::OpenOptions::new().open_receiver(&held).unwrap();
    let script = r#"exec 3>"$1"; echo "$$" > "$0"; exec sleep 1000"#;
    let provider = format!(
        "\n[[providers]]\nname = \"lingering\"\nexec = [\"sh\", \"-c\", {script:?}, {:?}, {:?}]\n",
        starts.to_str().unwrap(),
        held.to_str().unwrap(),
    );
    let base = single_widget_config("clock", 55);

    fs::write(daemon.config(), format!("{base}{provider}")).unwrap();
    let pid = next_start(&mut started).await;
    assert!(pid.parse::<u32>().is_ok(), "got {pid:?}");

    fs::write(daemon.config(), &base).unwrap();
    let mut leftover = Vec::new();
    timeout(WAIT, lifeline.read_to_end(&mut leftover))
        .await
        .expect("the removed provider is still running")
        .unwrap();
    assert!(leftover.is_empty());
}

#[tokio::test]
async fn supervised_provider_restarts() {
    let dir = tempfile::tempdir().unwrap();
    let starts = dir.path().join("starts");
    mkfifo(&starts, Mode::S_IRWXU).unwrap();
    // Opened for reading and writing, so the FIFO stays open between one run and the next.
    let receiver = pipe::OpenOptions::new()
        .read_write(true)
        .open_receiver(&starts)
        .unwrap();
    let mut started = BufReader::new(receiver).lines();
    let socket = dir.path().join(SOCKET_FILE);
    let spec = ProviderSpec {
        name: "flaky".into(),
        exec: vec![
            "sh".into(),
            "-c".into(),
            "echo \"$GHOST_SOCKET\" > \"$0\"; exit 0".into(),
            starts.to_str().unwrap().into(),
        ],
        restart: true,
    };

    let began = Instant::now();
    let _supervisor = providers::supervise(&[spec], &socket);

    let first_env = next_start(&mut started).await;
    let first_at = Instant::now();
    let second_env = next_start(&mut started).await;
    let second_at = Instant::now();

    assert_eq!(first_env, socket.to_str().unwrap());
    assert_eq!(second_env, socket.to_str().unwrap());
    assert!(second_at - began <= RESTART_WINDOW);
    assert!(
        second_at - first_at >= MIN_RESTART_GAP,
        "restarted after only {:?}",
        second_at - first_at
    );
}

/// A provider that waits on a `sleep` it leaves running in the background, as an `sh -c "prog"`
/// wrapper does. Both hold the write end of a FIFO, so the returned reader reaches end of file
/// only once the shell and the sleep are all gone. `preamble` is run by the shell first.
async fn provider_with_a_child(preamble: &str) -> (providers::Supervisor, pipe::Receiver, TempDir) {
    provider_that_starts_a_child(preamble, "wait", false).await
}

/// As [`provider_with_a_child`], with `after_start` as the shell's last command.
async fn provider_that_starts_a_child(
    preamble: &str,
    after_start: &str,
    restart: bool,
) -> (providers::Supervisor, pipe::Receiver, TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let starts = dir.path().join("starts");
    let held = dir.path().join("held");
    mkfifo(&starts, Mode::S_IRWXU).unwrap();
    mkfifo(&held, Mode::S_IRWXU).unwrap();
    let mut started = BufReader::new(
        pipe::OpenOptions::new()
            .read_write(true)
            .open_receiver(&starts)
            .unwrap(),
    )
    .lines();
    let lifeline = pipe::OpenOptions::new().open_receiver(&held).unwrap();
    // The sleep is forked before the start is announced, so it exists whenever the test acts.
    let script =
        format!(r#"{preamble}exec 3>"$1"; sleep 1000 & echo started > "$0"; {after_start}"#);
    let spec = ProviderSpec {
        name: "wrapper".into(),
        exec: vec![
            "sh".into(),
            "-c".into(),
            script,
            starts.to_str().unwrap().into(),
            held.to_str().unwrap().into(),
        ],
        restart,
    };
    let supervisor = providers::supervise(&[spec], &dir.path().join(SOCKET_FILE));
    next_start(&mut started).await;
    (supervisor, lifeline, dir)
}

async fn expect_every_process_gone(lifeline: &mut pipe::Receiver) {
    let mut leftover = Vec::new();
    timeout(WAIT, lifeline.read_to_end(&mut leftover))
        .await
        .expect("a process of the provider's group is still running")
        .unwrap();
}

#[tokio::test]
async fn removing_a_provider_stops_the_children_it_started() {
    let (mut supervisor, mut lifeline, _dir) = provider_with_a_child("").await;

    supervisor.reconcile(&[]).await;

    expect_every_process_gone(&mut lifeline).await;
}

#[tokio::test]
async fn a_provider_that_ignores_sigterm_is_killed_after_the_grace_period() {
    let (mut supervisor, mut lifeline, _dir) = provider_with_a_child("trap '' TERM; ").await;

    timeout(WAIT, supervisor.reconcile(&[]))
        .await
        .expect("the stop never finished");

    expect_every_process_gone(&mut lifeline).await;
}

#[tokio::test]
async fn a_provider_that_exits_on_its_own_does_not_leave_its_children_behind() {
    // It exits at once and is restarted after a second, so the children of its first run are
    // gone by the time the reader sees end of file only if the supervisor cleaned them up.
    let (supervisor, mut lifeline, _dir) = provider_that_starts_a_child("", "exit 0", true).await;

    expect_every_process_gone(&mut lifeline).await;

    drop(supervisor);
}

#[tokio::test]
async fn providers_that_ignore_sigterm_are_stopped_together() {
    let dir = tempfile::tempdir().unwrap();
    let starts = dir.path().join("starts");
    mkfifo(&starts, Mode::S_IRWXU).unwrap();
    let mut started = BufReader::new(
        pipe::OpenOptions::new()
            .read_write(true)
            .open_receiver(&starts)
            .unwrap(),
    )
    .lines();
    let stubborn = |index: usize| ProviderSpec {
        name: format!("stubborn-{index}"),
        exec: vec![
            "sh".into(),
            "-c".into(),
            r#"trap '' TERM; echo started > "$0"; sleep 1000 & wait"#.into(),
            starts.to_str().unwrap().into(),
        ],
        restart: false,
    };
    let specs: Vec<ProviderSpec> = (0..STUBBORN_PROVIDERS).map(stubborn).collect();
    let mut supervisor = providers::supervise(&specs, &dir.path().join(SOCKET_FILE));
    for _ in &specs {
        next_start(&mut started).await;
    }

    // Each one holds out for the whole grace period, so stopping them one after another would
    // take longer than WAIT.
    timeout(WAIT, supervisor.reconcile(&[]))
        .await
        .expect("the providers were stopped one after another");
}

#[tokio::test]
async fn dropping_the_supervisor_kills_the_children_of_its_providers() {
    let (supervisor, mut lifeline, _dir) = provider_with_a_child("").await;

    drop(supervisor);

    expect_every_process_gone(&mut lifeline).await;
}

#[tokio::test]
async fn stale_socket_is_replaced_and_live_one_is_kept() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(SOCKET_FILE);
    leave_stale_socket(&path);
    assert!(fs::metadata(&path).unwrap().file_type().is_socket());

    let (_listener, _lock) = server::bind(&path).expect("a stale socket file is replaced");

    let mode = fs::metadata(&path).unwrap().permissions().mode();
    assert_eq!(mode & PERMISSION_BITS, PRIVATE_SOCKET_MODE);
    assert!(
        matches!(server::bind(&path), Err(BindError::InUse(_))),
        "a socket with a live listener must not be taken over"
    );
    UnixStream::connect(&path)
        .await
        .expect("the first listener still owns the path");
}

/// What an earlier daemon leaves behind when it dies: a socket file nothing listens on. A socket
/// inode made directly cannot be held open by a child forked while a listener exists.
fn leave_stale_socket(path: &Path) {
    mknod(path, SFlag::S_IFSOCK, Mode::S_IRWXU, 0).unwrap();
}

#[tokio::test]
async fn a_daemon_holding_the_lock_keeps_the_path_before_it_listens() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(SOCKET_FILE);
    let lock_file = fs::File::create(dir.path().join(format!("{SOCKET_FILE}.lock"))).unwrap();
    let _held = Flock::lock(lock_file, FlockArg::LockExclusiveNonblock).unwrap();
    // The holder has not got as far as binding, so nothing answers on the path yet.
    leave_stale_socket(&path);

    let result = server::bind(&path);

    assert!(matches!(result, Err(BindError::InUse(_))));
    assert!(
        fs::symlink_metadata(&path).unwrap().file_type().is_socket(),
        "a second daemon replaced the path of one that is still starting"
    );
}

#[tokio::test]
async fn overlong_line_is_rejected() {
    let daemon = Daemon::start().await;
    let mut witness = daemon.subscribed_ui(&["big.data"]).await;
    let mut provider = daemon.provider("big", &["big.data"]).await;
    let envelope = json!({"t": "publish", "topic": "big.data", "data": ""}).to_string();

    let fits = "x".repeat(MAX_LINE_BYTES - envelope.len());
    provider
        .send(json!({"t": "publish", "topic": "big.data", "data": fits}))
        .await;
    assert_eq!(
        witness.recv_data("big.data").await["data"]
            .as_str()
            .unwrap()
            .len(),
        fits.len()
    );

    let too_long = "x".repeat(MAX_LINE_BYTES + 1 - envelope.len());
    provider
        .send(json!({"t": "publish", "topic": "big.data", "data": too_long}))
        .await;
    provider
        .expect_error(&format!("line exceeds {MAX_LINE_BYTES} bytes"))
        .await;
}

#[tokio::test]
async fn slow_ui_client_is_disconnected() {
    let daemon = Daemon::start().await;
    let mut slow = daemon.subscribed_ui(&["bulk.data"]).await;
    let mut provider = daemon.provider("bulk", &["bulk.data"]).await;
    let payload = "x".repeat(BULK_PAYLOAD_BYTES);

    for _ in 0..BULK_PUBLISHES {
        provider
            .send(json!({"t": "publish", "topic": "bulk.data", "data": payload}))
            .await;
    }

    slow.expect_closed().await;
}

#[tokio::test]
async fn invalid_subscription_closes_the_ui_client() {
    let daemon = Daemon::start().await;
    let mut ui = daemon.ui().await;

    ui.send(json!({"t": "subscribe", "topics": ["cpu", "Bad Topic"]}))
        .await;

    ui.expect_error("invalid topic name \"Bad Topic\"").await;
}
