//! A private D-Bus message bus, started the way `dbus-run-session` starts one, so that tests never
//! touch the user's session bus; and the desktop feed running on it.

use std::collections::BTreeSet;
use std::fs;
use std::future::poll_fn;
use std::io::{BufRead, BufReader};
use std::pin::Pin;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use futures_core::Stream;
use ghost_proto::{DesktopSnapshot, ResolvedConfig, topics};
use ghostd::feeds::desktop;
use ghostd::feeds::shortcuts::Registration;
use ghostd::server::{FeedContext, Publisher};
use serde_json::Value;
use tempfile::TempDir;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tokio::time::timeout;
use zbus::proxy::CacheProperties;
use zbus::{Connection, connection, fdo};

pub const GHOSTD: &str = "org.ghostshell.Ghostd";
pub const DESKTOP_PATH: &str = "/org/ghostshell/Desktop";
pub const DESKTOP_INTERFACE: &str = "org.ghostshell.Desktop1";
pub const BUS_WAIT: Duration = Duration::from_secs(5);
const CONFIG_FILE: &str = "bus.conf";
// No service directories, so nothing on this bus can be activated.
const CONFIG: &str = r#"<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-BUS Bus Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
  <type>session</type>
  <listen>unix:dir=DIR</listen>
  <policy context="default">
    <allow send_destination="*" eavesdrop="true"/>
    <allow eavesdrop="true"/>
    <allow own="*"/>
  </policy>
</busconfig>
"#;
const DIR_PLACEHOLDER: &str = "DIR";

pub struct PrivateBus {
    daemon: Child,
    address: String,
    _dir: TempDir,
}

impl PrivateBus {
    pub fn start() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join(CONFIG_FILE);
        let listen_dir = dir.path().to_str().expect("the temp dir is UTF-8");
        fs::write(&config, CONFIG.replace(DIR_PLACEHOLDER, listen_dir)).unwrap();
        let mut daemon = Command::new("dbus-daemon")
            .arg(format!("--config-file={}", config.display()))
            .args(["--nofork", "--print-address"])
            .stdout(Stdio::piped())
            .spawn()
            .expect("the private-bus tests need dbus-daemon");
        let mut address = String::new();
        BufReader::new(daemon.stdout.take().unwrap())
            .read_line(&mut address)
            .unwrap();
        Self {
            daemon,
            address: address.trim().to_owned(),
            _dir: dir,
        }
    }

    pub fn builder(&self) -> connection::Builder<'static> {
        connection::Builder::address(self.address.as_str())
    }

    pub async fn connect(&self) -> Connection {
        self.builder().build().await.unwrap()
    }
}

impl Drop for PrivateBus {
    fn drop(&mut self) {
        // The bus is this test's own child, started above.
        let _ = self.daemon.kill();
        let _ = self.daemon.wait();
    }
}

/// The next item of `stream`, or `None` once it has ended.
pub async fn next<S: Stream + Unpin>(stream: &mut S) -> Option<S::Item> {
    poll_fn(|context| Pin::new(&mut *stream).poll_next(context)).await
}

/// The real desktop feed on a private bus, and everything it publishes.
pub struct RunningDesktop {
    pub published: mpsc::UnboundedReceiver<(String, Value)>,
    pub config: watch::Sender<ResolvedConfig>,
    _demand: watch::Sender<BTreeSet<String>>,
    _feed: JoinHandle<()>,
}

impl RunningDesktop {
    /// Starts the feed and returns once it owns its bus name, its default already published.
    pub async fn start(bus: &PrivateBus, registration: Registration) -> Self {
        let watcher = bus.connect().await;
        let mut owners = fdo::DBusProxy::builder(&watcher)
            .cache_properties(CacheProperties::No)
            .build()
            .await
            .unwrap()
            .receive_name_owner_changed_with_args(&[(0, GHOSTD)])
            .await
            .unwrap();
        let (published_sender, published) = mpsc::unbounded_channel();
        let (demand, demand_receiver) = watch::channel(BTreeSet::new());
        let (config, config_receiver) = watch::channel(ResolvedConfig::default());
        let context = FeedContext {
            publisher: Publisher::new(move |topic, data| {
                published_sender.send((topic.to_owned(), data)).unwrap();
            }),
            demand: demand_receiver,
            config: config_receiver,
        };
        let feed = tokio::spawn(desktop::feed_on(bus.builder(), registration).run(context));
        timeout(BUS_WAIT, next(&mut owners))
            .await
            .expect("the feed never took its bus name")
            .unwrap();
        let mut running = Self {
            published,
            config,
            _demand: demand,
            _feed: feed,
        };
        assert_eq!(running.next_snapshot().await, DesktopSnapshot::default());
        running
    }

    pub async fn next_snapshot(&mut self) -> DesktopSnapshot {
        let (topic, data) = timeout(BUS_WAIT, self.published.recv())
            .await
            .expect("nothing was published in time")
            .unwrap();
        assert_eq!(topic, topics::DESKTOP);
        serde_json::from_value(data).unwrap()
    }
}

/// Calls `method` on the desktop service from `caller`, as the KWin script or a shortcut does.
pub async fn call_desktop<B>(caller: &Connection, method: &str, body: &B)
where
    B: serde::Serialize + zbus::zvariant::DynamicType,
{
    caller
        .call_method(
            Some(GHOSTD),
            DESKTOP_PATH,
            Some(DESKTOP_INTERFACE),
            method,
            body,
        )
        .await
        .unwrap();
}
