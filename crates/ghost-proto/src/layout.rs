use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::Direction;

/// The configured layout trees checked against the installed widgets.
///
/// Resolution never fails: a slot that cannot load carries its `error`, and the same message is
/// repeated in `errors`, prefixed with the slot's path, so the config can be fixed from one list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResolvedLayout {
    pub gutter: f32,
    pub margin: f32,
    pub views: BTreeMap<String, ResolvedNode>,
    pub errors: Vec<String>,
}

/// Untagged like [`crate::LayoutNode`], so the shell tells the variants apart by their keys.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResolvedNode {
    Slot(ResolvedSlot),
    Split {
        direction: Direction,
        size: Option<f32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        gutter: Option<f32>,
        children: Vec<ResolvedNode>,
    },
    Tabs {
        size: Option<f32>,
        default: usize,
        tabs: Vec<ResolvedTab>,
    },
}

/// `source` is set whenever the widget's manifest exists, even alongside an `error`; the shell
/// shows the error card whenever `error` is set.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResolvedSlot {
    pub widget: String,
    pub source: Option<String>,
    pub size: Option<f32>,
    pub settings: Map<String, Value>,
    pub requires: Vec<String>,
    pub animates: bool,
    pub error: Option<String>,
}

/// Flattened, so a tab entry carries the same keys as a slot plus its `title`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResolvedTab {
    pub title: String,
    #[serde(flatten)]
    pub slot: ResolvedSlot,
}
