//! The built-in sources: INMET weather alerts and dead drives. Each raises a notification through
//! `org.freedesktop.Notifications`, so it reaches whichever server owns the name.
//!
//! What a source has raised is kept in the history file, so a restart does not raise it again.
//! The sends never run on the feed's loop: when ghostd is the server, its own handler answers the
//! `Notify` through that loop, and a loop that waited for the answer would wait for itself.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

use ghost_proto::{AlertLevel, DiskSnapshot, DriveState, Urgency, WeatherAlert, WeatherAlerts};
use tokio::sync::mpsc;
use tracing::{debug, warn};
use zbus::{Connection, Variant};

use super::history::History;
use super::server::{LEVEL_HINT, WARN_LEVEL};
use crate::notify::{self, Outgoing};

const INMET_PREFIX: &str = "inmet:";
const DRIVE_PREFIX: &str = "drive:";
const UNKNOWN_BOOT: &str = "unknown";
const WEATHER_APP: &str = "WEATHER";
const WEATHER_ICON: &str = "weather-storm";
const STORAGE_APP: &str = "STORAGE";
const SUMMARY_SEPARATOR: &str = " · ";
const NEW_NOTIFICATION: u32 = 0;

/// The result of a send that carried an alert, for the loop to take.
pub struct Sent {
    alert: String,
    /// `None` when the send failed.
    id: Option<u32>,
}

/// What an alert last said, so that an unchanged one is not sent again.
#[derive(PartialEq)]
struct Shown {
    summary: String,
    body: String,
    level: AlertLevel,
}

struct Tracked {
    shown: Shown,
    /// Unknown until the first send is answered.
    id: Option<u32>,
    /// A change that came in before the id did.
    waiting: Option<Outgoing>,
}

pub struct Sources {
    /// Not the daemon's own connection, whose handlers answer the calls these sends become.
    helper: Option<Connection>,
    boot_id: String,
    tracked: HashMap<String, Tracked>,
    sent: mpsc::UnboundedSender<Sent>,
}

/// The kernel's id of this boot, which lets a drive that stays dead after a restart go unannounced
/// while one that dies in a later boot is announced again.
pub fn read_boot_id(path: &Path) -> String {
    match fs::read_to_string(path) {
        Ok(text) => text.trim().to_owned(),
        Err(error) => {
            debug!(%error, path = %path.display(), "cannot read the boot id");
            UNKNOWN_BOOT.to_owned()
        }
    }
}

fn inmet(alert: &WeatherAlert) -> Outgoing {
    let mut hints = HashMap::new();
    let urgency = match alert.level {
        AlertLevel::Crit => Urgency::Critical,
        AlertLevel::Warn => {
            hints.insert(LEVEL_HINT, Variant::from(WARN_LEVEL));
            Urgency::Normal
        }
    };
    Outgoing {
        app: WEATHER_APP.to_owned(),
        summary: format!("{}{SUMMARY_SEPARATOR}{}", alert.title, alert.severity),
        body: alert.risks.join(SUMMARY_SEPARATOR),
        urgency,
        replaces_id: NEW_NOTIFICATION,
        app_icon: WEATHER_ICON.to_owned(),
        hints,
    }
}

impl Sources {
    pub fn new(
        helper: Option<Connection>,
        boot_id: String,
        sent: mpsc::UnboundedSender<Sent>,
    ) -> Self {
        Self {
            helper,
            boot_id,
            tracked: HashMap::new(),
            sent,
        }
    }

    fn dispatch(&self, outgoing: Outgoing, alert: Option<String>) {
        let Some(helper) = self.helper.clone() else {
            debug!(
                summary = outgoing.summary,
                "no session bus; the notification is not sent"
            );
            return;
        };
        let sent = self.sent.clone();
        tokio::spawn(async move {
            let id = notify::send(&helper, &outgoing)
                .await
                .inspect_err(|error| {
                    warn!(%error, summary = outgoing.summary, "cannot send a notification");
                })
                .ok();
            if let Some(alert) = alert {
                // The loop is gone only while the daemon stops.
                let _ = sent.send(Sent { alert, id });
            }
        });
    }

    /// Raises the alerts not raised before and updates those that changed. Whether the history's
    /// record of what was raised changed.
    pub fn weather(&mut self, history: &mut History, alerts: &WeatherAlerts) -> bool {
        let present: HashSet<&str> = alerts
            .alerts
            .iter()
            .map(|alert| alert.id.as_str())
            .collect();
        let mut changed = history.retain_sources(INMET_PREFIX, |key| {
            present.contains(&key[INMET_PREFIX.len()..])
        });
        self.tracked.retain(|id, _| present.contains(id.as_str()));
        for alert in &alerts.alerts {
            let outgoing = inmet(alert);
            let shown = Shown {
                summary: outgoing.summary.clone(),
                body: outgoing.body.clone(),
                level: alert.level,
            };
            if history.mark_source_seen(&format!("{INMET_PREFIX}{}", alert.id)) {
                changed = true;
                self.tracked.insert(
                    alert.id.clone(),
                    Tracked {
                        shown,
                        id: None,
                        waiting: None,
                    },
                );
                self.dispatch(outgoing, Some(alert.id.clone()));
            } else if let Some(tracked) = self.tracked.get_mut(&alert.id)
                && tracked.shown != shown
            {
                tracked.shown = shown;
                match tracked.id {
                    Some(id) => {
                        let replacing = Outgoing {
                            replaces_id: id,
                            ..outgoing
                        };
                        self.dispatch(replacing, Some(alert.id.clone()));
                    }
                    None => tracked.waiting = Some(outgoing),
                }
            }
        }
        changed
    }

    /// Raises each dead drive not raised in this boot. Whether the history's record changed.
    pub fn disks(&mut self, history: &mut History, disks: &DiskSnapshot) -> bool {
        let mut changed = false;
        for drive in disks
            .drives
            .iter()
            .filter(|drive| drive.state == DriveState::Dead)
        {
            let key = format!("{DRIVE_PREFIX}{}:{}", drive.id, self.boot_id);
            if history.mark_source_seen(&key) {
                changed = true;
                self.dispatch(
                    Outgoing {
                        app: STORAGE_APP.to_owned(),
                        summary: format!(
                            "{}{SUMMARY_SEPARATOR}CONTROLLER STATE DEAD",
                            drive.model.to_uppercase()
                        ),
                        body: String::new(),
                        urgency: Urgency::Critical,
                        replaces_id: NEW_NOTIFICATION,
                        app_icon: String::new(),
                        hints: HashMap::new(),
                    },
                    None,
                );
            }
        }
        changed
    }

    /// Takes the answer to a send: remembers the id, and sends what changed in the meantime.
    pub fn sent(&mut self, sent: Sent) {
        let Some(tracked) = self.tracked.get_mut(&sent.alert) else {
            return;
        };
        let Some(id) = sent.id else {
            if tracked.id.is_none() {
                self.tracked.remove(&sent.alert);
            }
            return;
        };
        tracked.id = Some(id);
        if let Some(waiting) = tracked.waiting.take() {
            let replacing = Outgoing {
                replaces_id: id,
                ..waiting
            };
            self.dispatch(replacing, Some(sent.alert));
        }
    }
}
