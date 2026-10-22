use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Every section merges field by field, so a user file only names what it changes.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ResolvedConfig {
    pub appearance: Appearance,
    pub thresholds: Thresholds,
    pub display: DisplayConfig,
    pub sensors: Labels,
    pub disks: Labels,
    pub dashboard: Dashboard,
    pub bar: BarConfig,
    pub greeting: Greeting,
    pub location: Location,
    pub i18n: I18n,
    pub shortcuts: Shortcuts,
    pub notifications: Notifications,
    pub layout: LayoutConfig,
    pub widgets: Widgets,
    pub providers: Vec<ProviderSpec>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Japanese {
    Kanji,
    Katakana,
    #[default]
    Mixed,
    Off,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Appearance {
    pub font: String,
    pub font_cjk: String,
    pub japanese: Japanese,
    pub alert_chips: bool,
    pub panel_fill: u32,
    /// Frames a second for the media spectrum while music plays: 15 or 30.
    pub spectrum_fps: u32,
    /// Seconds for a boxed alert's stripes to travel one tile, blinking or held.
    pub alert_stripe_period_s: f64,
    /// Seconds between the steps of a held alert's stripes.
    pub alert_stripe_step_s: f64,
    /// Seconds a boxed alert flickers, blinks and scrolls its stripes before it holds still.
    pub alert_attention_s: f64,
    pub colors: Colors,
    pub dots: Dots,
    pub scanlines: Scanlines,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            font: "Kode Mono".into(),
            font_cjk: "Noto Sans CJK JP".into(),
            japanese: Japanese::default(),
            alert_chips: true,
            panel_fill: 55,
            spectrum_fps: 15,
            alert_stripe_period_s: 1.6,
            alert_stripe_step_s: 1.0,
            alert_attention_s: 10.0,
            colors: Colors::default(),
            dots: Dots::default(),
            scanlines: Scanlines::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Colors {
    pub ground: String,
    pub panel: String,
    pub hair: String,
    pub hair2: String,
    pub text: String,
    pub hi: String,
    pub dim: String,
    pub signal: String,
    pub warn: String,
    pub crit: String,
    pub dot: String,
}

impl Default for Colors {
    fn default() -> Self {
        Self {
            ground: "#0a0f11".into(),
            panel: "#0a0f11".into(),
            hair: "#2a3a3d".into(),
            hair2: "#1a2527".into(),
            text: "#b7d1cd".into(),
            hi: "#e4f1ef".into(),
            dim: "#587170".into(),
            signal: "#a3d4cc".into(),
            warn: "#e9a84c".into(),
            crit: "#ff3b45".into(),
            dot: "#a3d4cc".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Dots {
    pub spacing: u32,
    pub opacity: u32,
}

impl Default for Dots {
    fn default() -> Self {
        Self {
            spacing: 24,
            opacity: 16,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Scanlines {
    pub opacity: u32,
}

impl Default for Scanlines {
    fn default() -> Self {
        Self { opacity: 15 }
    }
}

/// Load, temperature and air quality alarm when a value rises past a limit; battery alarms when
/// it falls below one, so its `crit` sits under its `warn`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Thresholds {
    pub load_warn: f32,
    pub load_crit: f32,
    pub temp_warn: f32,
    pub temp_crit: f32,
    pub battery_warn: f32,
    pub battery_crit: f32,
    pub aqi_warn: f32,
    pub aqi_crit: f32,
    pub sensors: BTreeMap<String, SensorThreshold>,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            load_warn: 75.0,
            load_crit: 90.0,
            temp_warn: 80.0,
            temp_crit: 90.0,
            battery_warn: 20.0,
            battery_crit: 10.0,
            aqi_warn: 100.0,
            aqi_crit: 150.0,
            sensors: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SensorThreshold {
    pub warn: f32,
    pub crit: f32,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DisplayConfig {
    pub model: String,
}

/// Custom names keyed by the device's stable id (sensor id, drive serial).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Labels {
    pub labels: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Dashboard {
    pub main_views: Vec<String>,
    pub monitor_view: String,
    pub default_view: String,
    pub history_minutes: u32,
}

impl Default for Dashboard {
    fn default() -> Self {
        Self {
            main_views: vec!["v1".into(), "v2".into()],
            monitor_view: "monitor".into(),
            default_view: "v1".into(),
            history_minutes: 5,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BarLayout {
    #[default]
    A,
    B,
    C,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BarPosition {
    #[default]
    Top,
    Bottom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IconStyle {
    #[default]
    Mono,
    Colour,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BarConfig {
    pub layout: BarLayout,
    pub position: BarPosition,
    pub icons: IconStyle,
    pub pinned: Vec<String>,
    pub brand: Brand,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Brand {
    pub enabled: bool,
    pub text: String,
    pub subtitle: String,
}

impl Default for Brand {
    fn default() -> Self {
        Self {
            enabled: true,
            text: "GS//01".into(),
            subtitle: "攻殻".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Greeting {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Location {
    pub city: String,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    pub alerts: Alerts,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Alerts {
    pub provider: String,
    pub area: String,
}

/// Language tags such as `pt-BR`; with both empty, the alerts keep the issuer's words.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct I18n {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Shortcuts {
    pub monitor: String,
    pub swap: String,
}

impl Default for Shortcuts {
    fn default() -> Self {
        Self {
            monitor: "Meta+M".into(),
            swap: "Meta+Shift+M".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Notifications {
    pub history_limit: u32,
    pub popup_timeout_s: u32,
}

impl Default for Notifications {
    fn default() -> Self {
        Self {
            history_limit: 200,
            popup_timeout_s: 6,
        }
    }
}

/// Sizes are in design pixels; a node without `size` shares the leftover space equally.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LayoutConfig {
    pub gutter: f32,
    pub margin: f32,
    #[serde(deserialize_with = "crate::node_de::views")]
    pub views: BTreeMap<String, LayoutNode>,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            gutter: 12.0,
            margin: 12.0,
            views: BTreeMap::from([
                ("v1".to_owned(), default_v1()),
                ("v2".to_owned(), default_v2()),
                ("monitor".to_owned(), default_monitor()),
            ]),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    Row,
    Column,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TabSpec {
    pub widget: String,
    pub title: Option<String>,
    #[serde(default)]
    pub settings: Map<String, Value>,
}

/// Untagged, so the variant is chosen by which keys a TOML table carries; reading it back is in
/// `node_de`, which reports a bad key with the node's path.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum LayoutNode {
    Slot {
        widget: String,
        size: Option<f32>,
        settings: Map<String, Value>,
    },
    Split {
        direction: Direction,
        size: Option<f32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        gutter: Option<f32>,
        children: Vec<LayoutNode>,
    },
    Tabs {
        tabs: Vec<TabSpec>,
        size: Option<f32>,
        default: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Widgets {
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderSpec {
    pub name: String,
    pub exec: Vec<String>,
    #[serde(default = "restart_by_default")]
    pub restart: bool,
}

fn restart_by_default() -> bool {
    true
}

fn slot(widget: &str, size: Option<f32>) -> LayoutNode {
    LayoutNode::Slot {
        widget: widget.into(),
        size,
        settings: Map::new(),
    }
}

fn placeholder(label: &str, size: Option<f32>) -> LayoutNode {
    LayoutNode::Slot {
        widget: "placeholder".into(),
        size,
        settings: Map::from_iter([("label".to_owned(), Value::String(label.into()))]),
    }
}

fn styled(widget: &str, size: Option<f32>, style: &str) -> LayoutNode {
    LayoutNode::Slot {
        widget: widget.into(),
        size,
        settings: Map::from_iter([("style".to_owned(), Value::String(style.into()))]),
    }
}

fn placeholder_tab(label: &str, title: &str) -> TabSpec {
    TabSpec {
        widget: "placeholder".into(),
        title: Some(title.into()),
        settings: Map::from_iter([("label".to_owned(), Value::String(label.into()))]),
    }
}

fn split(direction: Direction, size: Option<f32>, children: Vec<LayoutNode>) -> LayoutNode {
    LayoutNode::Split {
        direction,
        size,
        gutter: None,
        children,
    }
}

fn default_v1() -> LayoutNode {
    use Direction::{Column, Row};
    split(
        Row,
        None,
        vec![
            split(
                Column,
                Some(300.0),
                vec![
                    slot("processor", Some(250.0)),
                    slot("graphics", Some(204.0)),
                    slot("memory", None),
                ],
            ),
            split(
                Column,
                None,
                vec![
                    styled("clock", Some(262.0), "hero"),
                    split(
                        Row,
                        None,
                        vec![
                            styled("media", Some(290.0), "stack"),
                            placeholder("Notifications", None),
                        ],
                    ),
                ],
            ),
            split(
                Column,
                Some(300.0),
                vec![
                    placeholder("Uplink", Some(300.0)),
                    slot("storage", None),
                    slot("sensors", Some(184.0)),
                ],
            ),
        ],
    )
}

fn default_v2() -> LayoutNode {
    use Direction::{Column, Row};
    split(
        Row,
        None,
        vec![
            split(
                Column,
                Some(300.0),
                vec![
                    slot("processor", Some(250.0)),
                    slot("graphics", Some(204.0)),
                    slot("memory", None),
                ],
            ),
            split(
                Column,
                None,
                vec![
                    styled("clock", Some(150.0), "band"),
                    LayoutNode::Tabs {
                        tabs: vec![
                            placeholder_tab("Uplink", "UPLINK"),
                            placeholder_tab("Silicon", "SILICON"),
                            placeholder_tab("Process tree", "PROCESS TREE"),
                        ],
                        size: None,
                        default: 0,
                    },
                    styled("media", Some(120.0), "strip"),
                ],
            ),
            split(
                Column,
                Some(300.0),
                vec![
                    placeholder("Notifications", Some(290.0)),
                    placeholder("Network", Some(164.0)),
                    slot("storage", None),
                ],
            ),
        ],
    )
}

fn default_monitor() -> LayoutNode {
    use Direction::{Column, Row};
    split(
        Column,
        None,
        vec![
            split(
                Row,
                Some(104.0),
                vec![
                    styled("clock", Some(250.0), "compact"),
                    slot("weather", None),
                    styled("media", Some(340.0), "compact"),
                ],
            ),
            split(
                Row,
                None,
                vec![
                    placeholder("Telemetry", None),
                    split(
                        Column,
                        Some(340.0),
                        vec![
                            placeholder("Process tree", None),
                            slot("sensors", Some(184.0)),
                        ],
                    ),
                ],
            ),
        ],
    )
}
