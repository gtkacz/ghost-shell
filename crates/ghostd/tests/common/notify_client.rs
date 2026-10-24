//! A client for `org.freedesktop.Notifications`, as an application or the shell's helpers use it.

use std::collections::HashMap;

use zbus::Variant;
use zbus::proxy::CacheProperties;
use zbus::{Connection, Proxy};

pub const NOTIFICATIONS: &str = "org.freedesktop.Notifications";
pub const NOTIFICATIONS_PATH: &str = "/org/freedesktop/Notifications";

pub type Hints<'a> = HashMap<&'a str, Variant<'a>>;

pub struct NotifyClient {
    pub proxy: Proxy<'static>,
}

impl NotifyClient {
    pub async fn new(connection: &Connection) -> Self {
        let proxy = zbus::proxy::Builder::new(connection)
            .destination(NOTIFICATIONS)
            .path(NOTIFICATIONS_PATH)
            .interface(NOTIFICATIONS)
            .cache_properties(CacheProperties::No)
            .build()
            .await
            .unwrap();
        Self { proxy }
    }

    pub async fn notify(
        &self,
        app_name: &str,
        summary: &str,
        body: &str,
        actions: &[&str],
        hints: &Hints<'_>,
    ) -> zbus::Result<u32> {
        self.proxy
            .call(
                "Notify",
                &(app_name, 0u32, "", summary, body, actions, hints, -1i32),
            )
            .await
    }

    pub async fn inhibited(&self) -> bool {
        self.proxy.get_property("Inhibited").await.unwrap()
    }
}
