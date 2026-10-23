use serde::{Deserialize, Deserializer, Serialize, de};
use serde_json::{Map, Value};

use crate::{BarLayout, ResolvedSlot};

const LAYOUT_BAR_PATH: &str = "layout.bar";

/// One widget placed in the bar or dock, like a [`crate::LayoutNode::Slot`] without a size.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BarSlotSpec {
    pub widget: String,
    #[serde(default)]
    pub settings: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct BarLayoutFields {
    left: Vec<BarSlotSpec>,
    centre: Vec<BarSlotSpec>,
    right: Vec<BarSlotSpec>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize)]
pub struct BarLayoutSpec {
    pub left: Vec<BarSlotSpec>,
    pub centre: Vec<BarSlotSpec>,
    pub right: Vec<BarSlotSpec>,
}

// Hand-written so a typo'd key names `layout.bar`, which the derived error cannot.
impl<'de> Deserialize<'de> for BarLayoutSpec {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let fields = BarLayoutFields::deserialize(deserializer)
            .map_err(|error| de::Error::custom(format!("{LAYOUT_BAR_PATH}: {error}")))?;
        Ok(Self {
            left: fields.left,
            centre: fields.centre,
            right: fields.right,
        })
    }
}

/// `dock` is empty outside layout `b`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ResolvedBar {
    pub left: Vec<ResolvedSlot>,
    pub centre: Vec<ResolvedSlot>,
    pub right: Vec<ResolvedSlot>,
    pub dock: Vec<ResolvedSlot>,
}

fn slot(widget: &str) -> BarSlotSpec {
    BarSlotSpec {
        widget: widget.into(),
        settings: Map::new(),
    }
}

fn with(widget: &str, key: &str, value: Value) -> BarSlotSpec {
    BarSlotSpec {
        widget: widget.into(),
        settings: Map::from_iter([(key.to_owned(), value)]),
    }
}

fn tasks(style: &str) -> BarSlotSpec {
    with("tasks", "style", style.into())
}

fn tail(parts: &[&str]) -> Vec<BarSlotSpec> {
    vec![
        with("status", "parts", parts.into()),
        slot("tray"),
        slot("view-chip"),
        slot("bar-clock"),
    ]
}

pub fn bar_preset(layout: BarLayout) -> BarLayoutSpec {
    match layout {
        BarLayout::A => BarLayoutSpec {
            left: vec![slot("brand"), tasks("icons")],
            centre: Vec::new(),
            right: tail(&["cpu", "net", "vpn", "weather"]),
        },
        BarLayout::B => BarLayoutSpec {
            left: vec![slot("brand")],
            centre: vec![slot("ticker")],
            right: tail(&["cpu", "gpu", "mem", "net", "vpn", "weather"]),
        },
        BarLayout::C => BarLayoutSpec {
            left: vec![slot("brand"), tasks("tabs")],
            centre: Vec::new(),
            right: tail(&["weather"]),
        },
    }
}

pub fn dock_preset() -> Vec<BarSlotSpec> {
    vec![slot("launcher-tile"), tasks("icons")]
}
