//! A fake KWin ScreenSaver on a private bus that stands in for the session bus, and a fake logind
//! on another that stands in for the system bus, each shaped as the live one: `GetActive` and
//! `ActiveChanged` at `/ScreenSaver`, and `GetUser`, `PrepareForSleep`, `PreparingForSleep` and
//! `User.Display`.

use nix::unistd::getuid;
use zbus::message::Header;
use zbus::object_server::SignalEmitter;
use zbus::{Connection, OwnedObjectPath, fdo, interface};

use super::bus::PrivateBus;
use super::calls::Calls;

const SCREEN_SAVER: &str = "org.freedesktop.ScreenSaver";
pub const SCREEN_SAVER_PATH: &str = "/ScreenSaver";
const LOGIN1: &str = "org.freedesktop.login1";
const LOGIN1_PATH: &str = "/org/freedesktop/login1";
const USER_PATH: &str = "/org/freedesktop/login1/user/_1000";
const SESSION_PATH: &str = "/org/freedesktop/login1/session/_32";

struct FakeSaver {
    active: bool,
    calls: Calls,
}

#[interface(name = "org.freedesktop.ScreenSaver")]
impl FakeSaver {
    fn get_active(&self, #[zbus(header)] header: Header<'_>) -> bool {
        self.calls.method(&header, "GetActive");
        self.active
    }

    #[zbus(signal)]
    async fn active_changed(emitter: &SignalEmitter<'_>, active: bool) -> zbus::Result<()>;
}

/// KWin's ScreenSaver, unlocked until told otherwise.
pub struct FakeScreenSaver {
    connection: Connection,
    pub calls: Calls,
}

impl FakeScreenSaver {
    /// Serves the object without taking the ScreenSaver's name.
    pub async fn serve(bus: &PrivateBus, active: bool) -> Self {
        let calls = Calls::default();
        let connection = bus.connect().await;
        let saver = FakeSaver {
            active,
            calls: calls.clone(),
        };
        connection
            .object_server()
            .at(SCREEN_SAVER_PATH, saver)
            .await
            .unwrap();
        Self { connection, calls }
    }

    pub async fn claim(&self) {
        self.connection.request_name(SCREEN_SAVER).await.unwrap();
    }

    pub async fn release(&self) {
        self.connection.release_name(SCREEN_SAVER).await.unwrap();
    }

    pub async fn start(bus: &PrivateBus, active: bool) -> Self {
        let saver = Self::serve(bus, active).await;
        saver.claim().await;
        saver
    }

    /// Locks or unlocks and says so, as KWin does.
    pub async fn set_active(&self, active: bool) {
        let saver = self
            .connection
            .object_server()
            .interface::<_, FakeSaver>(SCREEN_SAVER_PATH)
            .await
            .unwrap();
        saver.get_mut().await.active = active;
        FakeSaver::active_changed(saver.signal_emitter(), active)
            .await
            .unwrap();
    }
}

struct FakeManager {
    preparing_for_sleep: bool,
    calls: Calls,
}

#[interface(name = "org.freedesktop.login1.Manager")]
impl FakeManager {
    /// Knows only the uid the tests run as, as logind knows only the users logged in.
    fn get_user(
        &self,
        uid: u32,
        #[zbus(header)] header: Header<'_>,
    ) -> fdo::Result<OwnedObjectPath> {
        self.calls.method(&header, "GetUser");
        if uid != getuid().as_raw() {
            return Err(fdo::Error::Failed(format!("no user {uid}")));
        }
        Ok(OwnedObjectPath::try_from(USER_PATH).unwrap())
    }

    #[zbus(property)]
    fn preparing_for_sleep(&self, #[zbus(header)] header: Option<Header<'_>>) -> bool {
        self.calls.property(header.as_ref(), "PreparingForSleep");
        self.preparing_for_sleep
    }

    #[zbus(signal)]
    async fn prepare_for_sleep(emitter: &SignalEmitter<'_>, going: bool) -> zbus::Result<()>;
}

struct FakeUser {
    display_id: &'static str,
    calls: Calls,
}

#[interface(name = "org.freedesktop.login1.User")]
impl FakeUser {
    #[zbus(property)]
    fn display(&self, #[zbus(header)] header: Option<Header<'_>>) -> (String, OwnedObjectPath) {
        self.calls.property(header.as_ref(), "Display");
        (
            self.display_id.to_owned(),
            OwnedObjectPath::try_from(SESSION_PATH).unwrap(),
        )
    }
}

/// logind with the test user's graphical session at `display_id`.
pub struct FakeLogin1 {
    connection: Connection,
    pub calls: Calls,
}

impl FakeLogin1 {
    /// Serves the objects without taking logind's name.
    pub async fn serve(bus: &PrivateBus, display_id: &'static str) -> Self {
        let calls = Calls::default();
        let connection = bus.connect().await;
        let server = connection.object_server();
        let manager = FakeManager {
            preparing_for_sleep: false,
            calls: calls.clone(),
        };
        server.at(LOGIN1_PATH, manager).await.unwrap();
        let user = FakeUser {
            display_id,
            calls: calls.clone(),
        };
        server.at(USER_PATH, user).await.unwrap();
        Self { connection, calls }
    }

    pub async fn claim(&self) {
        self.connection.request_name(LOGIN1).await.unwrap();
    }

    pub async fn release(&self) {
        self.connection.release_name(LOGIN1).await.unwrap();
    }

    pub async fn start(bus: &PrivateBus, display_id: &'static str) -> Self {
        let login = Self::serve(bus, display_id).await;
        login.claim().await;
        login
    }

    /// Says the machine is about to sleep, or has resumed, as logind does, property first.
    pub async fn prepare_for_sleep(&self, going: bool) {
        let manager = self
            .connection
            .object_server()
            .interface::<_, FakeManager>(LOGIN1_PATH)
            .await
            .unwrap();
        manager.get_mut().await.preparing_for_sleep = going;
        FakeManager::prepare_for_sleep(manager.signal_emitter(), going)
            .await
            .unwrap();
    }
}

/// A connection that is not the ScreenSaver's owner emits `ActiveChanged`, as a stray client
/// could.
pub async fn emit_active_changed_from(other: &Connection, active: bool) {
    other
        .emit_signal(
            None::<&str>,
            SCREEN_SAVER_PATH,
            SCREEN_SAVER,
            "ActiveChanged",
            &(active,),
        )
        .await
        .unwrap();
}
