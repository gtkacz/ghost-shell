use ghost_proto::{Direction, LayoutNode, ResolvedConfig, TabSpec};
use serde_json::{Map, json};

fn sample_tree() -> LayoutNode {
    LayoutNode::Split {
        direction: Direction::Row,
        size: None,
        children: vec![
            LayoutNode::Slot {
                widget: "memory".into(),
                size: Some(250.0),
                settings: Map::from_iter([("rows".to_owned(), json!(6))]),
            },
            LayoutNode::Tabs {
                tabs: vec![TabSpec {
                    widget: "processor".into(),
                    title: Some("CPU".into()),
                    settings: Map::new(),
                }],
                size: None,
                default: 0,
            },
        ],
    }
}

#[test]
fn a_node_serializes_to_the_untagged_shape_the_shell_reads() {
    assert_eq!(
        serde_json::to_value(sample_tree()).unwrap(),
        json!({
            "direction": "row",
            "size": null,
            "children": [
                {"widget": "memory", "size": 250.0, "settings": {"rows": 6}},
                {
                    "tabs": [{"widget": "processor", "title": "CPU", "settings": {}}],
                    "size": null,
                    "default": 0,
                },
            ],
        })
    );
}

#[test]
fn a_node_reads_back_from_its_own_json() {
    let tree = sample_tree();

    let json = serde_json::to_value(&tree).unwrap();

    assert_eq!(serde_json::from_value::<LayoutNode>(json).unwrap(), tree);
}

#[test]
fn the_default_layout_reads_back_from_its_own_json() {
    let layout = ResolvedConfig::default().layout;

    let json = serde_json::to_value(&layout).unwrap();

    assert_eq!(serde_json::from_value(json).ok(), Some(layout));
}

#[test]
fn a_node_read_outside_a_config_still_names_the_key_at_fault() {
    let error = serde_json::from_value::<LayoutNode>(json!({"widget": "memory", "sise": 3}))
        .unwrap_err()
        .to_string();

    assert!(
        error.starts_with("layout: unknown field \"sise\""),
        "{error}"
    );
}
