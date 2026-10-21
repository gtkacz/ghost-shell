use ghost_proto::{LayoutNode, ResolvedConfig};
use ghostd::config::{DEFAULT_TOML, load_str_with_user};

#[test]
fn default_file_equals_builtin_defaults() {
    let mut expected = ResolvedConfig::default();
    expected.greeting.name = "gtkacz".into();
    assert_eq!(
        load_str_with_user(DEFAULT_TOML, "gtkacz").unwrap(),
        expected
    );
}

#[test]
fn defaults_are_machine_neutral() {
    let c = load_str_with_user("", "u").unwrap();
    assert_eq!(c.display.model, "");
    assert!(c.bar.pinned.is_empty());
    assert_eq!(c.location.lat, None);
    assert_eq!(c.bar.brand.text, "GS//01");
    assert!(c.bar.brand.enabled);
}

#[test]
fn partial_override_merges() {
    let c = load_str_with_user("[appearance]\npanel_fill = 40\n", "u").unwrap();
    assert_eq!(c.appearance.panel_fill, 40);
    assert_eq!(c.appearance.colors.warn, "#e9a84c");
}

#[test]
fn brand_can_be_turned_off_and_renamed() {
    let c = load_str_with_user("[bar.brand]\nenabled = false\ntext = \"TKZ//07\"\n", "u").unwrap();
    assert!(!c.bar.brand.enabled);
    assert_eq!(c.bar.brand.text, "TKZ//07");
}

#[test]
fn bad_colour_names_the_key() {
    assert_eq!(
        load_str_with_user("[appearance.colors]\nwarn = \"orange\"\n", "u")
            .unwrap_err()
            .message,
        "appearance.colors.warn: expected #rrggbb, got \"orange\""
    );
}

#[test]
fn warn_below_crit() {
    assert_eq!(
        load_str_with_user("[thresholds]\nload_warn = 95\n", "u")
            .unwrap_err()
            .message,
        "thresholds: load_warn (95) must be below load_crit (90)"
    );
}

#[test]
fn battery_order_is_reversed() {
    assert_eq!(
        load_str_with_user("[thresholds]\nbattery_crit = 30\n", "u")
            .unwrap_err()
            .message,
        "thresholds: battery_crit (30) must be below battery_warn (20)"
    );
}

#[test]
fn fill_out_of_range() {
    assert_eq!(
        load_str_with_user("[appearance]\npanel_fill = 120\n", "u")
            .unwrap_err()
            .message,
        "appearance.panel_fill: must be 0..=100, got 120"
    );
}

#[test]
fn unknown_key_rejected() {
    assert!(
        load_str_with_user("[appearance]\nglow = true\n", "u")
            .unwrap_err()
            .message
            .contains("unknown field `glow`")
    );
}

#[test]
fn default_v1_layout_shape() {
    let c = load_str_with_user("", "u").unwrap();
    let LayoutNode::Split { children, .. } = &c.layout.views["v1"] else {
        panic!("v1 is not a split");
    };
    assert_eq!(children.len(), 3);
}

#[test]
fn user_layout_replaces_default_view() {
    let c = load_str_with_user("[layout.views.v1]\nwidget = \"memory\"\n", "u").unwrap();
    assert_eq!(
        c.layout.views["v1"],
        LayoutNode::Slot {
            widget: "memory".into(),
            size: None,
            settings: Default::default(),
        }
    );
}

#[test]
fn views_must_exist() {
    assert_eq!(
        load_str_with_user("[dashboard]\nmain_views = [\"v1\", \"v2\"]\n", "u")
            .unwrap_err()
            .message,
        "dashboard.main_views: view \"v2\" is not defined in layout.views"
    );
}

#[test]
fn extra_view_and_tabs_parse() {
    let toml = r#"
[dashboard]
main_views = ["v1", "v3"]

[layout.views.v3]
tabs = [{ widget = "processor" }, { widget = "memory", title = "RAM" }]
default = 1
"#;
    let c = load_str_with_user(toml, "u").unwrap();
    let LayoutNode::Tabs { tabs, default, .. } = &c.layout.views["v3"] else {
        panic!("v3 is not tabs");
    };
    assert_eq!(tabs.len(), 2);
    assert_eq!(tabs[1].title.as_deref(), Some("RAM"));
    assert_eq!(*default, 1);
    assert!(c.layout.views.contains_key("v1"));
}

#[test]
fn provider_spec_parses() {
    let c = load_str_with_user(
        "[[providers]]\nname = \"inmet\"\nexec = [\"inmet-provider\", \"--area\", \"3550308\"]\n",
        "u",
    )
    .unwrap();
    assert_eq!(c.providers[0].exec[2], "3550308");
    assert!(c.providers[0].restart);
}

fn rejected(toml: &str) -> String {
    load_str_with_user(toml, "u").unwrap_err().message
}

#[test]
fn node_size_must_be_positive_and_finite() {
    for (written, shown) in [("-5", "-5"), ("0", "0"), ("nan", "NaN"), ("inf", "inf")] {
        assert_eq!(
            rejected(&format!(
                "[layout.views.v1]\nwidget = \"memory\"\nsize = {written}\n"
            )),
            format!("layout.views.v1: size must be a positive number, got {shown}"),
        );
    }
}

#[test]
fn nested_and_tabs_node_sizes_are_checked_with_their_path() {
    let nested = "[layout.views.v3]\ndirection = \"row\"\n\n\
                  [[layout.views.v3.children]]\nwidget = \"memory\"\n\n\
                  [[layout.views.v3.children]]\ntabs = [{ widget = \"processor\" }]\nsize = -1\n";

    assert_eq!(
        rejected(nested),
        "layout.views.v3.children[1]: size must be a positive number, got -1"
    );
}

#[test]
fn gutter_and_margin_must_not_be_negative_or_non_finite() {
    for key in ["gutter", "margin"] {
        for (written, shown) in [("-1", "-1"), ("nan", "NaN"), ("-inf", "-inf")] {
            assert_eq!(
                rejected(&format!("[layout]\n{key} = {written}\n")),
                format!("layout.{key}: must be a non-negative number, got {shown}"),
            );
        }
    }
    assert!(load_str_with_user("[layout]\ngutter = 0\nmargin = 0\n", "u").is_ok());
}

#[test]
fn main_views_must_not_repeat_a_view() {
    assert_eq!(
        rejected("[dashboard]\nmain_views = [\"v1\", \"v1\"]\n"),
        "dashboard.main_views[1]: duplicate view \"v1\""
    );
}

#[test]
fn provider_exec_entries_must_not_be_empty() {
    assert_eq!(
        rejected("[[providers]]\nname = \"inmet\"\nexec = [\"inmet-provider\", \"\"]\n"),
        "providers[0].exec[1]: must not be empty"
    );
    assert_eq!(
        rejected("[[providers]]\nname = \"inmet\"\nexec = [\"\"]\n"),
        "providers[0].exec[0]: must not be empty"
    );
}
