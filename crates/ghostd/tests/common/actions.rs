//! Stand-ins for every service the actions feed calls, on private buses only: a lock, logout or
//! suspend that reached a real bus would act on the machine running the tests.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use zbus::message::{Flags, Header};
use zbus::{Connection, interface};

use super::bus::PrivateBus;

const SCREEN_SAVER: &str = "org.freedesktop.ScreenSaver";
const SHUTDOWN: &str = "org.kde.Shutdown";
const KRUNNER: &str = "org.kde.krunner";
const KGLOBALACCEL: &str = "org.kde.kglobalaccel";
const LOGIN1: &str = "org.freedesktop.login1";
const SETTLE: Duration = Duration::from_secs(5);
const POLL: Duration = Duration::from_millis(20);

/// One call a stub received.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Seen {
    pub service: &'static str,
    pub path: &'static str,
    pub member: &'static str,
    pub arg: String,
    pub no_auto_start: bool,
}

#[derive(Clone, Default)]
pub struct Log(Arc<Mutex<Vec<Seen>>>);

impl Log {
    fn note(
        &self,
        header: &Header<'_>,
        service: &'static str,
        path: &'static str,
        member: &'static str,
        arg: String,
    ) {
        self.lock().push(Seen {
            service,
            path,
            member,
            arg,
            no_auto_start: header.primary().flags().contains(Flags::NO_AUTO_START),
        });
    }

    pub fn all(&self) -> Vec<Seen> {
        self.lock().clone()
    }

    /// Waits until `count` calls have arrived, then returns them.
    pub async fn wait_for(&self, count: usize) -> Vec<Seen> {
        let waited = tokio::time::timeout(SETTLE, async {
            while self.lock().len() < count {
                tokio::time::sleep(POLL).await;
            }
        })
        .await;
        let seen = self.all();
        assert!(waited.is_ok(), "expected {count} calls, saw {seen:?}");
        seen
    }

    fn lock(&self) -> MutexGuard<'_, Vec<Seen>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Stops a test from reaching the machine's own buses.
pub fn assert_private(bus: &PrivateBus) {
    let address = bus.address();
    assert!(
        address.starts_with("unix:")
            && !address.contains("/run/user/")
            && !address.contains("/run/dbus/"),
        "refusing bus address {address:?}"
    );
}

pub struct Saver(Log);

#[interface(name = "org.freedesktop.ScreenSaver")]
impl Saver {
    fn lock(&self, #[zbus(header)] header: Header<'_>) {
        self.0
            .note(&header, SCREEN_SAVER, "/ScreenSaver", "Lock", String::new());
    }
}

pub struct Shutdown(Log);

#[interface(name = "org.kde.Shutdown")]
impl Shutdown {
    #[zbus(name = "logout")]
    fn logout(&self, #[zbus(header)] header: Header<'_>) {
        self.0
            .note(&header, SHUTDOWN, "/Shutdown", "logout", String::new());
    }

    #[zbus(name = "logoutAndReboot")]
    fn logout_and_reboot(&self, #[zbus(header)] header: Header<'_>) {
        self.0.note(
            &header,
            SHUTDOWN,
            "/Shutdown",
            "logoutAndReboot",
            String::new(),
        );
    }

    #[zbus(name = "logoutAndShutdown")]
    fn logout_and_shutdown(&self, #[zbus(header)] header: Header<'_>) {
        self.0.note(
            &header,
            SHUTDOWN,
            "/Shutdown",
            "logoutAndShutdown",
            String::new(),
        );
    }
}

pub struct Runner(Log);

#[interface(name = "org.kde.krunner.App")]
impl Runner {
    #[zbus(name = "query")]
    fn query(&self, text: &str, #[zbus(header)] header: Header<'_>) {
        self.0
            .note(&header, KRUNNER, "/App", "query", text.to_owned());
    }
}

pub struct Component(Log);

#[interface(name = "org.kde.kglobalaccel.Component")]
impl Component {
    #[zbus(name = "invokeShortcut")]
    fn invoke_shortcut(&self, name: &str, #[zbus(header)] header: Header<'_>) {
        self.0.note(
            &header,
            KGLOBALACCEL,
            "/component/plasmashell",
            "invokeShortcut",
            name.to_owned(),
        );
    }
}

pub struct Manager(Log);

#[interface(name = "org.freedesktop.login1.Manager")]
impl Manager {
    fn suspend(&self, interactive: bool, #[zbus(header)] header: Header<'_>) {
        self.0.note(
            &header,
            LOGIN1,
            "/org/freedesktop/login1",
            "Suspend",
            interactive.to_string(),
        );
    }
}

/// Which stubs to put on the session bus.
#[derive(Clone, Copy)]
pub struct SessionStubs {
    pub screen_saver: bool,
}

impl SessionStubs {
    pub const ALL: Self = Self { screen_saver: true };
}

/// Serves and names the stubs, keeping the connections open for as long as it lives.
pub struct Stubs {
    pub log: Log,
    _connections: Vec<Connection>,
}

impl Stubs {
    pub async fn start(session: &PrivateBus, system: &PrivateBus, which: SessionStubs) -> Self {
        assert_private(session);
        assert_private(system);
        let log = Log::default();
        let mut connections = Vec::new();
        if which.screen_saver {
            connections
                .push(serve(session, SCREEN_SAVER, "/ScreenSaver", Saver(log.clone())).await);
        }
        connections.push(serve(session, SHUTDOWN, "/Shutdown", Shutdown(log.clone())).await);
        connections.push(serve(session, KRUNNER, "/App", Runner(log.clone())).await);
        connections.push(
            serve(
                session,
                KGLOBALACCEL,
                "/component/plasmashell",
                Component(log.clone()),
            )
            .await,
        );
        connections.push(
            serve(
                system,
                LOGIN1,
                "/org/freedesktop/login1",
                Manager(log.clone()),
            )
            .await,
        );
        Self {
            log,
            _connections: connections,
        }
    }
}

async fn serve<I: zbus::object_server::Interface>(
    bus: &PrivateBus,
    name: &'static str,
    path: &'static str,
    stub: I,
) -> Connection {
    let connection = bus.connect().await;
    connection.object_server().at(path, stub).await.unwrap();
    connection.request_name(name).await.unwrap();
    connection
}
