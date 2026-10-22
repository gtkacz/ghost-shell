//! Reads a layout tree so that every typo yields one error that names the node it is in.
//!
//! A derived untagged enum tries each variant in turn and, when none fits, can only say that the
//! data matched no variant, at the first line of the view. Reading the keys by hand lets the error
//! carry the node's path and the key at fault.

use std::collections::BTreeMap;
use std::fmt;

use serde::Deserialize;
use serde::de::{DeserializeSeed, Deserializer, Error, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Value};

use crate::config::{Direction, LayoutNode, TabSpec};

const VIEWS_PATH: &str = "layout.views";
// What a node reads as when it is deserialized on its own, outside any config.
const ROOT_PATH: &str = "layout";

const WIDGET: &str = "widget";
const SIZE: &str = "size";
const GUTTER: &str = "gutter";
const SETTINGS: &str = "settings";
const DIRECTION: &str = "direction";
const CHILDREN: &str = "children";
const TABS: &str = "tabs";
const DEFAULT: &str = "default";

const NODE_KEYS: [&str; 8] = [
    WIDGET, SIZE, GUTTER, SETTINGS, DIRECTION, CHILDREN, TABS, DEFAULT,
];
const KIND_KEYS: [&str; 3] = [WIDGET, CHILDREN, TABS];
const SLOT_KEYS: [&str; 3] = [WIDGET, SIZE, SETTINGS];
const SPLIT_KEYS: [&str; 4] = [CHILDREN, DIRECTION, SIZE, GUTTER];
const TABS_KEYS: [&str; 3] = [TABS, SIZE, DEFAULT];

impl<'de> Deserialize<'de> for LayoutNode {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Node::at(ROOT_PATH).deserialize(deserializer)
    }
}

/// Reads `layout.views`, naming each view in the errors of the nodes below it.
pub(crate) fn views<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<BTreeMap<String, LayoutNode>, D::Error> {
    deserializer.deserialize_map(Views)
}

struct Views;

impl<'de> Visitor<'de> for Views {
    type Value = BTreeMap<String, LayoutNode>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a table of views")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let mut views = BTreeMap::new();
        while let Some(name) = map.next_key::<String>()? {
            let node = map.next_value_seed(Node::at(&format!("{VIEWS_PATH}.{name}")))?;
            views.insert(name, node);
        }
        Ok(views)
    }
}

/// A node, read at `path`.
struct Node {
    path: String,
}

impl Node {
    fn at(path: &str) -> Self {
        Self {
            path: path.to_owned(),
        }
    }
}

impl<'de> DeserializeSeed<'de> for Node {
    type Value = LayoutNode;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_map(self)
    }
}

impl<'de> Visitor<'de> for Node {
    type Value = LayoutNode;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a layout node")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let path = &self.path;
        let mut raw = RawNode::default();
        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                WIDGET => raw.widget = Some(map.next_value()?),
                SIZE => raw.size = map.next_value()?,
                GUTTER => raw.gutter = map.next_value()?,
                SETTINGS => raw.settings = Some(map.next_value()?),
                DIRECTION => raw.direction = Some(map.next_value()?),
                CHILDREN => raw.children = Some(map.next_value_seed(Children { path })?),
                TABS => raw.tabs = Some(map.next_value()?),
                DEFAULT => raw.default = Some(map.next_value()?),
                _ => {
                    return Err(A::Error::custom(format!(
                        "{path}: unknown field {key:?}, expected one of {}",
                        NODE_KEYS.join(", ")
                    )));
                }
            }
        }
        raw.classify(path).map_err(A::Error::custom)
    }
}

struct Children<'a> {
    path: &'a str,
}

impl<'de> DeserializeSeed<'de> for Children<'_> {
    type Value = Vec<LayoutNode>;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_seq(self)
    }
}

impl<'de> Visitor<'de> for Children<'_> {
    type Value = Vec<LayoutNode>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("an array of layout nodes")
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
        let mut children = Vec::new();
        // The same path the validation errors use, so one tree reads the same in every message.
        while let Some(child) = seq.next_element_seed(Node::at(&format!(
            "{}.{CHILDREN}[{}]",
            self.path,
            children.len()
        )))? {
            children.push(child);
        }
        Ok(children)
    }
}

/// Every key a node may carry, whichever kind it turns out to be.
#[derive(Default)]
struct RawNode {
    widget: Option<String>,
    size: Option<f32>,
    gutter: Option<f32>,
    settings: Option<Map<String, Value>>,
    direction: Option<Direction>,
    children: Option<Vec<LayoutNode>>,
    tabs: Option<Vec<TabSpec>>,
    default: Option<usize>,
}

impl RawNode {
    fn present_keys(&self) -> Vec<&'static str> {
        [
            (WIDGET, self.widget.is_some()),
            (SIZE, self.size.is_some()),
            (GUTTER, self.gutter.is_some()),
            (SETTINGS, self.settings.is_some()),
            (DIRECTION, self.direction.is_some()),
            (CHILDREN, self.children.is_some()),
            (TABS, self.tabs.is_some()),
            (DEFAULT, self.default.is_some()),
        ]
        .into_iter()
        .filter_map(|(key, present)| present.then_some(key))
        .collect()
    }

    /// The kind is whichever of `widget`, `children` and `tabs` the node has, and the other keys
    /// must belong to that kind.
    fn classify(self, path: &str) -> Result<LayoutNode, String> {
        let present = self.present_keys();
        match (self.widget, self.children, self.tabs) {
            (Some(widget), None, None) => {
                check_keys(path, "a widget slot", &SLOT_KEYS, &present)?;
                Ok(LayoutNode::Slot {
                    widget,
                    size: self.size,
                    settings: self.settings.unwrap_or_default(),
                })
            }
            (None, Some(children), None) => {
                check_keys(path, "a split", &SPLIT_KEYS, &present)?;
                let direction = self
                    .direction
                    .ok_or_else(|| format!("{path}: a split needs a {DIRECTION}"))?;
                Ok(LayoutNode::Split {
                    direction,
                    size: self.size,
                    gutter: self.gutter,
                    children,
                })
            }
            (None, None, Some(tabs)) => {
                check_keys(path, "a tab group", &TABS_KEYS, &present)?;
                Ok(LayoutNode::Tabs {
                    tabs,
                    size: self.size,
                    default: self.default.unwrap_or_default(),
                })
            }
            (None, None, None) => Err(format!(
                "{path}: a layout node needs one of {WIDGET}, {CHILDREN} or {TABS}"
            )),
            _ => {
                let found: Vec<&str> = present
                    .into_iter()
                    .filter(|key| KIND_KEYS.contains(key))
                    .collect();
                Err(format!(
                    "{path}: a layout node takes only one of {WIDGET}, {CHILDREN} or {TABS}, \
                     found {}",
                    found.join(" and ")
                ))
            }
        }
    }
}

fn check_keys(path: &str, kind: &str, allowed: &[&str], present: &[&str]) -> Result<(), String> {
    match present.iter().find(|key| !allowed.contains(key)) {
        Some(stray) => Err(format!(
            "{path}: unknown field {stray:?} for {kind}, expected one of {}",
            allowed.join(", ")
        )),
        None => Ok(()),
    }
}
