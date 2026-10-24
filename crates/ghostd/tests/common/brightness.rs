//! A fake Plasma on a private bus that stands in for the session bus: powerdevil's
//! `org.kde.ScreenBrightness` with two displays, KWin's `NightLight` object, and the "kwin"
//! component of KGlobalAccel, shaped as the live ones answered `busctl introspect`.
//!
//! Every method a test must not see on the real desktop lands in a log instead.

use std::sync::{Arc, Mutex, PoisonError};

use zbus::object_server::SignalEmitter;
use zbus::{Connection, interface};

use super::bus::PrivateBus;

pub const SCREEN_BRIGHTNESS: &str = "org.kde.ScreenBrightness";
const SCREEN_BRIGHTNESS_PATH: &str = "/org/kde/ScreenBrightness";
const KWIN: &str = "org.kde.KWin";
const NIGHT_LIGHT_PATH: &str = "/org/kde/KWin/NightLight";
const KGLOBALACCEL: &str = "org.kde.kglobalaccel";
const KWIN_COMPONENT_PATH: &str = "/component/kwin";
pub const MONITOR: &str = "display0";
pub const MONITOR_LABEL: &str = "GIGA-BYTE TECHNOLOGY CO., LTD. M27Q";
pub const MONITOR_MAX: i32 = 10_000;
pub const PANEL: &str = "display1";
pub const PANEL_LABEL: &str = "Laptop panel";
pub const PANEL_MAX: i32 = 255;
pub const TOGGLE_ACTION: &str = "Toggle Night Color";

/// What the fake was asked to do, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    SetBrightness {
        display: String,
        value: i32,
        flags: u32,
    },
    InvokeShortcut(String),
}

#[derive(Clone, Default)]
pub struct Log(Arc<Mutex<Vec<Request>>>);

impl Log {
    fn push(&self, request: Request) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(request);
    }

    pub fn requests(&self) -> Vec<Request> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

struct FakeScreenBrightness {
    displays: Vec<String>,
}

#[interface(name = "org.kde.ScreenBrightness")]
impl FakeScreenBrightness {
    #[zbus(property, name = "DisplaysDBusNames")]
    fn displays(&self) -> Vec<String> {
        self.displays.clone()
    }

    #[zbus(signal)]
    async fn display_added(emitter: &SignalEmitter<'_>, display: String) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn display_removed(emitter: &SignalEmitter<'_>, display: String) -> zbus::Result<()>;
}

struct FakeDisplay {
    id: String,
    label: String,
    brightness: i32,
    max: i32,
    log: Log,
}

#[interface(name = "org.kde.ScreenBrightness.Display")]
impl FakeDisplay {
    #[zbus(property, name = "Label")]
    fn label(&self) -> String {
        self.label.clone()
    }

    #[zbus(property, name = "Brightness")]
    fn brightness(&self) -> i32 {
        self.brightness
    }

    #[zbus(property, name = "MaxBrightness")]
    fn max_brightness(&self) -> i32 {
        self.max
    }

    #[zbus(property, name = "IsInternal")]
    fn is_internal(&self) -> bool {
        false
    }

    /// Takes effect at once, as powerdevil's does for a display that answers quickly.
    #[zbus(name = "SetBrightness")]
    async fn set_brightness(
        &mut self,
        value: i32,
        flags: u32,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<()> {
        self.log.push(Request::SetBrightness {
            display: self.id.clone(),
            value,
            flags,
        });
        self.brightness = value;
        self.brightness_changed(&emitter)
            .await
            .map_err(|error| zbus::fdo::Error::Failed(error.to_string()))
    }
}

#[derive(Clone, Copy)]
pub struct NightLightState {
    pub available: bool,
    pub enabled: bool,
    pub inhibited: bool,
    pub running: bool,
}

struct FakeNightLight {
    state: NightLightState,
}

#[interface(name = "org.kde.KWin.NightLight")]
impl FakeNightLight {
    #[zbus(property, name = "available")]
    fn available(&self) -> bool {
        self.state.available
    }

    #[zbus(property, name = "enabled")]
    fn enabled(&self) -> bool {
        self.state.enabled
    }

    #[zbus(property, name = "inhibited")]
    fn inhibited(&self) -> bool {
        self.state.inhibited
    }

    #[zbus(property, name = "running")]
    fn running(&self) -> bool {
        self.state.running
    }
}

struct FakeComponent {
    log: Log,
}

#[interface(name = "org.kde.kglobalaccel.Component")]
impl FakeComponent {
    #[zbus(name = "invokeShortcut")]
    fn invoke_shortcut(&self, name: String) {
        self.log.push(Request::InvokeShortcut(name));
    }
}

pub struct FakePlasma {
    connection: Connection,
    pub log: Log,
}

impl FakePlasma {
    /// Serves both displays, a running Night Light and KGlobalAccel's kwin component, owning all
    /// three well-known names.
    pub async fn start(bus: &PrivateBus) -> Self {
        let night_light = NightLightState {
            available: true,
            enabled: true,
            inhibited: false,
            running: true,
        };
        Self::start_with(bus, night_light).await
    }

    pub async fn start_with(bus: &PrivateBus, night_light: NightLightState) -> Self {
        let plasma = Self::serve(bus, night_light).await;
        plasma.claim().await;
        plasma
    }

    pub async fn serve(bus: &PrivateBus, night_light: NightLightState) -> Self {
        let log = Log::default();
        let connection = bus.connect().await;
        let server = connection.object_server();
        let displays = [
            (MONITOR, MONITOR_LABEL, MONITOR_MAX, MONITOR_MAX),
            (PANEL, PANEL_LABEL, 100, PANEL_MAX),
        ];
        server
            .at(
                SCREEN_BRIGHTNESS_PATH,
                FakeScreenBrightness {
                    displays: displays.iter().map(|(id, ..)| (*id).to_owned()).collect(),
                },
            )
            .await
            .unwrap();
        for (id, label, brightness, max) in displays {
            let display = FakeDisplay {
                id: id.to_owned(),
                label: label.to_owned(),
                brightness,
                max,
                log: log.clone(),
            };
            server
                .at(format!("{SCREEN_BRIGHTNESS_PATH}/{id}").as_str(), display)
                .await
                .unwrap();
        }
        server
            .at(NIGHT_LIGHT_PATH, FakeNightLight { state: night_light })
            .await
            .unwrap();
        server
            .at(KWIN_COMPONENT_PATH, FakeComponent { log: log.clone() })
            .await
            .unwrap();
        Self { connection, log }
    }

    pub async fn claim(&self) {
        for name in [SCREEN_BRIGHTNESS, KWIN, KGLOBALACCEL] {
            self.connection.request_name(name).await.unwrap();
        }
    }

    pub async fn release_screen_brightness(&self) {
        self.connection
            .release_name(SCREEN_BRIGHTNESS)
            .await
            .unwrap();
    }

    /// Moves a display's brightness and says so, as powerdevil does when a key is pressed.
    pub async fn set_display_brightness(&self, id: &str, brightness: i32) {
        let display = self
            .connection
            .object_server()
            .interface::<_, FakeDisplay>(format!("{SCREEN_BRIGHTNESS_PATH}/{id}").as_str())
            .await
            .unwrap();
        let mut fields = display.get_mut().await;
        fields.brightness = brightness;
        fields
            .brightness_changed(display.signal_emitter())
            .await
            .unwrap();
    }

    /// Changes Night Light and says so, as KWin does.
    pub async fn set_night_light(&self, state: NightLightState) {
        let night_light = self
            .connection
            .object_server()
            .interface::<_, FakeNightLight>(NIGHT_LIGHT_PATH)
            .await
            .unwrap();
        let mut fields = night_light.get_mut().await;
        fields.state = state;
        let emitter = night_light.signal_emitter();
        fields.available_changed(emitter).await.unwrap();
        fields.enabled_changed(emitter).await.unwrap();
        fields.inhibited_changed(emitter).await.unwrap();
        fields.running_changed(emitter).await.unwrap();
    }
}
