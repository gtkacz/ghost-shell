//! A fixed notification history that answers the shell's calls by editing its own copy, so the
//! bell, the toasts and the centre have data without a session bus or a real server.
//!
//! `GHOST_FAKE_NOTIFY_EVERY_S=<n>` adds a record every n seconds, `GHOST_FAKE_NOTIFY_DND=1` starts
//! with do-not-disturb on and `GHOST_FAKE_NOTIFY_EMPTY=1` starts with no records.

use std::env;
use std::future::pending;
use std::time::Duration;

use ghost_proto::{
    Notification, NotificationAction, NotificationLevel, NotificationServer, NotificationsSnapshot,
    Urgency, calls, topics,
};
use serde_json::Value;
use tokio::sync::broadcast::error::RecvError;
use tokio::time::{Instant, Interval, MissedTickBehavior, interval_at};
use tracing::warn;

use crate::feeds::unix_now_s;
use crate::server::{Call, Feed};

const EVERY_S_ENV: &str = "GHOST_FAKE_NOTIFY_EVERY_S";
const DND_ENV: &str = "GHOST_FAKE_NOTIFY_DND";
const EMPTY_ENV: &str = "GHOST_FAKE_NOTIFY_EMPTY";
const SUPPRESSED_DND: &str = "dnd";
const OPEN_KEY: &str = "open";
const DEFAULT_KEY: &str = "default";
/// The daemon's own default for `notifications.history_limit`.
const HISTORY_LIMIT: usize = 200;
/// How many of the newest mockup records are unread.
const UNREAD: usize = 3;

/// The reference mockup's `NOTIFS`, newest first: app, summary, body and age in seconds.
const MOCKUP: [(&str, &str, &str, u64); 18] = [
    (
        "STORAGE",
        "nvme1 \u{b7} WD Green 960G",
        "controller state DEAD",
        60,
    ),
    ("WEATHER", "INMET: storm warning (orange)", "00:42:10", 240),
    ("DISCORD", "#rice-share", "3 new replies to your post", 540),
    ("SPOTIFY", "Now playing", "Derezzed \u{2014} Daft Punk", 720),
    (
        "DNF",
        "16 package updates available (kernel 7.2.12)",
        "",
        840,
    ),
    ("KDE CONNECT", "Phone battery at 15%", "", 1320),
    ("DUALSENSE", "Controller battery at 20%", "", 2460),
    ("STEAM", "Download complete", "", 3600),
    ("THORIUM", "Download finished: wrayth-main.zip", "", 3900),
    ("TAILSCALE", "Connected to tailnet", "", 7200),
    ("FIREWALLD", "Zone changed to home", "", 7500),
    ("DISCORD", "New direct message", "", 10_800),
    (
        "SYSTEMD",
        "User unit ghost-shell.service restarted",
        "",
        11_100,
    ),
    ("DNF", "Update check finished", "", 14_400),
    ("THORIUM", "Download finished: edex-ui.AppImage", "", 18_000),
    ("KDE CONNECT", "Clipboard received from phone", "", 18_300),
    ("STEAM", "Friend is now online", "", 21_600),
    ("BLUETOOTH", "DualSense connected", "", 21_900),
];
/// Which mockup records are critical and warn, and which offers an action.
const CRITICAL_AT: usize = 0;
const WARN_AT: usize = 1;
const ACTION_AT: usize = 2;
const ACTION_ENTRY: &str = "discord";

/// What arrives in turn under `GHOST_FAKE_NOTIFY_EVERY_S`: app, summary, body and level.
const ARRIVALS: [(&str, &str, &str, NotificationLevel); 5] = [
    (
        "DISCORD",
        "#rice-share",
        "new reply",
        NotificationLevel::None,
    ),
    ("STEAM", "Update queued", "", NotificationLevel::None),
    (
        "WEATHER",
        "INMET: storm warning (orange)",
        "00:42:10",
        NotificationLevel::Warn,
    ),
    (
        "KDE CONNECT",
        "Phone battery at 14%",
        "",
        NotificationLevel::None,
    ),
    (
        "STORAGE",
        "nvme1 \u{b7} WD Green 960G",
        "controller state DEAD",
        NotificationLevel::Crit,
    ),
];

struct Settings {
    every: Option<Duration>,
    dnd: bool,
    empty: bool,
}

impl Settings {
    fn from_env() -> Self {
        let flag = |name| env::var(name).is_ok_and(|value| value == "1");
        Self {
            every: env::var(EVERY_S_ENV)
                .ok()
                .and_then(|value| value.parse::<u64>().ok())
                .filter(|&seconds| seconds > 0)
                .map(Duration::from_secs),
            dnd: flag(DND_ENV),
            empty: flag(EMPTY_ENV),
        }
    }
}

struct Fake {
    /// Newest first.
    items: Vec<Notification>,
    dnd: bool,
    next_id: u32,
    arrivals: usize,
}

fn record(
    id: u32,
    (app, summary, body): (&str, &str, &str),
    level: NotificationLevel,
    at_s: u64,
) -> Notification {
    let urgency = if level == NotificationLevel::Crit {
        Urgency::Critical
    } else {
        Urgency::Normal
    };
    Notification {
        id,
        app: app.to_owned(),
        summary: summary.to_owned(),
        body: body.to_owned(),
        body_text: body.to_owned(),
        urgency,
        level,
        arrived_s: at_s,
        ..Notification::default()
    }
}

impl Fake {
    fn new(now_s: u64, settings: &Settings) -> Self {
        let total = MOCKUP.len();
        let items = if settings.empty {
            Vec::new()
        } else {
            MOCKUP
                .iter()
                .enumerate()
                .map(|(at, &(app, summary, body, age_s))| {
                    let level = match at {
                        CRITICAL_AT => NotificationLevel::Crit,
                        WARN_AT => NotificationLevel::Warn,
                        _ => NotificationLevel::None,
                    };
                    let mut item = record(
                        (total - at) as u32,
                        (app, summary, body),
                        level,
                        now_s.saturating_sub(age_s),
                    );
                    item.read = at >= UNREAD;
                    if at == ACTION_AT {
                        item.desktop_entry = ACTION_ENTRY.to_owned();
                        item.actions = vec![NotificationAction {
                            key: OPEN_KEY.to_owned(),
                            label: "Open".to_owned(),
                        }];
                        item.has_default = true;
                    }
                    item
                })
                .collect()
        };
        Self {
            items,
            dnd: settings.dnd,
            next_id: total as u32 + 1,
            arrivals: 0,
        }
    }

    fn snapshot(&self) -> NotificationsSnapshot {
        let unread = || self.items.iter().filter(|item| !item.read);
        NotificationsSnapshot {
            server: NotificationServer::GhostShell,
            items: self.items.clone(),
            unread: unread().count() as u32,
            unread_crit: unread()
                .filter(|item| item.urgency == Urgency::Critical)
                .count() as u32,
            dnd: self.dnd,
            inhibited: false,
            suppressed: if self.dnd { SUPPRESSED_DND } else { "" }.to_owned(),
        }
    }

    /// Adds the next of [`ARRIVALS`] as the newest, unread record.
    fn arrive(&mut self, now_s: u64) {
        let (app, summary, body, level) = ARRIVALS[self.arrivals % ARRIVALS.len()];
        self.arrivals += 1;
        self.items
            .insert(0, record(self.next_id, (app, summary, body), level, now_s));
        self.next_id += 1;
        self.items.truncate(HISTORY_LIMIT);
    }

    /// Edits the copy as the real feed would; whether anything changed.
    fn apply(&mut self, call: &Call) -> bool {
        let before = (self.items.clone(), self.dnd);
        let id = call
            .args
            .get("id")
            .and_then(Value::as_u64)
            .and_then(|id| u32::try_from(id).ok());
        match (call.method.as_str(), id) {
            (calls::NOTIFICATIONS_INVOKE, Some(id)) => self.invoke(id, &call.args),
            (calls::NOTIFICATIONS_DISMISS, Some(id)) => self.items.retain(|item| item.id != id),
            (calls::NOTIFICATIONS_CLEAR, _) => self.items.clear(),
            (calls::NOTIFICATIONS_READ, _) => self.read(&call.args),
            (calls::NOTIFICATIONS_DND, _) => {
                if let Some(on) = call.args.get("on").and_then(Value::as_bool) {
                    self.dnd = on;
                }
            }
            _ => {}
        }
        before != (self.items.clone(), self.dnd)
    }

    /// Only for a key the record offered, as the real feed does; a resident record stays.
    fn invoke(&mut self, id: u32, args: &Value) {
        let Some(key) = args.get("key").and_then(Value::as_str) else {
            return;
        };
        let Some(item) = self.items.iter().find(|item| item.id == id) else {
            return;
        };
        let offered = if key == DEFAULT_KEY {
            item.has_default
        } else {
            item.actions.iter().any(|action| action.key == key)
        };
        if offered && !item.resident {
            self.items.retain(|item| item.id != id);
        }
    }

    fn read(&mut self, args: &Value) {
        let ids: Option<Vec<u64>> = args
            .get("ids")
            .and_then(Value::as_array)
            .map(|ids| ids.iter().filter_map(Value::as_u64).collect());
        let all = args.get("all").and_then(Value::as_bool) == Some(true);
        for item in &mut self.items {
            if all
                || ids
                    .as_ref()
                    .is_some_and(|ids| ids.contains(&u64::from(item.id)))
            {
                item.read = true;
            }
        }
    }
}

/// Resolves at the next arrival, or never without `GHOST_FAKE_NOTIFY_EVERY_S`.
async fn arrival(timer: &mut Option<Interval>) {
    match timer {
        Some(timer) => {
            timer.tick().await;
        }
        None => pending::<()>().await,
    }
}

pub fn feed() -> Feed {
    Feed::new(topics::NOTIFICATIONS, |mut context| async move {
        let settings = Settings::from_env();
        let mut fake = Fake::new(unix_now_s(), &settings);
        let mut timer = settings.every.map(|every| {
            let mut timer = interval_at(Instant::now() + every, every);
            timer.set_missed_tick_behavior(MissedTickBehavior::Skip);
            timer
        });
        context
            .publisher
            .publish(topics::NOTIFICATIONS, &fake.snapshot());
        loop {
            tokio::select! {
                received = context.calls.recv() => match received {
                    Ok(call) => {
                        if fake.apply(&call) {
                            context.publisher.publish(topics::NOTIFICATIONS, &fake.snapshot());
                        }
                    }
                    Err(RecvError::Lagged(missed)) => {
                        warn!(missed, "fake notification calls were dropped: the feed fell behind");
                    }
                    Err(RecvError::Closed) => pending::<()>().await,
                },
                () = arrival(&mut timer) => {
                    fake.arrive(unix_now_s());
                    context.publisher.publish(topics::NOTIFICATIONS, &fake.snapshot());
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use ghost_proto::ResolvedConfig;
    use serde_json::json;
    use tokio::sync::{broadcast, mpsc, watch};
    use tokio::time::timeout;

    use super::*;
    use crate::server::{FeedContext, Publisher};

    const WAIT: Duration = Duration::from_secs(5);

    const NOW_S: u64 = 1_792_600_000;

    fn fake() -> Fake {
        Fake::new(
            NOW_S,
            &Settings {
                every: None,
                dnd: false,
                empty: false,
            },
        )
    }

    fn call(method: &str, args: Value) -> Call {
        Call {
            method: method.to_owned(),
            args,
        }
    }

    #[test]
    fn the_mockup_history_is_newest_first_with_three_unread() {
        let snapshot = fake().snapshot();
        assert_eq!(snapshot.items.len(), 18);
        assert!(
            snapshot
                .items
                .windows(2)
                .all(|pair| pair[0].arrived_s > pair[1].arrived_s && pair[0].id > pair[1].id)
        );
        assert_eq!((snapshot.unread, snapshot.unread_crit), (3, 1));
        assert_eq!(snapshot.server, NotificationServer::GhostShell);
        let levels: Vec<_> = snapshot.items.iter().map(|item| item.level).collect();
        assert_eq!(levels[0], NotificationLevel::Crit);
        assert_eq!(levels[1], NotificationLevel::Warn);
        assert_eq!(
            levels
                .iter()
                .filter(|&&level| level != NotificationLevel::None)
                .count(),
            2
        );
        let with_actions = snapshot
            .items
            .iter()
            .filter(|item| item.has_default)
            .count();
        assert_eq!(with_actions, 1);
    }

    #[test]
    fn the_environment_starts_it_empty_or_quiet() {
        let settings = Settings {
            every: None,
            dnd: true,
            empty: true,
        };
        let snapshot = Fake::new(NOW_S, &settings).snapshot();
        assert!(snapshot.items.is_empty());
        assert_eq!((snapshot.dnd, snapshot.suppressed.as_str()), (true, "dnd"));
    }

    #[test]
    fn an_arrival_is_the_newest_unread_record_and_the_set_cycles() {
        let mut fake = fake();
        for _ in 0..ARRIVALS.len() {
            fake.arrive(NOW_S + 1);
        }
        assert_eq!(fake.items[0].level, NotificationLevel::Crit);
        assert_eq!(fake.items[2].level, NotificationLevel::Warn);
        assert_eq!(fake.items[0].id, 18 + ARRIVALS.len() as u32);
        fake.arrive(NOW_S + 2);
        assert_eq!(fake.items[0].app, ARRIVALS[0].0);
        assert_eq!(fake.snapshot().unread, 3 + 6);
    }

    #[test]
    fn dismiss_clear_read_and_dnd_edit_the_copy() {
        let mut fake = fake();
        assert!(fake.apply(&call(calls::NOTIFICATIONS_DISMISS, json!({"id": 18}))));
        assert_eq!(fake.items.len(), 17);
        assert!(!fake.apply(&call(calls::NOTIFICATIONS_DISMISS, json!({"id": 999}))));
        assert!(fake.apply(&call(calls::NOTIFICATIONS_READ, json!({"ids": [16]}))));
        assert_eq!(fake.snapshot().unread, 1);
        assert!(fake.apply(&call(calls::NOTIFICATIONS_READ, json!({"all": true}))));
        assert_eq!(fake.snapshot().unread, 0);
        assert!(fake.apply(&call(calls::NOTIFICATIONS_DND, json!({"on": true}))));
        assert_eq!(fake.snapshot().suppressed, "dnd");
        assert!(fake.apply(&call(calls::NOTIFICATIONS_CLEAR, json!({}))));
        assert!(fake.items.is_empty());
    }

    #[test]
    fn invoke_closes_the_record_only_for_an_offered_key() {
        let mut fake = fake();
        let id = 16;
        assert!(!fake.apply(&call(
            calls::NOTIFICATIONS_INVOKE,
            json!({"id": id, "key": "nope"})
        )));
        assert!(!fake.apply(&call(
            calls::NOTIFICATIONS_INVOKE,
            json!({"id": 18, "key": "default"})
        )));
        assert!(fake.apply(&call(
            calls::NOTIFICATIONS_INVOKE,
            json!({"id": id, "key": "open"})
        )));
        assert!(fake.items.iter().all(|item| item.id != id));
    }

    #[tokio::test]
    async fn the_feed_republishes_after_a_call() {
        let (published, mut values) = mpsc::unbounded_channel();
        let (_demand, demand) = watch::channel(BTreeSet::new());
        let (_config, config) = watch::channel(ResolvedConfig::default());
        let (calls, incoming) = broadcast::channel(8);
        let context = FeedContext {
            publisher: Publisher::new(move |topic, data| {
                assert_eq!(topic, topics::NOTIFICATIONS);
                published.send(data).unwrap();
            }),
            demand,
            config,
            calls: incoming,
            topics: broadcast::channel(1).1,
        };
        let _running = tokio::spawn(feed().run(context));

        let first = timeout(WAIT, values.recv()).await.unwrap().unwrap();
        assert_eq!(first["items"].as_array().unwrap().len(), 18);
        calls
            .send(call(calls::NOTIFICATIONS_CLEAR, json!({})))
            .unwrap();
        let second = timeout(WAIT, values.recv()).await.unwrap().unwrap();
        assert_eq!(second["items"].as_array().unwrap().len(), 0);
        assert_eq!(second["server"], "ghost-shell");
    }
}
