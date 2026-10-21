//! Asks KWin to run its `ghost-shell-desktop` script again, so that the script reports the desktop
//! afresh.
//!
//! The script reports only when its state changes, and it cannot watch bus names. A ghostd that
//! starts while windows cover the dashboard would otherwise hear nothing until the next window
//! change, which can be hours.

use std::future::Future;
use std::time::Duration;

use tokio::time::timeout;
use tracing::{info, warn};
use zbus::names::BusName;
use zbus::proxy::CacheProperties;
use zbus::{Connection, fdo, proxy};

use super::desktop::KWIN;

const SCRIPT: &str = "ghost-shell-desktop";
// So that an unload KWin never answers cannot keep the script from being started again.
const CALL_TIMEOUT: Duration = Duration::from_millis(500);

#[proxy(
    interface = "org.kde.kwin.Scripting",
    default_service = "org.kde.KWin",
    default_path = "/Scripting"
)]
trait KWinScripting {
    #[zbus(name = "isScriptLoaded")]
    fn is_script_loaded(&self, plugin_name: &str) -> zbus::Result<bool>;

    #[zbus(name = "unloadScript")]
    fn unload_script(&self, plugin_name: &str) -> zbus::Result<bool>;

    /// Runs the scripts that are loaded but not running.
    #[zbus(name = "start")]
    fn start(&self) -> zbus::Result<()>;
}

enum Outcome {
    Asked,
    KWinAbsent,
    ScriptNotLoaded,
}

/// Unloads the desktop script and starts it again, if KWin has it loaded: unloading makes the
/// script take back its last report, and the new copy reports its current state as it loads.
///
/// Logs one line with the outcome and never fails, since the desktop topic works without it. Each
/// call to the bus gives up after `CALL_TIMEOUT`.
pub async fn reload(connection: &Connection) {
    match try_reload(connection).await {
        Ok(Outcome::Asked) => info!("asked KWin to reload {SCRIPT} so it reports again"),
        Ok(Outcome::KWinAbsent) => {
            info!("KWin is not on the bus, so {SCRIPT} is not asked to report again");
        }
        Ok(Outcome::ScriptNotLoaded) => {
            info!("KWin has not loaded {SCRIPT}, so it is not asked to report again");
        }
        Err(error) => warn!(
            %error,
            "cannot ask KWin to reload {SCRIPT}; the desktop topic may stay stale until the next \
             window change"
        ),
    }
}

async fn try_reload(connection: &Connection) -> zbus::Result<Outcome> {
    // Asked first, because a call to a name nobody owns could make the bus start KWin.
    let bus = fdo::DBusProxy::new(connection).await?;
    if !bounded(bus.name_has_owner(BusName::try_from(KWIN)?)).await? {
        return Ok(Outcome::KWinAbsent);
    }
    let scripting = KWinScriptingProxy::builder(connection)
        .cache_properties(CacheProperties::No)
        .build()
        .await?;
    if !bounded(scripting.is_script_loaded(SCRIPT)).await? {
        return Ok(Outcome::ScriptNotLoaded);
    }
    let unloaded = bounded(scripting.unload_script(SCRIPT)).await;
    // Started even when unloading failed, so that the script is never left unloaded.
    let started = bounded(scripting.start()).await;
    // The first failure is the one logged, so that a reload that fails twice still logs once.
    unloaded.and(started).map(|()| Outcome::Asked)
}

async fn bounded<T, E>(call: impl Future<Output = Result<T, E>>) -> zbus::Result<T>
where
    zbus::Error: From<E>,
{
    match timeout(CALL_TIMEOUT, call).await {
        Ok(answer) => Ok(answer?),
        Err(_) => Err(fdo::Error::TimedOut(format!("no answer within {CALL_TIMEOUT:?}")).into()),
    }
}
