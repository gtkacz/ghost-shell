//! Codex CLI's rollout files, session index and the hook-written session state files.

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

use ghost_proto::{AgentSession, AgentState, ProviderUsage};
use serde::{Deserialize, Serialize};

use super::{unavailable, window_as_of};

pub const ID: &str = "codex";
pub const LABEL: &str = "Codex CLI";
pub const TAIL_BYTES: u64 = 256 * 1024;
pub const SESSION_INDEX_FILE: &str = "session_index.jsonl";
pub const ROLLOUTS_SUBDIR: &str = "sessions";

const NO_READING: &str = "no reading yet";
const SESSION_ID_PREFIX_LEN: usize = 8;
const NEWEST_DAY_DIRS: usize = 2;
const SECONDS_PER_MINUTE: u64 = 60;
const PERMISSION_PROMPT: &str = "permission prompt";
const SESSIONS_DIR_UNDER_RUNTIME: &str = "ghost-shell/codex-sessions";

/// What `ghostd codex-hook` writes and the sessions feed reads, one file per session id.
#[derive(Serialize, Deserialize)]
pub struct SessionFile {
    pub state: AgentState,
    pub waiting_for: Option<String>,
    pub cwd: String,
    pub since_s: u64,
    pub pid: Option<u32>,
    pub proc_start: Option<u64>,
}

pub enum HookEffect {
    Set {
        state: AgentState,
        waiting_for: Option<&'static str>,
    },
    Remove,
}

#[derive(Deserialize)]
struct RolloutLine {
    #[serde(rename = "type")]
    kind: String,
    payload: Payload,
}

#[derive(Deserialize)]
struct Payload {
    #[serde(rename = "type")]
    kind: String,
    rate_limits: Option<RateLimits>,
}

#[derive(Deserialize)]
struct RateLimits {
    primary: Limit,
    secondary: Limit,
    plan_type: Option<String>,
}

#[derive(Deserialize)]
struct Limit {
    used_percent: f64,
    window_minutes: u64,
    resets_at: u64,
}

#[derive(Deserialize)]
struct IndexLine {
    id: String,
    thread_name: String,
}

/// `None` for an event the hook does not track.
pub fn hook_effect(event_name: &str) -> Option<HookEffect> {
    let set = |state, waiting_for| HookEffect::Set { state, waiting_for };
    match event_name {
        "SessionStart" | "UserPromptSubmit" | "PreToolUse" | "PostToolUse" => {
            Some(set(AgentState::Busy, None))
        }
        "PermissionRequest" => Some(set(AgentState::Waiting, Some(PERMISSION_PROMPT))),
        "Stop" | "Interrupt" => Some(set(AgentState::Idle, None)),
        "SessionEnd" => Some(HookEffect::Remove),
        _ => None,
    }
}

/// The last complete `token_count` line of `tail`; a partial first line is skipped.
pub fn parse_rollout_tail(tail: &[u8], now_s: u64, read_at_s: u64) -> ProviderUsage {
    let text = String::from_utf8_lossy(tail);
    let mut lines = text.split('\n');
    // A tail read starts at an arbitrary byte, so its first chunk may be half a line.
    if !tail.starts_with(b"{") {
        lines.next();
    }
    let Some(limits) = lines.rev().find_map(|line| {
        let line: RolloutLine = serde_json::from_str(line).ok()?;
        (line.kind == "event_msg" && line.payload.kind == "token_count")
            .then_some(line.payload.rate_limits)
            .flatten()
    }) else {
        return unavailable(ID, LABEL, NO_READING);
    };
    let window = |limit: &Limit| {
        window_as_of(
            limit.window_minutes * SECONDS_PER_MINUTE,
            limit.used_percent,
            Some(limit.resets_at),
            now_s,
        )
    };
    ProviderUsage {
        id: ID.to_owned(),
        label: LABEL.to_owned(),
        plan: limits.plan_type.clone(),
        available: true,
        reason: None,
        read_at_s,
        windows: vec![window(&limits.primary), window(&limits.secondary)],
    }
}

/// id → thread_name, the last line per id winning; malformed lines skipped.
pub fn thread_names(index: &str) -> HashMap<String, String> {
    index
        .lines()
        .filter_map(|line| serde_json::from_str::<IndexLine>(line).ok())
        .map(|line| (line.id, line.thread_name))
        .collect()
}

pub fn session(id: &str, file: &SessionFile, names: &HashMap<String, String>) -> AgentSession {
    let name = names.get(id).cloned().unwrap_or_else(|| {
        let basename = file.cwd.rsplit('/').next().unwrap_or_default();
        let prefix: String = id.chars().take(SESSION_ID_PREFIX_LEN).collect();
        format!("{basename}-{prefix}")
    });
    AgentSession {
        provider: ID.to_owned(),
        id: id.to_owned(),
        name,
        cwd: file.cwd.clone(),
        state: file.state,
        waiting_for: file.waiting_for.clone(),
        since_s: file.since_s,
        pid: file.pid,
        tmux: None,
    }
}

/// Child directories of `dir`, names sorted descending (zero-padded dates sort lexically).
fn subdirs_desc(dir: &Path) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    dirs.sort_by(|a, b| b.file_name().cmp(&a.file_name()));
    dirs
}

/// The newest `rollout-*.jsonl` under `<codex_home>/sessions`, by mtime, looking in the last two day dirs by name.
pub fn newest_rollout(codex_home: &Path) -> Option<PathBuf> {
    let root = codex_home.join(ROLLOUTS_SUBDIR);
    subdirs_desc(&root)
        .iter()
        .flat_map(|year| subdirs_desc(year))
        .flat_map(|month| subdirs_desc(&month))
        .take(NEWEST_DAY_DIRS)
        .flat_map(|day| fs::read_dir(day).into_iter().flatten().flatten())
        .filter(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            name.starts_with("rollout-") && name.ends_with(".jsonl")
        })
        .filter_map(|entry| {
            let modified = entry.metadata().ok()?.modified().ok()?;
            Some((modified, entry.path()))
        })
        .max_by_key(|(modified, _): &(SystemTime, PathBuf)| *modified)
        .map(|(_, path)| path)
}

/// `$XDG_RUNTIME_DIR/ghost-shell/codex-sessions`, or `/run/user/<uid>/…` when the variable is unset.
pub fn sessions_dir() -> PathBuf {
    let runtime = std::env::var("XDG_RUNTIME_DIR")
        .ok()
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(format!("/run/user/{}", nix::unistd::getuid())));
    runtime.join(SESSIONS_DIR_UNDER_RUNTIME)
}
