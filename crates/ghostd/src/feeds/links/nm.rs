//! NetworkManager's devices, saved VPNs and active connections, read through its D-Bus API with
//! plain property reads and `GetSettings`, which returns no secrets.

use std::collections::HashMap;
use std::ops::RangeInclusive;
use std::pin::Pin;
use std::task::{Context, Poll, Waker};

use futures_core::Stream;
use ghost_proto::{LanLink, LinkState, VpnLink};
use tracing::debug;
use zbus::fdo::NameOwnerChangedStream;
use zbus::proxy::CacheProperties;
use zbus::{
    Connection, MatchRule, MessageStream, OwnedObjectPath, OwnedVariant, fdo, message, proxy,
};

use crate::feeds::next;

const NM: &str = "org.freedesktop.NetworkManager";
const NM_PATH: &str = "/org/freedesktop/NetworkManager";
const PROPERTIES_INTERFACE: &str = "org.freedesktop.DBus.Properties";
const PROPERTIES_CHANGED: &str = "PropertiesChanged";
const ACTIVE_INTERFACE: &str = "org.freedesktop.NetworkManager.Connection.Active";
const STATE_CHANGED: &str = "StateChanged";
/// The manager's properties whose change can add, drop or move a link.
const LINK_PROPERTIES: [&str; 2] = ["ActiveConnections", "Devices"];
const NO_PATH: &str = "/";
// `NMDeviceType` and `NMDeviceState` in NetworkManager's D-Bus API reference.
const DEVICE_ETHERNET: u32 = 1;
const DEVICE_WIFI: u32 = 2;
const DEVICE_ACTIVATED: u32 = 100;
// From PREPARE up to SECONDARIES, the steps between choosing a connection and being up.
const DEVICE_CONNECTING: RangeInclusive<u32> = 40..=90;
// `NMActiveConnectionState`.
const ACTIVE_ACTIVATING: u32 = 1;
const ACTIVE_ACTIVATED: u32 = 2;
const SETTING_CONNECTION: &str = "connection";
const SETTING_VPN: &str = "vpn";
const KEY_ID: &str = "id";
const KEY_TYPE: &str = "type";
const KEY_SERVICE_TYPE: &str = "service-type";
const TYPE_VPN: &str = "vpn";
const TYPE_WIREGUARD: &str = "wireguard";
const SERVICE_TYPE_SEPARATOR: char = '.';
const KEY_ADDRESS: &str = "address";
const KEY_PREFIX: &str = "prefix";

type Settings = HashMap<String, HashMap<String, OwnedVariant>>;

#[proxy(
    interface = "org.freedesktop.NetworkManager",
    default_service = "org.freedesktop.NetworkManager",
    default_path = "/org/freedesktop/NetworkManager"
)]
trait Manager {
    #[zbus(property)]
    fn devices(&self) -> zbus::Result<Vec<OwnedObjectPath>>;

    #[zbus(property)]
    fn active_connections(&self) -> zbus::Result<Vec<OwnedObjectPath>>;
}

#[proxy(
    interface = "org.freedesktop.NetworkManager.Device",
    default_service = "org.freedesktop.NetworkManager"
)]
trait Device {
    #[zbus(property)]
    fn interface(&self) -> zbus::Result<String>;

    #[zbus(property)]
    fn device_type(&self) -> zbus::Result<u32>;

    #[zbus(property)]
    fn managed(&self) -> zbus::Result<bool>;

    #[zbus(property)]
    fn state(&self) -> zbus::Result<u32>;

    #[zbus(property, name = "Ip4Config")]
    fn ip4_config(&self) -> zbus::Result<OwnedObjectPath>;
}

#[proxy(
    interface = "org.freedesktop.NetworkManager.Device.Wired",
    default_service = "org.freedesktop.NetworkManager"
)]
trait Wired {
    #[zbus(property)]
    fn speed(&self) -> zbus::Result<u32>;
}

#[proxy(
    interface = "org.freedesktop.NetworkManager.IP4Config",
    default_service = "org.freedesktop.NetworkManager"
)]
trait Ip4Config {
    #[zbus(property)]
    fn address_data(&self) -> zbus::Result<Vec<HashMap<String, OwnedVariant>>>;
}

#[proxy(
    interface = "org.freedesktop.NetworkManager.Connection.Active",
    default_service = "org.freedesktop.NetworkManager"
)]
trait Active {
    #[zbus(property)]
    fn connection(&self) -> zbus::Result<OwnedObjectPath>;

    #[zbus(property)]
    fn state(&self) -> zbus::Result<u32>;

    #[zbus(property, name = "Ip4Config")]
    fn ip4_config(&self) -> zbus::Result<OwnedObjectPath>;
}

#[proxy(
    interface = "org.freedesktop.NetworkManager.Settings",
    default_service = "org.freedesktop.NetworkManager",
    default_path = "/org/freedesktop/NetworkManager/Settings"
)]
trait SettingsList {
    fn list_connections(&self) -> zbus::Result<Vec<OwnedObjectPath>>;
}

#[proxy(
    interface = "org.freedesktop.NetworkManager.Settings.Connection",
    default_service = "org.freedesktop.NetworkManager"
)]
trait SavedConnection {
    fn get_settings(&self) -> zbus::Result<Settings>;
}

/// What NetworkManager reports; empty while it is absent.
#[derive(Debug, Default)]
pub struct NmLinks {
    pub lan: Vec<LanLink>,
    pub vpns: Vec<VpnLink>,
}

/// The signals after which the links are read again.
pub struct Changes {
    owners: NameOwnerChangedStream,
    manager: MessageStream,
    active_states: MessageStream,
}

impl Changes {
    /// Listens on `bus`; the links must be read only after this, so that no change is missed.
    pub async fn listen(bus: &Connection) -> zbus::Result<Self> {
        let owners = fdo::DBusProxy::builder(bus)
            .cache_properties(CacheProperties::No)
            .build()
            .await?
            .receive_name_owner_changed_with_args(&[(0, NM)])
            .await?;
        // The bus, not zbus, applies a rule's well-known sender, following the name to its owner.
        let manager = MatchRule::builder()
            .msg_type(message::Type::Signal)
            .sender(NM)
            .path(NM_PATH)
            .interface(PROPERTIES_INTERFACE)
            .member(PROPERTIES_CHANGED)
            .add_arg(NM)
            .build()?;
        let active_states = MatchRule::builder()
            .msg_type(message::Type::Signal)
            .sender(NM)
            .interface(ACTIVE_INTERFACE)
            .member(STATE_CHANGED)
            .build()?;
        Ok(Self {
            owners,
            manager: MessageStream::for_match_rule(manager, bus, None).await?,
            active_states: MessageStream::for_match_rule(active_states, bus, None).await?,
        })
    }

    /// Drops every change already queued.
    pub fn drain(&mut self) {
        drain(&mut self.owners);
        drain(&mut self.manager);
        drain(&mut self.active_states);
    }

    /// Waits for NetworkManager to come or go, for its devices or active connections to change,
    /// or for an active connection to change state. Never returns once the bus is gone.
    pub async fn next(&mut self) {
        loop {
            tokio::select! {
                Some(_) = next(&mut self.owners) => return,
                Some(_) = next(&mut self.active_states) => return,
                Some(message) = next(&mut self.manager) => {
                    if message.is_ok_and(|message| touches_links(&message)) {
                        return;
                    }
                }
                else => std::future::pending::<()>().await,
            }
        }
    }
}

fn drain<S: Stream + Unpin>(stream: &mut S) {
    let mut context = Context::from_waker(Waker::noop());
    while let Poll::Ready(Some(_)) = Pin::new(&mut *stream).poll_next(&mut context) {}
}

fn touches_links(message: &zbus::Message) -> bool {
    message
        .body()
        .deserialize::<(String, HashMap<String, OwnedVariant>, Vec<String>)>()
        .is_ok_and(|(_, changed, invalidated)| {
            LINK_PROPERTIES.iter().any(|&property| {
                changed.contains_key(property) || invalidated.iter().any(|name| name == property)
            })
        })
}

/// Reads every link in full. An absent NetworkManager reads as no links, and a device or
/// connection it cannot answer for is left out: it can drop objects at any time, and says so with
/// a signal that brings the next read.
pub async fn read(bus: &Connection) -> NmLinks {
    match read_manager(bus).await {
        Ok(links) => links,
        Err(error) => {
            debug!(%error, "NetworkManager is not answering; no LAN or VPN links");
            NmLinks::default()
        }
    }
}

struct DeviceFacts {
    path: OwnedObjectPath,
    interface: String,
    device_type: u32,
    ip4_config: OwnedObjectPath,
    proxy: DeviceProxy<'static>,
}

struct ActiveFacts {
    state: u32,
    ip4_config: OwnedObjectPath,
}

async fn read_manager(bus: &Connection) -> zbus::Result<NmLinks> {
    let manager = ManagerProxy::builder(bus)
        .cache_properties(CacheProperties::No)
        .build()
        .await?;
    let mut devices = Vec::new();
    for path in manager.devices().await? {
        if let Some(device) = read_device(bus, path).await {
            devices.push(device);
        }
    }
    let mut active = HashMap::new();
    for path in manager.active_connections().await? {
        if let Some((connection, facts)) = read_active(bus, path).await {
            active.insert(connection, facts);
        }
    }
    let mut lan = Vec::new();
    for device in devices
        .iter()
        .filter(|device| [DEVICE_ETHERNET, DEVICE_WIFI].contains(&device.device_type))
    {
        if let Some(link) = lan_link(bus, device).await {
            lan.push(link);
        }
    }
    let vpns = vpn_links(bus, &devices, &active).await?;
    Ok(NmLinks { lan, vpns })
}

async fn read_device(bus: &Connection, path: OwnedObjectPath) -> Option<DeviceFacts> {
    let proxy = DeviceProxy::builder(bus)
        .path(path.clone())
        .cache_properties(CacheProperties::No)
        .build()
        .await
        .ok()?;
    Some(DeviceFacts {
        path,
        interface: proxy.interface().await.ok()?,
        device_type: proxy.device_type().await.ok()?,
        ip4_config: proxy.ip4_config().await.ok()?,
        proxy,
    })
}

/// The settings path of the active connection at `path`, with its state.
async fn read_active(
    bus: &Connection,
    path: OwnedObjectPath,
) -> Option<(OwnedObjectPath, ActiveFacts)> {
    let proxy = ActiveProxy::builder(bus)
        .path(path)
        .cache_properties(CacheProperties::No)
        .build()
        .await
        .ok()?;
    let facts = ActiveFacts {
        state: proxy.state().await.ok()?,
        ip4_config: proxy.ip4_config().await.ok()?,
    };
    Some((proxy.connection().await.ok()?, facts))
}

async fn lan_link(bus: &Connection, device: &DeviceFacts) -> Option<LanLink> {
    if !device.proxy.managed().await.ok()? {
        return None;
    }
    let state = match device.proxy.state().await.ok()? {
        DEVICE_ACTIVATED => LinkState::Up,
        state if DEVICE_CONNECTING.contains(&state) => LinkState::Connecting,
        _ => LinkState::Down,
    };
    let speed_mbps = if device.device_type == DEVICE_ETHERNET {
        // A device without the Wired interface answers UnknownInterface: no speed.
        wired_speed(bus, device.path.clone()).await
    } else {
        None
    };
    Some(LanLink {
        iface: device.interface.clone(),
        address: first_address(bus, &device.ip4_config).await,
        speed_mbps,
        state,
    })
}

async fn wired_speed(bus: &Connection, path: OwnedObjectPath) -> Option<u32> {
    WiredProxy::builder(bus)
        .path(path)
        .cache_properties(CacheProperties::No)
        .build()
        .await
        .ok()?
        .speed()
        .await
        .ok()
}

/// `address/prefix` of the first address in the IPv4 config at `path`.
async fn first_address(bus: &Connection, path: &OwnedObjectPath) -> Option<String> {
    if path.as_str() == NO_PATH {
        return None;
    }
    let addresses = Ip4ConfigProxy::builder(bus)
        .path(path.clone())
        .cache_properties(CacheProperties::No)
        .build()
        .await
        .ok()?
        .address_data()
        .await
        .ok()?;
    let first = addresses.first()?;
    let address: &str = first.get(KEY_ADDRESS)?.downcast_ref().ok()?;
    let prefix: u32 = first.get(KEY_PREFIX)?.downcast_ref().ok()?;
    Some(format!("{address}/{prefix}"))
}

/// Every saved VPN or WireGuard connection, in NetworkManager's order, with the state of its
/// active connection. Tunnels NetworkManager only watches, such as Tailscale's, are of type
/// `tun` and left out.
async fn vpn_links(
    bus: &Connection,
    devices: &[DeviceFacts],
    active: &HashMap<OwnedObjectPath, ActiveFacts>,
) -> zbus::Result<Vec<VpnLink>> {
    let saved = SettingsListProxy::builder(bus)
        .cache_properties(CacheProperties::No)
        .build()
        .await?
        .list_connections()
        .await?;
    let mut vpns = Vec::new();
    for path in saved {
        let Some(settings) = read_settings(bus, path.clone()).await else {
            continue;
        };
        let Some((name, kind)) = vpn_identity(&settings) else {
            continue;
        };
        let activation = active.get(&path);
        let state = match activation.map(|facts| facts.state) {
            Some(ACTIVE_ACTIVATED) => LinkState::Up,
            Some(ACTIVE_ACTIVATING) => LinkState::Connecting,
            _ => LinkState::Down,
        };
        // A VPN's active connection lists the device it runs over, not its tunnel (fx C1); the
        // tunnel is the device that carries the same IPv4 config.
        let iface = activation
            .map(|facts| &facts.ip4_config)
            .filter(|config| config.as_str() != NO_PATH)
            .and_then(|config| devices.iter().find(|device| &device.ip4_config == config))
            .map(|device| device.interface.clone());
        vpns.push(VpnLink {
            name,
            kind,
            iface,
            state,
        });
    }
    Ok(vpns)
}

async fn read_settings(bus: &Connection, path: OwnedObjectPath) -> Option<Settings> {
    SavedConnectionProxy::builder(bus)
        .path(path)
        .cache_properties(CacheProperties::No)
        .build()
        .await
        .ok()?
        .get_settings()
        .await
        .ok()
}

/// The name and kind of a saved VPN or WireGuard connection, or `None` for any other.
fn vpn_identity(settings: &Settings) -> Option<(String, String)> {
    let text = |setting: &str, key: &str| -> Option<String> {
        let value: &str = settings.get(setting)?.get(key)?.downcast_ref().ok()?;
        Some(value.to_owned())
    };
    let kind = match text(SETTING_CONNECTION, KEY_TYPE)?.as_str() {
        TYPE_VPN => text(SETTING_VPN, KEY_SERVICE_TYPE)
            .and_then(|service| {
                service
                    .rsplit(SERVICE_TYPE_SEPARATOR)
                    .next()
                    .map(str::to_owned)
            })
            .unwrap_or_default(),
        TYPE_WIREGUARD => TYPE_WIREGUARD.to_owned(),
        _ => return None,
    };
    Some((text(SETTING_CONNECTION, KEY_ID)?, kind))
}
