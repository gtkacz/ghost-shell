//! Usage windows and sessions of the LLM coding agents, one submodule per provider.

pub mod claude;
pub mod codex;

use std::cmp::Reverse;
use std::ffi::OsStr;
use std::fs::{self, File};
use std::future::pending;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, UNIX_EPOCH};

use ghost_proto::{
    AgentProvider, AgentState, AgentsSessions, AgentsUsage, ProviderUsage, UsageWindow, topics,
};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::mpsc;
use tokio::time::{Instant, MissedTickBehavior, interval_at, sleep, timeout};
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
    let codex_home = paths.codex_home.clone();
    let watcher = event_watcher(move |path| {
        // Codex also rewrites a sqlite state file in its home on every event.
        if path.parent() == Some(codex_home.as_path()) {
            path.file_name() == Some(OsStr::new(codex::SESSION_INDEX_FILE))
        } else {
            path.extension() == Some(OsStr::new(JSON_EXTENSION))
        }
    });
    let (mut watcher, mut events) = match watcher {
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

/// Wakes the receiver for every change to a path `accepts`; `accepts` is the only thing keeping noisy
/// directories from re-triggering a read.
fn event_watcher(
    accepts: impl Fn(&Path) -> bool + Send + 'static,
) -> notify::Result<(RecommendedWatcher, mpsc::UnboundedReceiver<()>)> {
    let (event_tx, events) = mpsc::unbounded_channel();
    let watcher = notify::recommended_watcher(move |result: notify::Result<Event>| {
        // inotify reports our own reads as opens, which would retrigger forever.
        if let Ok(event) = result
            && !matches!(event.kind, EventKind::Access(_))
            && event.paths.iter().any(|path| accepts(path))
        {
            // A send only fails once the feed stopped listening.
            let _ = event_tx.send(());
        }
    })?;
    Ok((watcher, events))
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

pub const CAPTURE_FILE: &str = "claude-statusline.json";
const CAPTURE_DIR: &str = "ghost-shell";
const SETTINGS_FILE: &str = "settings.json";
const STATE_HOME_ENV: &str = "XDG_STATE_HOME";
const STATE_HOME_SUBDIR: &str = ".local/state";
const ROLLOUT_PREFIX: &str = "rollout-";
const ROLLOUT_SUFFIX: &str = ".jsonl";
const NO_READING: &str = "no reading yet";
const TAP_MISSING: &str = "tap";

pub struct UsagePaths {
    pub claude_capture: PathBuf,
    pub claude_settings: PathBuf,
    pub codex_home: PathBuf,
}

/// `$XDG_STATE_HOME/ghost-shell/claude-statusline.json`, or `~/.local/state/…` when unset.
pub fn capture_path() -> PathBuf {
    let state_home = std::env::var_os(STATE_HOME_ENV)
        .map(PathBuf::from)
        .filter(|path| path.is_absolute());
    let base = state_home.unwrap_or_else(|| {
        PathBuf::from(std::env::var_os(HOME_ENV).unwrap_or_default()).join(STATE_HOME_SUBDIR)
    });
    base.join(CAPTURE_DIR).join(CAPTURE_FILE)
}

/// Reads `[agents]` from the live config once at start; `~` expands to `$HOME`.
pub fn usage_feed() -> Feed {
    Feed::new(topics::AGENTS_USAGE, move |context| {
        let agents = context.config.borrow().agents.clone();
        let home = std::env::var_os(HOME_ENV);
        let expand = |path: &str| expand_home(path, home.as_deref());
        let claude_dir = agents
            .claude_config_dirs
            .first()
            .map_or_else(PathBuf::new, |dir| expand(dir));
        let paths = UsagePaths {
            claude_capture: capture_path(),
            claude_settings: claude_dir.join(SETTINGS_FILE),
            codex_home: expand(&agents.codex_home),
        };
        run_usage(agents.providers, paths, Arc::new(unix_now_s), context)
    })
}

pub fn usage_feed_at(providers: Vec<AgentProvider>, paths: UsagePaths, clock: WallClock) -> Feed {
    Feed::new(topics::AGENTS_USAGE, move |context| {
        run_usage(providers, paths, clock, context)
    })
}

async fn run_usage(
    providers: Vec<AgentProvider>,
    paths: UsagePaths,
    clock: WallClock,
    mut context: FeedContext,
) {
    context
        .publisher
        .publish(topics::AGENTS_USAGE, &AgentsUsage::default());
    loop {
        until_wanted(&mut context.demand, topics::AGENTS_USAGE).await;
        // Dropping the watcher inside `watch_usage` is what stops it, so nothing is watched while unwanted.
        tokio::select! {
            biased;
            () = until_unwanted(&mut context.demand, topics::AGENTS_USAGE) => {}
            () = watch_usage(&providers, &paths, &clock, context.publisher.clone()) => {}
        }
    }
}

async fn watch_usage(
    providers: &[AgentProvider],
    paths: &UsagePaths,
    clock: &WallClock,
    publisher: Publisher,
) {
    let capture_name = paths.claude_capture.file_name().map(OsStr::to_owned);
    let watcher = event_watcher(move |path| {
        // ~/.claude is written constantly by every session and the state dir by other tools,
        // so only the files read here may wake the loop.
        let name = path.file_name();
        name == capture_name.as_deref()
            || name == Some(OsStr::new(SETTINGS_FILE))
            || name.and_then(OsStr::to_str).is_some_and(|name| {
                name.starts_with(ROLLOUT_PREFIX) && name.ends_with(ROLLOUT_SUFFIX)
            })
    });
    let (mut watcher, mut events) = match watcher {
        Ok(watcher) => watcher,
        Err(error) => {
            warn!(%error, "cannot watch the agent usage; it stays as read once");
            publish_usage(providers, paths, clock, &publisher);
            return pending().await;
        }
    };
    // The tap only creates the capture later, so its directory has to exist to be watched.
    if let Some(dir) = paths.claude_capture.parent()
        && let Err(error) = fs::create_dir_all(dir)
    {
        warn!(%error, "cannot create the Claude capture directory");
    }
    let targets = [
        (paths.claude_capture.parent(), RecursiveMode::NonRecursive),
        (paths.claude_settings.parent(), RecursiveMode::NonRecursive),
    ]
    .into_iter()
    .filter_map(|(dir, mode)| Some((dir?.to_path_buf(), mode)))
    .chain([(
        paths.codex_home.join(codex::ROLLOUTS_SUBDIR),
        RecursiveMode::Recursive,
    )]);
    let watched = targets
        .filter(|(dir, mode)| match watcher.watch(dir, *mode) {
            Ok(()) => true,
            Err(error) => {
                warn!(%error, dir = %dir.display(), "cannot watch an agent usage directory");
                false
            }
        })
        .count();

    // Read after the watches are in place, so a change in between is not lost.
    let mut published = publish_usage(providers, paths, clock, &publisher);
    if watched == 0 {
        return pending().await;
    }
    loop {
        let until_reset = next_reset_delay(&published, clock());
        tokio::select! {
            event = events.recv() => {
                if event.is_none() {
                    return pending().await;
                }
                while let Ok(Some(())) = timeout(DEBOUNCE, events.recv()).await {}
            }
            // A passed reset changes what the widget must show without touching any watched file.
            () = async {
                match until_reset {
                    Some(delay) => sleep(delay).await,
                    None => pending().await,
                }
            } => {}
        }
        let usage = read_usage(providers, paths, clock());
        if usage != published {
            publisher.publish(topics::AGENTS_USAGE, &usage);
            published = usage;
        }
    }
}

/// Time until one second past the earliest reset still ahead of `now_s`.
fn next_reset_delay(usage: &AgentsUsage, now_s: u64) -> Option<Duration> {
    usage
        .providers
        .iter()
        .flat_map(|provider| &provider.windows)
        .filter_map(|window| window.resets_at_s)
        .min()
        .map(|reset| Duration::from_secs((reset + 1).saturating_sub(now_s)))
}

fn publish_usage(
    providers: &[AgentProvider],
    paths: &UsagePaths,
    clock: &WallClock,
    publisher: &Publisher,
) -> AgentsUsage {
    let usage = read_usage(providers, paths, clock());
    publisher.publish(topics::AGENTS_USAGE, &usage);
    usage
}

/// One pass: an entry per enabled provider, in config order.
pub fn read_usage(providers: &[AgentProvider], paths: &UsagePaths, now_s: u64) -> AgentsUsage {
    let providers = providers
        .iter()
        .map(|provider| match provider {
            AgentProvider::Claude => read_claude(paths, now_s),
            AgentProvider::Codex => read_codex(paths, now_s),
        })
        .collect();
    AgentsUsage { providers }
}

fn read_claude(paths: &UsagePaths, now_s: u64) -> ProviderUsage {
    let tap_missing = fs::read_to_string(&paths.claude_settings)
        .is_ok_and(|settings| !claude::tap_installed(&settings));
    if tap_missing {
        return unavailable(claude::ID, claude::LABEL, TAP_MISSING);
    }
    match fs::read_to_string(&paths.claude_capture) {
        Ok(json) => claude::parse_statusline(&json, now_s, mtime_s(&paths.claude_capture)),
        Err(_) => unavailable(claude::ID, claude::LABEL, NO_READING),
    }
}

fn read_codex(paths: &UsagePaths, now_s: u64) -> ProviderUsage {
    newest_tail(&paths.codex_home).map_or_else(
        || unavailable(codex::ID, codex::LABEL, NO_READING),
        |(tail, read_at_s)| codex::parse_rollout_tail(&tail, now_s, read_at_s),
    )
}

/// The last `TAIL_BYTES` of the newest rollout with its mtime; the file can vanish under a read.
fn newest_tail(codex_home: &Path) -> Option<(Vec<u8>, u64)> {
    let path = codex::newest_rollout(codex_home)?;
    let mut file = File::open(&path).ok()?;
    let len = file.metadata().ok()?.len();
    file.seek(SeekFrom::End(-(len.min(codex::TAIL_BYTES) as i64)))
        .ok()?;
    let mut tail = Vec::new();
    file.read_to_end(&mut tail).ok()?;
    Some((tail, mtime_s(&path)))
}

fn mtime_s(path: &Path) -> u64 {
    fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |since| since.as_secs())
}
