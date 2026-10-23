use std::collections::BTreeMap;

use ghost_proto::{
    BarConfig, BarLayout, BarSlotSpec, LayoutConfig, LayoutNode, ResolvedBar, ResolvedLayout,
    ResolvedNode, ResolvedSlot, ResolvedTab, TabSpec, bar_preset, dock_preset,
};
use serde_json::{Map, Value};

use super::manifest::{Kind, Manifest, Setting, SettingType, WIDGET_API};
use crate::config::is_hex_colour;

/// Checks every slot of every view against `manifests`.
///
/// Never fails: each problem lands on the slot that caused it and in [`ResolvedLayout::errors`],
/// so one bad slot does not stop the rest of the dashboard from loading.
pub fn resolve(
    layout: &LayoutConfig,
    bar: &BarConfig,
    manifests: &BTreeMap<String, Manifest>,
) -> ResolvedLayout {
    let mut resolver = Resolver {
        manifests,
        errors: Vec::new(),
    };
    let views = layout
        .views
        .iter()
        .map(|(name, node)| (name.clone(), resolver.node(&format!("layout.{name}"), node)))
        .collect();
    let spec = layout.bar.clone().unwrap_or_else(|| bar_preset(bar.layout));
    let dock = match bar.layout {
        BarLayout::B => layout.dock.clone().unwrap_or_else(dock_preset),
        BarLayout::A | BarLayout::C => Vec::new(),
    };
    let bar = ResolvedBar {
        left: resolver.bar_slots("layout.bar.left", &spec.left),
        centre: resolver.bar_slots("layout.bar.centre", &spec.centre),
        right: resolver.bar_slots("layout.bar.right", &spec.right),
        dock: resolver.bar_slots("layout.dock", &dock),
    };
    ResolvedLayout {
        gutter: layout.gutter,
        margin: layout.margin,
        views,
        bar,
        errors: resolver.errors,
    }
}

struct Resolver<'a> {
    manifests: &'a BTreeMap<String, Manifest>,
    errors: Vec<String>,
}

impl Resolver<'_> {
    fn node(&mut self, path: &str, node: &LayoutNode) -> ResolvedNode {
        match node {
            LayoutNode::Slot {
                widget,
                size,
                settings,
            } => ResolvedNode::Slot(self.slot(path, Kind::Panel, widget, *size, settings)),
            LayoutNode::Split {
                direction,
                size,
                gutter,
                children,
            } => ResolvedNode::Split {
                direction: *direction,
                size: *size,
                gutter: *gutter,
                children: children
                    .iter()
                    .enumerate()
                    .map(|(index, child)| self.node(&format!("{path}[{index}]"), child))
                    .collect(),
            },
            LayoutNode::Tabs {
                tabs,
                size,
                default,
            } => ResolvedNode::Tabs {
                size: *size,
                default: *default,
                tabs: tabs
                    .iter()
                    .enumerate()
                    .map(|(index, tab)| self.tab(&format!("{path}[{index}]"), tab))
                    .collect(),
            },
        }
    }

    fn bar_slots(&mut self, path: &str, slots: &[BarSlotSpec]) -> Vec<ResolvedSlot> {
        slots
            .iter()
            .enumerate()
            .map(|(index, spec)| {
                self.slot(
                    &format!("{path}[{index}]"),
                    Kind::Bar,
                    &spec.widget,
                    None,
                    &spec.settings,
                )
            })
            .collect()
    }

    fn tab(&mut self, path: &str, tab: &TabSpec) -> ResolvedTab {
        let slot = self.slot(path, Kind::Panel, &tab.widget, None, &tab.settings);
        let title = tab
            .title
            .clone()
            .unwrap_or_else(|| self.default_title(&tab.widget));
        ResolvedTab { title, slot }
    }

    fn default_title(&self, widget: &str) -> String {
        self.manifests
            .get(widget)
            .map_or(widget, |manifest| manifest.name.as_str())
            .to_uppercase()
    }

    fn slot(
        &mut self,
        path: &str,
        kind: Kind,
        widget: &str,
        size: Option<f32>,
        overrides: &Map<String, Value>,
    ) -> ResolvedSlot {
        let slot = match self.manifests.get(widget) {
            Some(manifest) => installed_slot(manifest, kind, size, overrides),
            None => ResolvedSlot {
                widget: widget.to_owned(),
                source: None,
                size,
                settings: Map::new(),
                requires: Vec::new(),
                animates: false,
                error: Some(format!("no widget named {widget:?} is installed")),
            },
        };
        if let Some(error) = &slot.error {
            self.errors.push(format!("{path}: {error}"));
        }
        slot
    }
}

fn installed_slot(
    manifest: &Manifest,
    kind: Kind,
    size: Option<f32>,
    overrides: &Map<String, Value>,
) -> ResolvedSlot {
    let mut settings: Map<String, Value> = manifest
        .settings
        .iter()
        .map(|(key, setting)| (key.clone(), setting.default.clone()))
        .collect();
    let error = check(manifest, kind, overrides).err();
    // Overrides that failed the check stay out, so the slot never carries a value of the wrong type.
    if error.is_none() {
        settings.extend(overrides.clone());
    }
    ResolvedSlot {
        widget: manifest.id.clone(),
        source: Some(manifest.source.clone()),
        size,
        settings,
        requires: manifest.requires.clone(),
        animates: manifest.animates,
        error,
    }
}

/// Reports only the first failure, so a slot never shows more than one error card message.
fn check(manifest: &Manifest, kind: Kind, overrides: &Map<String, Value>) -> Result<(), String> {
    let id = &manifest.id;
    if manifest.api != WIDGET_API {
        return Err(format!(
            "widget {id:?} targets API {}; this ghost-shell supports API {WIDGET_API}",
            manifest.api
        ));
    }
    if !manifest.kinds.contains(&kind) {
        let place = match kind {
            Kind::Panel => "a panel slot",
            Kind::Bar => "the bar",
        };
        return Err(format!("widget {id:?} can't be placed in {place}"));
    }
    overrides
        .iter()
        .try_for_each(|(key, value)| check_setting(manifest, key, value))
}

fn check_setting(manifest: &Manifest, key: &str, value: &Value) -> Result<(), String> {
    let id = &manifest.id;
    let Some(setting) = manifest.settings.get(key) else {
        return Err(format!("widget {id:?} has no setting {key:?}"));
    };
    if accepts(setting, value) {
        return Ok(());
    }
    Err(format!(
        "widget {id:?} setting {key:?}: expected {}, got {value}",
        expectation(setting)
    ))
}

fn accepts(setting: &Setting, value: &Value) -> bool {
    match setting.value_type {
        SettingType::Bool => value.is_boolean(),
        SettingType::Int => value.is_i64() || value.is_u64(),
        SettingType::Float => value.is_number(),
        SettingType::String => value.is_string(),
        SettingType::Color => value.as_str().is_some_and(is_hex_colour),
        SettingType::Enum => value
            .as_str()
            .is_some_and(|choice| setting.values.iter().any(|allowed| allowed == choice)),
        SettingType::List => value.is_array(),
    }
}

fn expectation(setting: &Setting) -> String {
    match setting.value_type {
        SettingType::Bool => "bool".to_owned(),
        SettingType::Int => "int".to_owned(),
        SettingType::Float => "float".to_owned(),
        SettingType::String => "string".to_owned(),
        SettingType::Color => "color".to_owned(),
        SettingType::Enum => format!("one of {}", Value::from(setting.values.clone())),
        SettingType::List => "list".to_owned(),
    }
}
