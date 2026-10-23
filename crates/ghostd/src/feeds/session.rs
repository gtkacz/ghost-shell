//! The `session` topic: whether the screen is locked or the machine asleep, and when the shell
//! should greet.
//!
//! The lock comes from `org.freedesktop.ScreenSaver` on the session bus, which KWin owns, and
//! sleep from logind's `PrepareForSleep` on the system bus. Logind's own `Lock` and `Unlock`
//! signals ask lockers to lock rather than report that the screen is locked.
//!
//! The feed runs whether or not any client wants the topic: a lock missed while no shell was
//! connected would break the pairing of a wake with the unlock that follows it. Each service is
//! D-Bus activatable, so every call asks the bus not to start it; an absent service reads as
//! unchanged, and the feed waits for its name to gain an owner.
//!
//! The first snapshot is published once both buses have been read, or have failed to be, so the
//! shell never sees the session id arrive before the lock state.

use std::future::pending;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use ghost_proto::{GreetingKind, SessionSnapshot, topics};
use nix::unistd::getuid;
use serde::Serialize;
use tokio::time::error::Elapsed;
use tokio::time::timeout;
use tracing::{debug, warn};
use zbus::fdo::NameOwnerChangedStream;
use zbus::proxy::{self, CacheProperties, MethodFlags};
use zbus::wire::DynamicDeserialize;
use zbus::{Connection, DynamicType, MatchRule, MessageStream, OwnedObjectPath, OwnedVariant};
use zbus::{fdo, message};

use super::{drain, next};
use crate::server::{Feed, Publisher};

const SCREEN_SAVER: &str = "org.freedesktop.ScreenSaver";
const SCREEN_SAVER_PATH: &str = "/ScreenSaver";
const ACTIVE_CHANGED: &str = "ActiveChanged";
const GET_ACTIVE: &str = "GetActive";
const LOGIN1: &str = "org.freedesktop.login1";
const LOGIN1_PATH: &str = "/org/freedesktop/login1";
const LOGIN1_MANAGER: &str = "org.freedesktop.login1.Manager";
const LOGIN1_USER: &str = "org.freedesktop.login1.User";
const PREPARE_FOR_SLEEP: &str = "PrepareForSleep";
const PREPARING_FOR_SLEEP: &str = "PreparingForSleep";
const GET_USER: &str = "GetUser";
const DISPLAY: &str = "Display";
const PROPERTIES: &str = "org.freedesktop.DBus.Properties";
const GET: &str = "Get";
/// Either service answers in milliseconds; one that takes longer is stuck, and what was read
/// before stays up meanwhile.
const READ_TIMEOUT: Duration = Duration::from_secs(2);

/// What the screen and the machine have done, and the greeting that follows from it.
#[derive(Debug, Default)]
pub struct SessionState {
    snapshot: SessionSnapshot,
    /// The machine resumed while the screen was locked, so the unlock that follows is the wake.
    woke_while_locked: bool,
}

impl SessionState {
    /// Returns whether anything changed. An unlock asks for a greeting; a wake that happened under
    /// the lock turns it into a `Wake`.
    pub fn lock(&mut self, active: bool) -> bool {
        if active == self.snapshot.locked {
            return false;
        }
        self.snapshot.locked = active;
        if !active {
            let kind = if self.woke_while_locked {
                GreetingKind::Wake
            } else {
                GreetingKind::Unlock
            };
            self.woke_while_locked = false;
            self.greet(kind);
        }
        true
    }

    /// Returns whether anything changed. A resume under the lock waits for the unlock to greet; a
    /// resume without a lock, even from a suspend that failed, greets at once.
    pub fn sleep(&mut self, going: bool) -> bool {
        if going == self.snapshot.sleeping {
            return false;
        }
        self.snapshot.sleeping = going;
        if !going {
            if self.snapshot.locked {
                self.woke_while_locked = true;
            } else {
                self.greet(GreetingKind::Wake);
            }
        }
        true
    }

    pub fn set_id(&mut self, id: String) -> bool {
        if id == self.snapshot.id {
            return false;
        }
        self.snapshot.id = id;
        true
    }

    /// The ScreenSaver's owner going away is no unlock the user made, so it clears the lock
    /// without a greeting, and with it any wake that was waiting for that lock's unlock.
    pub fn owner_lost(&mut self) -> bool {
        self.woke_while_locked = false;
        if !self.snapshot.locked {
            return false;
        }
        self.snapshot.locked = false;
        true
    }

    pub fn snapshot(&self) -> SessionSnapshot {
        self.snapshot.clone()
    }

    fn greet(&mut self, kind: GreetingKind) {
        self.snapshot.greeting = Some(kind);
        self.snapshot.greeting_seq += 1;
    }
}

#[derive(Clone, Copy)]
enum Lane {
    ScreenSaver,
    Login,
}

#[derive(Default)]
struct Shared {
    state: SessionState,
    screen_saver_read: bool,
    login_read: bool,
    published: bool,
}

/// The state both lanes change, and the publisher it goes out on.
struct Session {
    publisher: Publisher,
    shared: Mutex<Shared>,
}

impl Session {
    fn new(publisher: Publisher) -> Self {
        Self {
            publisher,
            shared: Mutex::default(),
        }
    }

    /// Applies `change` for `lane`, which counts as that lane having read, and publishes once both
    /// have, then on every change.
    fn update(&self, lane: Lane, change: impl FnOnce(&mut SessionState) -> bool) {
        let mut shared = self.lock();
        let changed = change(&mut shared.state);
        match lane {
            Lane::ScreenSaver => shared.screen_saver_read = true,
            Lane::Login => shared.login_read = true,
        }
        if shared.screen_saver_read && shared.login_read && (changed || !shared.published) {
            // Published under the lock, so two quick changes can never reach clients swapped.
            self.publisher
                .publish(topics::SESSION, &shared.state.snapshot());
            shared.published = true;
        }
    }

    fn lock(&self) -> MutexGuard<'_, Shared> {
        self.shared.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Reads the ScreenSaver on the session bus and logind on the system bus, each connected to at
/// start.
pub fn feed() -> Feed {
    Feed::new(topics::SESSION, |context| async move {
        let session = Connection::session()
            .await
            .inspect_err(|error| {
                warn!(%error, "cannot connect to the session bus; session never reports a lock");
            })
            .ok();
        let system = Connection::system()
            .await
            .inspect_err(|error| {
                warn!(%error, "cannot connect to the system bus; session has no id and no sleep");
            })
            .ok();
        run(session, system, context.publisher).await;
    })
}

/// As [`feed`], reading the ScreenSaver on `session` and logind on `system`.
pub fn feed_on(session: Connection, system: Option<Connection>) -> Feed {
    Feed::new(topics::SESSION, move |context| {
        run(Some(session), system, context.publisher)
    })
}

async fn run(session: Option<Connection>, system: Option<Connection>, publisher: Publisher) {
    let state = Session::new(publisher);
    tokio::join!(
        watch_screen_saver(session.as_ref(), &state),
        watch_login(system.as_ref(), &state),
    );
}

/// Follows the lock: read now, again whenever the ScreenSaver gains or loses an owner, and by its
/// `ActiveChanged` signal in between.
async fn watch_screen_saver(bus: Option<&Connection>, session: &Session) {
    let Some(bus) = bus else {
        session.update(Lane::ScreenSaver, |_| false);
        return pending().await;
    };
    let mut changes = ScreenSaverChanges::listen(bus)
        .await
        .inspect_err(|error| {
            warn!(%error, "cannot watch the ScreenSaver; session reads the lock only once");
        })
        .ok();
    loop {
        if let Some(changes) = &mut changes {
            // A read covers every change queued before it starts.
            changes.drain();
        }
        match timeout(READ_TIMEOUT, read_active(bus)).await {
            Ok(Ok(active)) => session.update(Lane::ScreenSaver, |state| state.lock(active)),
            Ok(Err(error)) => {
                debug!(%error, "the ScreenSaver is not answering; the screen reads as unlocked");
                session.update(Lane::ScreenSaver, SessionState::owner_lost);
            }
            Err(_) => {
                warn!("the ScreenSaver did not answer within {READ_TIMEOUT:?}; keeping the lock");
                session.update(Lane::ScreenSaver, |_| false);
            }
        }
        match &mut changes {
            Some(changes) => changes.next(session).await,
            None => return pending().await,
        }
    }
}

/// Follows sleep by logind's `PrepareForSleep`, and reads the session id and whether the machine is
/// going to sleep again whenever logind gains or loses an owner, so a resume missed while it had
/// none cannot leave the session asleep.
async fn watch_login(bus: Option<&Connection>, session: &Session) {
    let Some(bus) = bus else {
        session.update(Lane::Login, |_| false);
        return pending().await;
    };
    let mut changes = LoginChanges::listen(bus)
        .await
        .inspect_err(|error| {
            warn!(%error, "cannot watch logind; session reads its id only once and never sleeps");
        })
        .ok();
    loop {
        if let Some(changes) = &mut changes {
            // A read covers every change queued before it starts.
            changes.drain();
        }
        let id = answered(
            timeout(READ_TIMEOUT, read_session_id(bus)).await,
            "the session id",
        );
        let going = answered(
            timeout(READ_TIMEOUT, read_preparing_for_sleep(bus)).await,
            "the sleep state",
        );
        session.update(Lane::Login, |state| {
            let id_changed = id.is_some_and(|id| state.set_id(id));
            let sleep_changed = going.is_some_and(|going| state.sleep(going));
            id_changed || sleep_changed
        });
        match &mut changes {
            Some(changes) => changes.next(session).await,
            None => return pending().await,
        }
    }
}

/// What logind answered, or `None` when it did not, which keeps `what` as it was.
fn answered<T>(read: Result<zbus::Result<T>, Elapsed>, what: &str) -> Option<T> {
    match read {
        Ok(Ok(value)) => Some(value),
        Ok(Err(error)) => {
            debug!(%error, "logind is not answering; keeping {what}");
            None
        }
        Err(_) => {
            warn!("logind did not answer within {READ_TIMEOUT:?}; keeping {what}");
            None
        }
    }
}

async fn owner_changes(
    bus: &Connection,
    name: &'static str,
) -> zbus::Result<NameOwnerChangedStream> {
    fdo::DBusProxy::builder(bus)
        .cache_properties(CacheProperties::No)
        .build()
        .await?
        .receive_name_owner_changed_with_args(&[(0, name)])
        .await
}

/// A signal of `interface` that only `service`'s owner can send. The bus, not zbus, applies a
/// rule's well-known sender, following the name to its owner, so another connection's signal of
/// the same name never matches.
async fn signals(
    bus: &Connection,
    service: &'static str,
    path: &'static str,
    interface: &'static str,
    member: &'static str,
) -> zbus::Result<MessageStream> {
    let rule = MatchRule::builder()
        .msg_type(message::Type::Signal)
        .sender(service)
        .path(path)
        .interface(interface)
        .member(member)
        .build()?;
    MessageStream::for_match_rule(rule, bus, None).await
}

/// The bool a signal carries, or `None` for one that carries something else.
fn flag(message: zbus::Result<zbus::Message>) -> Option<bool> {
    message
        .and_then(|message| message.body().deserialize::<bool>())
        .inspect_err(|error| warn!(%error, "ignoring a malformed session signal"))
        .ok()
}

/// The ScreenSaver's owner and `ActiveChanged` signals.
struct ScreenSaverChanges {
    owners: NameOwnerChangedStream,
    active: MessageStream,
}

impl ScreenSaverChanges {
    /// Listens on `bus`; the ScreenSaver must be read only after this, so that no change is missed.
    async fn listen(bus: &Connection) -> zbus::Result<Self> {
        Ok(Self {
            owners: owner_changes(bus, SCREEN_SAVER).await?,
            active: signals(
                bus,
                SCREEN_SAVER,
                SCREEN_SAVER_PATH,
                SCREEN_SAVER,
                ACTIVE_CHANGED,
            )
            .await?,
        })
    }

    /// Drops every change already queued.
    fn drain(&mut self) {
        drain(&mut self.owners);
        drain(&mut self.active);
    }

    /// Applies the lock signals as they come, and returns once the ScreenSaver gains or loses an
    /// owner. Never returns once the bus is gone.
    async fn next(&mut self, session: &Session) {
        loop {
            tokio::select! {
                Some(_) = next(&mut self.owners) => return,
                Some(message) = next(&mut self.active) => {
                    if let Some(active) = flag(message) {
                        session.update(Lane::ScreenSaver, |state| state.lock(active));
                    }
                }
                else => pending::<()>().await,
            }
        }
    }
}

/// Logind's owner and `PrepareForSleep` signals.
struct LoginChanges {
    owners: NameOwnerChangedStream,
    sleep: MessageStream,
}

impl LoginChanges {
    /// Listens on `bus`; logind must be read only after this, so that no change is missed.
    async fn listen(bus: &Connection) -> zbus::Result<Self> {
        Ok(Self {
            owners: owner_changes(bus, LOGIN1).await?,
            sleep: signals(bus, LOGIN1, LOGIN1_PATH, LOGIN1_MANAGER, PREPARE_FOR_SLEEP).await?,
        })
    }

    fn drain(&mut self) {
        drain(&mut self.owners);
        drain(&mut self.sleep);
    }

    /// As [`ScreenSaverChanges::next`], for sleep.
    async fn next(&mut self, session: &Session) {
        loop {
            tokio::select! {
                Some(_) = next(&mut self.owners) => return,
                Some(message) = next(&mut self.sleep) => {
                    if let Some(going) = flag(message) {
                        session.update(Lane::Login, |state| state.sleep(going));
                    }
                }
                else => pending::<()>().await,
            }
        }
    }
}

/// Calls `method` without letting the bus start `service` for it: an absent service must read as
/// absent, not be launched by a dashboard.
async fn call<B, R>(
    bus: &Connection,
    service: &'static str,
    path: &str,
    interface: &'static str,
    method: &'static str,
    body: &B,
) -> zbus::Result<R>
where
    B: Serialize + DynamicType,
    R: for<'d> DynamicDeserialize<'d>,
{
    let proxy: zbus::Proxy<'_> = proxy::Builder::new(bus)
        .destination(service)
        .path(path)
        .interface(interface)
        .cache_properties(CacheProperties::No)
        .build()
        .await?;
    proxy
        .call_with_flags(method, MethodFlags::NO_AUTO_START, body)
        .await?
        .ok_or_else(|| zbus::Error::Failure(format!("{method} returned no reply")))
}

async fn read_active(bus: &Connection) -> zbus::Result<bool> {
    call(
        bus,
        SCREEN_SAVER,
        SCREEN_SAVER_PATH,
        SCREEN_SAVER,
        GET_ACTIVE,
        &(),
    )
    .await
}

/// The id of the daemon's own user's graphical session, `""` when that user has none.
async fn read_session_id(bus: &Connection) -> zbus::Result<String> {
    let user: OwnedObjectPath = call(
        bus,
        LOGIN1,
        LOGIN1_PATH,
        LOGIN1_MANAGER,
        GET_USER,
        &(getuid().as_raw(),),
    )
    .await?;
    let display: OwnedVariant = call(
        bus,
        LOGIN1,
        user.as_str(),
        PROPERTIES,
        GET,
        &(LOGIN1_USER, DISPLAY),
    )
    .await?;
    let (id, _path) = <(String, OwnedObjectPath)>::try_from(display)?;
    Ok(id)
}

async fn read_preparing_for_sleep(bus: &Connection) -> zbus::Result<bool> {
    let going: OwnedVariant = call(
        bus,
        LOGIN1,
        LOGIN1_PATH,
        PROPERTIES,
        GET,
        &(LOGIN1_MANAGER, PREPARING_FOR_SLEEP),
    )
    .await?;
    bool::try_from(going)
}

#[cfg(test)]
mod tests {
    use ghost_proto::GreetingKind;

    use super::SessionState;

    #[test]
    fn unlock_greets_once() {
        let mut state = SessionState::default();
        assert!(state.lock(true));
        assert!(state.lock(false));
        let snapshot = state.snapshot();
        assert_eq!(snapshot.greeting, Some(GreetingKind::Unlock));
        assert_eq!(snapshot.greeting_seq, 1);
        assert!(!snapshot.locked);
    }

    #[test]
    fn wake_then_unlock_is_one_greeting() {
        let mut state = SessionState::default();
        state.lock(true);
        state.sleep(true);
        state.sleep(false);
        assert_eq!(state.snapshot().greeting_seq, 0);
        assert!(!state.snapshot().sleeping);
        state.lock(false);
        assert_eq!(state.snapshot().greeting, Some(GreetingKind::Wake));
        assert_eq!(state.snapshot().greeting_seq, 1);
    }

    #[test]
    fn failed_suspend_without_lock_greets_once() {
        let mut state = SessionState::default();
        state.sleep(true);
        state.sleep(false);
        assert_eq!(state.snapshot().greeting, Some(GreetingKind::Wake));
        assert_eq!(state.snapshot().greeting_seq, 1);
        assert!(!state.sleep(false));
        assert_eq!(state.snapshot().greeting_seq, 1);
    }

    #[test]
    fn lock_after_resume_waits_for_the_unlock() {
        let mut state = SessionState::default();
        state.sleep(true);
        state.sleep(false);
        assert_eq!(state.snapshot().greeting_seq, 1);
        state.lock(true);
        assert_eq!(state.snapshot().greeting_seq, 1);
        state.lock(false);
        assert_eq!(state.snapshot().greeting, Some(GreetingKind::Unlock));
        assert_eq!(state.snapshot().greeting_seq, 2);
    }

    #[test]
    fn repeated_reports_are_no_change() {
        let mut state = SessionState::default();
        assert!(state.lock(true));
        assert!(!state.lock(true));
        state.lock(false);
        let seq = state.snapshot().greeting_seq;
        assert!(!state.lock(false));
        assert_eq!(state.snapshot().greeting_seq, seq);
    }

    #[test]
    fn lost_screensaver_owner_unlocks_without_a_greeting() {
        let mut state = SessionState::default();
        state.lock(true);
        assert!(state.owner_lost());
        assert!(!state.snapshot().locked);
        assert_eq!(state.snapshot().greeting_seq, 0);
        assert!(!state.owner_lost());
    }

    #[test]
    fn lost_screensaver_owner_forgets_the_wake() {
        let mut state = SessionState::default();
        state.lock(true);
        state.sleep(true);
        state.sleep(false);
        state.owner_lost();
        state.lock(true);
        state.lock(false);
        assert_eq!(state.snapshot().greeting, Some(GreetingKind::Unlock));
        assert_eq!(state.snapshot().greeting_seq, 1);
    }

    #[test]
    fn session_id_is_published_once_known() {
        let mut state = SessionState::default();
        assert!(state.set_id("2".to_owned()));
        assert_eq!(state.snapshot().id, "2");
        assert!(!state.set_id("2".to_owned()));
    }
}
