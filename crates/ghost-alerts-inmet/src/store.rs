//! The translations that are not built in: the online translator's answers, which the user may
//! correct, kept in `$XDG_STATE_HOME/ghost-shell/translations.json`.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::{env, fs};

use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;
use tracing::{debug, warn};

use crate::translate::normalise;

const XDG_STATE_HOME_ENV: &str = "XDG_STATE_HOME";
const HOME_ENV: &str = "HOME";
const STATE_HOME_SUBDIR: &str = ".local/state";
const APP_STATE_DIR: &str = "ghost-shell";
const STORE_FILE: &str = "translations.json";

/// One stored translation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct Stored {
    pub text: String,
    /// The online translator gave it and nobody has checked it; a user who has sets it to false.
    pub auto: bool,
    /// Who gave it, such as `mymemory`, and when, in RFC 3339. A user writing an entry by hand
    /// may leave both out.
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub at: String,
}

/// The entries by language pair, such as `pt-BR|en-US`, then by the Portuguese text.
type Pairs = BTreeMap<String, BTreeMap<String, Stored>>;

/// The store as read from its file at the start of one fetch, for looking terms up.
pub(crate) struct Store {
    pairs: Pairs,
}

/// Where the store lives: `$XDG_STATE_HOME/ghost-shell/translations.json`, or under
/// `~/.local/state` when that variable is unset.
pub fn default_store_path() -> PathBuf {
    store_path_in(env::var_os(XDG_STATE_HOME_ENV), env::var_os(HOME_ENV))
}

fn store_path_in(xdg_state_home: Option<OsString>, home: Option<OsString>) -> PathBuf {
    xdg_state_home
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(home.unwrap_or_default()).join(STATE_HOME_SUBDIR))
        .join(APP_STATE_DIR)
        .join(STORE_FILE)
}

impl Store {
    /// Reads `path`. A missing file is an empty store; one that cannot be read or parsed is logged
    /// and also taken as empty.
    pub fn open(path: &Path) -> Self {
        let pairs = match read(path) {
            Ok(pairs) => pairs,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                debug!(path = %path.display(), "no translations stored yet");
                Pairs::new()
            }
            Err(error) => {
                warn!(
                    path = %path.display(),
                    %error,
                    "cannot read the stored translations; using none, and leaving the file as it is"
                );
                Pairs::new()
            }
        };
        Self { pairs }
    }

    /// The entry for `pair` whose Portuguese text normalises to `key`, preferring one the user has
    /// checked when several do. An entry with blank text counts as none.
    pub fn find(&self, pair: &str, key: &str) -> Option<&Stored> {
        let mut matching = self
            .pairs
            .get(pair)?
            .iter()
            .filter(|(text, stored)| !stored.text.trim().is_empty() && normalise(text) == key)
            .map(|(_, stored)| stored);
        let first = matching.next()?;
        Some(if first.auto {
            matching.find(|stored| !stored.auto).unwrap_or(first)
        } else {
            first
        })
    }
}

/// Adds `learned`, Portuguese text and translation, to `pair`'s entries as the file holds them
/// now rather than as they were read, so that an edit the user made in the meantime survives. An
/// entry the user has checked is never replaced. The write is skipped, with a warning, when the
/// file cannot be read back, so that it never replaces what the user wrote; any failure is logged.
pub(crate) fn merge(path: &Path, pair: &str, learned: Vec<(String, Stored)>) {
    let mut pairs = match read(path) {
        Ok(pairs) => pairs,
        Err(error) if error.kind() == io::ErrorKind::NotFound => Pairs::new(),
        Err(error) => {
            warn!(
                path = %path.display(),
                %error,
                "not storing the new translations, since the file cannot be read back"
            );
            return;
        }
    };
    let entries = pairs.entry(pair.to_owned()).or_default();
    for (text, stored) in learned {
        let key = normalise(&text);
        let checked = entries
            .iter()
            .any(|(existing, entry)| !entry.auto && normalise(existing) == key);
        if !checked {
            entries.insert(text, stored);
        }
    }
    if let Err(error) = write(path, &pairs) {
        warn!(path = %path.display(), %error, "cannot store the new translations");
    }
}

fn read(path: &Path) -> io::Result<Pairs> {
    let json = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&json)?)
}

/// Writes through a temporary file renamed over the old one, so that a crash or a full disk never
/// leaves half a file.
fn write(path: &Path, pairs: &Pairs) -> io::Result<()> {
    let dir = path.parent().unwrap_or(Path::new(""));
    fs::create_dir_all(dir)?;
    let mut file = NamedTempFile::new_in(dir)?;
    serde_json::to_writer_pretty(&mut file, pairs)?;
    file.write_all(b"\n")?;
    file.as_file().sync_all()?;
    file.persist(path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path_in(xdg_state_home: Option<&str>, home: Option<&str>) -> PathBuf {
        store_path_in(xdg_state_home.map(OsString::from), home.map(OsString::from))
    }

    #[test]
    fn xdg_state_home_wins_over_home() {
        assert_eq!(
            path_in(Some("/srv/state"), Some("/home/u")),
            Path::new("/srv/state/ghost-shell/translations.json")
        );
    }

    #[test]
    fn an_unset_or_empty_xdg_state_home_falls_back_to_local_state() {
        for xdg in [None, Some("")] {
            assert_eq!(
                path_in(xdg, Some("/home/u")),
                Path::new("/home/u/.local/state/ghost-shell/translations.json")
            );
        }
    }
}
