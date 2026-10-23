//! The snapshot of the user's style and look that the lock and login screens read. They are
//! separate Qt processes that cannot reach ghostd, so it keeps an INI file current for them.

use std::ffi::OsString;
use std::fmt::Write as _;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::{env, future};

use ghost_proto::{LockStyle, ResolvedConfig};
use tokio::sync::watch;
use tracing::warn;

const XDG_STATE_HOME_ENV: &str = "XDG_STATE_HOME";
const HOME_ENV: &str = "HOME";
const STATE_HOME_SUBDIR: &str = ".local/state";
const APP_STATE_DIR: &str = "ghost-shell";
const SYSTEM_DIR: &str = "/var/lib/ghost-shell";
const SNAPSHOT_FILE: &str = "gate.ini";
const SNAPSHOT_VERSION: u32 = 1;
const SNAPSHOT_MODE: u32 = 0o644;

/// Every string is double-quoted with QSettings' escapes, because it would otherwise read an
/// unquoted `;` or `,` in a value as a comment or a list separator.
fn quoted(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for c in value.chars() {
        if matches!(c, '\\' | '"') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('"');
    out
}

fn style_name(style: LockStyle) -> &'static str {
    match style {
        LockStyle::Prompt => "prompt",
        LockStyle::Terminal => "terminal",
        LockStyle::Idcheck => "idcheck",
    }
}

fn japanese_name(config: &ResolvedConfig) -> String {
    serde_json::to_value(config.appearance.japanese)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default()
}

pub fn snapshot_ini(config: &ResolvedConfig, host: &str) -> String {
    let look = &config.appearance;
    let colors = &look.colors;
    let mut ini = String::new();
    let _ = write!(
        ini,
        "[gate]\nversion={SNAPSHOT_VERSION}\nstyle={}\nhost={}\ndisplay_model={}\n",
        style_name(config.lock.style),
        quoted(host),
        quoted(&config.display.model),
    );
    let _ = write!(
        ini,
        "[appearance]\nfont={}\nfont_cjk={}\njapanese={}\npanel_fill={}\n\
         alert_stripe_period_s={}\nalert_stripe_step_s={}\n",
        quoted(&look.font),
        quoted(&look.font_cjk),
        quoted(&japanese_name(config)),
        look.panel_fill,
        look.alert_stripe_period_s,
        look.alert_stripe_step_s,
    );
    ini.push_str("[colors]\n");
    for (key, value) in [
        ("ground", &colors.ground),
        ("panel", &colors.panel),
        ("hair", &colors.hair),
        ("hair2", &colors.hair2),
        ("text", &colors.text),
        ("hi", &colors.hi),
        ("dim", &colors.dim),
        ("signal", &colors.signal),
        ("warn", &colors.warn),
        ("crit", &colors.crit),
        ("dot", &colors.dot),
        ("burn", &colors.burn),
    ] {
        let _ = writeln!(ini, "{key}={}", quoted(value));
    }
    let _ = write!(
        ini,
        "[dots]\nspacing={}\nopacity={}\nsize_range={}\nbrightness={}\ncover={}\nscale={}\n\
         [scanlines]\nopacity={}\n",
        look.dots.spacing,
        look.dots.opacity,
        look.dots.size_range,
        look.dots.brightness,
        look.dots.cover,
        look.dots.scale,
        look.scanlines.opacity,
    );
    ini
}

/// Replaces `path` whole, so a reader never sees half a snapshot. The mode is set explicitly
/// because the greeters run as another user and the umask must not hide the file from them.
pub fn write_atomic(path: &Path, contents: &str) -> io::Result<()> {
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let tmp = path.with_file_name(format!(".{name}.tmp"));
    let written = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(SNAPSHOT_MODE)
            .open(&tmp)?;
        file.set_permissions(fs::Permissions::from_mode(SNAPSHOT_MODE))?;
        file.write_all(contents.as_bytes())?;
        file.sync_all()?;
        fs::rename(&tmp, path)
    })();
    if written.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    written
}

/// The per-user copy under the state dir, plus the system-wide copy the login screen reads when
/// `/var/lib/ghost-shell` has been set up for it.
pub fn snapshot_paths() -> Vec<PathBuf> {
    snapshot_paths_in(
        env::var_os(XDG_STATE_HOME_ENV),
        env::var_os(HOME_ENV),
        Path::new(SYSTEM_DIR),
    )
}

fn snapshot_paths_in(
    state_home: Option<OsString>,
    home: Option<OsString>,
    system_dir: &Path,
) -> Vec<PathBuf> {
    let state_dir = state_home
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(home.unwrap_or_default()).join(STATE_HOME_SUBDIR))
        .join(APP_STATE_DIR);
    if let Err(error) = fs::create_dir_all(&state_dir) {
        warn!(%error, dir = %state_dir.display(), "cannot create the gate snapshot's directory");
    }
    let mut paths = vec![state_dir.join(SNAPSHOT_FILE)];
    if system_dir.is_dir() {
        paths.push(system_dir.join(SNAPSHOT_FILE));
    }
    paths
}

/// Writes the snapshot at start and again after every config change.
pub async fn keep(mut updates: watch::Receiver<ResolvedConfig>, host: String, paths: Vec<PathBuf>) {
    loop {
        let ini = snapshot_ini(&updates.borrow_and_update(), &host);
        for path in &paths {
            if let Err(error) = write_atomic(path, &ini) {
                warn!(%error, path = %path.display(), "cannot write the gate snapshot");
            }
        }
        if updates.changed().await.is_err() {
            // The config can no longer change, so the last snapshot stands.
            future::pending::<()>().await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEYS: &[&str] = &[
        "[gate]",
        "version=1",
        "style=prompt",
        "host=",
        "display_model=",
        "[appearance]",
        "font=",
        "font_cjk=",
        "japanese=",
        "panel_fill=",
        "alert_stripe_period_s=",
        "alert_stripe_step_s=",
        "[colors]",
        "ground=",
        "panel=",
        "hair=",
        "hair2=",
        "text=",
        "hi=",
        "dim=",
        "signal=",
        "warn=",
        "crit=",
        "dot=",
        "burn=",
        "[dots]",
        "spacing=",
        "opacity=",
        "size_range=",
        "brightness=",
        "cover=",
        "scale=",
        "[scanlines]",
    ];

    fn default_ini() -> String {
        snapshot_ini(&crate::config::load_str("").unwrap(), "ghost")
    }

    #[test]
    fn snapshot_has_every_key() {
        let ini = default_ini();
        for key in KEYS {
            assert!(
                ini.contains(&format!("\n{key}")) || ini.starts_with(key),
                "{key}\n{ini}"
            );
        }
        assert!(ini.contains("\nground=\"#0a0f11\"\n"));
        assert!(ini.contains("\nhost=\"ghost\"\n"));
        assert!(ini.contains("\nsize_range=2.5\nbrightness=1.6\ncover=0.4\nscale=1\n[scanlines]\nopacity=15\n"));
    }

    #[test]
    fn strings_are_quoted_and_escaped_for_qsettings() {
        let ini = default_ini();
        assert!(ini.contains("\nfont=\"Kode Mono\"\n"));
        assert_eq!(quoted(r#"a"b\c;d"#), r#""a\"b\\c;d""#);
    }

    #[test]
    fn snapshot_reflects_style_and_fill() {
        let mut config = crate::config::load_str("").unwrap();
        config.lock.style = LockStyle::Idcheck;
        config.appearance.panel_fill = 40;
        let ini = snapshot_ini(&config, "ghost");
        assert!(ini.contains("\nstyle=idcheck\n"));
        assert!(ini.contains("\npanel_fill=40\n"));
    }

    #[test]
    fn write_atomic_replaces_whole_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("gate.ini");
        write_atomic(&path, "a").unwrap();
        write_atomic(&path, "b").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "b");
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            SNAPSHOT_MODE
        );
        let names: Vec<_> = fs::read_dir(dir.path()).unwrap().collect();
        assert_eq!(names.len(), 1, "a temporary file was left behind");
    }

    #[test]
    fn system_path_skipped_when_dir_missing() {
        let dir = tempfile::tempdir().unwrap();
        let tmp = dir.path();
        let paths = snapshot_paths_in(Some(tmp.into()), None, &tmp.join("absent"));
        assert_eq!(paths, vec![tmp.join("ghost-shell/gate.ini")]);
    }

    #[test]
    fn system_path_kept_when_dir_exists() {
        let dir = tempfile::tempdir().unwrap();
        let tmp = dir.path();
        let paths = snapshot_paths_in(Some(tmp.into()), None, tmp);
        assert_eq!(paths.len(), 2);
        assert_eq!(paths[1], tmp.join("gate.ini"));
    }
}
