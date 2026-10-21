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

/// The store as read from its file at the start of one fetch.
pub(crate) struct Store {
    path: PathBuf,
    pairs: Pairs,
    /// False when the file could not be read, so that saving never replaces what the user wrote.
    writable: bool,
    changed: bool,
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
    /// and also taken as empty, and is then left as it is.
    pub fn open(path: &Path) -> Self {
        let read = fs::read_to_string(path)
            .and_then(|json| serde_json::from_str::<Pairs>(&json).map_err(io::Error::from));
        let (pairs, writable) = match read {
            Ok(pairs) => (pairs, true),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                debug!(path = %path.display(), "no translations stored yet");
                (Pairs::new(), true)
            }
            Err(error) => {
                warn!(
                    path = %path.display(),
                    %error,
                    "cannot read the stored translations; using none, and leaving the file as it is"
                );
                (Pairs::new(), false)
            }
        };
        Self {
            path: path.to_owned(),
            pairs,
            writable,
            changed: false,
        }
    }

    /// The entry for `pair` whose Portuguese text normalises to `key`.
    pub fn find(&self, pair: &str, key: &str) -> Option<&Stored> {
        self.pairs
            .get(pair)?
            .iter()
            .find(|(text, _)| normalise(text) == key)
            .map(|(_, stored)| stored)
    }

    pub fn insert(&mut self, pair: &str, text: &str, stored: Stored) {
        self.pairs
            .entry(pair.to_owned())
            .or_default()
            .insert(text.to_owned(), stored);
        self.changed = true;
    }

    /// Writes the store back if an entry was added, through a temporary file renamed over the old
    /// one, so that a crash or a full disk never leaves half a file. A failure is logged.
    pub fn save(&self) {
        if !self.changed || !self.writable {
            return;
        }
        if let Err(error) = self.write() {
            warn!(path = %self.path.display(), %error, "cannot store the new translations");
        }
    }

    fn write(&self) -> io::Result<()> {
        let dir = self.path.parent().unwrap_or(Path::new(""));
        fs::create_dir_all(dir)?;
        let mut file = NamedTempFile::new_in(dir)?;
        serde_json::to_writer_pretty(&mut file, &self.pairs)?;
        file.write_all(b"\n")?;
        file.as_file().sync_all()?;
        file.persist(&self.path)?;
        Ok(())
    }
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
