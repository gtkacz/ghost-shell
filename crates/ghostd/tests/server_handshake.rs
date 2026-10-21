mod common;

use std::fs;
use std::os::unix::fs::{FileTypeExt, PermissionsExt};
use std::path::Path;
use std::time::{Duration, Instant};

use common::server::*;
use ghostd::server::{self, BindError};
use nix::fcntl::{Flock, FlockArg};
use nix::sys::stat::{Mode, SFlag, mknod};
use serde_json::{Value, json};
use tokio::net::UnixStream;
use tokio::time::timeout;

const HANDSHAKE_DEADLINE: Duration = Duration::from_millis(2500);
const QUIET_WINDOW: Duration = Duration::from_millis(600);
const HANDSHAKE_ERROR: &str = "expected hello or provide with proto 1";
const PRIVATE_SOCKET_MODE: u32 = 0o600;
const PERMISSION_BITS: u32 = 0o777;
const MAX_LINE_BYTES: usize = 64 * 1024;
const BULK_PAYLOAD_BYTES: usize = 60 * 1024;
// Well past the 256 KiB queue limit plus the kernel's socket buffer, so the client must be dropped.
const BULK_PUBLISHES: usize = 20;
const CPU_HISTORY_KEYS: [&str; 3] = ["load", "ccd0", "ccd1"];

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
    assert_eq!(config["data"]["appearance"]["alert_chips"], true);
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

    // Another client keeps the gpu sampled, so `gpu_load` is recorded and a history that leaks
    // unsubscribed series would show it now.
    let mut gpu_watcher = daemon.subscribed_ui(&["gpu"]).await;
    gpu_watcher.recv_data("gpu").await;
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
    let message = problem["message"].as_str().unwrap();
    assert!(
        !message.is_empty() && !message.contains('\n'),
        "a config error must fit one notification line: {message:?}"
    );
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
