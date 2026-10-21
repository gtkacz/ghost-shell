use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct CpuSnapshot {
    pub model: String,
    pub threads: Vec<f32>,
    pub ccds: Vec<CcdLoad>,
    pub load: f32,
    pub freq_mhz: u32,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct CcdLoad {
    pub id: u8,
    pub cpus: Vec<u16>,
    pub l3_kib: u32,
    pub vcache: bool,
    pub load: f32,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct MemSnapshot {
    pub total_kib: u64,
    pub used_kib: u64,
    pub swap_total_kib: u64,
    pub swap_used_kib: u64,
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewAction {
    Monitor,
    Swap,
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
