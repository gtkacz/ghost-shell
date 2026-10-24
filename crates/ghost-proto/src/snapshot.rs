use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct CpuSnapshot {
    pub model: String,
    pub threads: Vec<f32>,
    pub ccds: Vec<CcdLoad>,
    /// One per physical core, ordered by its lowest cpu.
    pub cores: Vec<CoreLoad>,
    pub load: f32,
    pub freq_mhz: u32,
    /// Watts drawn by the CPU package, or `None` while the RAPL counter is unreadable or has no
    /// interval to measure yet.
    pub package_w: Option<f32>,
    /// Busy percentage of an AMD integrated GPU, or `None` when no driver reports one.
    pub igpu_busy_pct: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct CcdLoad {
    pub id: u8,
    pub cpus: Vec<u16>,
    pub l3_kib: u32,
    pub vcache: bool,
    pub load: f32,
}

/// A physical core: its SMT threads and their mean load.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct CoreLoad {
    pub cpus: Vec<u16>,
    pub load: f32,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct MemSnapshot {
    pub total_kib: u64,
    pub used_kib: u64,
    pub swap_total_kib: u64,
    pub swap_used_kib: u64,
}

/// The busiest processes, busiest first.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ProcsSnapshot {
    pub top: Vec<ProcUsage>,
}

/// `cpu` and `mem` are both percentages of the whole machine.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ProcUsage {
    pub pid: u32,
    pub name: String,
    pub cpu: f32,
    pub mem: f32,
}

/// Every user process, by pid. Kernel threads are only counted.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ProcTree {
    pub kernel_threads: u32,
    pub procs: Vec<ProcNode>,
}

/// `cpu` is the process's share of the whole machine since the previous process scan.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ProcNode {
    pub pid: u32,
    pub ppid: u32,
    pub name: String,
    pub cpu: f32,
    pub rss_kib: u64,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct SensorSnapshot {
    pub temps: Vec<Reading>,
    pub fans: Vec<Reading>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Reading {
    pub id: String,
    pub label: String,
    pub value: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct GpuSnapshot {
    pub present: bool,
    pub name: String,
    pub load: f32,
    pub vram_used_mib: u32,
    pub vram_total_mib: u32,
    pub temp_c: f32,
    pub power_w: f32,
    pub power_limit_w: f32,
    pub fan_pct: f32,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct DiskSnapshot {
    pub drives: Vec<Drive>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Drive {
    pub id: String,
    pub label: String,
    pub model: String,
    pub kernel_name: String,
    pub state: DriveState,
    pub mounts: Vec<String>,
    pub used_pct: Option<f32>,
    pub temp_c: Option<f32>,
    pub read_bps: u64,
    pub write_bps: u64,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DriveState {
    #[default]
    Live,
    Dead,
    Unmounted,
}

/// Network rates in bytes per second. The headline rates are those of `default_iface`, the
/// interface of the lowest-metric IPv4 default route that is up, and read 0 without one.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct NetSnapshot {
    pub default_iface: Option<String>,
    pub rx_bps: u64,
    pub tx_bps: u64,
    /// Every interface but loopback, in the kernel's order.
    pub interfaces: Vec<NetIface>,
}

/// An interface's rates read 0 on the sample that first sees it, and on one whose counters went
/// backwards.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct NetIface {
    pub name: String,
    /// Carrying traffic: its operstate is `up`, or `unknown` with the interface administratively
    /// up, as tunnels report it.
    pub oper_up: bool,
    pub rx_bps: u64,
    pub tx_bps: u64,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct HostSnapshot {
    pub hostname: String,
    pub kernel: String,
    /// Seconds since the Unix epoch.
    pub boot_time_s: u64,
}

/// What the KWin script last reported, and the last view action a shortcut asked for.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct DesktopSnapshot {
    /// The connector names (`DP-1`) of the outputs that windows cover completely.
    pub covered_outputs: Vec<String>,
    /// The desktop, not an app window, has focus, so shortcuts act on the dashboard.
    pub active: bool,
    pub action: Option<ViewAction>,
    /// Counts accepted actions, so pressing the same shortcut twice is two changes.
    pub action_seq: u64,
    /// Counts launcher toggles, which happen whatever has focus.
    pub launcher_seq: u64,
    /// The pinned app (0-based) that the latest task shortcut asked for.
    pub task: Option<u8>,
    /// Counts task presses, so pressing the same one twice is two changes.
    pub task_seq: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewAction {
    Monitor,
    Swap,
}

/// Whether the screen is locked or the machine asleep, and the latest greeting to play.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct SessionSnapshot {
    /// logind's id for the graphical session, "" until known.
    pub id: String,
    pub locked: bool,
    pub sleeping: bool,
    /// The latest greeting asked for, `None` before the first.
    pub greeting: Option<GreetingKind>,
    /// Counts greetings, so two unlocks in a row are two changes.
    pub greeting_seq: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GreetingKind {
    Unlock,
    Wake,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct PerfSnapshot {
    pub daemon_cpu: f32,
    pub daemon_rss_kib: u64,
    pub shell_cpu: Option<f32>,
    pub shell_rss_kib: Option<u64>,
}

/// Open-Meteo's weather at `location`. Until a forecast for the configured place arrives, and
/// always without one, `available` is false and every other field keeps its default.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct WeatherSnapshot {
    pub available: bool,
    /// The last good forecast is older than two forecast cadences (failed fetches or a suspend).
    pub stale: bool,
    /// When the forecast was fetched, in seconds since the Unix epoch.
    pub updated_s: u64,
    pub temp_c: f32,
    /// The WMO weather code that `condition` names.
    pub code: u8,
    pub condition: String,
    pub is_day: bool,
    pub humidity_pct: u8,
    pub wind_kmh: f32,
    /// Today's, in the place's own time zone.
    pub high_c: f32,
    pub low_c: f32,
    /// The hours from the next full hour on.
    pub hours: Vec<WeatherHour>,
    /// The likeliest rain in the next 12 hours; `None` when no rain is forecast.
    pub rain_peak: Option<RainPeak>,
    /// The US AQI; `None` until air quality is fetched, and when Open-Meteo reports no value.
    pub aqi: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct WeatherHour {
    /// Seconds since the Unix epoch.
    pub at_s: u64,
    pub temp_c: f32,
    /// The chance of rain, as a percentage.
    pub rain_pct: u8,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct RainPeak {
    pub pct: u8,
    /// Seconds since the Unix epoch.
    pub at_s: u64,
}

/// What an alerts provider publishes on `weather.alerts`: the alerts in force or ahead for the
/// configured area.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct WeatherAlerts {
    /// Who issued the alerts, such as `INMET`.
    pub source: String,
    /// When the alerts were fetched, in seconds since the Unix epoch.
    pub updated_s: u64,
    /// Sorted by start.
    pub alerts: Vec<WeatherAlert>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WeatherAlert {
    pub id: String,
    /// This and `severity` are uppercased, and translated per `[i18n]` where the provider knows or
    /// can find a translation; `risks` are always the issuer's own words.
    pub title: String,
    pub severity: String,
    /// The issuer's own `title` and `severity`, uppercased, each present only when that field was
    /// translated.
    pub original_title: Option<String>,
    pub original_severity: Option<String>,
    /// A shown translation came from an online translator, directly or through an entry nobody
    /// has checked, rather than from a reviewed glossary.
    pub auto_translated: bool,
    pub level: AlertLevel,
    /// Seconds since the Unix epoch.
    pub start_s: u64,
    pub end_s: u64,
    pub risks: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertLevel {
    Warn,
    Crit,
}

/// The sun and moon at `location`, computed locally. Without a location `available` is false and
/// every other field keeps its default.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct SkySnapshot {
    pub available: bool,
    /// Seconds since the Unix epoch, for the machine's calendar day; `None` on a polar day or
    /// night, when the sun does not cross the horizon.
    pub sunrise_s: Option<u64>,
    pub sunset_s: Option<u64>,
    /// How far through the lunar cycle, from 0 at a new moon to 1 at the next.
    pub moon_phase: f32,
    /// The lit fraction of the disc, from 0 to 1.
    pub moon_illumination: f32,
    pub moon_name: String,
}

/// What the default output plays: bands from 50 Hz up to 16 kHz on a log scale, lowest first. Each
/// is the level of its loudest frequency, from 0 (−60 dBFS or quieter) up to 255 (0 dBFS).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct SpectrumSnapshot {
    pub bands: Vec<u8>,
}

/// The machine's network links: the wired and wireless devices and the saved VPNs NetworkManager
/// knows, Tailscale, and the round trip to `network.ping`. Each part is empty or `None` while its
/// source is absent.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct LinksSnapshot {
    pub lan: Vec<LanLink>,
    pub vpns: Vec<VpnLink>,
    pub tailscale: Option<TailscaleLink>,
    pub ping_ms: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LanLink {
    pub iface: String,
    /// The first IPv4 address with its prefix, such as `192.168.15.5/24`.
    pub address: Option<String>,
    /// Known for wired devices only.
    pub speed_mbps: Option<u32>,
    pub state: LinkState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VpnLink {
    /// The name of the saved connection.
    pub name: String,
    /// `openvpn`, `wireguard` and the like.
    pub kind: String,
    /// The tunnel interface, while the VPN has one.
    pub iface: Option<String>,
    pub state: LinkState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TailscaleLink {
    /// This machine's first tailnet address.
    pub address: Option<String>,
    pub peers_online: u32,
    pub peers_total: u32,
    pub state: LinkState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkState {
    Up,
    Connecting,
    Down,
}

/// Each drive's wear and power-on hours, from the SMART data UDisks2 last cached, so reading them
/// never wakes a drive. Empty while UDisks2 is absent.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct DiskHealth {
    pub drives: Vec<DriveHealth>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DriveHealth {
    /// The drive's serial, which a `disks` drive id ends with, after its last `#`.
    pub serial: String,
    /// How much of its rated endurance an NVMe drive has used; past 100 once it is exceeded.
    /// Always `None` for ATA drives.
    pub wear_pct: Option<u8>,
    pub power_on_hours: Option<u32>,
}

/// Device batteries: UPower's peripherals, then the phones KDE Connect reaches. Empty while
/// neither service reports any.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Batteries {
    pub devices: Vec<Battery>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Battery {
    /// UPower's `NativePath`, or `kdeconnect:<device id>`; the key of `[batteries] labels`.
    pub id: String,
    /// The configured label, or else the device's model or name, uppercased.
    pub label: String,
    pub kind: BatteryKind,
    /// `None` while the device has not reported a charge.
    pub pct: Option<f32>,
    pub charging: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BatteryKind {
    Controller,
    Phone,
    Mouse,
    Keyboard,
    Headset,
    Other,
}

/// Where the live connections lead, located offline, for the uplink globe: at most 8 endpoints,
/// the busiest first.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct UplinkSnapshot {
    pub endpoints: Vec<Endpoint>,
    /// Every endpoint before the cut to 8, the ones with no known place included.
    pub total: u32,
    /// The city database's licence notice, `""` while none is loaded.
    pub attribution: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Endpoint {
    /// The organisation, a configured label, the owning process or `TAILNET:<HOST>`.
    pub label: String,
    pub lat: f32,
    pub lon: f32,
    pub sockets: u32,
    pub tailnet: bool,
}

/// The windows the taskbar lists, most recently activated first.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct WindowsSnapshot {
    pub windows: Vec<WindowInfo>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct WindowInfo {
    /// KWin's `internalId`, which is what the window commands take.
    pub id: String,
    pub app_id: String,
    pub resource_class: String,
    pub caption: String,
    pub pid: u32,
    pub active: bool,
    pub minimized: bool,
    pub fullscreen: bool,
    pub demands_attention: bool,
}

/// What apps ask the launcher to show on their icons, keyed by desktop id (`org.kde.dolphin`).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct LauncherEntries {
    pub entries: BTreeMap<String, LauncherEntry>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct LauncherEntry {
    /// The badge number, or `None` while the app hides it.
    pub count: Option<i64>,
    /// 0 to 1, or `None` while the app hides it.
    pub progress: Option<f64>,
    pub urgent: bool,
}

/// What the bar's ticker shows beyond the other topics. `None` means unknown, and the shell leaves
/// that segment out.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct TickerSnapshot {
    /// Failed units of the system manager and the user manager together.
    pub failed_units: Option<u32>,
    pub firewalld: Option<bool>,
    /// Updates PackageKit has cached; it is never asked to refresh.
    pub updates: Option<u32>,
    pub nvidia_driver: Option<String>,
}

/// The files the user opened most recently, newest first.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct RecentFiles {
    pub files: Vec<RecentFile>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct RecentFile {
    pub path: String,
    pub name: String,
    /// The containing folder, with `$HOME` shortened to `~`.
    pub dir: String,
    pub modified_s: u64,
}

/// One entry per enabled provider, in config order.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct AgentsUsage {
    pub providers: Vec<ProviderUsage>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderUsage {
    pub id: String,
    pub label: String,
    pub plan: Option<String>,
    pub available: bool,
    /// Why the usage could not be read, when `available` is false.
    pub reason: Option<String>,
    pub read_at_s: u64,
    pub windows: Vec<UsageWindow>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UsageWindow {
    pub length_s: u64,
    pub used_pct: f64,
    pub resets_at_s: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct AgentsSessions {
    pub sessions: Vec<AgentSession>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentSession {
    pub provider: String,
    pub id: String,
    pub name: String,
    pub cwd: String,
    pub state: AgentState,
    pub waiting_for: Option<String>,
    pub since_s: u64,
    pub pid: Option<u32>,
    pub tmux: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentState {
    Busy,
    Idle,
    Waiting,
}

/// The displays whose backlight KDE's `org.kde.ScreenBrightness` can set, and Night Light. Empty
/// and unavailable while neither service answers.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Brightness {
    pub displays: Vec<DisplayBrightness>,
    pub night_light: NightLight,
}

/// `brightness` and `max` are in the display's own units, which `brightness.set` takes too.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisplayBrightness {
    /// The display's D-Bus name under `/org/kde/ScreenBrightness`, such as `display0`.
    pub id: String,
    pub label: String,
    pub brightness: i32,
    pub max: i32,
}

/// KWin's Night Light: `enabled` is its setting, `inhibited` whether it is suspended (what KWin's
/// "Toggle Night Color" flips), and `running` whether it is tinting the screen now, which a
/// scheduled Night Light does not do by day.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct NightLight {
    pub available: bool,
    pub enabled: bool,
    pub inhibited: bool,
    pub running: bool,
}
