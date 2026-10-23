use std::fs;
use std::path::PathBuf;

use ghost_proto::{ResolvedConfig, calls};
use ghostd::config::edit::{EditError, PinChange, import_plasma_launchers, set_pinned};
use ghostd::feeds::pins;
use ghostd::server::{Call, FeedContext, Publisher};
use serde_json::json;
use std::collections::BTreeSet;
use std::time::Duration;
use tempfile::TempDir;
use tokio::sync::{broadcast, watch};
use tokio::time::{sleep, timeout};

const APPLETSRC_FILE: &str = "plasma-org.kde.plasma.desktop-appletsrc";
const MIMEAPPS_FILE: &str = "mimeapps.list";

const COMMENTED: &str = "# my config\n[appearance]\n# keep me\naccent = \"#112233\"\n\n[bar]\n# pins below\nposition = \"bottom\"\npinned = [\"a\"] # trailing\nicons = \"mono\"\n";

const APPLETSRC: &str = "[Containments][2][Applets][5]\nplugin=org.kde.plasma.icontasks\n\n[Containments][2][Applets][5][Configuration][General]\nlaunchers=applications:org.kde.dolphin.desktop,applications:steam.desktop,preferred://filemanager,file:///usr/share/applications/firefox.desktop\n";

const MIMEAPPS: &str = "[Added Associations]\ninode/directory=other.desktop;\n\n[Default Applications]\ninode/directory=org.kde.dolphin.desktop;thunar.desktop;\n";

struct Sandbox {
    dir: TempDir,
}

impl Sandbox {
    fn new() -> Self {
        Self {
            dir: TempDir::new().unwrap(),
        }
    }

    fn file(&self, name: &str, contents: &str) -> PathBuf {
        let path = self.dir.path().join(name);
        fs::write(&path, contents).unwrap();
        path
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }
}

fn add(app: &str) -> PinChange {
    PinChange::Add(app.to_owned())
}

fn remove(app: &str) -> PinChange {
    PinChange::Remove(app.to_owned())
}

#[test]
fn pin_appends_and_keeps_comments() {
    let sandbox = Sandbox::new();
    let config = sandbox.file("config.toml", COMMENTED);

    assert!(set_pinned(&config, add("b")).unwrap());

    let expected = COMMENTED.replace("[\"a\"]", "[\"a\", \"b\"]");
    assert_eq!(fs::read_to_string(&config).unwrap(), expected);
    assert!(!sandbox.path("config.toml.tmp").exists());
}

#[test]
fn pin_twice_is_one_entry() {
    let sandbox = Sandbox::new();
    let config = sandbox.file("config.toml", "[bar]\npinned = []\n");

    assert!(set_pinned(&config, add("steam")).unwrap());
    assert!(!set_pinned(&config, add("steam")).unwrap());

    let text = fs::read_to_string(&config).unwrap();
    assert_eq!(text.matches("steam").count(), 1);
}

#[test]
fn pin_into_a_file_without_a_bar_table() {
    let sandbox = Sandbox::new();
    let config = sandbox.file("config.toml", "# top\n[appearance]\naccent = \"#112233\"\n");

    assert!(set_pinned(&config, add("steam")).unwrap());

    let text = fs::read_to_string(&config).unwrap();
    assert!(text.starts_with("# top\n[appearance]\naccent = \"#112233\"\n"));
    let parsed: toml::Table = toml::from_str(&text).unwrap();
    assert_eq!(parsed["bar"]["pinned"].as_array().unwrap().len(), 1);
}

#[test]
fn pin_creates_a_missing_file() {
    let sandbox = Sandbox::new();
    let config = sandbox.path("nested/config.toml");

    assert!(set_pinned(&config, add("steam")).unwrap());

    let parsed: toml::Table = toml::from_str(&fs::read_to_string(&config).unwrap()).unwrap();
    assert_eq!(parsed["bar"]["pinned"][0].as_str(), Some("steam"));
}

#[test]
fn unpin_removes_only_that_app() {
    let sandbox = Sandbox::new();
    let config = sandbox.file(
        "config.toml",
        "[bar]\npinned = [\"a\", \"b\", \"a\", \"c\"]\n",
    );

    assert!(set_pinned(&config, remove("a")).unwrap());
    assert!(!set_pinned(&config, remove("a")).unwrap());

    let parsed: toml::Table = toml::from_str(&fs::read_to_string(&config).unwrap()).unwrap();
    let pinned: Vec<_> = parsed["bar"]["pinned"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|value| value.as_str())
        .collect();
    assert_eq!(pinned, ["b", "c"]);
}

#[test]
fn unparsable_file_is_never_written() {
    let sandbox = Sandbox::new();
    for broken in ["[bar\npinned = ", "bar = 3\n", "[bar]\npinned = \"x\"\n"] {
        let config = sandbox.file("config.toml", broken);
        let result = set_pinned(&config, add("steam"));
        assert!(matches!(result, Err(EditError::Unparsable(_))), "{broken}");
        assert_eq!(fs::read_to_string(&config).unwrap(), broken);
        assert!(!sandbox.path("config.toml.tmp").exists());
    }
}

#[test]
fn invalid_app_id_is_refused() {
    let sandbox = Sandbox::new();
    let config = sandbox.file("config.toml", "");
    for bad in ["../x", "a b", "", "x\"y"] {
        assert!(matches!(
            set_pinned(&config, add(bad)),
            Err(EditError::InvalidAppId(_))
        ));
        assert!(matches!(
            set_pinned(&config, remove(bad)),
            Err(EditError::InvalidAppId(_))
        ));
    }
    assert_eq!(fs::read_to_string(&config).unwrap(), "");
}

fn plasma(sandbox: &Sandbox, appletsrc: &str, mimeapps: &str) -> (PathBuf, PathBuf) {
    (
        sandbox.file(APPLETSRC_FILE, appletsrc),
        sandbox.file(MIMEAPPS_FILE, mimeapps),
    )
}

#[test]
fn import_reads_plasma_launchers_once() {
    let sandbox = Sandbox::new();
    let config = sandbox.file(
        "config.toml",
        "# mine\n[appearance]\naccent = \"#112233\"\n",
    );
    let (appletsrc, mimeapps) = plasma(&sandbox, APPLETSRC, MIMEAPPS);

    let imported = import_plasma_launchers(&config, &appletsrc, &mimeapps).unwrap();
    assert_eq!(imported.unwrap(), ["org.kde.dolphin", "steam", "firefox"]);
    let text = fs::read_to_string(&config).unwrap();
    assert!(text.starts_with("# mine\n"));
    let parsed: toml::Table = toml::from_str(&text).unwrap();
    assert_eq!(parsed["bar"]["pinned"].as_array().unwrap().len(), 3);

    assert_eq!(
        import_plasma_launchers(&config, &appletsrc, &mimeapps).unwrap(),
        None
    );
}

#[test]
fn explicit_empty_pinned_is_not_overwritten() {
    let sandbox = Sandbox::new();
    let config = sandbox.file("config.toml", "[bar]\npinned = []\n");
    let (appletsrc, mimeapps) = plasma(&sandbox, APPLETSRC, MIMEAPPS);

    assert_eq!(
        import_plasma_launchers(&config, &appletsrc, &mimeapps).unwrap(),
        None
    );
    assert_eq!(fs::read_to_string(&config).unwrap(), "[bar]\npinned = []\n");
}

#[test]
fn preferred_filemanager_resolves_through_mimeapps() {
    let sandbox = Sandbox::new();
    let config = sandbox.file("config.toml", "");
    let appletsrc = APPLETSRC.replace("launchers=", "launchers=preferred://filemanager,");
    let (appletsrc, mimeapps) = plasma(&sandbox, &appletsrc, MIMEAPPS);

    let imported = import_plasma_launchers(&config, &appletsrc, &mimeapps).unwrap();
    assert_eq!(imported.unwrap()[0], "org.kde.dolphin");
}

#[test]
fn unresolvable_preferred_launcher_is_dropped() {
    let sandbox = Sandbox::new();
    let config = sandbox.file("config.toml", "");
    let (appletsrc, mimeapps) = plasma(
        &sandbox,
        "[Containments][1][Applets][2]\nplugin=org.kde.plasma.taskmanager\n[Containments][1][Applets][2][Configuration][General]\nlaunchers=preferred://filemanager,preferred://browser,applications:steam.desktop,applications:../evil.desktop\n",
        "[Default Applications]\ntext/plain=kate.desktop;\n",
    );

    let imported = import_plasma_launchers(&config, &appletsrc, &mimeapps).unwrap();
    assert_eq!(imported.unwrap(), ["steam"]);
}

#[test]
fn import_without_a_task_manager_writes_nothing() {
    let sandbox = Sandbox::new();
    let config = sandbox.file("config.toml", "# mine\n");
    let (appletsrc, mimeapps) = plasma(&sandbox, "[Containments][1]\nplugin=x\n", MIMEAPPS);

    assert_eq!(
        import_plasma_launchers(&config, &appletsrc, &mimeapps).unwrap(),
        None
    );
    assert_eq!(fs::read_to_string(&config).unwrap(), "# mine\n");
}

#[tokio::test]
async fn feed_imports_at_start_then_serves_pin_calls() {
    let sandbox = Sandbox::new();
    let config = sandbox.path("config.toml");
    sandbox.file(APPLETSRC_FILE, APPLETSRC);
    sandbox.file(MIMEAPPS_FILE, MIMEAPPS);
    let (calls, receiver) = broadcast::channel(8);
    let (_demand, demand_receiver) = watch::channel(BTreeSet::new());
    let (_config, config_receiver) = watch::channel(ResolvedConfig::default());
    let context = FeedContext {
        publisher: Publisher::new(|_, _| panic!("the pins feed publishes nothing")),
        demand: demand_receiver,
        config: config_receiver,
        calls: receiver,
    };
    let task = tokio::spawn(pins::feed(config.clone(), sandbox.dir.path().to_owned()).run(context));
    let call = |method: &str, args| {
        calls
            .send(Call {
                method: method.to_owned(),
                args,
            })
            .unwrap();
    };

    call(calls::BAR_UNPIN, json!({ "app": "steam" }));
    call(calls::BAR_PIN, json!({ "app": "../bad" }));
    call(calls::BAR_PIN, json!({}));
    call(calls::BAR_PIN, json!({ "app": "kate" }));
    timeout(Duration::from_secs(5), async {
        while !fs::read_to_string(&config).is_ok_and(|text| text.contains("kate")) {
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();

    let parsed: toml::Table = toml::from_str(&fs::read_to_string(&config).unwrap()).unwrap();
    let pinned: Vec<_> = parsed["bar"]["pinned"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|value| value.as_str())
        .collect();
    assert_eq!(pinned, ["org.kde.dolphin", "firefox", "kate"]);
    task.abort();
}

#[test]
fn default_config_leaves_pinned_unset_so_the_import_runs() {
    let sandbox = Sandbox::new();
    let config = sandbox.file("config.toml", ghostd::config::DEFAULT_TOML);
    let (appletsrc, mimeapps) = plasma(&sandbox, APPLETSRC, MIMEAPPS);

    let loaded = ghostd::config::load_str(ghostd::config::DEFAULT_TOML).unwrap();
    assert!(loaded.bar.pinned.is_empty());
    let imported = import_plasma_launchers(&config, &appletsrc, &mimeapps).unwrap();
    assert_eq!(imported.unwrap().len(), 3);
}
