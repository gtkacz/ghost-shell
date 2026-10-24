use std::fs;
use std::path::Path;

use ghost_proto::{BarConfig, BarLayout, ClockSeconds, LayoutConfig, LayoutNode, ResolvedLayout};
use ghostd::config::load_str_with_user;
use ghostd::widgets::{discover, resolve};
use serde_json::json;

const BAR_WIDGETS: [&str; 9] = [
    "brand",
    "tasks",
    "ticker",
    "status",
    "tray",
    "view-chip",
    "bar-clock",
    "launcher-tile",
    "agents",
];

fn install(dir: &Path, id: &str, kind: &str) {
    let widget_dir = dir.join(id);
    fs::create_dir_all(&widget_dir).unwrap();
    let manifest = format!(
        "id = {id:?}\nname = {id:?}\nversion = \"1.0.0\"\napi = 1\nkinds = [{kind:?}]\n\
         [settings.style]\ntype = \"string\"\ndefault = \"icons\"\n\
         [settings.parts]\ntype = \"list\"\ndefault = []\n"
    );
    fs::write(widget_dir.join("manifest.toml"), manifest).unwrap();
}

fn resolve_config(toml: &str, layout: BarLayout) -> ResolvedLayout {
    let dir = tempfile::tempdir().unwrap();
    BAR_WIDGETS
        .iter()
        .for_each(|id| install(dir.path(), id, "bar"));
    install(dir.path(), "processor", "panel");
    let (manifests, problems) = discover(&[dir.path().to_path_buf()]);
    assert!(problems.is_empty(), "{problems:?}");
    let config = load_str_with_user(toml, "u").unwrap();
    let bar = BarConfig {
        layout,
        ..config.bar
    };
    resolve(&config.layout, &bar, &manifests)
}

// The default views name widgets this fixture does not install, so each test reads only its own.
fn errors_at<'a>(resolved: &'a ResolvedLayout, prefix: &str) -> Vec<&'a str> {
    resolved
        .errors
        .iter()
        .map(String::as_str)
        .filter(|error| error.starts_with(prefix))
        .collect()
}

fn widgets(slots: &[ghost_proto::ResolvedSlot]) -> Vec<&str> {
    slots.iter().map(|slot| slot.widget.as_str()).collect()
}

#[test]
fn bar_preset_follows_bar_layout() {
    let resolved = resolve_config("", BarLayout::C);

    assert_eq!(widgets(&resolved.bar.left), ["brand", "tasks"]);
    assert_eq!(resolved.bar.left[1].settings["style"], json!("tabs"));
    assert!(resolved.bar.dock.is_empty());
    assert!(errors_at(&resolved, "layout.bar").is_empty());
}

#[test]
fn dock_exists_only_in_layout_b() {
    let b = resolve_config("", BarLayout::B);
    assert_eq!(widgets(&b.bar.dock), ["launcher-tile", "tasks"]);
    assert_eq!(b.bar.dock[1].settings["style"], json!("icons"));

    assert!(resolve_config("", BarLayout::A).bar.dock.is_empty());
}

#[test]
fn user_bar_layout_replaces_the_preset() {
    let resolved = resolve_config(
        "[layout.bar]\nleft = [{ widget = \"bar-clock\" }]\n",
        BarLayout::A,
    );

    assert_eq!(widgets(&resolved.bar.left), ["bar-clock"]);
    assert!(resolved.bar.right.is_empty());
}

#[test]
fn panel_widget_in_the_bar_is_an_error() {
    let resolved = resolve_config(
        "[layout.bar]\nleft = [{ widget = \"processor\" }]\n",
        BarLayout::A,
    );

    let message = "widget \"processor\" can't be placed in the bar";
    assert_eq!(resolved.bar.left[0].error.as_deref(), Some(message));
    assert_eq!(
        errors_at(&resolved, "layout.bar"),
        [format!("layout.bar.left[0]: {message}")]
    );
}

#[test]
fn bar_widget_in_a_view_is_an_error() {
    let resolved = resolve_config("[layout.views.v1]\nwidget = \"tray\"\n", BarLayout::A);

    assert_eq!(
        errors_at(&resolved, "layout.v1:"),
        ["layout.v1: widget \"tray\" can't be placed in a panel slot"]
    );
}

#[test]
fn a_bad_dock_slot_names_its_path() {
    let resolved = resolve_config(
        "[layout]\ndock = [{ widget = \"tasks\" }, { widget = \"processor\" }]\n",
        BarLayout::B,
    );

    assert_eq!(errors_at(&resolved, "layout.dock").len(), 1);
    assert!(errors_at(&resolved, "layout.dock[1]: ").len() == 1);
}

#[test]
fn task_shortcuts_default_to_meta_digits() {
    let tasks = load_str_with_user("", "u").unwrap().shortcuts.tasks;

    assert_eq!(tasks.len(), 9);
    assert_eq!(tasks[0], "Meta+1");
    assert_eq!(tasks[8], "Meta+9");
}

#[test]
fn task_shortcut_clashing_with_monitor_is_rejected() {
    let error = load_str_with_user("[shortcuts]\ntasks = [\"Meta+M\"]\n", "u").unwrap_err();

    assert!(error.message.contains("shortcuts.tasks[0]"), "{error}");
}

#[test]
fn task_shortcuts_sharing_a_key_are_rejected() {
    let error =
        load_str_with_user("[shortcuts]\ntasks = [\"Meta+1\", \"Meta+1\"]\n", "u").unwrap_err();

    assert!(error.message.contains("shortcuts.tasks[1]"), "{error}");
}

#[test]
fn more_than_ten_task_shortcuts_are_rejected() {
    let keys = (0..11)
        .map(|n| format!("\"Meta+Alt+{}\"", (b'a' + n) as char))
        .collect::<Vec<_>>()
        .join(", ");
    let error = load_str_with_user(&format!("[shortcuts]\ntasks = [{keys}]\n"), "u").unwrap_err();

    assert!(error.message.contains("shortcuts.tasks"), "{error}");
}

#[test]
fn launcher_style_other_than_drop_is_rejected() {
    let error = load_str_with_user("[launcher]\nstyle = \"panes\"\n", "u").unwrap_err();

    assert!(error.message.contains("launcher.style"), "{error}");
}

#[test]
fn unknown_bar_layout_key_names_its_path() {
    let error = load_str_with_user("[layout.bar]\nmiddle = []\n", "u").unwrap_err();

    assert!(error.message.contains("layout.bar"), "{error}");
    assert!(error.message.contains("middle"), "{error}");
}

#[test]
fn default_layout_has_no_bar_override() {
    let layout = LayoutConfig::default();

    assert!(layout.bar.is_none() && layout.dock.is_none());
    assert!(matches!(layout.views["v1"], LayoutNode::Split { .. }));
}

#[test]
fn bar_clock_shows_seconds_only_when_covered_by_default() {
    let config = load_str_with_user("", "u").unwrap();

    assert_eq!(config.bar.clock.seconds, ClockSeconds::Covered);
}

#[test]
fn bar_clock_seconds_always_is_accepted() {
    let config = load_str_with_user("[bar.clock]\nseconds = \"always\"\n", "u").unwrap();

    assert_eq!(config.bar.clock.seconds, ClockSeconds::Always);
}

#[test]
fn bar_clock_seconds_other_than_covered_or_always_names_its_path() {
    let error = load_str_with_user("[bar.clock]\nseconds = \"sometimes\"\n", "u").unwrap_err();

    assert!(error.message.contains("bar.clock.seconds"), "{error}");
}

#[test]
fn agents_is_in_no_preset_but_a_user_bar_can_place_it() {
    for layout in [BarLayout::A, BarLayout::B, BarLayout::C] {
        assert!(!widgets(&resolve_config("", layout).bar.right).contains(&"agents"));
    }
    let resolved = resolve_config(
        "[layout.bar]\nright = [{ widget = \"status\" }, { widget = \"agents\" }, { widget = \"tray\" }]\n",
        BarLayout::A,
    );
    assert_eq!(widgets(&resolved.bar.right), ["status", "agents", "tray"]);
    assert!(errors_at(&resolved, "layout.bar").is_empty());
}
