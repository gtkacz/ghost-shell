use std::collections::BTreeMap;
use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::Path;
use std::{env, fs};

use ghost_proto::{
    Direction, LayoutConfig, LayoutNode, ResolvedLayout, ResolvedNode, ResolvedSlot, ServerMsg,
    TabSpec,
};
use ghostd::config::default_path;
use ghostd::widgets::{default_dirs, discover, resolve};
use serde_json::{Value, json};

const VIEW: &str = "v1";
const MANIFEST_FILE: &str = "manifest.toml";
const HISTORY_SETTING: &str = "[settings.history]\ntype = \"bool\"\ndefault = true\n";

struct Spec<'a> {
    id: &'a str,
    name: &'a str,
    version: &'a str,
    api: u32,
    kinds: &'a str,
    extra: &'a str,
}

impl Default for Spec<'_> {
    fn default() -> Self {
        Self {
            id: "widget",
            name: "Widget",
            version: "1.0.0",
            api: 1,
            kinds: r#"["panel"]"#,
            extra: "",
        }
    }
}

impl Spec<'_> {
    fn toml(&self) -> String {
        format!(
            "id = {:?}\nname = {:?}\nversion = {:?}\napi = {}\nkinds = {}\n{}",
            self.id, self.name, self.version, self.api, self.kinds, self.extra
        )
    }
}

fn install(dir: &Path, folder: &str, manifest: &str) {
    let widget_dir = dir.join(folder);
    fs::create_dir_all(&widget_dir).unwrap();
    fs::write(widget_dir.join(MANIFEST_FILE), manifest).unwrap();
}

fn named_view(name: &str, node: LayoutNode) -> LayoutConfig {
    LayoutConfig {
        views: BTreeMap::from([(name.to_owned(), node)]),
        ..LayoutConfig::default()
    }
}

fn resolve_named(specs: &[Spec], name: &str, node: LayoutNode) -> ResolvedLayout {
    let dir = tempfile::tempdir().unwrap();
    for spec in specs {
        install(dir.path(), spec.id, &spec.toml());
    }
    let (manifests, problems) = discover(&[dir.path().to_path_buf()]);
    assert!(problems.is_empty(), "{problems:?}");
    resolve(&named_view(name, node), &manifests)
}

fn resolve_with(specs: &[Spec], node: LayoutNode) -> ResolvedLayout {
    resolve_named(specs, VIEW, node)
}

fn slot(widget: &str) -> LayoutNode {
    slot_with(widget, json!({}))
}

fn slot_with(widget: &str, settings: Value) -> LayoutNode {
    let Value::Object(settings) = settings else {
        panic!("slot settings must be a JSON object");
    };
    LayoutNode::Slot {
        widget: widget.to_owned(),
        size: None,
        settings,
    }
}

fn split(direction: Direction, children: Vec<LayoutNode>) -> LayoutNode {
    LayoutNode::Split {
        direction,
        size: None,
        children,
    }
}

fn tab(widget: &str, title: Option<&str>) -> TabSpec {
    TabSpec {
        widget: widget.to_owned(),
        title: title.map(str::to_owned),
        settings: serde_json::Map::new(),
    }
}

fn tabs(tabs: Vec<TabSpec>) -> LayoutNode {
    LayoutNode::Tabs {
        tabs,
        size: None,
        default: 0,
    }
}

fn as_slot(node: &ResolvedNode) -> &ResolvedSlot {
    let ResolvedNode::Slot(slot) = node else {
        panic!("expected a slot, got {node:?}");
    };
    slot
}

fn children(node: &ResolvedNode) -> &[ResolvedNode] {
    let ResolvedNode::Split { children, .. } = node else {
        panic!("expected a split, got {node:?}");
    };
    children
}

fn view(layout: &ResolvedLayout) -> &ResolvedNode {
    &layout.views[VIEW]
}

#[test]
fn resolves_known_widget_with_defaults() {
    let layout = resolve_with(
        &[Spec {
            id: "graphics",
            extra: concat!(
                "requires = [\"gpu\"]\nanimates = true\n",
                "[settings.history]\ntype = \"bool\"\ndefault = true\n"
            ),
            ..Spec::default()
        }],
        slot("graphics"),
    );

    let resolved = as_slot(view(&layout));
    let source = resolved.source.as_deref().unwrap();
    assert!(source.starts_with("file:///"), "{source}");
    assert!(source.ends_with("graphics/Widget.qml"), "{source}");
    assert_eq!(resolved.settings["history"], true);
    assert_eq!(resolved.requires, ["gpu"]);
    assert!(resolved.animates);
    assert_eq!(resolved.error, None);
    assert!(layout.errors.is_empty());
}

#[test]
fn unknown_widget_is_a_slot_error_not_a_failure() {
    let layout = resolve_with(
        &[Spec {
            id: "graphics",
            ..Spec::default()
        }],
        split(Direction::Row, vec![slot("nope"), slot("graphics")]),
    );

    let row = children(view(&layout));
    assert_eq!(
        as_slot(&row[0]).error.as_deref(),
        Some(r#"no widget named "nope" is installed"#)
    );
    assert_eq!(as_slot(&row[0]).source, None);
    assert_eq!(as_slot(&row[1]).error, None);
}

#[test]
fn api_mismatch_is_reported() {
    let layout = resolve_with(
        &[Spec {
            id: "x",
            api: 2,
            ..Spec::default()
        }],
        slot("x"),
    );

    let resolved = as_slot(view(&layout));
    assert_eq!(
        resolved.error.as_deref(),
        Some(r#"widget "x" targets API 2; this ghost-shell supports API 1"#)
    );
    assert!(resolved.source.is_some());
}

#[test]
fn bad_setting_key_and_type() {
    let layout = resolve_with(
        &[Spec {
            id: "graphics",
            extra: HISTORY_SETTING,
            ..Spec::default()
        }],
        split(
            Direction::Row,
            vec![
                slot_with("graphics", json!({ "foo": 1 })),
                slot_with("graphics", json!({ "history": "yes" })),
            ],
        ),
    );

    let row = children(view(&layout));
    assert_eq!(
        as_slot(&row[0]).error.as_deref(),
        Some(r#"widget "graphics" has no setting "foo""#)
    );
    assert_eq!(
        as_slot(&row[1]).error.as_deref(),
        Some(r#"widget "graphics" setting "history": expected bool, got "yes""#)
    );
}

#[test]
fn errors_carry_layout_path() {
    let layout = resolve_with(
        &[Spec {
            id: "graphics",
            ..Spec::default()
        }],
        split(
            Direction::Row,
            vec![split(
                Direction::Column,
                vec![slot("graphics"), slot("nope")],
            )],
        ),
    );

    assert_eq!(
        layout.errors,
        [r#"layout.v1[0][1]: no widget named "nope" is installed"#]
    );
}

#[test]
fn user_dir_overrides_builtin() {
    let root = tempfile::tempdir().unwrap();
    let builtin = root.path().join("builtin");
    let user = root.path().join("user");
    let graphics = Spec {
        id: "graphics",
        ..Spec::default()
    }
    .toml();
    install(&builtin, "graphics", &graphics);
    install(&user, "graphics", &graphics);

    let (manifests, problems) = discover(&[builtin, user.clone()]);
    let layout = resolve(&named_view(VIEW, slot("graphics")), &manifests);

    assert!(problems.is_empty(), "{problems:?}");
    assert_eq!(
        as_slot(view(&layout)).source,
        Some(format!("file://{}/graphics/Widget.qml", user.display()))
    );
}

fn discovered_source(widget_dir: &Path) -> String {
    let (manifests, problems) = discover(&[widget_dir.to_path_buf()]);
    assert!(problems.is_empty(), "{problems:?}");
    manifests["graphics"].source.clone()
}

#[test]
fn source_percent_encodes_the_characters_a_url_path_cannot_carry() {
    let root = tempfile::tempdir().unwrap();
    let widget_dir = root.path().join("my widgets #1 100% é?");
    install(
        &widget_dir,
        "graphics",
        &Spec {
            id: "graphics",
            ..Spec::default()
        }
        .toml(),
    );

    assert_eq!(
        discovered_source(&widget_dir),
        format!(
            "file://{}/my%20widgets%20%231%20100%25%20%C3%A9%3F/graphics/Widget.qml",
            root.path().display()
        )
    );
}

#[test]
fn source_encodes_path_bytes_that_are_not_utf8_instead_of_replacing_them() {
    let root = tempfile::tempdir().unwrap();
    let widget_dir = root.path().join(OsString::from_vec(vec![b'w', 0xff]));
    install(
        &widget_dir,
        "graphics",
        &Spec {
            id: "graphics",
            ..Spec::default()
        }
        .toml(),
    );

    assert_eq!(
        discovered_source(&widget_dir),
        format!("file://{}/w%FF/graphics/Widget.qml", root.path().display())
    );
}

#[test]
fn tabs_resolve_each_tab() {
    let layout = resolve_named(
        &[Spec {
            id: "processor",
            name: "Processor",
            ..Spec::default()
        }],
        "v3",
        tabs(vec![tab("processor", None), tab("nope", None)]),
    );

    let ResolvedNode::Tabs { tabs, default, .. } = &layout.views["v3"] else {
        panic!("expected tabs");
    };
    assert_eq!(*default, 0);
    assert_eq!(tabs[0].title, "PROCESSOR");
    assert_eq!(tabs[0].slot.error, None);
    assert!(tabs[0].slot.source.is_some());
    assert_eq!(tabs[1].title, "NOPE");
    assert_eq!(
        tabs[1].slot.error.as_deref(),
        Some(r#"no widget named "nope" is installed"#)
    );
    assert_eq!(
        layout.errors,
        [r#"layout.v3[1]: no widget named "nope" is installed"#]
    );
}

#[test]
fn bar_only_widget_rejected_in_panel_slot() {
    let layout = resolve_with(
        &[Spec {
            id: "x",
            kinds: r#"["bar"]"#,
            ..Spec::default()
        }],
        slot("x"),
    );

    assert_eq!(
        as_slot(view(&layout)).error.as_deref(),
        Some(r#"widget "x" can't be placed in a panel slot"#)
    );
}

struct SettingCase {
    value_type: &'static str,
    default: &'static str,
    values: &'static str,
    accepted: &'static str,
    rejected: &'static str,
    message: &'static str,
}

const SETTING_CASES: [SettingCase; 7] = [
    SettingCase {
        value_type: "bool",
        default: "true",
        values: "",
        accepted: "false",
        rejected: r#""yes""#,
        message: r#"expected bool, got "yes""#,
    },
    SettingCase {
        value_type: "int",
        default: "1",
        values: "",
        accepted: "7",
        rejected: "1.5",
        message: "expected int, got 1.5",
    },
    SettingCase {
        value_type: "float",
        default: "1.0",
        values: "",
        accepted: "2",
        rejected: r#""2""#,
        message: r#"expected float, got "2""#,
    },
    SettingCase {
        value_type: "string",
        default: r#""a""#,
        values: "",
        accepted: r#""b""#,
        rejected: "1",
        message: "expected string, got 1",
    },
    SettingCase {
        value_type: "color",
        default: r##""#000000""##,
        values: "",
        accepted: r##""#a3d4cc""##,
        rejected: r#""red""#,
        message: r#"expected color, got "red""#,
    },
    SettingCase {
        value_type: "enum",
        default: r#""a""#,
        values: r#"values = ["a", "b"]"#,
        accepted: r#""b""#,
        rejected: r#""c""#,
        message: r#"expected one of ["a","b"], got "c""#,
    },
    SettingCase {
        value_type: "list",
        default: "[]",
        values: "",
        accepted: "[1, 2]",
        rejected: r#""x""#,
        message: r#"expected list, got "x""#,
    },
];

#[test]
fn setting_values_must_match_their_declared_type() {
    for case in SETTING_CASES {
        let extra = format!(
            "[settings.k]\ntype = {:?}\ndefault = {}\n{}\n",
            case.value_type, case.default, case.values
        );
        let spec = || Spec {
            id: "w",
            extra: &extra,
            ..Spec::default()
        };

        let accepted: Value = serde_json::from_str(case.accepted).unwrap();
        let layout = resolve_with(&[spec()], slot_with("w", json!({ "k": accepted.clone() })));
        let resolved = as_slot(view(&layout));
        assert_eq!(resolved.error, None, "{}", case.value_type);
        assert_eq!(resolved.settings["k"], accepted, "{}", case.value_type);

        let rejected: Value = serde_json::from_str(case.rejected).unwrap();
        let layout = resolve_with(&[spec()], slot_with("w", json!({ "k": rejected })));
        assert_eq!(
            as_slot(view(&layout)).error,
            Some(format!(r#"widget "w" setting "k": {}"#, case.message)),
            "{}",
            case.value_type
        );
    }
}

#[test]
fn tab_title_prefers_the_override() {
    let layout = resolve_with(
        &[Spec {
            id: "mem",
            name: "Memory",
            ..Spec::default()
        }],
        tabs(vec![tab("mem", Some("RAM")), tab("mem", None)]),
    );

    let ResolvedNode::Tabs { tabs, .. } = view(&layout) else {
        panic!("expected tabs");
    };
    assert_eq!(tabs[0].title, "RAM");
    assert_eq!(tabs[1].title, "MEMORY");
}

#[test]
fn resolved_layout_serializes_flat_tabs_and_round_trips() {
    let layout = resolve_with(
        &[Spec {
            id: "processor",
            name: "Processor",
            ..Spec::default()
        }],
        split(
            Direction::Row,
            vec![
                slot("processor"),
                tabs(vec![tab("processor", None), tab("nope", None)]),
            ],
        ),
    );

    let json = serde_json::to_value(&layout).unwrap();
    let tab_entry = &json["views"][VIEW]["children"][1]["tabs"][0];
    assert_eq!(tab_entry["title"], "PROCESSOR");
    assert_eq!(tab_entry["widget"], "processor");
    assert_eq!(json["views"][VIEW]["children"][1]["default"], 0);
    assert_eq!(json["views"][VIEW]["direction"], "row");
    assert_eq!(
        serde_json::from_value::<ResolvedLayout>(json).unwrap(),
        layout
    );

    assert_eq!(
        layout.errors,
        [r#"layout.v1[1][1]: no widget named "nope" is installed"#]
    );

    let message = serde_json::to_value(ServerMsg::Layout { data: layout }).unwrap();
    assert_eq!(message["t"], "layout");
}

#[test]
fn invalid_manifests_are_skipped_with_a_problem_naming_the_file() {
    let root = tempfile::tempdir().unwrap();
    let broken = [
        (
            "bad-id",
            Spec {
                id: "Bad_Id",
                ..Spec::default()
            }
            .toml(),
        ),
        (
            "short-version",
            Spec {
                version: "1.0",
                ..Spec::default()
            }
            .toml(),
        ),
        (
            "leading-zero-version",
            Spec {
                version: "01.0.0",
                ..Spec::default()
            }
            .toml(),
        ),
        (
            "unknown-key",
            Spec {
                extra: "flavour = \"mint\"\n",
                ..Spec::default()
            }
            .toml(),
        ),
        (
            "bad-topic",
            Spec {
                extra: "requires = [\"Not A Topic\"]\n",
                ..Spec::default()
            }
            .toml(),
        ),
        ("not-toml", "id = ".to_owned()),
    ];
    for (folder, text) in &broken {
        install(root.path(), folder, text);
    }
    let prerelease = Spec {
        id: "ok",
        version: "1.2.3-beta.1+build.5",
        ..Spec::default()
    };
    install(root.path(), "ok", &prerelease.toml());

    let (manifests, problems) = discover(&[root.path().to_path_buf()]);

    assert_eq!(manifests.len(), 1);
    assert!(manifests.contains_key("ok"));
    assert_eq!(problems.len(), broken.len(), "{problems:#?}");
    for (folder, _) in &broken {
        let manifest_path = root.path().join(folder).join(MANIFEST_FILE);
        let prefix = manifest_path.display().to_string();
        assert!(
            problems.iter().any(|problem| problem.starts_with(&prefix)),
            "no problem names {prefix}: {problems:#?}"
        );
    }
}

#[test]
fn missing_dirs_and_folders_without_a_manifest_are_skipped_silently() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("notes")).unwrap();
    fs::write(root.path().join("stray.txt"), "x").unwrap();

    let (manifests, problems) = discover(&[root.path().join("absent"), root.path().to_path_buf()]);

    assert!(manifests.is_empty());
    assert!(problems.is_empty(), "{problems:?}");
}

fn assert_problem_names(problems: &[String], manifest_path: &Path) {
    let prefix = manifest_path.display().to_string();
    assert!(
        problems.iter().any(|problem| problem.starts_with(&prefix)),
        "no problem names {prefix}: {problems:#?}"
    );
}

#[test]
fn a_manifest_that_cannot_be_read_is_reported_not_skipped() {
    let root = tempfile::tempdir().unwrap();
    let valid = Spec {
        id: "ok",
        ..Spec::default()
    }
    .toml();
    install(root.path(), "ok", &valid);
    let manifest_of = |folder: &str| root.path().join(folder).join(MANIFEST_FILE);

    fs::create_dir_all(manifest_of("dangling").parent().unwrap()).unwrap();
    symlink("absent.toml", manifest_of("dangling")).unwrap();
    fs::create_dir_all(manifest_of("directory")).unwrap();
    fs::create_dir_all(manifest_of("loop").parent().unwrap()).unwrap();
    symlink(MANIFEST_FILE, manifest_of("loop")).unwrap();

    let (manifests, problems) = discover(&[root.path().to_path_buf()]);

    assert_eq!(manifests.keys().collect::<Vec<_>>(), ["ok"]);
    assert_eq!(problems.len(), 3, "{problems:#?}");
    for folder in ["dangling", "directory", "loop"] {
        assert_problem_names(&problems, &manifest_of(folder));
    }
}

#[test]
fn a_widget_folder_that_is_a_dangling_symlink_is_reported_not_skipped() {
    let root = tempfile::tempdir().unwrap();
    symlink("absent-folder", root.path().join("ghost")).unwrap();
    // A folder that is a working link to a folder without a manifest is no widget, and no problem.
    fs::create_dir(root.path().join("notes")).unwrap();
    symlink("notes", root.path().join("linked-notes")).unwrap();

    let (manifests, problems) = discover(&[root.path().to_path_buf()]);

    assert!(manifests.is_empty());
    assert_eq!(problems.len(), 1, "{problems:#?}");
    assert_problem_names(&problems, &root.path().join("ghost").join(MANIFEST_FILE));
}

#[test]
fn a_widget_folder_without_permission_is_reported_not_skipped() {
    let root = tempfile::tempdir().unwrap();
    install(root.path(), "locked", &Spec::default().toml());
    let locked = root.path().join("locked");
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
    // Permission bits do not bind root, so there the folder stays readable and nothing is tested.
    let binds_us = fs::read_dir(&locked).is_err();

    let (manifests, problems) = discover(&[root.path().to_path_buf()]);

    // Restored before the assertions so a failure does not leave a directory the cleanup cannot enter.
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
    if !binds_us {
        return;
    }
    assert!(manifests.is_empty());
    assert_eq!(problems.len(), 1, "{problems:#?}");
    assert_problem_names(&problems, &locked.join(MANIFEST_FILE));
}

#[test]
fn two_folders_declaring_one_id_in_the_same_dir_are_reported() {
    let root = tempfile::tempdir().unwrap();
    let graphics = Spec {
        id: "graphics",
        ..Spec::default()
    }
    .toml();
    install(root.path(), "a-first", &graphics);
    install(root.path(), "b-second", &graphics);

    let (manifests, problems) = discover(&[root.path().to_path_buf()]);

    assert_eq!(problems.len(), 1, "{problems:#?}");
    let later = root.path().join("b-second").join(MANIFEST_FILE);
    assert_problem_names(&problems, &later);
    assert!(problems[0].contains(r#"id "graphics""#), "{problems:#?}");
    assert!(
        problems[0].contains(&root.path().join("a-first").display().to_string()),
        "{problems:#?}"
    );
    // The later folder still wins, as it did before the duplicate was reported.
    assert!(
        manifests["graphics"]
            .source
            .ends_with("b-second/Widget.qml")
    );
}

#[test]
fn default_dirs_expand_a_leading_home_in_extra_paths() {
    let home = env::var("HOME").unwrap();

    let dirs = default_dirs(&["~/extra".to_owned(), "/opt/widgets".to_owned()]);

    assert_eq!(dirs.len(), 4);
    assert_eq!(dirs[2], Path::new(&home).join("extra"));
    assert_eq!(dirs[3], Path::new("/opt/widgets"));
}

#[test]
fn the_user_widget_dir_sits_beside_the_config_file() {
    let config_dir = default_path();

    let dirs = default_dirs(&[]);

    assert_eq!(
        dirs[1],
        config_dir.parent().unwrap().join("widgets"),
        "the widgets must live in the folder the user edits their config in"
    );
}
