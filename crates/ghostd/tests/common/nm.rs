//! A fake NetworkManager on the private bus, shaped as fx/nm.json found the live one.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use zbus::object_server::SignalEmitter;
use zbus::{Connection, OwnedObjectPath, OwnedVariant, Variant, interface};

use super::bus::PrivateBus;

const NM: &str = "org.freedesktop.NetworkManager";
const NM_PATH: &str = "/org/freedesktop/NetworkManager";
const SETTINGS_PATH: &str = "/org/freedesktop/NetworkManager/Settings";
const DEVICE: &str = "/org/freedesktop/NetworkManager/Devices/";
const IP4_CONFIG: &str = "/org/freedesktop/NetworkManager/IP4Config/";
const ACTIVE: &str = "/org/freedesktop/NetworkManager/ActiveConnection/";
const SETTING: &str = "/org/freedesktop/NetworkManager/Settings/";
// NM's device types and states, from its D-Bus API reference.
const ETHERNET: u32 = 1;
const TUN: u32 = 16;
const GENERIC: u32 = 32;
const ACTIVATED: u32 = 100;
// NM's active connection states.
pub const ACTIVATING: u32 = 1;
pub const ACTIVE_ACTIVATED: u32 = 2;
const NO_REASON: u32 = 0;
/// tun0 carries the VPN's IP4Config in fx/nm.json; this one belongs to no device.
const UNMATCHED_IP4_CONFIG: &str = "99";
const VPN_ACTIVE: &str = "9";
pub const VPN_NAME: &str = "OpenVPN Flxpoint";
pub const SAVED_VPN_NAME: &str = "Work";

fn path(prefix: &str, id: &str) -> OwnedObjectPath {
    OwnedObjectPath::try_from(format!("{prefix}{id}")).unwrap()
}

fn variant(value: &'static str) -> OwnedVariant {
    OwnedVariant::try_from(Variant::from(value)).unwrap()
}

/// Counts every property read and method call the fake NetworkManager answers.
#[derive(Clone, Default)]
pub struct Calls(Arc<AtomicUsize>);

impl Calls {
    fn note(&self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }

    pub fn count(&self) -> usize {
        self.0.load(Ordering::SeqCst)
    }
}

struct FakeManager {
    devices: Vec<OwnedObjectPath>,
    active_connections: Vec<OwnedObjectPath>,
    calls: Calls,
}

#[interface(name = "org.freedesktop.NetworkManager")]
impl FakeManager {
    #[zbus(property)]
    fn devices(&self) -> Vec<OwnedObjectPath> {
        self.calls.note();
        self.devices.clone()
    }

    #[zbus(property)]
    fn active_connections(&self) -> Vec<OwnedObjectPath> {
        self.calls.note();
        self.active_connections.clone()
    }
}

struct FakeDevice {
    interface: &'static str,
    device_type: u32,
    ip4_config: OwnedObjectPath,
    calls: Calls,
}

#[interface(name = "org.freedesktop.NetworkManager.Device")]
impl FakeDevice {
    #[zbus(property)]
    fn interface(&self) -> String {
        self.calls.note();
        self.interface.to_owned()
    }

    #[zbus(property)]
    fn device_type(&self) -> u32 {
        self.calls.note();
        self.device_type
    }

    #[zbus(property)]
    fn managed(&self) -> bool {
        self.calls.note();
        true
    }

    #[zbus(property)]
    fn state(&self) -> u32 {
        self.calls.note();
        ACTIVATED
    }

    #[zbus(property, name = "Ip4Config")]
    fn ip4_config(&self) -> OwnedObjectPath {
        self.calls.note();
        self.ip4_config.clone()
    }
}

struct FakeWired {
    speed: u32,
    calls: Calls,
}

#[interface(name = "org.freedesktop.NetworkManager.Device.Wired")]
impl FakeWired {
    #[zbus(property)]
    fn speed(&self) -> u32 {
        self.calls.note();
        self.speed
    }
}

struct FakeIp4Config {
    address: &'static str,
    prefix: u32,
    calls: Calls,
}

#[interface(name = "org.freedesktop.NetworkManager.IP4Config")]
impl FakeIp4Config {
    #[zbus(property)]
    fn address_data(&self) -> Vec<HashMap<String, OwnedVariant>> {
        self.calls.note();
        vec![HashMap::from([
            ("address".to_owned(), variant(self.address)),
            (
                "prefix".to_owned(),
                OwnedVariant::try_from(Variant::from(self.prefix)).unwrap(),
            ),
        ])]
    }
}

struct FakeActive {
    kind: &'static str,
    id: &'static str,
    state: u32,
    devices: Vec<OwnedObjectPath>,
    ip4_config: OwnedObjectPath,
    connection: OwnedObjectPath,
    calls: Calls,
}

#[interface(name = "org.freedesktop.NetworkManager.Connection.Active")]
impl FakeActive {
    #[zbus(property, name = "Type")]
    fn kind(&self) -> String {
        self.calls.note();
        self.kind.to_owned()
    }

    #[zbus(property)]
    fn id(&self) -> String {
        self.calls.note();
        self.id.to_owned()
    }

    #[zbus(property)]
    fn state(&self) -> u32 {
        self.calls.note();
        self.state
    }

    #[zbus(property)]
    fn devices(&self) -> Vec<OwnedObjectPath> {
        self.calls.note();
        self.devices.clone()
    }

    #[zbus(property, name = "Ip4Config")]
    fn ip4_config(&self) -> OwnedObjectPath {
        self.calls.note();
        self.ip4_config.clone()
    }

    #[zbus(property)]
    fn connection(&self) -> OwnedObjectPath {
        self.calls.note();
        self.connection.clone()
    }

    // Named apart from the change signal zbus derives for the State property.
    #[zbus(signal, name = "StateChanged")]
    async fn connection_state_changed(
        emitter: &SignalEmitter<'_>,
        state: u32,
        reason: u32,
    ) -> zbus::Result<()>;
}

struct FakeSettings {
    connections: Vec<OwnedObjectPath>,
    calls: Calls,
}

#[interface(name = "org.freedesktop.NetworkManager.Settings")]
impl FakeSettings {
    fn list_connections(&self) -> Vec<OwnedObjectPath> {
        self.calls.note();
        self.connections.clone()
    }
}

struct FakeConnectionSettings {
    settings: HashMap<String, HashMap<String, OwnedVariant>>,
    calls: Calls,
}

#[interface(name = "org.freedesktop.NetworkManager.Settings.Connection")]
impl FakeConnectionSettings {
    fn get_settings(&self) -> HashMap<String, HashMap<String, OwnedVariant>> {
        self.calls.note();
        self.settings.clone()
    }
}

/// How the fake NetworkManager departs from fx/nm.json.
#[derive(Clone, Copy)]
pub struct NmShape {
    pub wired_speed: bool,
    pub tun0_carries_the_vpn_config: bool,
    pub vpn_state: u32,
}

impl Default for NmShape {
    fn default() -> Self {
        Self {
            wired_speed: true,
            tun0_carries_the_vpn_config: true,
            vpn_state: ACTIVE_ACTIVATED,
        }
    }
}

/// NetworkManager as fx/nm.json found it, plus a saved WireGuard connection, "Work", that is not
/// active.
pub struct FakeNm {
    connection: Connection,
    pub calls: Calls,
}

impl FakeNm {
    /// Serves the objects without taking NetworkManager's name.
    pub async fn serve(bus: &PrivateBus, shape: NmShape) -> Self {
        let calls = Calls::default();
        let connection = bus.connect().await;
        let server = connection.object_server();
        server
            .at(
                NM_PATH,
                FakeManager {
                    devices: ["1", "2", "21", "22"].map(|id| path(DEVICE, id)).to_vec(),
                    active_connections: ["10", VPN_ACTIVE, "8", "2", "1"]
                        .map(|id| path(ACTIVE, id))
                        .to_vec(),
                    calls: calls.clone(),
                },
            )
            .await
            .unwrap();
        let tun0_config = if shape.tun0_carries_the_vpn_config {
            "22"
        } else {
            UNMATCHED_IP4_CONFIG
        };
        let devices = [
            ("1", "lo", GENERIC, "1"),
            ("2", "enp14s0", ETHERNET, "2"),
            ("21", "tailscale0", TUN, "21"),
            ("22", "tun0", TUN, tun0_config),
        ];
        for (id, interface, device_type, ip4_config) in devices {
            let device = FakeDevice {
                interface,
                device_type,
                ip4_config: path(IP4_CONFIG, ip4_config),
                calls: calls.clone(),
            };
            server.at(path(DEVICE, id), device).await.unwrap();
        }
        if shape.wired_speed {
            let wired = FakeWired {
                speed: 1000,
                calls: calls.clone(),
            };
            server.at(path(DEVICE, "2"), wired).await.unwrap();
        }
        let configs = [
            ("1", "127.0.0.1", 8),
            ("2", "192.168.15.5", 24),
            ("21", "100.64.0.1", 32),
            ("22", "172.27.224.164", 21),
            (UNMATCHED_IP4_CONFIG, "172.27.224.164", 21),
        ];
        for (id, address, prefix) in configs {
            let config = FakeIp4Config {
                address,
                prefix,
                calls: calls.clone(),
            };
            server.at(path(IP4_CONFIG, id), config).await.unwrap();
        }
        // fx C1: the VPN's Devices[0] is the parent NIC, and only its Ip4Config leads to tun0.
        let actives = [
            ("10", "tun", "tun0", ACTIVE_ACTIVATED, "22", "22", "8"),
            (VPN_ACTIVE, "vpn", VPN_NAME, shape.vpn_state, "2", "22", "1"),
            ("8", "tun", "tailscale0", ACTIVE_ACTIVATED, "21", "21", "7"),
            (
                "2",
                "802-3-ethernet",
                "Wired connection 1",
                ACTIVE_ACTIVATED,
                "2",
                "2",
                "3",
            ),
            ("1", "loopback", "lo", ACTIVE_ACTIVATED, "1", "1", "2"),
        ];
        for (id, kind, name, state, device, ip4_config, setting) in actives {
            let active = FakeActive {
                kind,
                id: name,
                state,
                devices: vec![path(DEVICE, device)],
                ip4_config: path(IP4_CONFIG, ip4_config),
                connection: path(SETTING, setting),
                calls: calls.clone(),
            };
            server.at(path(ACTIVE, id), active).await.unwrap();
        }
        let saved = [
            (
                "1",
                VPN_NAME,
                "vpn",
                Some("org.freedesktop.NetworkManager.openvpn"),
            ),
            ("2", "lo", "loopback", None),
            ("3", "Wired connection 1", "802-3-ethernet", None),
            ("7", "tailscale0", "tun", None),
            ("8", "tun0", "tun", None),
            ("9", SAVED_VPN_NAME, "wireguard", None),
        ];
        server
            .at(
                SETTINGS_PATH,
                FakeSettings {
                    connections: saved.iter().map(|(id, ..)| path(SETTING, id)).collect(),
                    calls: calls.clone(),
                },
            )
            .await
            .unwrap();
        for (id, name, kind, service_type) in saved {
            let mut settings = HashMap::from([(
                "connection".to_owned(),
                HashMap::from([
                    ("id".to_owned(), variant(name)),
                    ("type".to_owned(), variant(kind)),
                ]),
            )]);
            if let Some(service_type) = service_type {
                settings.insert(
                    "vpn".to_owned(),
                    HashMap::from([("service-type".to_owned(), variant(service_type))]),
                );
            }
            let connection_settings = FakeConnectionSettings {
                settings,
                calls: calls.clone(),
            };
            server
                .at(path(SETTING, id), connection_settings)
                .await
                .unwrap();
        }
        Self { connection, calls }
    }

    pub async fn claim(&self) {
        self.connection.request_name(NM).await.unwrap();
    }

    pub async fn start(bus: &PrivateBus, shape: NmShape) -> Self {
        let nm = Self::serve(bus, shape).await;
        nm.claim().await;
        nm
    }

    /// Moves the VPN's active connection to `state` and says so, as NetworkManager does.
    pub async fn set_vpn_state(&self, state: u32) {
        let active = self
            .connection
            .object_server()
            .interface::<_, FakeActive>(path(ACTIVE, VPN_ACTIVE))
            .await
            .unwrap();
        active.get_mut().await.state = state;
        FakeActive::connection_state_changed(active.signal_emitter(), state, NO_REASON)
            .await
            .unwrap();
    }
}
