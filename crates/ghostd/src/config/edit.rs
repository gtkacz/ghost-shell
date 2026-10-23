//! Edits `bar.pinned` in the user's `config.toml` without disturbing anything else in it, and
//! imports the Plasma task manager's launchers into it once.

use std::collections::HashSet;
use std::ffi::OsString;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use toml_edit::{Array, DocumentMut, Item, Table, value};

use super::load::config_dir;

const BAR: &str = "bar";
const PINNED: &str = "pinned";
const TASK_PLUGINS: [&str; 2] = ["org.kde.plasma.icontasks", "org.kde.plasma.taskmanager"];
const LAUNCHERS_SECTION_SUFFIX: &str = "[Configuration][General]";
const LAUNCHERS_KEY: &str = "launchers";
const PLUGIN_KEY: &str = "plugin";
const LAUNCHER_SEPARATOR: char = ',';
const APPLICATIONS_SCHEME: &str = "applications:";
const FILE_SCHEME: &str = "file://";
const PREFERRED_FILE_MANAGER: &str = "preferred://filemanager";
const DESKTOP_SUFFIX: &str = ".desktop";
const MIMEAPPS_DEFAULTS_SECTION: &str = "[Default Applications]";
const DIRECTORY_MIME: &str = "inode/directory";
const MIME_HANDLER_SEPARATOR: char = ';';
const TEMP_SUFFIX: &str = ".tmp";

/// Where the Plasma files `import_plasma_launchers` reads live: `$XDG_CONFIG_HOME`, or `~/.config`.
pub fn plasma_config_dir() -> PathBuf {
    config_dir()
        .parent()
        .map_or_else(PathBuf::new, Path::to_path_buf)
}

#[derive(Debug, thiserror::Error)]
pub enum EditError {
    #[error("{0:?} is not a desktop-file id")]
    InvalidAppId(String),
    #[error("config file cannot be edited: {0}")]
    Unparsable(String),
    #[error("config file I/O: {0}")]
    Io(#[from] io::Error),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PinChange {
    Add(String),
    Remove(String),
}

/// Ids come from calls any client can make, and end up in a file that is also read back by hand.
pub fn is_app_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

/// Applies `change` to `bar.pinned` in the file at `path`, and returns whether the file changed.
/// A missing file counts as empty. A file that is not valid TOML, or whose `bar.pinned` is not an
/// array, is left as it is.
pub fn set_pinned(path: &Path, change: PinChange) -> Result<bool, EditError> {
    let (PinChange::Add(app) | PinChange::Remove(app)) = &change;
    if !is_app_id(app) {
        return Err(EditError::InvalidAppId(app.clone()));
    }
    let mut document = read_document(path)?;
    let changed = match change {
        PinChange::Add(app) => {
            let pinned = pinned_array(&mut document)?;
            let present = pinned.iter().any(|entry| entry.as_str() == Some(&app));
            if !present {
                pinned.push(app);
            }
            !present
        }
        PinChange::Remove(app) => match existing_pinned(&mut document)? {
            Some(pinned) => {
                let before = pinned.len();
                pinned.retain(|entry| entry.as_str() != Some(&app));
                pinned.len() != before
            }
            None => false,
        },
    };
    if changed {
        write_atomically(path, &document.to_string())?;
    }
    Ok(changed)
}

/// Pins what the Plasma task manager has pinned, but only if the config file has no
/// `bar.pinned` key at all, so a user's own choice (even an empty list) always stands. Returns
/// the imported ids, or `None` when nothing was imported.
pub fn import_plasma_launchers(
    config: &Path,
    appletsrc: &Path,
    mimeapps: &Path,
) -> Result<Option<Vec<String>>, EditError> {
    let mut document = read_document(config)?;
    if existing_pinned(&mut document)?.is_some() {
        return Ok(None);
    }
    let Some(launchers) = read_optional(appletsrc)?.and_then(|text| task_launchers(&text)) else {
        return Ok(None);
    };
    let mimeapps = read_optional(mimeapps)?.unwrap_or_default();
    let mut imported: Vec<String> = Vec::new();
    for id in launchers
        .split(LAUNCHER_SEPARATOR)
        .filter_map(|launcher| launcher_id(launcher.trim(), &mimeapps))
    {
        if !imported.contains(&id) {
            imported.push(id);
        }
    }
    pinned_array(&mut document)?.extend(imported.iter().map(String::as_str));
    write_atomically(config, &document.to_string())?;
    Ok(Some(imported))
}

fn read_optional(path: &Path) -> Result<Option<String>, EditError> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn read_document(path: &Path) -> Result<DocumentMut, EditError> {
    read_optional(path)?
        .unwrap_or_default()
        .parse()
        .map_err(|error: toml_edit::TomlError| EditError::Unparsable(error.to_string()))
}

fn unusable(what: &str) -> EditError {
    EditError::Unparsable(format!("{what} is not what a config file holds there"))
}

/// `bar.pinned`, or `None` when the document has no such key.
fn existing_pinned(document: &mut DocumentMut) -> Result<Option<&mut Array>, EditError> {
    let Some(bar) = document.get_mut(BAR) else {
        return Ok(None);
    };
    let bar = bar.as_table_like_mut().ok_or_else(|| unusable(BAR))?;
    match bar.get_mut(PINNED) {
        Some(pinned) => pinned
            .as_array_mut()
            .map(Some)
            .ok_or_else(|| unusable("bar.pinned")),
        None => Ok(None),
    }
}

/// `bar.pinned`, with `[bar]` and the key created if missing.
fn pinned_array(document: &mut DocumentMut) -> Result<&mut Array, EditError> {
    if !document.contains_key(BAR) {
        let mut bar = Table::new();
        // Implicit, so that an empty table can never show up as a bare `[bar]` header.
        bar.set_implicit(true);
        document.insert(BAR, Item::Table(bar));
    }
    let bar = document[BAR]
        .as_table_like_mut()
        .ok_or_else(|| unusable(BAR))?;
    if !bar.contains_key(PINNED) {
        bar.insert(PINNED, value(Array::new()));
    }
    bar.get_mut(PINNED)
        .and_then(Item::as_array_mut)
        .ok_or_else(|| unusable("bar.pinned"))
}

/// A crash mid-write must not leave half a config file for the watcher to load.
fn write_atomically(path: &Path, contents: &str) -> Result<(), EditError> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    let mut temporary = OsString::from(path);
    temporary.push(TEMP_SUFFIX);
    let temporary = PathBuf::from(temporary);
    let written =
        write_synced(&temporary, path, contents).and_then(|()| fs::rename(&temporary, path));
    if written.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    Ok(written?)
}

/// The `launchers=` value of the first task manager applet that has one.
fn task_launchers(appletsrc: &str) -> Option<String> {
    let mut section = "";
    let mut task_managers: HashSet<&str> = HashSet::new();
    let mut launchers: Vec<(&str, &str)> = Vec::new();
    for line in appletsrc.lines().map(str::trim) {
        if line.starts_with('[') && line.ends_with(']') {
            section = line;
        } else if let Some((key, setting)) = line.split_once('=') {
            let setting = setting.trim();
            if key.trim_end() == PLUGIN_KEY && TASK_PLUGINS.contains(&setting) {
                task_managers.insert(section);
            } else if key.trim_end() == LAUNCHERS_KEY
                && let Some(applet) = section.strip_suffix(LAUNCHERS_SECTION_SUFFIX)
            {
                launchers.push((applet, setting));
            }
        }
    }
    launchers
        .into_iter()
        .find(|(applet, _)| task_managers.contains(applet))
        .map(|(_, setting)| setting.to_owned())
}

/// The desktop-file id a launcher URL stands for, if it names one this config can hold.
fn launcher_id(launcher: &str, mimeapps: &str) -> Option<String> {
    let desktop_file = if launcher == PREFERRED_FILE_MANAGER {
        file_manager(mimeapps)?
    } else if let Some(id) = launcher.strip_prefix(APPLICATIONS_SCHEME) {
        id.to_owned()
    } else {
        launcher
            .strip_prefix(FILE_SCHEME)?
            .rsplit('/')
            .next()?
            .to_owned()
    };
    desktop_file
        .strip_suffix(DESKTOP_SUFFIX)
        .filter(|id| is_app_id(id))
        .map(str::to_owned)
}

/// The first `inode/directory` handler the user's `[Default Applications]` lists.
fn file_manager(mimeapps: &str) -> Option<String> {
    let mut in_defaults = false;
    for line in mimeapps.lines().map(str::trim) {
        if line.starts_with('[') {
            in_defaults = line == MIMEAPPS_DEFAULTS_SECTION;
        } else if in_defaults
            && let Some((mime, handlers)) = line.split_once('=')
            && mime.trim_end() == DIRECTORY_MIME
        {
            return handlers
                .split(MIME_HANDLER_SEPARATOR)
                .map(str::trim)
                .find(|handler| !handler.is_empty())
                .map(str::to_owned);
        }
    }
    None
}

/// Synced before the rename, so a power loss cannot leave the new name pointing at empty data.
/// The original's permissions carry over, since a rename would otherwise reset them.
fn write_synced(temporary: &Path, original: &Path, contents: &str) -> io::Result<()> {
    let mut file = fs::File::create(temporary)?;
    file.write_all(contents.as_bytes())?;
    if let Ok(metadata) = fs::metadata(original) {
        file.set_permissions(metadata.permissions())?;
    }
    file.sync_all()
}
