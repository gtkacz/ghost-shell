//! Claude Code's session files, status-line payload and settings.json.

use ghost_proto::{AgentSession, AgentState, ProviderUsage};
use serde::Deserialize;

use super::{FIVE_HOURS_S, SEVEN_DAYS_S, unavailable, window_as_of};

pub const ID: &str = "claude";
pub const LABEL: &str = "Claude Code";
pub const TAP_COMMAND_SUFFIX: &str = "ghostd claude-statusline-tap";

const NO_READING: &str = "no reading yet";
const SESSION_ID_PREFIX_LEN: usize = 8;
const BACKGROUND_KINDS: [&str; 2] = ["daemon", "daemon-worker"];

pub struct ClaudeSession {
    pub session: AgentSession,
    /// The process start time Claude recorded, to tell a live pid from a recycled one.
    pub proc_start: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SessionFile {
    pid: u32,
    #[serde(default)]
    proc_start: u64,
    session_id: String,
    #[serde(default)]
    cwd: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    waiting_for: Option<String>,
    #[serde(default)]
    status_updated_at: u64,
    #[serde(default)]
    tmux: Option<String>,
}

#[derive(Deserialize)]
struct StatusLine {
    rate_limits: Option<RateLimits>,
}

#[derive(Deserialize)]
struct RateLimits {
    five_hour: Option<Limit>,
    seven_day: Option<Limit>,
}

#[derive(Deserialize)]
struct Limit {
    used_percentage: Option<f64>,
    resets_at: Option<u64>,
}

#[derive(Deserialize)]
struct Settings {
    #[serde(rename = "statusLine")]
    status_line: Option<StatusLineSetting>,
}

#[derive(Deserialize)]
struct StatusLineSetting {
    command: Option<String>,
}

/// `None` for an unparseable file and for the `daemon` and `daemon-worker` kinds.
pub fn parse_session(json: &str) -> Option<ClaudeSession> {
    let file: SessionFile = serde_json::from_str(json).ok()?;
    if file
        .kind
        .as_deref()
        .is_some_and(|kind| BACKGROUND_KINDS.contains(&kind))
    {
        return None;
    }
    let state = match file.status.as_deref() {
        Some("busy") => AgentState::Busy,
        Some("waiting") => AgentState::Waiting,
        _ => AgentState::Idle,
    };
    let name = match file.name.filter(|name| !name.is_empty()) {
        Some(name) => name,
        None => {
            let basename = file.cwd.rsplit('/').next().unwrap_or_default();
            let prefix: String = file
                .session_id
                .chars()
                .take(SESSION_ID_PREFIX_LEN)
                .collect();
            format!("{basename}-{prefix}")
        }
    };
    Some(ClaudeSession {
        session: AgentSession {
            provider: ID.to_owned(),
            id: file.session_id,
            name,
            cwd: file.cwd,
            state,
            waiting_for: file.waiting_for.filter(|_| state == AgentState::Waiting),
            since_s: file.status_updated_at / 1000,
            pid: Some(file.pid),
            tmux: file.tmux,
        },
        proc_start: file.proc_start,
    })
}

pub fn parse_statusline(json: &str, now_s: u64, read_at_s: u64) -> ProviderUsage {
    let Some(limits) = serde_json::from_str::<StatusLine>(json)
        .ok()
        .and_then(|status| status.rate_limits)
    else {
        return unavailable(ID, LABEL, NO_READING);
    };
    let window = |length_s, limit: Option<Limit>| {
        let (used, resets_at) = limit.map_or((None, None), |l| (l.used_percentage, l.resets_at));
        window_as_of(length_s, used.unwrap_or(0.0), resets_at, now_s)
    };
    ProviderUsage {
        id: ID.to_owned(),
        label: LABEL.to_owned(),
        plan: None,
        available: true,
        reason: None,
        read_at_s,
        windows: vec![
            window(FIVE_HOURS_S, limits.five_hour),
            window(SEVEN_DAYS_S, limits.seven_day),
        ],
    }
}

/// Whether `statusLine.command` of a settings.json ends with the tap.
pub fn tap_installed(settings_json: &str) -> bool {
    serde_json::from_str::<Settings>(settings_json)
        .ok()
        .and_then(|settings| settings.status_line)
        .and_then(|line| line.command)
        .is_some_and(|command| command.ends_with(TAP_COMMAND_SUFFIX))
}
