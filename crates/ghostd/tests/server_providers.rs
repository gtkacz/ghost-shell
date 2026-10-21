mod common;

use std::fs;
use std::time::{Duration, Instant};

use common::server::*;
use ghost_proto::ProviderSpec;
use ghostd::providers;
use nix::sys::stat::Mode;
use nix::unistd::mkfifo;
use serde_json::json;
use tempfile::TempDir;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader, Lines};
use tokio::net::unix::pipe;
use tokio::time::timeout;

const RESTART_WINDOW: Duration = Duration::from_secs(3);
// Half the first restart delay, so a supervisor that restarts without waiting is caught while
// scheduler jitter is not.
const MIN_RESTART_GAP: Duration = Duration::from_millis(500);
// Three grace periods of two seconds do not fit in WAIT; one does.
const STUBBORN_PROVIDERS: usize = 3;

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

/// The `GHOST_SOCKET` value the next provider run wrote into the FIFO.
async fn next_start(started: &mut Lines<BufReader<pipe::Receiver>>) -> String {
    timeout(RESTART_WINDOW, started.next_line())
        .await
        .expect("provider not started in time")
        .unwrap()
        .expect("the FIFO closed")
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
