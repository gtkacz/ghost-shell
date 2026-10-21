use std::collections::BTreeMap;
use std::os::unix::ffi::OsStrExt;
use std::path::{self, Path, PathBuf};
use std::{env, fs, io};

use ghost_proto::is_valid_topic;
use serde::Deserialize;
use serde_json::Value;

pub const WIDGET_API: u32 = 1;

const MANIFEST_FILE: &str = "manifest.toml";
const QML_FILE: &str = "Widget.qml";
const FILE_URL_SCHEME: &str = "file://";
// Besides letters and digits; the slash is kept because it separates the path's segments.
const URL_UNRESERVED_PUNCTUATION: [u8; 5] = *b"-._~/";
const BUILTIN_DIR_ENV: &str = "GHOST_WIDGETS_DIR";
const DATA_HOME_SUBDIR: &str = ".local/share";
const CONFIG_HOME_SUBDIR: &str = ".config";
const APP_DIR: &str = "ghost-shell";
const WIDGETS_DIR: &str = "widgets";
const HOME_PREFIX: &str = "~/";
const SEMVER_CORE_PARTS: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Panel,
    Bar,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SettingType {
    Bool,
    Int,
    Float,
    String,
    Color,
    Enum,
    List,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Setting {
    #[serde(rename = "type")]
    pub value_type: SettingType,
    pub default: Value,
    /// The allowed choices, read only when `value_type` is [`SettingType::Enum`].
    #[serde(default)]
    pub values: Vec<String>,
}

/// Design pixels as `[width, height]`.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SizeHints {
    pub min: Option<[f32; 2]>,
    pub preferred: Option<[f32; 2]>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub api: u32,
    pub kinds: Vec<Kind>,
    #[serde(default)]
    pub requires: Vec<String>,
    #[serde(default)]
    pub animates: bool,
    #[serde(default)]
    pub size: SizeHints,
    #[serde(default)]
    pub settings: BTreeMap<String, Setting>,
    /// The `file://` URL of the folder's `Widget.qml`, filled in by discovery.
    #[serde(skip)]
    pub source: String,
}

#[derive(Debug, thiserror::Error)]
enum ManifestError {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Parse(#[from] toml::de::Error),
    #[error("id {0:?} must match ^[a-z][a-z0-9-]*$")]
    InvalidId(String),
    #[error("version {0:?} is not semver (expected N.N.N, optionally followed by -pre and +build)")]
    InvalidVersion(String),
    #[error("requires {0:?} is not a valid topic name")]
    InvalidTopic(String),
    #[error("is not a regular file")]
    NotAFile,
    #[error("is a symlink to a file that does not exist")]
    DanglingLink,
    #[error("sits in a folder that is a symlink to a path that does not exist")]
    DanglingFolder,
}

/// Searches `dirs` in order, so a widget in a later directory replaces one with the same id from an
/// earlier directory.
///
/// Returns the widgets by id and the non-fatal problems, each starting with the path it concerns.
/// A directory that does not exist is skipped silently, and so is a folder without a
/// `manifest.toml`. Two folders of one directory declaring the same id are reported, and the later
/// folder's widget is the one kept.
pub fn discover(dirs: &[PathBuf]) -> (BTreeMap<String, Manifest>, Vec<String>) {
    let mut manifests = BTreeMap::new();
    let mut problems = Vec::new();
    for dir in dirs {
        // Per directory, because replacing an id from an earlier directory is how a user
        // overrides a built-in widget, which is no mistake to report.
        let mut declared_here: BTreeMap<String, PathBuf> = BTreeMap::new();
        for outcome in scan_dir(dir) {
            match outcome {
                Ok((manifest_path, manifest)) => {
                    let earlier = declared_here.insert(manifest.id.clone(), manifest_path.clone());
                    if let Some(earlier) = earlier {
                        problems.push(format!(
                            "{}: id {:?} is already declared by {}; this one is used",
                            manifest_path.display(),
                            manifest.id,
                            earlier.display()
                        ));
                    }
                    manifests.insert(manifest.id.clone(), manifest);
                }
                Err(problem) => problems.push(problem),
            }
        }
    }
    (manifests, problems)
}

/// The built-in widget directory, then the user's, then each of `paths` with a leading `~/`
/// expanded.
pub fn default_dirs(paths: &[String]) -> Vec<PathBuf> {
    let home = PathBuf::from(env::var_os("HOME").unwrap_or_default());
    let builtin = env::var_os(BUILTIN_DIR_ENV)
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(DATA_HOME_SUBDIR).join(APP_DIR).join(WIDGETS_DIR));
    let user = home
        .join(CONFIG_HOME_SUBDIR)
        .join(APP_DIR)
        .join(WIDGETS_DIR);
    [builtin, user]
        .into_iter()
        .chain(paths.iter().map(|path| expand_home(path, &home)))
        .collect()
}

fn expand_home(path: &str, home: &Path) -> PathBuf {
    path.strip_prefix(HOME_PREFIX)
        .map_or_else(|| PathBuf::from(path), |rest| home.join(rest))
}

fn scan_dir(dir: &Path) -> Vec<Result<(PathBuf, Manifest), String>> {
    let entries = match sorted_entries(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Vec::new(),
        Err(e) => return vec![Err(format!("{}: {e}", dir.display()))],
    };
    entries
        .iter()
        .map(|entry| entry.join(MANIFEST_FILE))
        .filter_map(|manifest_path| {
            let loaded = match has_manifest(&manifest_path) {
                Ok(true) => load_manifest(&manifest_path),
                Ok(false) => return None,
                Err(error) => Err(error),
            };
            Some(match loaded {
                Ok(manifest) => Ok((manifest_path, manifest)),
                Err(error) => Err(format!("{}: {error}", manifest_path.display())),
            })
        })
        .collect()
}

/// Whether `manifest_path` names a manifest to load. `false` means the folder is not a widget,
/// which is no problem; anything that stops a manifest from being read is an error instead, so a
/// widget that vanishes from the layout always comes with a reason.
fn has_manifest(manifest_path: &Path) -> Result<bool, ManifestError> {
    match fs::metadata(manifest_path) {
        Ok(metadata) if metadata.is_file() => Ok(true),
        Ok(_) => Err(ManifestError::NotAFile),
        // `metadata` follows links, so a link to nothing, here or in the folder's own name, reads
        // as not found as well.
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            if fs::symlink_metadata(manifest_path).is_ok() {
                Err(ManifestError::DanglingLink)
            } else if manifest_path.parent().is_some_and(is_dangling_link) {
                Err(ManifestError::DanglingFolder)
            } else {
                Ok(false)
            }
        }
        // A plain file sits where a widget folder would.
        Err(e) if e.kind() == io::ErrorKind::NotADirectory => Ok(false),
        Err(e) => Err(e.into()),
    }
}

fn is_dangling_link(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok() && fs::metadata(path).is_err()
}

fn sorted_entries(dir: &Path) -> io::Result<Vec<PathBuf>> {
    let mut entries = fs::read_dir(dir)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<io::Result<Vec<_>>>()?;
    // read_dir order is unspecified, and two widgets sharing an id in one directory must resolve
    // the same way on every run.
    entries.sort();
    Ok(entries)
}

fn load_manifest(manifest_path: &Path) -> Result<Manifest, ManifestError> {
    let mut manifest: Manifest = toml::from_str(&fs::read_to_string(manifest_path)?)?;
    validate(&manifest)?;
    let qml = path::absolute(manifest_path.with_file_name(QML_FILE))?;
    manifest.source = file_url(&qml);
    Ok(manifest)
}

/// Qt parses the source as a URL, where an unescaped `#` or `?` would cut the path short and a
/// `%` would start an escape. Everything outside RFC 3986's unreserved set is escaped, which also
/// covers the sub-delimiters a path may carry, so the rule stays one line. The bytes are escaped
/// as the OS holds them, because a path need not be UTF-8.
fn file_url(path: &Path) -> String {
    let mut url = String::from(FILE_URL_SCHEME);
    for &byte in path.as_os_str().as_bytes() {
        if byte.is_ascii_alphanumeric() || URL_UNRESERVED_PUNCTUATION.contains(&byte) {
            url.push(char::from(byte));
        } else {
            url.push_str(&format!("%{byte:02X}"));
        }
    }
    url
}

fn validate(manifest: &Manifest) -> Result<(), ManifestError> {
    if !is_valid_id(&manifest.id) {
        return Err(ManifestError::InvalidId(manifest.id.clone()));
    }
    if !is_semver(&manifest.version) {
        return Err(ManifestError::InvalidVersion(manifest.version.clone()));
    }
    match manifest
        .requires
        .iter()
        .find(|topic| !is_valid_topic(topic))
    {
        Some(topic) => Err(ManifestError::InvalidTopic(topic.clone())),
        None => Ok(()),
    }
}

fn is_valid_id(id: &str) -> bool {
    let mut chars = id.chars();
    matches!(chars.next(), Some('a'..='z'))
        && chars.all(|c| matches!(c, 'a'..='z' | '0'..='9' | '-'))
}

fn is_semver(version: &str) -> bool {
    let (without_build, build) = split_suffix(version, '+');
    let (core, pre_release) = split_suffix(without_build, '-');
    let core_parts: Vec<&str> = core.split('.').collect();
    core_parts.len() == SEMVER_CORE_PARTS
        && core_parts.iter().all(|part| is_numeric_identifier(part))
        && [pre_release, build]
            .into_iter()
            .flatten()
            .all(is_dotted_identifiers)
}

fn split_suffix(text: &str, separator: char) -> (&str, Option<&str>) {
    match text.split_once(separator) {
        Some((head, suffix)) => (head, Some(suffix)),
        None => (text, None),
    }
}

fn is_numeric_identifier(part: &str) -> bool {
    !part.is_empty()
        && part.bytes().all(|b| b.is_ascii_digit())
        && (part.len() == 1 || !part.starts_with('0'))
}

fn is_dotted_identifiers(text: &str) -> bool {
    text.split('.').all(|identifier| {
        !identifier.is_empty()
            && identifier
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    })
}
