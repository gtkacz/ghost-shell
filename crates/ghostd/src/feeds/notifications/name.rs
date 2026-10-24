//! Who holds `org.freedesktop.Notifications`, read through the helper connection so that no
//! question goes out on the server's own connection.

use std::future::pending;
use std::time::Duration;

use ghost_proto::NotificationServer;
use tokio::time::timeout;
use zbus::Connection;
use zbus::fdo::{self, NameOwnerChangedStream};
use zbus::names::BusName;
use zbus::proxy::CacheProperties;

use super::server::NAME;
use crate::feeds::next;

/// The bus answers in microseconds; the limit only keeps a wedged bus from stalling the feed.
const BUS_QUERY_TIMEOUT: Duration = Duration::from_millis(500);

/// Two connections to one bus. `helper` must be a connection of its own: the server's handlers
/// wait on the feed, so the feed may never wait on a call made over `bus`.
#[derive(Clone)]
pub struct Buses {
    pub bus: Connection,
    pub helper: Connection,
}

pub struct NameWatch {
    proxy: fdo::DBusProxy<'static>,
    owners: NameOwnerChangedStream,
    own_name: Option<String>,
}

impl NameWatch {
    /// Listens for changes of the name's owner; read the owner only after this, so none is missed.
    pub async fn open(buses: &Buses) -> zbus::Result<Self> {
        let proxy = fdo::DBusProxy::builder(&buses.helper)
            .cache_properties(CacheProperties::No)
            .build()
            .await?;
        Ok(Self {
            owners: proxy
                .receive_name_owner_changed_with_args(&[(0, NAME)])
                .await?,
            proxy,
            own_name: buses.bus.unique_name().map(ToString::to_string),
        })
    }

    pub async fn server(&self) -> NotificationServer {
        let Ok(name) = BusName::try_from(NAME) else {
            return NotificationServer::None;
        };
        match timeout(BUS_QUERY_TIMEOUT, self.proxy.get_name_owner(name)).await {
            Ok(Ok(owner)) if Some(owner.as_str()) == self.own_name.as_deref() => {
                NotificationServer::GhostShell
            }
            Ok(Ok(_)) => NotificationServer::Plasma,
            _ => NotificationServer::None,
        }
    }

    /// The process behind a connection of the bus, or 0 when the bus cannot say.
    pub async fn pid_of(&self, unique_name: &str) -> u32 {
        let Ok(name) = BusName::try_from(unique_name) else {
            return 0;
        };
        timeout(
            BUS_QUERY_TIMEOUT,
            self.proxy.get_connection_unix_process_id(name),
        )
        .await
        .map_or(0, |pid| pid.unwrap_or(0))
    }

    /// Resolves after the owner of the name changes. Never resolves once the bus is gone.
    pub async fn changed(&mut self) {
        if next(&mut self.owners).await.is_none() {
            pending::<()>().await;
        }
    }
}
