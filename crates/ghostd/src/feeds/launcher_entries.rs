//! The `launcher.entries` topic: the badges and progress bars apps ask the launcher to show.
//!
//! Apps broadcast the Unity LauncherEntry `Update` signal from whatever name they hold, so the
//! feed matches it from any sender. An update lists only the properties that changed, so each
//! sender's entries are merged, and dropped once that sender leaves the bus.

use std::collections::{BTreeMap, HashMap};
use std::future::pending;

use ghost_proto::{LauncherEntries, LauncherEntry, topics};
use tracing::warn;
use zbus::{Connection, MatchRule, MessageStream, OwnedVariant, message};

use super::{next, until_unwanted, until_wanted};
use crate::server::{Feed, FeedContext, Publisher};

const INTERFACE: &str = "com.canonical.Unity.LauncherEntry";
const MEMBER: &str = "Update";
const URI_PREFIX: &str = "application://";
const DESKTOP_SUFFIX: &str = ".desktop";
const COUNT: &str = "count";
const COUNT_VISIBLE: &str = "count-visible";
const PROGRESS: &str = "progress";
const PROGRESS_VISIBLE: &str = "progress-visible";
const URGENT: &str = "urgent";
const UNIQUE_NAME_PREFIX: char = ':';

/// An entry as its app last set it, with the properties it has not hidden still kept.
#[derive(Debug, Default, Clone, PartialEq)]
struct Raw {
    count: i64,
    count_visible: bool,
    progress: f64,
    progress_visible: bool,
    urgent: bool,
}

impl Raw {
    fn apply(&mut self, properties: &HashMap<String, OwnedVariant>) {
        if let Some(count) = number::<i64>(properties, COUNT) {
            self.count = count;
        }
        if let Some(visible) = number::<bool>(properties, COUNT_VISIBLE) {
            self.count_visible = visible;
        }
        // A NaN would serialize as null and read as a missing bar.
        if let Some(progress) = number::<f64>(properties, PROGRESS).filter(|p| p.is_finite()) {
            self.progress = progress.clamp(0.0, 1.0);
        }
        if let Some(visible) = number::<bool>(properties, PROGRESS_VISIBLE) {
            self.progress_visible = visible;
        }
        if let Some(urgent) = number::<bool>(properties, URGENT) {
            self.urgent = urgent;
        }
    }

    fn shown(&self) -> LauncherEntry {
        LauncherEntry {
            count: self.count_visible.then_some(self.count),
            progress: self.progress_visible.then_some(self.progress),
            urgent: self.urgent,
        }
    }
}

fn number<T>(properties: &HashMap<String, OwnedVariant>, key: &str) -> Option<T>
where
    T: TryFrom<OwnedVariant>,
{
    properties.get(key)?.try_clone().ok()?.try_into().ok()
}

/// `application://org.kde.dolphin.desktop` as `org.kde.dolphin`.
fn desktop_id(uri: &str) -> Option<&str> {
    let name = uri.strip_prefix(URI_PREFIX)?;
    let id = name.strip_suffix(DESKTOP_SUFFIX).unwrap_or(name);
    (!id.is_empty()).then_some(id)
}

/// Every sender's entries, and what was last published from them.
struct Entries {
    publisher: Publisher,
    senders: BTreeMap<String, BTreeMap<String, Raw>>,
    published: Option<LauncherEntries>,
}

impl Entries {
    fn new(publisher: Publisher) -> Self {
        Self {
            publisher,
            senders: BTreeMap::new(),
            published: None,
        }
    }

    fn update(&mut self, sender: &str, uri: &str, properties: &HashMap<String, OwnedVariant>) {
        let Some(id) = desktop_id(uri) else {
            return;
        };
        self.senders
            .entry(sender.to_owned())
            .or_default()
            .entry(id.to_owned())
            .or_default()
            .apply(properties);
        self.publish();
    }

    fn sender_left(&mut self, sender: &str) {
        if self.senders.remove(sender).is_some() {
            self.publish();
        }
    }

    fn publish(&mut self) {
        let entries = self
            .senders
            .values()
            .flatten()
            .map(|(id, raw)| (id.clone(), raw.shown()))
            .collect();
        let snapshot = LauncherEntries { entries };
        if self.published.as_ref() != Some(&snapshot) {
            self.publisher.publish(topics::LAUNCHER_ENTRIES, &snapshot);
            self.published = Some(snapshot);
        }
    }
}

/// Listens on the session bus while some client wants `launcher.entries`.
pub fn feed() -> Feed {
    Feed::new(topics::LAUNCHER_ENTRIES, |context| async move {
        match Connection::session().await {
            Ok(bus) => run(bus, context).await,
            Err(error) => {
                warn!(%error, "cannot connect to the session bus; launcher.entries stays empty");
                pending().await
            }
        }
    })
}

/// As [`feed`], listening on `bus`.
pub fn feed_on(bus: Connection) -> Feed {
    Feed::new(topics::LAUNCHER_ENTRIES, move |context| run(bus, context))
}

async fn run(bus: Connection, mut context: FeedContext) {
    loop {
        until_wanted(&mut context.demand, topics::LAUNCHER_ENTRIES).await;
        // Dropping the listener removes its match rule, so nothing reaches the daemon while unwanted.
        tokio::select! {
            biased;
            () = until_unwanted(&mut context.demand, topics::LAUNCHER_ENTRIES) => {}
            () = listen(&bus, context.publisher.clone()) => {}
        }
    }
}

async fn listen(bus: &Connection, publisher: Publisher) {
    let (mut updates, mut owners) = match streams(bus).await {
        Ok(streams) => streams,
        Err(error) => {
            warn!(%error, "cannot watch launcher entries; launcher.entries stays empty");
            return pending().await;
        }
    };
    let mut entries = Entries::new(publisher);
    // Published only once the rules are in place, so an update after this cannot be missed.
    entries.publish();
    loop {
        tokio::select! {
            Some(Ok(message)) = next(&mut updates) => {
                let sender = message.header().sender().map(ToString::to_string);
                let body = message.body().deserialize::<(String, HashMap<String, OwnedVariant>)>();
                match (sender, body) {
                    (Some(sender), Ok((uri, properties))) => entries.update(&sender, &uri, &properties),
                    _ => warn!("ignored a malformed launcher entry update"),
                }
            }
            Some(owner) = next(&mut owners) => {
                if let Ok(args) = owner.args()
                    && args.name.starts_with(UNIQUE_NAME_PREFIX)
                    && args.new_owner.is_none()
                {
                    entries.sender_left(&args.name);
                }
            }
            else => return pending().await,
        }
    }
}

async fn streams(
    bus: &Connection,
) -> zbus::Result<(MessageStream, zbus::fdo::NameOwnerChangedStream)> {
    let owners = zbus::fdo::DBusProxy::new(bus)
        .await?
        .receive_name_owner_changed()
        .await?;
    let rule = MatchRule::builder()
        .msg_type(message::Type::Signal)
        .interface(INTERFACE)
        .member(MEMBER)
        .build()?;
    Ok((
        MessageStream::for_match_rule(rule, bus, None).await?,
        owners,
    ))
}
