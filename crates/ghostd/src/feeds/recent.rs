//! The `recent` topic: the files the desktop lists as recently used, read from
//! `$XDG_DATA_HOME/recently-used.xbel`.

use std::collections::HashMap;
use std::env;
use std::ffi::OsString;
use std::fs;
use std::future::pending;
use std::path::{Path, PathBuf};
use std::time::Duration;

use ghost_proto::{RecentFile, RecentFiles, topics};
use notify::{Event, EventKind, RecursiveMode, Watcher};
use percent_encoding::percent_decode_str;
use quick_xml::Reader;
use quick_xml::XmlVersion;
use quick_xml::events::Event as Xml;
use tokio::sync::mpsc;
use tokio::time::timeout;
use tracing::warn;

use super::{until_unwanted, until_wanted};
use crate::server::{Feed, FeedContext, Publisher};

/// How long the file must stay quiet before it is read again; GLib writes it in several steps.
const DEBOUNCE: Duration = Duration::from_millis(250);
const SHOWN: usize = 5;
const XBEL_FILE: &str = "recently-used.xbel";
const DATA_HOME_SUBDIR: &str = ".local/share";
const XDG_DATA_HOME_ENV: &str = "XDG_DATA_HOME";
const HOME_ENV: &str = "HOME";
const BOOKMARK: &str = "bookmark";
const HREF: &str = "href";
const MODIFIED: &str = "modified";
const FILE_URL_PREFIX: &str = "file://";
const HOME_SHORTHAND: &str = "~";

/// Watches the user's xbel while some client wants `recent`.
pub fn feed() -> Feed {
    let home = PathBuf::from(env::var_os(HOME_ENV).unwrap_or_default());
    feed_at(xbel_path(env::var_os(XDG_DATA_HOME_ENV), &home), home)
}

/// As [`feed`], reading `xbel` and shortening folders under `home`.
pub fn feed_at(xbel: PathBuf, home: PathBuf) -> Feed {
    Feed::new(topics::RECENT, move |context| run(xbel, home, context))
}

fn xbel_path(xdg_data_home: Option<OsString>, home: &Path) -> PathBuf {
    xdg_data_home
        .filter(|dir| !dir.is_empty())
        .map_or_else(|| home.join(DATA_HOME_SUBDIR), PathBuf::from)
        .join(XBEL_FILE)
}

async fn run(xbel: PathBuf, home: PathBuf, mut context: FeedContext) {
    loop {
        until_wanted(&mut context.demand, topics::RECENT).await;
        // Dropping the watcher inside `watch` is what stops it, so nothing is watched while unwanted.
        tokio::select! {
            biased;
            () = until_unwanted(&mut context.demand, topics::RECENT) => {}
            () = watch(&xbel, &home, context.publisher.clone()) => {}
        }
    }
}

async fn watch(xbel: &Path, home: &Path, publisher: Publisher) {
    let (Some(directory), Some(file_name)) = (xbel.parent(), xbel.file_name()) else {
        warn!("the recent files path names no file; recent stays empty");
        return pending().await;
    };
    let (event_tx, mut events) = mpsc::unbounded_channel();
    let file_name = file_name.to_owned();
    let watcher = notify::recommended_watcher(move |result: notify::Result<Event>| {
        // inotify reports our own reads as opens, which would retrigger forever.
        if let Ok(event) = result
            && !matches!(event.kind, EventKind::Access(_))
            && event
                .paths
                .iter()
                .any(|path| path.file_name() == Some(file_name.as_os_str()))
        {
            // A send only fails once the feed stopped listening.
            let _ = event_tx.send(());
        }
    })
    .and_then(|mut watcher| {
        // The directory, not the file, so atomic-rename saves that replace the inode keep working.
        watcher
            .watch(directory, RecursiveMode::NonRecursive)
            .map(|()| watcher)
    });
    let _watcher = match watcher {
        Ok(watcher) => watcher,
        Err(error) => {
            warn!(%error, "cannot watch the recent files; recent stays as read once");
            publisher.publish(topics::RECENT, &read(xbel, home));
            return pending().await;
        }
    };

    // Read after the watch is in place, so a change in between is not lost.
    let mut published = read(xbel, home);
    publisher.publish(topics::RECENT, &published);
    while events.recv().await.is_some() {
        while let Ok(Some(())) = timeout(DEBOUNCE, events.recv()).await {}
        let files = read(xbel, home);
        if files != published {
            publisher.publish(topics::RECENT, &files);
            published = files;
        }
    }
    pending().await
}

/// The newest existing files of the xbel; an unreadable or malformed file lists none.
fn read(xbel: &Path, home: &Path) -> RecentFiles {
    let Ok(text) = fs::read_to_string(xbel) else {
        return RecentFiles::default();
    };
    let bookmarks = parse(&text).unwrap_or_else(|error| {
        warn!(%error, "the recent files list is malformed; showing none");
        Vec::new()
    });
    present(newest_per_path(bookmarks), home)
}

/// Every `file://` bookmark as `(path, modified_s)`, in document order.
fn parse(text: &str) -> quick_xml::Result<Vec<(PathBuf, u64)>> {
    let mut reader = Reader::from_str(text);
    let mut bookmarks = Vec::new();
    loop {
        match reader.read_event()? {
            Xml::Eof => return Ok(bookmarks),
            Xml::Start(tag) | Xml::Empty(tag) if tag.name().as_ref() == BOOKMARK => {
                let (mut path, mut modified_s) = (None, 0);
                for attribute in tag.attributes().flatten() {
                    let Ok(value) = attribute.normalized_value(XmlVersion::Implicit1_0) else {
                        continue;
                    };
                    match attribute.key.as_ref() {
                        HREF => path = file_path(&value),
                        MODIFIED => modified_s = seconds(&value),
                        _ => {}
                    }
                }
                bookmarks.extend(path.map(|path| (path, modified_s)));
            }
            _ => {}
        }
    }
}

fn file_path(href: &str) -> Option<PathBuf> {
    let encoded = href.strip_prefix(FILE_URL_PREFIX)?;
    let decoded = percent_decode_str(encoded).decode_utf8().ok()?;
    Some(PathBuf::from(decoded.into_owned()))
}

fn seconds(timestamp: &str) -> u64 {
    timestamp
        .parse::<jiff::Timestamp>()
        .ok()
        .and_then(|time| u64::try_from(time.as_second()).ok())
        .unwrap_or(0)
}

fn newest_per_path(bookmarks: Vec<(PathBuf, u64)>) -> Vec<(PathBuf, u64)> {
    let mut newest: HashMap<PathBuf, u64> = HashMap::new();
    for (path, modified_s) in bookmarks {
        let slot = newest.entry(path).or_default();
        *slot = (*slot).max(modified_s);
    }
    let mut ordered: Vec<_> = newest.into_iter().collect();
    // The path breaks ties so equal timestamps list in a stable order.
    ordered.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    ordered
}

fn present(ordered: Vec<(PathBuf, u64)>, home: &Path) -> RecentFiles {
    let files = ordered
        .into_iter()
        .filter(|(path, _)| path.exists())
        .take(SHOWN)
        .map(|(path, modified_s)| RecentFile {
            name: path
                .file_name()
                .map_or_else(String::new, |name| name.to_string_lossy().into_owned()),
            dir: shorten(path.parent().unwrap_or(Path::new("")), home),
            path: path.to_string_lossy().into_owned(),
            modified_s,
        })
        .collect();
    RecentFiles { files }
}

fn shorten(dir: &Path, home: &Path) -> String {
    match dir.strip_prefix(home) {
        Ok(rest) if rest.as_os_str().is_empty() => HOME_SHORTHAND.to_owned(),
        Ok(rest) => format!("{HOME_SHORTHAND}/{}", rest.display()),
        Err(_) => dir.to_string_lossy().into_owned(),
    }
}
