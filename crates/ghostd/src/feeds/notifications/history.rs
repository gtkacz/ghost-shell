//! The notification history: ids, replacement, the cap, read state and the file format. Pure, so
//! the server and the feed decide when to call it and when to persist.

use std::borrow::Cow;
use std::collections::{BTreeSet, HashSet};

use ghost_proto::{Notification, NotificationAction, NotificationLevel, Urgency};
use serde::de::Error as _;
use serde::{Deserialize, Serialize};

/// Calls that repeat the previous accepted one inside this window are refused.
const DEDUPE_WINDOW_MS: u64 = 1_000;
const MS_PER_S: u64 = 1_000;
const FILE_VERSION: u32 = 1;
/// `replaces_id = 0` means "new" on the wire, so 0 is never a record's id.
const FIRST_ID: u32 = 1;

/// One `Notify` call after the server has parsed and sanitised it.
#[derive(Debug, Clone, Default)]
pub struct Incoming {
    pub app: String,
    pub desktop_entry: String,
    pub app_icon: String,
    pub image: String,
    pub summary: String,
    pub body: String,
    pub body_text: String,
    pub urgency: Urgency,
    pub level: NotificationLevel,
    pub actions: Vec<NotificationAction>,
    pub has_default: bool,
    pub pid: u32,
    pub resident: bool,
    pub transient: bool,
    pub replaces_id: u32,
}

/// A call refused for repeating the previous one within the dedupe window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Excess;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Outcome {
    pub id: u32,
    pub replaced: bool,
    /// False for a transient call: it has an id but no place in the history.
    pub kept: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Counts {
    pub unread: u32,
    pub unread_crit: u32,
}

/// What a restart loses on purpose: the window only matters for the seconds after a call.
#[derive(Debug)]
struct LastCall {
    at_ms: u64,
    incoming: Incoming,
}

#[derive(Debug)]
pub struct History {
    /// Newest first.
    items: Vec<Notification>,
    dnd: bool,
    next_id: u32,
    last: Option<LastCall>,
    /// What the built-in sources have raised, so that a restart does not raise it again.
    seen_sources: BTreeSet<String>,
}

#[derive(Serialize, Deserialize)]
struct Stored<'a> {
    version: u32,
    next_id: u32,
    dnd: bool,
    items: Cow<'a, [Notification]>,
    // Absent from files written before the sources existed.
    #[serde(default)]
    seen_sources: Cow<'a, BTreeSet<String>>,
}

impl Default for History {
    fn default() -> Self {
        Self::new()
    }
}

/// The cap never drops these: the user has not seen them yet.
fn is_unread_critical(record: &Notification) -> bool {
    record.urgency == Urgency::Critical && !record.read
}

fn repeats(last: &Incoming, incoming: &Incoming) -> bool {
    last.app == incoming.app
        && last.summary == incoming.summary
        && last.body == incoming.body
        && last.actions == incoming.actions
        && last.urgency == incoming.urgency
}

pub(super) fn record(id: u32, incoming: Incoming, arrived_s: u64, held: bool) -> Notification {
    Notification {
        id,
        app: incoming.app,
        desktop_entry: incoming.desktop_entry,
        app_icon: incoming.app_icon,
        image: incoming.image,
        summary: incoming.summary,
        body: incoming.body,
        body_text: incoming.body_text,
        urgency: incoming.urgency,
        level: incoming.level,
        actions: incoming.actions,
        has_default: incoming.has_default,
        pid: incoming.pid,
        arrived_s,
        read: false,
        resident: incoming.resident,
        held,
    }
}

impl History {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            dnd: false,
            next_id: FIRST_ID,
            last: None,
            seen_sources: BTreeSet::new(),
        }
    }

    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        let stored: Stored = serde_json::from_str(json)?;
        if stored.version != FILE_VERSION {
            return Err(serde_json::Error::custom(format!(
                "history file version {} is not {FILE_VERSION}",
                stored.version
            )));
        }
        let items = stored.items.into_owned();
        // Colliding ids would make a close or a replacement hit the wrong record, so the whole
        // file is refused. `u32::MAX` is refused too so that "past every id" cannot overflow.
        let mut seen = HashSet::new();
        if let Some(bad) = items
            .iter()
            .find(|item| item.id == 0 || item.id == u32::MAX || !seen.insert(item.id))
        {
            return Err(serde_json::Error::custom(format!(
                "history file has an unusable id {}",
                bad.id
            )));
        }
        // A stale `next_id` must not hand out an id that is already in use.
        let past_every_id = items
            .iter()
            .map(|item| item.id + 1)
            .max()
            .unwrap_or(FIRST_ID);
        Ok(Self {
            items,
            dnd: stored.dnd,
            next_id: stored.next_id.max(past_every_id),
            last: None,
            seen_sources: stored.seen_sources.into_owned(),
        })
    }

    pub fn to_json(&self) -> String {
        let stored = Stored {
            version: FILE_VERSION,
            next_id: self.next_id,
            dnd: self.dnd,
            items: Cow::Borrowed(&self.items),
            seen_sources: Cow::Borrowed(&self.seen_sources),
        };
        serde_json::to_string(&stored).expect("strings and numbers always serialise")
    }

    /// Takes one call. A known `replaces_id` rewrites that record in place, keeping its arrival
    /// and `held`; anything else gets a fresh id. `limit` caps the records that are not unread
    /// criticals.
    pub fn notify(
        &mut self,
        incoming: Incoming,
        now_ms: u64,
        held: bool,
        limit: usize,
    ) -> Result<Outcome, Excess> {
        if let Some(last) = &self.last
            && now_ms.saturating_sub(last.at_ms) < DEDUPE_WINDOW_MS
            && repeats(&last.incoming, &incoming)
        {
            return Err(Excess);
        }
        self.last = Some(LastCall {
            at_ms: now_ms,
            incoming: incoming.clone(),
        });
        let arrived_s = now_ms / MS_PER_S;
        let transient = incoming.transient;
        let existing = self
            .items
            .iter()
            .position(|item| incoming.replaces_id != 0 && item.id == incoming.replaces_id)
            .filter(|_| !transient);
        let outcome = match existing {
            Some(at) => {
                let old = &self.items[at];
                let rewritten = record(old.id, incoming, old.arrived_s, old.held);
                let id = rewritten.id;
                self.items[at] = rewritten;
                Outcome {
                    id,
                    replaced: true,
                    kept: true,
                }
            }
            None => {
                let id = self.take_id();
                if !transient {
                    self.items.insert(0, record(id, incoming, arrived_s, held));
                }
                Outcome {
                    id,
                    replaced: false,
                    kept: !transient,
                }
            }
        };
        self.enforce(limit);
        Ok(outcome)
    }

    fn take_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id = id.checked_add(1).unwrap_or(FIRST_ID);
        id
    }

    fn enforce(&mut self, limit: usize) {
        let capped = self
            .items
            .iter()
            .filter(|item| !is_unread_critical(item))
            .count();
        let dropped: Vec<u32> = self
            .items
            .iter()
            .rev()
            .filter(|item| !is_unread_critical(item))
            .take(capped.saturating_sub(limit))
            .map(|item| item.id)
            .collect();
        self.items.retain(|item| !dropped.contains(&item.id));
    }

    /// Removes the record, critical or not: dismissing is explicit.
    pub fn close(&mut self, id: u32) -> bool {
        let before = self.items.len();
        self.items.retain(|item| item.id != id);
        self.items.len() != before
    }

    pub fn clear(&mut self) {
        self.items.retain(is_unread_critical);
    }

    /// Marks the listed records read, or every record for `None`.
    pub fn mark_read(&mut self, ids: Option<&[u32]>) {
        for item in &mut self.items {
            if ids.is_none_or(|ids| ids.contains(&item.id)) {
                item.read = true;
            }
        }
    }

    /// Points the record at its picture once the file exists; false when the record is gone.
    pub fn set_image(&mut self, id: u32, image: String) -> bool {
        let Some(item) = self.items.iter_mut().find(|item| item.id == id) else {
            return false;
        };
        item.image = image;
        true
    }

    pub fn set_dnd(&mut self, on: bool) {
        self.dnd = on;
    }

    pub fn dnd(&self) -> bool {
        self.dnd
    }

    /// Newest first.
    pub fn items(&self) -> &[Notification] {
        &self.items
    }

    pub fn get(&self, id: u32) -> Option<&Notification> {
        self.items.iter().find(|item| item.id == id)
    }

    pub fn counts(&self) -> Counts {
        let unread = || self.items.iter().filter(|item| !item.read);
        Counts {
            unread: unread().count() as u32,
            unread_crit: unread()
                .filter(|item| item.urgency == Urgency::Critical)
                .count() as u32,
        }
    }

    /// Notes that a source raised `key`; whether it is new.
    pub fn mark_source_seen(&mut self, key: &str) -> bool {
        self.seen_sources.insert(key.to_owned())
    }

    /// Takes `key` back, so that the source raises it again at its next update; whether it was there.
    pub fn forget_source(&mut self, key: &str) -> bool {
        self.seen_sources.remove(key)
    }

    /// Forgets the keys that start with `prefix` and that `keep` refuses; whether any went.
    pub fn retain_sources(&mut self, prefix: &str, keep: impl Fn(&str) -> bool) -> bool {
        let before = self.seen_sources.len();
        self.seen_sources
            .retain(|key| !key.starts_with(prefix) || keep(key));
        self.seen_sources.len() != before
    }

    /// The unread records that arrived after `since_s`.
    pub fn unread_since(&self, since_s: u64) -> u32 {
        self.items
            .iter()
            .filter(|item| !item.read && item.arrived_s > since_s)
            .count() as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ghost_proto::{NotificationAction, Urgency};

    const LIMIT: usize = 200;

    fn incoming(summary: &str) -> Incoming {
        Incoming {
            app: "app".into(),
            summary: summary.into(),
            ..Incoming::default()
        }
    }

    fn critical(summary: &str) -> Incoming {
        Incoming {
            urgency: Urgency::Critical,
            ..incoming(summary)
        }
    }

    fn ids(history: &History) -> Vec<u32> {
        history.items().iter().map(|item| item.id).collect()
    }

    #[test]
    fn new_record_gets_the_next_id_and_is_unread() {
        let mut history = History::new();
        let first = history.notify(incoming("a"), 5_000, false, LIMIT).unwrap();
        let second = history.notify(incoming("b"), 9_000, false, LIMIT).unwrap();
        assert_eq!((first.id, second.id), (1, 2));
        assert!(!first.replaced && first.kept);
        let record = history.get(1).unwrap();
        assert!(!record.read);
        assert_eq!(record.arrived_s, 5);
        assert_eq!(ids(&history), [2, 1]);
    }

    #[test]
    fn replacement_keeps_arrival_and_resets_read() {
        let mut history = History::new();
        history.notify(incoming("a"), 5_000, false, LIMIT).unwrap();
        history.mark_read(None);
        let replacement = Incoming {
            replaces_id: 1,
            ..incoming("b")
        };
        let outcome = history.notify(replacement, 50_000, false, LIMIT).unwrap();
        assert_eq!((outcome.id, outcome.replaced), (1, true));
        let record = history.get(1).unwrap();
        assert_eq!(record.summary, "b");
        assert_eq!(record.arrived_s, 5);
        assert!(!record.read);
        assert_eq!(history.items().len(), 1);
    }

    #[test]
    fn unknown_replaces_id_is_a_new_record() {
        let mut history = History::new();
        let replacement = Incoming {
            replaces_id: 77,
            ..incoming("a")
        };
        let outcome = history.notify(replacement, 0, false, LIMIT).unwrap();
        assert_eq!((outcome.id, outcome.replaced), (1, false));
    }

    #[test]
    fn identical_notify_within_a_second_is_refused() {
        let mut history = History::new();
        history.notify(incoming("a"), 10_000, false, LIMIT).unwrap();
        assert_eq!(
            history.notify(incoming("a"), 10_999, false, LIMIT),
            Err(Excess)
        );
        assert!(history.notify(incoming("a"), 11_001, false, LIMIT).is_ok());
        assert_eq!(history.items().len(), 2);
    }

    #[test]
    fn a_differing_call_is_not_refused_and_a_refusal_does_not_move_the_window() {
        let mut history = History::new();
        history.notify(incoming("a"), 10_000, false, LIMIT).unwrap();
        assert!(history.notify(incoming("b"), 10_100, false, LIMIT).is_ok());
        assert!(history.notify(incoming("b"), 10_200, false, LIMIT).is_err());
        let with_action = Incoming {
            actions: vec![NotificationAction {
                key: "k".into(),
                label: "K".into(),
            }],
            ..incoming("b")
        };
        assert!(history.notify(with_action, 10_300, false, LIMIT).is_ok());
        assert!(history.notify(critical("b"), 10_400, false, LIMIT).is_ok());
    }

    #[test]
    fn cap_keeps_the_newest_and_every_unread_critical() {
        let mut history = History::new();
        history.notify(critical("old"), 0, false, 3).unwrap();
        for (n, at) in (1..=4).zip([2_000, 4_000, 6_000, 8_000]) {
            history
                .notify(incoming(&n.to_string()), at, false, 3)
                .unwrap();
        }
        assert_eq!(ids(&history), [5, 4, 3, 1]);
    }

    #[test]
    fn a_read_critical_is_dropped_like_any_other() {
        let mut history = History::new();
        history.notify(critical("old"), 0, false, 2).unwrap();
        history.mark_read(Some(&[1]));
        history.notify(incoming("a"), 2_000, false, 2).unwrap();
        history.notify(incoming("b"), 4_000, false, 2).unwrap();
        assert_eq!(ids(&history), [3, 2]);
    }

    #[test]
    fn transient_is_not_kept() {
        let mut history = History::new();
        let transient = Incoming {
            transient: true,
            ..incoming("t")
        };
        let outcome = history.notify(transient, 0, false, LIMIT).unwrap();
        assert!(!outcome.kept);
        assert!(history.items().is_empty());
        let next = history.notify(incoming("n"), 5_000, false, LIMIT).unwrap();
        assert_eq!(next.id, outcome.id + 1);
    }

    #[test]
    fn clear_keeps_unread_criticals() {
        let mut history = History::new();
        history.notify(critical("c1"), 0, false, LIMIT).unwrap();
        history.notify(critical("c2"), 2_000, false, LIMIT).unwrap();
        history.notify(incoming("n"), 4_000, false, LIMIT).unwrap();
        history.mark_read(Some(&[2]));
        history.clear();
        assert_eq!(ids(&history), [1]);
    }

    #[test]
    fn close_removes_any_record() {
        let mut history = History::new();
        history.notify(critical("c"), 0, false, LIMIT).unwrap();
        assert!(history.close(1));
        assert!(!history.close(1));
        assert!(history.items().is_empty());
    }

    #[test]
    fn mark_read_all_and_by_ids() {
        let mut history = History::new();
        for (n, at) in (1..=3).zip([0, 2_000, 4_000]) {
            history
                .notify(incoming(&n.to_string()), at, false, LIMIT)
                .unwrap();
        }
        history.mark_read(Some(&[1, 99]));
        assert_eq!(history.counts().unread, 2);
        history.mark_read(None);
        assert_eq!(history.counts().unread, 0);
    }

    #[test]
    fn counts_follow_read_and_level() {
        let mut history = History::new();
        history.notify(critical("c"), 0, false, LIMIT).unwrap();
        history.notify(incoming("n"), 2_000, false, LIMIT).unwrap();
        let counts = history.counts();
        assert_eq!((counts.unread, counts.unread_crit), (2, 1));
        history.mark_read(Some(&[1]));
        let counts = history.counts();
        assert_eq!((counts.unread, counts.unread_crit), (1, 0));
    }

    #[test]
    fn unread_since_counts_arrivals_after_the_moment() {
        let mut history = History::new();
        for (n, at) in (1..=3).zip([10_000, 20_000, 30_000]) {
            history
                .notify(incoming(&n.to_string()), at, false, LIMIT)
                .unwrap();
        }
        history.mark_read(Some(&[3]));
        assert_eq!(history.unread_since(10), 1);
        assert_eq!(history.unread_since(0), 2);
        assert_eq!(history.unread_since(30), 0);
    }

    #[test]
    fn json_round_trips_records_dnd_and_next_id() {
        let mut history = History::new();
        history.notify(incoming("a"), 1_000, true, LIMIT).unwrap();
        history.notify(critical("b"), 3_000, false, LIMIT).unwrap();
        history.mark_read(Some(&[1]));
        history.set_dnd(true);
        history.close(2);
        let mut restored = History::from_json(&history.to_json()).unwrap();
        assert_eq!(restored.items(), history.items());
        assert!(restored.dnd());
        let next = restored.notify(incoming("c"), 9_000, false, LIMIT).unwrap();
        assert_eq!(next.id, 3);
    }

    fn file(version: u32, next_id: u32, ids: &[u32]) -> String {
        let items: Vec<_> = ids
            .iter()
            .map(|&id| Notification {
                id,
                ..Notification::default()
            })
            .collect();
        serde_json::to_string(&Stored {
            version,
            next_id,
            dnd: false,
            items: Cow::Owned(items),
            seen_sources: Cow::default(),
        })
        .unwrap()
    }

    #[test]
    fn seen_sources_survive_a_round_trip_and_default_when_absent() {
        let mut history = History::new();
        assert!(history.mark_source_seen("inmet:1"));
        assert!(!history.mark_source_seen("inmet:1"));

        let mut restored = History::from_json(&history.to_json()).unwrap();
        assert!(!restored.mark_source_seen("inmet:1"));
        let mut old =
            History::from_json("{\"version\":1,\"next_id\":1,\"dnd\":false,\"items\":[]}").unwrap();
        assert!(old.mark_source_seen("inmet:1"));
    }

    #[test]
    fn retain_sources_only_touches_its_prefix() {
        let mut history = History::new();
        history.mark_source_seen("inmet:1");
        history.mark_source_seen("inmet:2");
        history.mark_source_seen("drive:a:boot");

        assert!(history.retain_sources("inmet:", |key| key == "inmet:2"));

        assert!(history.mark_source_seen("inmet:1"));
        assert!(!history.mark_source_seen("inmet:2"));
        assert!(!history.mark_source_seen("drive:a:boot"));
        assert!(!history.retain_sources("inmet:", |_| true));
    }

    #[test]
    fn future_version_is_rejected() {
        assert!(
            History::from_json("{\"version\":2,\"next_id\":1,\"dnd\":false,\"items\":[]}").is_err()
        );
        assert!(History::from_json(&file(2, 1, &[])).is_err());
    }

    #[test]
    fn stale_next_id_is_lifted_past_stored_ids() {
        let mut history = History::from_json(&file(1, 1, &[7, 3])).unwrap();
        let outcome = history.notify(incoming("a"), 0, false, LIMIT).unwrap();
        assert_eq!(outcome.id, 8);
        let mut empty = History::from_json(&file(1, 0, &[])).unwrap();
        assert_eq!(empty.notify(incoming("a"), 0, false, LIMIT).unwrap().id, 1);
    }

    #[test]
    fn duplicate_or_zero_ids_are_rejected() {
        assert!(History::from_json(&file(1, 5, &[2, 2])).is_err());
        assert!(History::from_json(&file(1, 5, &[0, 1])).is_err());
    }

    #[test]
    fn max_id_does_not_overflow() {
        assert!(History::from_json(&file(1, u32::MAX, &[u32::MAX])).is_err());
        let mut history = History::from_json(&file(1, 1, &[u32::MAX - 1])).unwrap();
        let outcome = history.notify(incoming("a"), 0, false, LIMIT).unwrap();
        assert_eq!(outcome.id, u32::MAX);
        let after = history.notify(incoming("b"), 2_000, false, LIMIT).unwrap();
        assert_eq!(after.id, FIRST_ID);
    }

    #[test]
    fn corrupt_json_is_an_error() {
        assert!(History::from_json("{").is_err());
    }

    #[test]
    fn held_is_stamped_once_and_never_changes_on_replacement() {
        let mut history = History::new();
        history.notify(incoming("a"), 0, true, LIMIT).unwrap();
        assert!(history.get(1).unwrap().held);
        let replacement = Incoming {
            replaces_id: 1,
            ..incoming("b")
        };
        history.notify(replacement, 5_000, false, LIMIT).unwrap();
        assert!(history.get(1).unwrap().held);
    }
}
