//! Usage windows and sessions of the LLM coding agents, one submodule per provider.

pub mod claude;
pub mod codex;

use std::cmp::Reverse;
use std::ffi::OsStr;
use std::fs;
use std::future::pending;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use ghost_proto::{AgentProvider, AgentState, AgentsSessions, ProviderUsage, UsageWindow, topics};
use notify::{Event, EventKind, RecursiveMode, Watcher};
use tokio::sync::mpsc;
use tokio::time::{Instant, MissedTickBehavior, interval_at, timeout};
use tracing::{debug, warn};

use super::{WallClock, unix_now_s, until_unwanted, until_wanted};
use crate::collect::identity;
use crate::config::DEBOUNCE;
use crate::server::{Feed, FeedContext, Publisher};
use crate::sysroot::SysRoot;

pub const FIVE_HOURS_S: u64 = 5 * 3600;
pub const SEVEN_DAYS_S: u64 = 7 * 24 * 3600;
/// Process liveness is not watched by inotify, so a dead session is noticed on this tick at the latest.
pub const SWEEP_EVERY: Duration = Duration::from_secs(30);

const CLAUDE_SESSIONS_SUBDIR: &str = "sessions";
const JSON_EXTENSION: &str = "json";
const HOME_SHORTHAND: &str = "~";
const HOME_ENV: &str = "HOME";
// A hook that never saw the session end leaves its file behind; a day idle is not a live session.
const UNTRACKED_CODEX_MAX_AGE_S: u64 = 24 * 3600;

pub struct SessionPaths {
    pub claude_sessions_dirs: Vec<PathBuf>,
    pub codex_sessions_dir: PathBuf,
    pub codex_home: PathBuf,
}

/// A window as the widget should see it at `now_s`: a reset at or before now zeroes it and drops the reset.
pub fn window_as_of(
    length_s: u64,
    used_pct: f64,
    resets_at_s: Option<u64>,
    now_s: u64,
) -> UsageWindow {
    match resets_at_s {
        Some(reset) if reset > now_s => UsageWindow {
            length_s,
            used_pct,
            resets_at_s,
        },
        _ => UsageWindow {
            length_s,
            used_pct: 0.0,
            resets_at_s: None,
        },
    }
}

pub fn unavailable(id: &str, label: &str, reason: &str) -> ProviderUsage {
    ProviderUsage {
        id: id.to_owned(),
        label: label.to_owned(),
        plan: None,
        available: false,
        reason: Some(reason.to_owned()),
        read_at_s: 0,
        windows: Vec::new(),
    }
}

/// Reads `[agents]` from the live config once at start; `~` expands to `$HOME`.
pub fn sessions_feed(root: SysRoot) -> Feed {
    Feed::new(topics::AGENTS_SESSIONS, move |context| {
        let agents = context.config.borrow().agents.clone();
        let home = std::env::var_os(HOME_ENV);
        let expand = |path: &str| expand_home(path, home.as_deref());
        let paths = SessionPaths {
            claude_sessions_dirs: agents
                .claude_config_dirs
                .iter()
                .map(|dir| expand(dir).join(CLAUDE_SESSIONS_SUBDIR))
                .collect(),
            codex_sessions_dir: codex::sessions_dir(),
            codex_home: expand(&agents.codex_home),
        };
        run(root, agents.providers, paths, Arc::new(unix_now_s), context)
    })
}

pub fn sessions_feed_at(
    root: SysRoot,
    providers: Vec<AgentProvider>,
    paths: SessionPaths,
    clock: WallClock,
) -> Feed {
    Feed::new(topics::AGENTS_SESSIONS, move |context| {
        run(root, providers, paths, clock, context)
    })
}

fn expand_home(path: &str, home: Option<&OsStr>) -> PathBuf {
    let rest = path.strip_prefix(HOME_SHORTHAND);
    match (rest, home) {
        (Some(rest), Some(home)) if rest.is_empty() || rest.starts_with('/') => {
            Path::new(home).join(rest.trim_start_matches('/'))
        }
        _ => PathBuf::from(path),
    }
}

async fn run(
    root: SysRoot,
    providers: Vec<AgentProvider>,
    paths: SessionPaths,
    clock: WallClock,
    mut context: FeedContext,
) {
    context
        .publisher
        .publish(topics::AGENTS_SESSIONS, &AgentsSessions::default());
    loop {
        until_wanted(&mut context.demand, topics::AGENTS_SESSIONS).await;
        // Dropping the watcher inside `watch` is what stops it, so nothing is watched while unwanted.
        tokio::select! {
            biased;
            () = until_unwanted(&mut context.demand, topics::AGENTS_SESSIONS) => {}
            () = watch(&root, &providers, &paths, &clock, context.publisher.clone()) => {}
        }
    }
}

async fn watch(
    root: &SysRoot,
    providers: &[AgentProvider],
    paths: &SessionPaths,
    clock: &WallClock,
    publisher: Publisher,
) {
    let (event_tx, mut events) = mpsc::unbounded_channel();
    let watcher = notify::recommended_watcher(move |result: notify::Result<Event>| {
        // inotify reports our own reads as opens, which would retrigger forever.
        if let Ok(event) = result
            && !matches!(event.kind, EventKind::Access(_))
        {
            // A send only fails once the feed stopped listening.
            let _ = event_tx.send(());
        }
    });
    let mut watcher = match watcher {
        Ok(watcher) => watcher,
        Err(error) => {
            warn!(%error, "cannot watch the agent sessions; they stay as read once");
            publish_pass(root, providers, paths, clock, &publisher);
            return pending().await;
        }
    };
    // The hook only creates files here later, so the directory has to exist to be watched.
    if let Err(error) = fs::create_dir_all(&paths.codex_sessions_dir) {
        warn!(%error, "cannot create the Codex sessions directory");
    }
    let watched = paths
        .claude_sessions_dirs
        .iter()
        .chain([&paths.codex_sessions_dir, &paths.codex_home])
        .filter(
            |dir| match watcher.watch(dir, RecursiveMode::NonRecursive) {
                Ok(()) => true,
                Err(error) => {
                    warn!(%error, dir = %dir.display(), "cannot watch an agent directory");
                    false
                }
            },
        )
        .count();

    // Read after the watches are in place, so a change in between is not lost.
    let mut published = publish_pass(root, providers, paths, clock, &publisher);
    if watched == 0 {
        return pending().await;
    }
    let mut sweep = interval_at(Instant::now() + SWEEP_EVERY, SWEEP_EVERY);
    sweep.set_missed_tick_behavior(MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            _ = events.recv() => while let Ok(Some(())) = timeout(DEBOUNCE, events.recv()).await {},
            _ = sweep.tick() => {}
        }
        remove_stale_codex(paths, clock());
        let sessions = read_sessions(root, providers, paths);
        if sessions != published {
            publisher.publish(topics::AGENTS_SESSIONS, &sessions);
            published = sessions;
        }
    }
}

fn publish_pass(
    root: &SysRoot,
    providers: &[AgentProvider],
    paths: &SessionPaths,
    clock: &WallClock,
    publisher: &Publisher,
) -> AgentsSessions {
    remove_stale_codex(paths, clock());
    let sessions = read_sessions(root, providers, paths);
    publisher.publish(topics::AGENTS_SESSIONS, &sessions);
    sessions
}

/// One pass: every live session, sorted waiting → busy → idle, newest `since_s` first within a group.
pub fn read_sessions(
    root: &SysRoot,
    providers: &[AgentProvider],
    paths: &SessionPaths,
) -> AgentsSessions {
    let is_live = |pid: u32, proc_start: u64| {
        identity(root, pid).is_some_and(|process| process.start_time == proc_start)
    };
    let mut sessions = Vec::new();
    if providers.contains(&AgentProvider::Claude) {
        for dir in &paths.claude_sessions_dirs {
            for (path, text) in json_files(dir) {
                match claude::parse_session(&text) {
                    Some(found) => {
                        let alive = found
                            .session
                            .pid
                            .is_some_and(|pid| is_live(pid, found.proc_start));
                        if alive {
                            sessions.push(found.session);
                        }
                    }
                    None => debug!(path = %path.display(), "skipped a Claude session file"),
                }
            }
        }
    }
    if providers.contains(&AgentProvider::Codex) {
        let names = fs::read_to_string(paths.codex_home.join(codex::SESSION_INDEX_FILE))
            .map(|index| codex::thread_names(&index))
            .unwrap_or_default();
        for (path, text) in json_files(&paths.codex_sessions_dir) {
            let Ok(file) = serde_json::from_str::<codex::SessionFile>(&text) else {
                debug!(path = %path.display(), "skipped a Codex session file");
                continue;
            };
            let Some(id) = path.file_stem().and_then(OsStr::to_str) else {
                continue;
            };
            let alive = match (file.pid, file.proc_start) {
                (None, _) => true,
                (Some(pid), Some(proc_start)) => is_live(pid, proc_start),
                (Some(_), None) => false,
            };
            if alive {
                sessions.push(codex::session(id, &file, &names));
            }
        }
    }
    sessions.sort_by(|a, b| {
        (state_rank(a.state), Reverse(a.since_s), &a.id).cmp(&(
            state_rank(b.state),
            Reverse(b.since_s),
            &b.id,
        ))
    });
    AgentsSessions { sessions }
}

fn state_rank(state: AgentState) -> u8 {
    match state {
        AgentState::Waiting => 0,
        AgentState::Busy => 1,
        AgentState::Idle => 2,
    }
}

/// The `*.json` files of `dir` with their text; unreadable ones are skipped.
fn json_files(dir: &Path) -> Vec<(PathBuf, String)> {
    fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension() == Some(OsStr::new(JSON_EXTENSION)))
        // The CLIs rewrite these in place, so a read can be torn or race a delete.
        .filter_map(|path| fs::read_to_string(&path).ok().map(|text| (path, text)))
        .collect()
}

/// Deletes Codex files without a pid that nothing has touched for a day, since no pid means no liveness check.
fn remove_stale_codex(paths: &SessionPaths, now_s: u64) {
    for (path, text) in json_files(&paths.codex_sessions_dir) {
        let stale = serde_json::from_str::<codex::SessionFile>(&text).is_ok_and(|file| {
            file.pid.is_none() && file.since_s + UNTRACKED_CODEX_MAX_AGE_S < now_s
        });
        if stale && let Err(error) = fs::remove_file(&path) {
            warn!(%error, path = %path.display(), "cannot remove a stale Codex session file");
        }
    }
}
