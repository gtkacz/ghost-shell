//! A fake KDE Connect daemon on a private bus that stands in for the session bus, shaped as
//! fx/kdeconnect.json found the live one: the `devices` overload that takes two booleans, the
//! device's `name` and `type`, and its battery's `charge` as a D-Bus `i`.

use zbus::message::Header;
use zbus::{Connection, interface};

use super::bus::PrivateBus;
use super::calls::Calls;

const KDE_CONNECT: &str = "org.kde.kdeconnect";
const DAEMON_PATH: &str = "/modules/kdeconnect";
const DEVICES: &str = "/modules/kdeconnect/devices/";
pub const PHONE_ID: &str = "abc123";
pub const PHONE_NAME: &str = "Phone";
/// Paired, but out of reach.
const LAPTOP_ID: &str = "def456";
/// Reachable, but never paired.
const STRANGER_ID: &str = "0f0f0f";

struct FakeDaemon {
    /// Every device the daemon knows, with whether it is reachable and paired.
    devices: Vec<(&'static str, bool, bool)>,
    calls: Calls,
}

#[interface(name = "org.kde.kdeconnect.daemon")]
impl FakeDaemon {
    #[zbus(name = "devices")]
    fn devices(
        &self,
        only_reachable: bool,
        only_paired: bool,
        #[zbus(header)] header: Header<'_>,
    ) -> Vec<String> {
        self.calls.method(&header, "devices");
        self.devices
            .iter()
            .filter(|(_, reachable, paired)| {
                (*reachable || !only_reachable) && (*paired || !only_paired)
            })
            .map(|(id, ..)| (*id).to_owned())
            .collect()
    }

    /// Makes every link provider look for devices again, which wakes the phone's radio.
    #[zbus(name = "forceOnNetworkChange")]
    fn force_on_network_change(&self, #[zbus(header)] header: Header<'_>) {
        self.calls.method(&header, "forceOnNetworkChange");
    }
}

struct FakeDevice {
    name: &'static str,
    kind: &'static str,
    calls: Calls,
}

#[interface(name = "org.kde.kdeconnect.device")]
impl FakeDevice {
    #[zbus(property, name = "name")]
    fn name(&self, #[zbus(header)] header: Option<Header<'_>>) -> String {
        self.calls.property(header.as_ref(), "name");
        self.name.to_owned()
    }

    #[zbus(property, name = "type")]
    fn kind(&self, #[zbus(header)] header: Option<Header<'_>>) -> String {
        self.calls.property(header.as_ref(), "type");
        self.kind.to_owned()
    }

    #[zbus(name = "reloadPlugins")]
    fn reload_plugins(&self, #[zbus(header)] header: Header<'_>) {
        self.calls.method(&header, "reloadPlugins");
    }
}

/// What the phone's battery plugin reports.
#[derive(Debug, Clone, Copy)]
pub struct PhoneBattery {
    pub has_battery: bool,
    pub charge: i32,
    pub charging: bool,
}

pub const PHONE_BATTERY: PhoneBattery = PhoneBattery {
    has_battery: true,
    charge: 15,
    charging: true,
};

struct FakeBattery {
    battery: PhoneBattery,
    calls: Calls,
}

#[interface(name = "org.kde.kdeconnect.device.battery")]
impl FakeBattery {
    #[zbus(property, name = "charge")]
    fn charge(&self, #[zbus(header)] header: Option<Header<'_>>) -> i32 {
        self.calls.property(header.as_ref(), "charge");
        self.battery.charge
    }

    #[zbus(property, name = "isCharging")]
    fn is_charging(&self, #[zbus(header)] header: Option<Header<'_>>) -> bool {
        self.calls.property(header.as_ref(), "isCharging");
        self.battery.charging
    }

    #[zbus(property, name = "hasBattery")]
    fn has_battery(&self, #[zbus(header)] header: Option<Header<'_>>) -> bool {
        self.calls.property(header.as_ref(), "hasBattery");
        self.battery.has_battery
    }
}

/// KDE Connect with a paired, reachable phone whose battery reads `battery`, a paired laptop out
/// of reach and a reachable device that was never paired.
pub struct FakeKdeConnect {
    connection: Connection,
    pub calls: Calls,
}

impl FakeKdeConnect {
    /// Serves the objects without taking KDE Connect's name.
    pub async fn serve(bus: &PrivateBus, battery: PhoneBattery) -> Self {
        let calls = Calls::default();
        let connection = bus.connect().await;
        let server = connection.object_server();
        let daemon = FakeDaemon {
            devices: vec![
                (LAPTOP_ID, false, true),
                (PHONE_ID, true, true),
                (STRANGER_ID, true, false),
            ],
            calls: calls.clone(),
        };
        server.at(DAEMON_PATH, daemon).await.unwrap();
        let devices = [
            (PHONE_ID, PHONE_NAME, "phone"),
            (LAPTOP_ID, "Laptop", "laptop"),
            (STRANGER_ID, "Stranger", "phone"),
        ];
        for (id, name, kind) in devices {
            let device = FakeDevice {
                name,
                kind,
                calls: calls.clone(),
            };
            let path = format!("{DEVICES}{id}");
            server.at(path.as_str(), device).await.unwrap();
            let battery = FakeBattery {
                battery,
                calls: calls.clone(),
            };
            server
                .at(format!("{path}/battery").as_str(), battery)
                .await
                .unwrap();
        }
        Self { connection, calls }
    }

    /// Method calls other than `allowed`; property reads do not count.
    pub fn method_calls_except(&self, allowed: &[&str]) -> usize {
        self.calls.method_calls_except(allowed)
    }

    pub async fn claim(&self) {
        self.connection.request_name(KDE_CONNECT).await.unwrap();
    }

    pub async fn start(bus: &PrivateBus, battery: PhoneBattery) -> Self {
        let kdeconnect = Self::serve(bus, battery).await;
        kdeconnect.claim().await;
        kdeconnect
    }

    /// Changes the phone's charge without saying so: KDE Connect declares its battery properties
    /// without change signals.
    pub async fn set_phone_charge(&self, charge: i32) {
        let battery = self
            .connection
            .object_server()
            .interface::<_, FakeBattery>(format!("{DEVICES}{PHONE_ID}/battery").as_str())
            .await
            .unwrap();
        battery.get_mut().await.battery.charge = charge;
    }
}
