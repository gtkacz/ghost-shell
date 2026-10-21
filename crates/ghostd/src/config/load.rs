use std::collections::BTreeSet;
use std::ffi::OsString;
use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};
use std::{env, fs, io};

use ghost_proto::{
    Alerts, Appearance, Dashboard, LayoutConfig, LayoutNode, Location, ProviderSpec,
    ResolvedConfig, Shortcuts, Thresholds,
};

use crate::keyseq;
use crate::providers::ALERTS_NAME_PREFIX;

const PERCENT: RangeInclusive<u32> = 0..=100;
const HISTORY_MINUTES: RangeInclusive<u32> = 1..=60;
const LATITUDE: RangeInclusive<f64> = -90.0..=90.0;
const LONGITUDE: RangeInclusive<f64> = -180.0..=180.0;
const MIN_DOT_SPACING: u32 = 1;
// The shell's two frame clock rates.
const SPECTRUM_FPS_CHOICES: [u32; 2] = [15, 30];
const COLOUR_HEX_DIGITS: usize = 6;
const XDG_CONFIG_HOME_ENV: &str = "XDG_CONFIG_HOME";
const HOME_ENV: &str = "HOME";
const CONFIG_HOME_SUBDIR: &str = ".config";
const APP_CONFIG_DIR: &str = "ghost-shell";
const CONFIG_FILE: &str = "config.toml";

/// A config problem that fits on one line, which is how the shell and the desktop notification
/// show it. `Display` gives the fuller `detail` when there is one, for the log.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("{}", .detail.as_deref().unwrap_or(.message))]
pub struct ConfigError {
    pub message: String,
    /// The parser's own report, which quotes the source and spans several lines.
    pub detail: Option<String>,
}

fn invalid(path: impl std::fmt::Display, problem: impl std::fmt::Display) -> ConfigError {
    ConfigError {
        message: format!("{path}: {problem}"),
        detail: None,
    }
}

fn parse_error(source: &str, error: &toml::de::Error) -> ConfigError {
    let problem = error
        .message()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let message = match error.span() {
        Some(span) => {
            let (line, column) = line_and_column(source, span.start);
            format!("line {line}, column {column}: {problem}")
        }
        None => problem,
    };
    ConfigError {
        message,
        detail: Some(error.to_string().trim_end().to_owned()),
    }
}

fn line_and_column(source: &str, offset: usize) -> (usize, usize) {
    let before = source.get(..offset).unwrap_or(source);
    let line = before.matches('\n').count() + 1;
    let column = before.chars().rev().take_while(|&c| c != '\n').count() + 1;
    (line, column)
}

pub fn load_str(s: &str) -> Result<ResolvedConfig, ConfigError> {
    load_str_with_user(s, &env::var("USER").unwrap_or_default())
}

pub fn load_str_with_user(s: &str, user: &str) -> Result<ResolvedConfig, ConfigError> {
    let mut config: ResolvedConfig = toml::from_str(s).map_err(|error| parse_error(s, &error))?;
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
    config_dir().join(CONFIG_FILE)
}

/// Where the user's config file and widgets live.
pub(crate) fn config_dir() -> PathBuf {
    config_dir_in(env::var_os(XDG_CONFIG_HOME_ENV), env::var_os(HOME_ENV))
}

fn config_dir_in(xdg_config_home: Option<OsString>, home: Option<OsString>) -> PathBuf {
    xdg_config_home
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(home.unwrap_or_default()).join(CONFIG_HOME_SUBDIR))
        .join(APP_CONFIG_DIR)
}

fn validate(config: &ResolvedConfig) -> Result<(), ConfigError> {
    validate_appearance(&config.appearance)?;
    validate_thresholds(&config.thresholds)?;
    validate_layout(&config.layout)?;
    validate_dashboard(&config.dashboard, &config.layout)?;
    validate_location(&config.location)?;
    validate_alerts(&config.location.alerts)?;
    validate_shortcuts(&config.shortcuts)?;
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
    if !SPECTRUM_FPS_CHOICES.contains(&appearance.spectrum_fps) {
        return Err(invalid(
            "appearance.spectrum_fps",
            format!(
                "must be {} or {}, got {}",
                SPECTRUM_FPS_CHOICES[0], SPECTRUM_FPS_CHOICES[1], appearance.spectrum_fps
            ),
        ));
    }
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

pub(crate) fn is_hex_colour(value: &str) -> bool {
    value.strip_prefix('#').is_some_and(|digits| {
        digits.len() == COLOUR_HEX_DIGITS && digits.bytes().all(|b| b.is_ascii_hexdigit())
    })
}

fn check_range<T: PartialOrd + std::fmt::Display>(
    path: &str,
    value: T,
    range: &RangeInclusive<T>,
) -> Result<(), ConfigError> {
    if range.contains(&value) {
        return Ok(());
    }
    Err(invalid(
        path,
        format!("must be {}..={}, got {value}", range.start(), range.end()),
    ))
}

fn validate_thresholds(thresholds: &Thresholds) -> Result<(), ConfigError> {
    check_below(
        "thresholds",
        ("load_warn", thresholds.load_warn),
        ("load_crit", thresholds.load_crit),
    )?;
    check_below(
        "thresholds",
        ("temp_warn", thresholds.temp_warn),
        ("temp_crit", thresholds.temp_crit),
    )?;
    check_below(
        "thresholds",
        ("battery_crit", thresholds.battery_crit),
        ("battery_warn", thresholds.battery_warn),
    )?;
    check_below(
        "thresholds",
        ("aqi_warn", thresholds.aqi_warn),
        ("aqi_crit", thresholds.aqi_crit),
    )?;
    for (id, sensor) in &thresholds.sensors {
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
    check_non_negative("layout.gutter", layout.gutter)?;
    check_non_negative("layout.margin", layout.margin)?;
    layout
        .views
        .iter()
        .try_for_each(|(name, node)| validate_node(&format!("layout.views.{name}"), node))
}

fn check_non_negative(path: &str, value: f32) -> Result<(), ConfigError> {
    // Infinity is not negative, so finiteness has to be checked on its own.
    if value.is_finite() && value >= 0.0 {
        return Ok(());
    }
    Err(invalid(
        path,
        format!("must be a non-negative number, got {value}"),
    ))
}

fn validate_node(path: &str, node: &LayoutNode) -> Result<(), ConfigError> {
    let (LayoutNode::Slot { size, .. }
    | LayoutNode::Split { size, .. }
    | LayoutNode::Tabs { size, .. }) = node;
    if let Some(size) = *size
        && !(size.is_finite() && size > 0.0)
    {
        return Err(invalid(
            path,
            format!("size must be a positive number, got {size}"),
        ));
    }
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
    let mut seen = BTreeSet::new();
    for (index, view) in dashboard.main_views.iter().enumerate() {
        check_view_defined("dashboard.main_views", view, layout)?;
        if !seen.insert(view.as_str()) {
            return Err(invalid(
                format!("dashboard.main_views[{index}]"),
                format!("duplicate view {view:?}"),
            ));
        }
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

fn validate_location(location: &Location) -> Result<(), ConfigError> {
    match (location.lat, location.lon) {
        (Some(lat), Some(lon)) => {
            check_range("location.lat", lat, &LATITUDE)?;
            check_range("location.lon", lon, &LONGITUDE)
        }
        (None, None) => Ok(()),
        _ => Err(invalid("location", "lat and lon must be set together")),
    }
}

fn validate_alerts(alerts: &Alerts) -> Result<(), ConfigError> {
    if alerts.provider.is_empty() {
        return Ok(());
    }
    // The name becomes part of a program name and a provider name.
    if !is_alerts_provider_name(&alerts.provider) {
        return Err(invalid(
            "location.alerts.provider",
            format!("{:?} must match ^[a-z][a-z0-9-]*$", alerts.provider),
        ));
    }
    if alerts.area.is_empty() {
        return Err(invalid(
            "location.alerts.area",
            "must not be empty when a provider is set",
        ));
    }
    Ok(())
}

fn is_alerts_provider_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some('a'..='z'))
        && chars.all(|c| matches!(c, 'a'..='z' | '0'..='9' | '-'))
}

fn validate_shortcuts(shortcuts: &Shortcuts) -> Result<(), ConfigError> {
    let parse = |name: &str, sequence: &str| {
        keyseq::parse(sequence).map_err(|error| invalid(format!("shortcuts.{name}"), error))
    };
    let monitor = parse("monitor", &shortcuts.monitor)?;
    let swap = parse("swap", &shortcuts.swap)?;
    // KGlobalAccel would take one key for both, and a press would run only one of them.
    if swap == monitor {
        return Err(invalid(
            "shortcuts.swap",
            format!("{:?} is the same key as shortcuts.monitor", shortcuts.swap),
        ));
    }
    Ok(())
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
        if provider.name.starts_with(ALERTS_NAME_PREFIX) {
            return Err(invalid(
                format!("providers[{index}].name"),
                format!("{:?} is reserved for location.alerts", provider.name),
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
        if let Some(entry) = provider.exec.iter().position(String::is_empty) {
            return Err(invalid(
                format!("providers[{index}].exec[{entry}]"),
                "must not be empty",
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir_in(xdg_config_home: Option<&str>, home: Option<&str>) -> PathBuf {
        config_dir_in(
            xdg_config_home.map(OsString::from),
            home.map(OsString::from),
        )
    }

    #[test]
    fn xdg_config_home_wins_over_home() {
        assert_eq!(
            dir_in(Some("/srv/cfg"), Some("/home/u")),
            Path::new("/srv/cfg/ghost-shell")
        );
    }

    #[test]
    fn an_unset_or_empty_xdg_config_home_falls_back_to_dot_config() {
        for xdg in [None, Some("")] {
            assert_eq!(
                dir_in(xdg, Some("/home/u")),
                Path::new("/home/u/.config/ghost-shell")
            );
        }
    }

    #[test]
    fn without_either_variable_the_dir_is_relative() {
        assert_eq!(dir_in(None, None), Path::new(".config/ghost-shell"));
    }
}
