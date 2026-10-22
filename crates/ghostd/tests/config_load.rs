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
fn alert_chips_default_to_shown() {
    assert!(load_str_with_user("", "u").unwrap().appearance.alert_chips);
}

#[test]
fn alert_chips_can_be_turned_off() {
    let c = load_str_with_user("[appearance]\nalert_chips = false\n", "u").unwrap();
    assert!(!c.appearance.alert_chips);
}

#[test]
fn a_non_bool_alert_chips_points_at_its_line_and_the_report_quotes_the_key() {
    let error = load_str_with_user("[appearance]\nalert_chips = \"no\"\n", "u").unwrap_err();

    assert_eq!(
        error.message,
        "line 2, column 15: invalid type: string \"no\", expected a boolean"
    );
    let detail = error.detail.as_deref().expect("a TOML error has a report");
    assert!(detail.contains("2 | alert_chips = \"no\""));
}

#[test]
fn spectrum_fps_defaults_to_15() {
    assert_eq!(
        load_str_with_user("", "u").unwrap().appearance.spectrum_fps,
        15
    );
}

#[test]
fn spectrum_fps_can_be_30() {
    let c = load_str_with_user("[appearance]\nspectrum_fps = 30\n", "u").unwrap();
    assert_eq!(c.appearance.spectrum_fps, 30);
}

#[test]
fn spectrum_fps_must_be_15_or_30() {
    for fps in [0, 14, 16, 29, 31, 60] {
        assert_eq!(
            rejected(&format!("[appearance]\nspectrum_fps = {fps}\n")),
            format!("appearance.spectrum_fps: must be 15 or 30, got {fps}")
        );
    }
}

const STRIPE_KEYS: [&str; 2] = ["alert_stripe_period_s", "alert_stripe_step_s"];

#[test]
fn alert_stripe_timings_default_to_1_6_and_1_second() {
    let appearance = load_str_with_user("", "u").unwrap().appearance;
    assert_eq!(appearance.alert_stripe_period_s, 1.6);
    assert_eq!(appearance.alert_stripe_step_s, 1.0);
}

#[test]
fn alert_stripe_timings_can_be_set_separately() {
    let c = load_str_with_user("[appearance]\nalert_stripe_period_s = 3.2\n", "u").unwrap();
    assert_eq!(c.appearance.alert_stripe_period_s, 3.2);
    assert_eq!(c.appearance.alert_stripe_step_s, 1.0);

    let c = load_str_with_user("[appearance]\nalert_stripe_step_s = 0.5\n", "u").unwrap();
    assert_eq!(c.appearance.alert_stripe_period_s, 1.6);
    assert_eq!(c.appearance.alert_stripe_step_s, 0.5);
}

#[test]
fn alert_stripe_timings_accept_whole_numbers() {
    let c = load_str_with_user("[appearance]\nalert_stripe_step_s = 2\n", "u").unwrap();
    assert_eq!(c.appearance.alert_stripe_step_s, 2.0);
}

#[test]
fn alert_stripe_timings_must_be_greater_than_zero() {
    for key in STRIPE_KEYS {
        for (written, shown) in [("0", "0"), ("0.0", "0"), ("-1.5", "-1.5"), ("-0.0", "-0")] {
            assert_eq!(
                rejected(&format!("[appearance]\n{key} = {written}\n")),
                format!("appearance.{key}: must be greater than 0, got {shown}"),
            );
        }
    }
}

#[test]
fn alert_stripe_timings_must_be_finite() {
    for key in STRIPE_KEYS {
        for (written, shown) in [("nan", "NaN"), ("inf", "inf"), ("-inf", "-inf")] {
            assert_eq!(
                rejected(&format!("[appearance]\n{key} = {written}\n")),
                format!("appearance.{key}: must be finite, got {shown}"),
            );
        }
    }
}

#[test]
fn a_non_number_alert_stripe_timing_points_at_its_line() {
    let error =
        load_str_with_user("[appearance]\nalert_stripe_step_s = \"fast\"\n", "u").unwrap_err();

    assert_eq!(
        error.message,
        "line 2, column 23: invalid type: string \"fast\", expected f64"
    );
}

const ATTENTION_KEY: &str = "alert_attention_s";

#[test]
fn alert_attention_defaults_to_10_seconds() {
    let appearance = load_str_with_user("", "u").unwrap().appearance;
    assert_eq!(appearance.alert_attention_s, 10.0);
}

#[test]
fn alert_attention_can_be_set_to_minutes_or_fractions() {
    for (written, expected) in [("600", 600.0), ("2.5", 2.5), ("0.25", 0.25)] {
        let c = load_str_with_user(&format!("[appearance]\n{ATTENTION_KEY} = {written}\n"), "u")
            .unwrap();
        assert_eq!(c.appearance.alert_attention_s, expected);
        assert_eq!(c.appearance.alert_stripe_period_s, 1.6);
        assert_eq!(c.appearance.alert_stripe_step_s, 1.0);
    }
}

#[test]
fn alert_attention_must_be_greater_than_zero() {
    for (written, shown) in [("0", "0"), ("0.0", "0"), ("-1.5", "-1.5"), ("-0.0", "-0")] {
        assert_eq!(
            rejected(&format!("[appearance]\n{ATTENTION_KEY} = {written}\n")),
            format!("appearance.{ATTENTION_KEY}: must be greater than 0, got {shown}"),
        );
    }
}

#[test]
fn alert_attention_must_be_finite() {
    for (written, shown) in [("nan", "NaN"), ("inf", "inf"), ("-inf", "-inf")] {
        assert_eq!(
            rejected(&format!("[appearance]\n{ATTENTION_KEY} = {written}\n")),
            format!("appearance.{ATTENTION_KEY}: must be finite, got {shown}"),
        );
    }
}

#[test]
fn a_non_number_alert_attention_points_at_its_line() {
    let error =
        load_str_with_user("[appearance]\nalert_attention_s = \"long\"\n", "u").unwrap_err();

    assert_eq!(
        error.message,
        "line 2, column 21: invalid type: string \"long\", expected f64"
    );
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
fn default_views_and_dashboard() {
    let c = load_str_with_user("", "u").unwrap();
    assert_eq!(c.dashboard.main_views, ["v1", "v2"]);
    assert_eq!(c.dashboard.monitor_view, "monitor");
    assert_eq!(c.dashboard.default_view, "v1");
    assert!(c.layout.views.contains_key("v2") && c.layout.views.contains_key("monitor"));
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
    assert!(c.layout.views.contains_key("v2") && c.layout.views.contains_key("monitor"));
}

#[test]
fn bad_shortcut_names_the_key() {
    assert_eq!(
        load_str_with_user("[shortcuts]\nmonitor = \"Meta+\"\n", "u")
            .unwrap_err()
            .message,
        "shortcuts.monitor: cannot parse \"Meta+\" as a key sequence"
    );
}

#[test]
fn a_bare_key_shortcut_is_rejected() {
    assert_eq!(
        load_str_with_user("[shortcuts]\nswap = \"M\"\n", "u")
            .unwrap_err()
            .message,
        "shortcuts.swap: \"M\" needs Meta, Ctrl or Alt"
    );
}

#[test]
fn shift_alone_does_not_make_a_shortcut() {
    assert_eq!(
        load_str_with_user("[shortcuts]\nswap = \"Shift+M\"\n", "u")
            .unwrap_err()
            .message,
        "shortcuts.swap: \"Shift+M\" needs Meta, Ctrl or Alt"
    );
}

#[test]
fn function_keys_need_no_meta_ctrl_or_alt() {
    let config = load_str_with_user("[shortcuts]\nmonitor = \"Shift+F5\"\n", "u").unwrap();
    assert_eq!(config.shortcuts.monitor, "Shift+F5");
    assert_eq!(config.shortcuts.swap, "Meta+Shift+M");
}

#[test]
fn the_two_shortcuts_must_differ() {
    assert_eq!(
        load_str_with_user("[shortcuts]\nswap = \"meta+m\"\n", "u")
            .unwrap_err()
            .message,
        "shortcuts.swap: \"meta+m\" is the same key as shortcuts.monitor"
    );
}

#[test]
fn default_shortcuts_parse() {
    assert!(load_str_with_user("", "u").is_ok());
}

#[test]
fn views_must_exist() {
    assert_eq!(
        load_str_with_user("[dashboard]\nmain_views = [\"v1\", \"v9\"]\n", "u")
            .unwrap_err()
            .message,
        "dashboard.main_views: view \"v9\" is not defined in layout.views"
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
fn a_negative_gutter_names_its_node() {
    let e = load_str_with_user(
        "[layout.views.v9]\ndirection='row'\ngutter=-1\nchildren=[]",
        "u",
    )
    .unwrap_err();
    assert_eq!(
        e.message,
        "layout.views.v9.gutter: must be a non-negative number, got -1"
    );
}

#[test]
fn a_nested_split_gutter_names_its_path() {
    assert_eq!(
        rejected(
            "[layout.views.v9]\ndirection='row'\nchildren=[{direction='column', gutter=nan, children=[]}]"
        ),
        "layout.views.v9.children[0].gutter: must be a non-negative number, got NaN"
    );
}

#[test]
fn a_zero_gutter_is_valid() {
    let c = load_str_with_user(
        "[layout.views.v9]\ndirection='row'\ngutter=0\nchildren=[]",
        "u",
    )
    .unwrap();
    assert!(matches!(
        c.layout.views["v9"],
        LayoutNode::Split { gutter: Some(g), .. } if g == 0.0
    ));
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

#[test]
fn bad_alert_provider_names_the_key() {
    assert_eq!(
        rejected("[location.alerts]\nprovider = \"In Met\"\narea = \"3550308\"\n"),
        "location.alerts.provider: \"In Met\" must match ^[a-z][a-z0-9-]*$"
    );
}

#[test]
fn alert_area_required_with_provider() {
    assert_eq!(
        rejected("[location.alerts]\nprovider = \"inmet\"\n"),
        "location.alerts.area: must not be empty when a provider is set"
    );
    assert!(
        load_str_with_user(
            "[location.alerts]\nprovider = \"inmet\"\narea = \"3550308\"\n",
            "u"
        )
        .is_ok()
    );
}

#[test]
fn reserved_provider_name_rejected() {
    for name in ["alerts-inmet", "alerts-elsewhere"] {
        assert_eq!(
            rejected(&format!(
                "[[providers]]\nname = {name:?}\nexec = [\"provider\"]\n"
            )),
            format!("providers[0].name: {name:?} is reserved for location.alerts")
        );
    }
}

const NODE_KEYS: &str = "widget, size, gutter, settings, direction, children, tabs, default";

fn row_view_with_children(children: &str) -> String {
    format!("[layout.views.v1]\ndirection = \"row\"\nchildren = {children}\n")
}

#[test]
fn an_unknown_key_in_a_nested_node_names_the_key_and_the_node() {
    let nested = row_view_with_children(
        "[{ widget = \"memory\" }, \
         { direction = \"column\", children = [{ widget = \"cpu\", sise = 300 }] }]",
    );

    assert_eq!(
        rejected(&nested),
        format!(
            "line 3, column 72: layout.views.v1.children[1].children[0]: \
             unknown field \"sise\", expected one of {NODE_KEYS}"
        )
    );
}

#[test]
fn a_misspelled_node_kind_is_an_unknown_key_not_a_missing_one() {
    assert_eq!(
        rejected("[layout.views.v1]\nwidgte = \"memory\"\n"),
        format!(
            "line 1, column 1: layout.views.v1: unknown field \"widgte\", \
             expected one of {NODE_KEYS}"
        )
    );
}

#[test]
fn a_node_with_none_of_widget_children_or_tabs_is_rejected() {
    assert_eq!(
        rejected(&row_view_with_children(
            "[{ widget = \"memory\" }, { size = 300 }]"
        )),
        "line 3, column 36: layout.views.v1.children[1]: \
         a layout node needs one of widget, children or tabs"
    );
}

#[test]
fn a_node_with_two_of_widget_children_or_tabs_is_rejected() {
    assert_eq!(
        rejected("[layout.views.v1]\nwidget = \"memory\"\ntabs = [{ widget = \"cpu\" }]\n"),
        "line 1, column 1: layout.views.v1: \
         a layout node takes only one of widget, children or tabs, found widget and tabs"
    );
}

#[test]
fn a_key_that_belongs_to_another_kind_of_node_is_rejected() {
    assert_eq!(
        rejected("[layout.views.v1]\nwidget = \"memory\"\ndirection = \"row\"\n"),
        "line 1, column 1: layout.views.v1: \
         unknown field \"direction\" for a widget slot, expected one of widget, size, settings"
    );
}

#[test]
fn a_split_without_a_direction_is_rejected() {
    assert_eq!(
        rejected("[layout.views.v1]\nchildren = []\n"),
        "line 1, column 1: layout.views.v1: a split needs a direction"
    );
}

#[test]
fn a_bad_value_in_a_node_points_at_its_line() {
    assert_eq!(
        rejected("[layout.views.v1]\ndirection = \"rows\"\nchildren = []\n"),
        "line 2, column 13: unknown variant `rows`, expected `row` or `column`"
    );
}

#[test]
fn a_parse_error_is_one_line_with_the_parsers_full_report_kept_apart() {
    let error = load_str_with_user("[appearance\npanel_fill = 1\n", "u").unwrap_err();

    assert_eq!(
        error.message,
        "line 1, column 12: unclosed table, expected `]`"
    );
    let detail = error.detail.as_deref().expect("a TOML error has a report");
    assert!(detail.starts_with("TOML parse error at line 1, column 12"));
    assert!(detail.contains("1 | [appearance"));
    assert_eq!(error.to_string(), detail);
}

#[test]
fn a_validation_error_has_no_separate_report() {
    let error = load_str_with_user("[appearance]\npanel_fill = 120\n", "u").unwrap_err();

    assert_eq!(error.detail, None);
    assert_eq!(error.to_string(), error.message);
}

#[test]
fn lat_without_lon_is_rejected() {
    let message = "location: lat and lon must be set together";
    assert_eq!(rejected("[location]\nlat = -23.55\n"), message);
    assert_eq!(rejected("[location]\nlon = -46.63\n"), message);
}

#[test]
fn lat_out_of_range_names_the_key() {
    assert_eq!(
        rejected("[location]\nlat = 95.0\nlon = 0.0\n"),
        "location.lat: must be -90..=90, got 95"
    );
}

#[test]
fn lon_out_of_range_names_the_key() {
    assert_eq!(
        rejected("[location]\nlat = 0.0\nlon = 200.0\n"),
        "location.lon: must be -180..=180, got 200"
    );
}

#[test]
fn translation_is_off_by_default() {
    let c = load_str_with_user("", "u").unwrap();
    assert_eq!((c.i18n.from.as_str(), c.i18n.to.as_str()), ("", ""));
}

#[test]
fn language_tags_with_or_without_a_region_are_accepted() {
    for (from, to) in [("pt-BR", "en-US"), ("pt", "en"), ("pt-BR", "ast")] {
        let c =
            load_str_with_user(&format!("[i18n]\nfrom = {from:?}\nto = {to:?}\n"), "u").unwrap();
        assert_eq!((c.i18n.from.as_str(), c.i18n.to.as_str()), (from, to));
    }
}

#[test]
fn from_and_to_must_be_set_together() {
    let message = "i18n: from and to must be set together";
    assert_eq!(rejected("[i18n]\nfrom = \"pt-BR\"\n"), message);
    assert_eq!(rejected("[i18n]\nto = \"en-US\"\n"), message);
}

#[test]
fn from_and_to_must_differ() {
    assert_eq!(
        rejected("[i18n]\nfrom = \"pt-BR\"\nto = \"pt-BR\"\n"),
        "i18n: from and to must differ"
    );
    assert!(load_str_with_user("[i18n]\nfrom = \"pt-BR\"\nto = \"pt\"\n", "u").is_ok());
}

#[test]
fn a_tag_that_is_not_a_language_tag_names_the_key() {
    for tag in [
        "EN", "en-us", "en_US", "english", "e", "en-USA", "en-", "-US",
    ] {
        assert_eq!(
            rejected(&format!("[i18n]\nfrom = \"pt-BR\"\nto = {tag:?}\n")),
            format!("i18n.to: {tag:?} is not a language tag such as en-US")
        );
    }
    assert_eq!(
        rejected("[i18n]\nfrom = \"PT-br\"\nto = \"en-US\"\n"),
        "i18n.from: \"PT-br\" is not a language tag such as en-US"
    );
}

#[test]
fn inmet_alerts_translate_only_from_pt_br() {
    let inmet = "[location.alerts]\nprovider = 'inmet'\narea = '3550308'\n";
    assert_eq!(
        rejected(&format!("{inmet}[i18n]\nfrom = 'en-US'\nto = 'pt-BR'\n")),
        "i18n.from: \"en-US\" must be \"pt-BR\" while location.alerts.provider is \"inmet\""
    );
    assert!(
        load_str_with_user(
            &format!("{inmet}[i18n]\nfrom = 'pt-BR'\nto = 'en-US'\n"),
            "u"
        )
        .is_ok()
    );
    assert!(load_str_with_user("[i18n]\nfrom = 'en-US'\nto = 'pt-BR'\n", "u").is_ok());
}
