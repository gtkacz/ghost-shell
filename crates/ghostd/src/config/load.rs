use std::collections::BTreeSet;
use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};
use std::{env, fs, io};

use ghost_proto::{
    Appearance, Dashboard, LayoutConfig, LayoutNode, ProviderSpec, ResolvedConfig, Thresholds,
};

const PERCENT: RangeInclusive<u32> = 0..=100;
const HISTORY_MINUTES: RangeInclusive<u32> = 1..=60;
const MIN_DOT_SPACING: u32 = 1;
const COLOUR_HEX_DIGITS: usize = 6;
const APP_CONFIG_DIR: &str = "ghost-shell";
const CONFIG_FILE: &str = "config.toml";

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("{message}")]
pub struct ConfigError {
    pub message: String,
}

fn invalid(path: impl std::fmt::Display, problem: impl std::fmt::Display) -> ConfigError {
    ConfigError {
        message: format!("{path}: {problem}"),
    }
}

pub fn load_str(s: &str) -> Result<ResolvedConfig, ConfigError> {
    load_str_with_user(s, &env::var("USER").unwrap_or_default())
}

pub fn load_str_with_user(s: &str, user: &str) -> Result<ResolvedConfig, ConfigError> {
    let mut config: ResolvedConfig = toml::from_str(s).map_err(|e| ConfigError {
        message: e.to_string(),
    })?;
    // Deserialization replaces the whole map when the user names any view, which would drop the
    // default views they did not mention.
    let mut views = LayoutConfig::default().views;
    views.append(&mut config.layout.views);
    config.layout.views = views;
    if config.greeting.name.is_empty() {
        config.greeting.name = user.to_owned();
    }
    validate(&config)?;
    Ok(config)
}

/// A missing file yields the defaults, so a fresh install runs without any setup.
pub fn load_path(path: &Path) -> Result<ResolvedConfig, ConfigError> {
    match fs::read_to_string(path) {
        Ok(contents) => load_str(&contents),
        Err(e) if e.kind() == io::ErrorKind::NotFound => load_str(""),
        Err(e) => Err(invalid(path.display(), e)),
    }
}

pub fn default_path() -> PathBuf {
    let config_home = env::var_os("XDG_CONFIG_HOME")
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env::var_os("HOME").unwrap_or_default()).join(".config"));
    config_home.join(APP_CONFIG_DIR).join(CONFIG_FILE)
}

fn validate(config: &ResolvedConfig) -> Result<(), ConfigError> {
    validate_appearance(&config.appearance)?;
    validate_thresholds(&config.thresholds)?;
    validate_layout(&config.layout)?;
    validate_dashboard(&config.dashboard, &config.layout)?;
    validate_providers(&config.providers)
}

fn validate_appearance(appearance: &Appearance) -> Result<(), ConfigError> {
    check_range("appearance.panel_fill", appearance.panel_fill, &PERCENT)?;
    check_range("appearance.dots.opacity", appearance.dots.opacity, &PERCENT)?;
    check_range(
        "appearance.scanlines.opacity",
        appearance.scanlines.opacity,
        &PERCENT,
    )?;
    if appearance.dots.spacing < MIN_DOT_SPACING {
        return Err(invalid(
            "appearance.dots.spacing",
            format!(
                "must be at least {MIN_DOT_SPACING}, got {}",
                appearance.dots.spacing
            ),
        ));
    }
    let colors = &appearance.colors;
    let named_colors = [
        ("ground", &colors.ground),
        ("panel", &colors.panel),
        ("hair", &colors.hair),
        ("hair2", &colors.hair2),
        ("text", &colors.text),
        ("hi", &colors.hi),
        ("dim", &colors.dim),
        ("signal", &colors.signal),
        ("warn", &colors.warn),
        ("crit", &colors.crit),
        ("dot", &colors.dot),
    ];
    for (name, value) in named_colors {
        if !is_hex_colour(value) {
            return Err(invalid(
                format!("appearance.colors.{name}"),
                format!("expected #rrggbb, got {value:?}"),
            ));
        }
    }
    Ok(())
}

fn is_hex_colour(value: &str) -> bool {
    value.strip_prefix('#').is_some_and(|digits| {
        digits.len() == COLOUR_HEX_DIGITS && digits.bytes().all(|b| b.is_ascii_hexdigit())
    })
}

fn check_range(path: &str, value: u32, range: &RangeInclusive<u32>) -> Result<(), ConfigError> {
    if range.contains(&value) {
        return Ok(());
    }
    Err(invalid(
        path,
        format!("must be {}..={}, got {value}", range.start(), range.end()),
    ))
}

fn validate_thresholds(thresholds: &Thresholds) -> Result<(), ConfigError> {
    let t = thresholds;
    check_below(
        "thresholds",
        ("load_warn", t.load_warn),
        ("load_crit", t.load_crit),
    )?;
    check_below(
        "thresholds",
        ("temp_warn", t.temp_warn),
        ("temp_crit", t.temp_crit),
    )?;
    check_below(
        "thresholds",
        ("battery_crit", t.battery_crit),
        ("battery_warn", t.battery_warn),
    )?;
    check_below(
        "thresholds",
        ("aqi_warn", t.aqi_warn),
        ("aqi_crit", t.aqi_crit),
    )?;
    for (id, sensor) in &t.sensors {
        check_below(
            &format!("thresholds.sensors.{id}"),
            ("warn", sensor.warn),
            ("crit", sensor.crit),
        )?;
    }
    Ok(())
}

fn check_below(
    path: &str,
    (lower_name, lower): (&str, f32),
    (upper_name, upper): (&str, f32),
) -> Result<(), ConfigError> {
    // Written as a positive comparison so that NaN, which orders against nothing, is rejected.
    if lower < upper {
        return Ok(());
    }
    Err(invalid(
        path,
        format!("{lower_name} ({lower}) must be below {upper_name} ({upper})"),
    ))
}

fn validate_layout(layout: &LayoutConfig) -> Result<(), ConfigError> {
    layout
        .views
        .iter()
        .try_for_each(|(name, node)| validate_node(&format!("layout.views.{name}"), node))
}

fn validate_node(path: &str, node: &LayoutNode) -> Result<(), ConfigError> {
    match node {
        LayoutNode::Slot { .. } => Ok(()),
        LayoutNode::Split { children, .. } => {
            children.iter().enumerate().try_for_each(|(index, child)| {
                validate_node(&format!("{path}.children[{index}]"), child)
            })
        }
        LayoutNode::Tabs { tabs, default, .. } => {
            if tabs.is_empty() {
                return Err(invalid(
                    format!("{path}.tabs"),
                    "must have at least one tab",
                ));
            }
            if *default >= tabs.len() {
                return Err(invalid(
                    format!("{path}.default"),
                    format!("{default} is out of range for {} tabs", tabs.len()),
                ));
            }
            Ok(())
        }
    }
}

fn validate_dashboard(dashboard: &Dashboard, layout: &LayoutConfig) -> Result<(), ConfigError> {
    check_range(
        "dashboard.history_minutes",
        dashboard.history_minutes,
        &HISTORY_MINUTES,
    )?;
    for view in &dashboard.main_views {
        check_view_defined("dashboard.main_views", view, layout)?;
    }
    check_view_defined("dashboard.default_view", &dashboard.default_view, layout)?;
    if !dashboard.monitor_view.is_empty() {
        check_view_defined("dashboard.monitor_view", &dashboard.monitor_view, layout)?;
    }
    if !dashboard.main_views.contains(&dashboard.default_view) {
        return Err(invalid(
            "dashboard.default_view",
            format!(
                "view {:?} is not listed in dashboard.main_views",
                dashboard.default_view
            ),
        ));
    }
    Ok(())
}

fn check_view_defined(path: &str, view: &str, layout: &LayoutConfig) -> Result<(), ConfigError> {
    if layout.views.contains_key(view) {
        return Ok(());
    }
    Err(invalid(
        path,
        format!("view {view:?} is not defined in layout.views"),
    ))
}

fn validate_providers(providers: &[ProviderSpec]) -> Result<(), ConfigError> {
    let mut seen = BTreeSet::new();
    for (index, provider) in providers.iter().enumerate() {
        if provider.name.is_empty() {
            return Err(invalid(
                format!("providers[{index}].name"),
                "must not be empty",
            ));
        }
        if !seen.insert(provider.name.as_str()) {
            return Err(invalid(
                format!("providers[{index}].name"),
                format!("duplicate provider {:?}", provider.name),
            ));
        }
        if provider.exec.is_empty() {
            return Err(invalid(
                format!("providers[{index}].exec"),
                "must not be empty",
            ));
        }
    }
    Ok(())
}
