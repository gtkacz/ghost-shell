//! A fake UPower on a private bus that stands in for the system bus, shaped as fx/upower.json
//! found the live one, plus the DisplayDevice and a laptop battery the feed must leave out.

use zbus::message::Header;
use zbus::object_server::SignalEmitter;
use zbus::{Connection, OwnedObjectPath, interface};

use super::bus::PrivateBus;
use super::calls::Calls;

const UPOWER: &str = "org.freedesktop.UPower";
const UPOWER_PATH: &str = "/org/freedesktop/UPower";
const DEVICES: &str = "/org/freedesktop/UPower/devices/";
pub const DUALSENSE_ID: &str = "ps-controller-battery-test";
pub const DUALSENSE_MODEL: &str = "DualSense Wireless Controller";
pub const DUALSENSE_PCT: f64 = 80.0;
pub const MOUSE_ID: &str = "hidpp_battery_0";
// UPower's device types and states, from its D-Bus API reference.
const TYPE_UNKNOWN: u32 = 0;
const TYPE_BATTERY: u32 = 2;
const TYPE_MOUSE: u32 = 5;
const TYPE_GAMING_INPUT: u32 = 12;
pub const STATE_CHARGING: u32 = 1;
const STATE_DISCHARGING: u32 = 2;

fn device_path(name: &str) -> OwnedObjectPath {
    OwnedObjectPath::try_from(format!("{DEVICES}{name}")).unwrap()
}

struct FakeManager {
    devices: Vec<OwnedObjectPath>,
    calls: Calls,
}

#[interface(name = "org.freedesktop.UPower")]
impl FakeManager {
    fn enumerate_devices(&self, #[zbus(header)] header: Header<'_>) -> Vec<OwnedObjectPath> {
        self.calls.method(&header, "EnumerateDevices");
        self.devices.clone()
    }

    #[zbus(signal)]
    async fn device_added(emitter: &SignalEmitter<'_>, device: OwnedObjectPath)
    -> zbus::Result<()>;
}

struct FakeDevice {
    kind: u32,
    power_supply: bool,
    native_path: &'static str,
    model: &'static str,
    percentage: f64,
    state: u32,
    calls: Calls,
}

#[interface(name = "org.freedesktop.UPower.Device")]
impl FakeDevice {
    #[zbus(property, name = "Type")]
    fn kind(&self, #[zbus(header)] header: Option<Header<'_>>) -> u32 {
        self.calls.property(header.as_ref(), "Type");
        self.kind
    }

    #[zbus(property)]
    fn power_supply(&self, #[zbus(header)] header: Option<Header<'_>>) -> bool {
        self.calls.property(header.as_ref(), "PowerSupply");
        self.power_supply
    }

    #[zbus(property)]
    fn native_path(&self, #[zbus(header)] header: Option<Header<'_>>) -> String {
        self.calls.property(header.as_ref(), "NativePath");
        self.native_path.to_owned()
    }

    #[zbus(property)]
    fn model(&self, #[zbus(header)] header: Option<Header<'_>>) -> String {
        self.calls.property(header.as_ref(), "Model");
        self.model.to_owned()
    }

    #[zbus(property)]
    fn percentage(&self, #[zbus(header)] header: Option<Header<'_>>) -> f64 {
        self.calls.property(header.as_ref(), "Percentage");
        self.percentage
    }

    #[zbus(property)]
    fn state(&self, #[zbus(header)] header: Option<Header<'_>>) -> u32 {
        self.calls.property(header.as_ref(), "State");
        self.state
    }
}

/// UPower with the DualSense at [`DUALSENSE_PCT`], discharging. Unlike the live one, it also
/// enumerates its DisplayDevice and a laptop's own battery (`PowerSupply` true).
pub struct FakeUpower {
    connection: Connection,
    pub calls: Calls,
}

impl FakeUpower {
    /// Serves the objects without taking UPower's name.
    pub async fn serve(bus: &PrivateBus) -> Self {
        let calls = Calls::default();
        let connection = bus.connect().await;
        let server = connection.object_server();
        let devices = [
            ("DisplayDevice", TYPE_UNKNOWN, false, "", "", 0.0),
            (
                "battery_BAT0",
                TYPE_BATTERY,
                true,
                "BAT0",
                "Laptop pack",
                50.0,
            ),
            (
                "battery_ps_controller_battery_test",
                TYPE_GAMING_INPUT,
                false,
                DUALSENSE_ID,
                DUALSENSE_MODEL,
                DUALSENSE_PCT,
            ),
        ];
        for (name, kind, power_supply, native_path, model, percentage) in devices {
            let device = FakeDevice {
                kind,
                power_supply,
                native_path,
                model,
                percentage,
                state: STATE_DISCHARGING,
                calls: calls.clone(),
            };
            server.at(device_path(name), device).await.unwrap();
        }
        let manager = FakeManager {
            devices: devices.iter().map(|(name, ..)| device_path(name)).collect(),
            calls: calls.clone(),
        };
        server.at(UPOWER_PATH, manager).await.unwrap();
        Self { connection, calls }
    }

    pub async fn claim(&self) {
        self.connection.request_name(UPOWER).await.unwrap();
    }

    pub async fn release(&self) {
        self.connection.release_name(UPOWER).await.unwrap();
    }

    pub async fn start(bus: &PrivateBus) -> Self {
        let upower = Self::serve(bus).await;
        upower.claim().await;
        upower
    }

    /// Moves the DualSense to `percentage` and `state` and says so, as UPower does.
    pub async fn set_dualsense(&self, percentage: f64, state: u32) {
        let device = self
            .connection
            .object_server()
            .interface::<_, FakeDevice>(device_path("battery_ps_controller_battery_test"))
            .await
            .unwrap();
        let mut fields = device.get_mut().await;
        fields.percentage = percentage;
        fields.state = state;
        fields
            .percentage_changed(device.signal_emitter())
            .await
            .unwrap();
        fields.state_changed(device.signal_emitter()).await.unwrap();
    }

    /// Plugs in a mouse and says so, as UPower does.
    pub async fn add_mouse(&self, percentage: f64) {
        let path = device_path("mouse_hidpp_battery_0");
        let mouse = FakeDevice {
            kind: TYPE_MOUSE,
            power_supply: false,
            native_path: MOUSE_ID,
            model: "MX Master 3",
            percentage,
            state: STATE_DISCHARGING,
            calls: self.calls.clone(),
        };
        let server = self.connection.object_server();
        server.at(path.clone(), mouse).await.unwrap();
        let manager = server
            .interface::<_, FakeManager>(UPOWER_PATH)
            .await
            .unwrap();
        manager.get_mut().await.devices.push(path.clone());
        FakeManager::device_added(manager.signal_emitter(), path)
            .await
            .unwrap();
    }
}
