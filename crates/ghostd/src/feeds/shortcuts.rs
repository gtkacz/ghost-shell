//! The view shortcuts, registered with KDE's global shortcut service, KGlobalAccel.
//!
//! The keys always come from `shortcuts.*` in the config, never from what KGlobalAccel stored, and
//! a press takes the same path as a D-Bus `Activate`.

use std::future::poll_fn;
use std::pin::Pin;

use futures_core::Stream;
use ghost_proto::{ResolvedConfig, Shortcuts};
use tokio::sync::watch;
use tracing::warn;
use zbus::proxy::CacheProperties;
use zbus::{Connection, MatchRule, Message, MessageStream, message, proxy};

use super::desktop::{DesktopService, MONITOR, SWAP};
use crate::keyseq;

const KGLOBALACCEL: &str = "org.kde.kglobalaccel";
const COMPONENT: &str = "ghost-shell";
// KGlobalAccel names a component's object after its id, with `-` replaced by `_`.
const COMPONENT_PATH: &str = "/component/ghost_shell";
const COMPONENT_INTERFACE: &str = "org.kde.kglobalaccel.Component";
const PRESSED_SIGNAL: &str = "globalShortcutPressed";
const MONITOR_DESCRIPTION: &str = "Toggle the monitor view";
const SWAP_DESCRIPTION: &str = "Next main view";
// `SetShortcutFlag` in kglobalaccel 6.31's src/kglobalaccel_p.h.
const SET_PRESENT: u32 = 2;
const NO_AUTOLOADING: u32 = 4;
// A Qt key sequence holds four keys, and KGlobalAccel's own client always sends all four.
const NO_KEY: i32 = 0;

#[proxy(
    interface = "org.kde.KGlobalAccel",
    default_service = "org.kde.kglobalaccel",
    default_path = "/kglobalaccel"
)]
trait KGlobalAccel {
    #[zbus(name = "doRegister")]
    fn do_register(&self, action_id: &[&str]) -> zbus::Result<()>;

    /// Returns the keys the action ends up with.
    #[zbus(name = "setShortcutKeys")]
    fn set_shortcut_keys(
        &self,
        action_id: &[&str],
        keys: &[(&[i32],)],
        flags: u32,
    ) -> zbus::Result<Vec<(Vec<i32>,)>>;
}

/// Registers the view shortcuts on `conn`, registers them again whenever `shortcuts.*` changes,
/// and activates a shortcut's view when it is pressed.
///
/// Without KGlobalAccel it logs once and keeps waiting for config changes; `Activate` still works.
/// Returns once the connection is closed and the config can no longer change, or at once when it
/// cannot listen for presses at all.
pub async fn run(
    conn: Connection,
    mut config: watch::Receiver<ResolvedConfig>,
    desktop: DesktopService,
) {
    // Listening starts before registering, so that no press can slip in between.
    let (kglobalaccel, mut presses) = match connect(&conn).await {
        Ok(connected) => connected,
        Err(error) => {
            warn!(%error, "cannot listen for global shortcuts; only Activate changes views");
            return;
        }
    };
    let mut shortcuts = config.borrow_and_update().shortcuts.clone();
    let mut failing = register_or_log(&kglobalaccel, &shortcuts, false).await;
    loop {
        tokio::select! {
            Some(press) = next(&mut presses) => activate(&desktop, press),
            Ok(()) = config.changed() => {
                let latest = config.borrow_and_update().shortcuts.clone();
                if latest != shortcuts {
                    shortcuts = latest;
                    failing = register_or_log(&kglobalaccel, &shortcuts, failing).await;
                }
            }
            else => return,
        }
    }
}

async fn connect(conn: &Connection) -> zbus::Result<(KGlobalAccelProxy<'static>, MessageStream)> {
    // The bus, not zbus, applies a rule's well-known sender, following the name to its owner.
    let pressed = MatchRule::builder()
        .msg_type(message::Type::Signal)
        .sender(KGLOBALACCEL)
        .path(COMPONENT_PATH)
        .interface(COMPONENT_INTERFACE)
        .member(PRESSED_SIGNAL)
        .build()?;
    let presses = MessageStream::for_match_rule(pressed, conn, None).await?;
    let kglobalaccel = KGlobalAccelProxy::builder(conn)
        .cache_properties(CacheProperties::No)
        .build()
        .await?;
    Ok((kglobalaccel, presses))
}

/// Logs a failure only after a success or at the start, so a session without KGlobalAccel logs
/// once. Returns whether this attempt failed.
async fn register_or_log(
    kglobalaccel: &KGlobalAccelProxy<'_>,
    shortcuts: &Shortcuts,
    failed_before: bool,
) -> bool {
    match register(kglobalaccel, shortcuts).await {
        Ok(()) => false,
        Err(error) => {
            if !failed_before {
                warn!(%error, "cannot register the view shortcuts; only Activate changes views");
            }
            true
        }
    }
}

async fn register(kglobalaccel: &KGlobalAccelProxy<'_>, shortcuts: &Shortcuts) -> zbus::Result<()> {
    let actions = [
        (MONITOR, MONITOR_DESCRIPTION, &shortcuts.monitor),
        (SWAP, SWAP_DESCRIPTION, &shortcuts.swap),
    ];
    for (action, description, sequence) in actions {
        // Loading the config already rejected sequences that do not parse.
        let key = match keyseq::parse(sequence) {
            Ok(key) => key,
            Err(error) => {
                warn!(%error, "the {action} shortcut is not registered");
                continue;
            }
        };
        let action_id = [COMPONENT, action, COMPONENT, description];
        kglobalaccel.do_register(&action_id).await?;
        let keys: &[i32] = &[key, NO_KEY, NO_KEY, NO_KEY];
        kglobalaccel
            .set_shortcut_keys(&action_id, &[(keys,)], SET_PRESENT | NO_AUTOLOADING)
            .await?;
    }
    Ok(())
}

fn activate(desktop: &DesktopService, press: zbus::Result<Message>) {
    let activated = press.and_then(|press| {
        let body = press.body();
        let (_component, shortcut, _timestamp): (&str, &str, i64) = body.deserialize()?;
        Ok(desktop.activate(shortcut)?)
    });
    if let Err(error) = activated {
        warn!(%error, "cannot act on a pressed shortcut");
    }
}

/// The next item of `stream`, or `None` once it has ended.
async fn next<S: Stream + Unpin>(stream: &mut S) -> Option<S::Item> {
    poll_fn(|context| Pin::new(&mut *stream).poll_next(context)).await
}
