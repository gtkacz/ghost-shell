use std::path::Path;
use std::process::ExitCode;
use std::{fs, io};

use serde::Deserialize;

use super::{read_stdin, write_atomic};
use crate::collect::identity;
use crate::feeds::agents::codex::{HookEffect, SessionFile, hook_effect, sessions_dir};
use crate::feeds::unix_now_s;
use crate::sysroot::SysRoot;

pub const SESSION_ID_CHARS: &str = "ASCII letters, digits, '-' and '_'";
const CODEX_COMM: &str = "codex";
const MAX_ANCESTOR_STEPS: usize = 64;
const INIT_PID: u32 = 1;

#[derive(Deserialize)]
struct HookInput {
    session_id: String,
    hook_event_name: String,
    #[serde(default)]
    cwd: String,
}

/// The nearest ancestor of `pid` (itself included) whose comm is `codex`, as (pid, start_time).
pub fn codex_ancestor(root: &SysRoot, pid: u32) -> Option<(u32, u64)> {
    let mut current = pid;
    for _ in 0..MAX_ANCESTOR_STEPS {
        if current <= INIT_PID {
            return None;
        }
        let found = identity(root, current)?;
        if found.name == CODEX_COMM {
            return Some((current, found.start_time));
        }
        current = found.ppid;
    }
    None
}

/// Records one Codex hook event as a per-session state file. Always succeeds and prints nothing:
/// Codex runs this on every event and a failure here must not disturb it.
pub fn codex_hook() -> ExitCode {
    if let Err(message) = apply_hook() {
        eprintln!("ghostd codex-hook: {message}");
    }
    ExitCode::SUCCESS
}

fn apply_hook() -> Result<(), String> {
    let input = read_stdin().map_err(|error| format!("reading stdin failed: {error}"))?;
    let hook: HookInput =
        serde_json::from_slice(&input).map_err(|error| format!("unreadable event: {error}"))?;
    // The id becomes a file name, so anything else could escape the sessions directory.
    let valid = !hook.session_id.is_empty()
        && hook
            .session_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_');
    if !valid {
        return Err(format!("session_id must be {SESSION_ID_CHARS}"));
    }
    let path = sessions_dir().join(format!("{}.json", hook.session_id));
    match hook_effect(&hook.hook_event_name) {
        None => Ok(()),
        Some(HookEffect::Remove) => {
            let _ = fs::remove_file(&path);
            Ok(())
        }
        Some(HookEffect::Set { state, waiting_for }) => {
            let since_s = previous(&path)
                .filter(|before| before.state == state)
                .map_or_else(unix_now_s, |before| before.since_s);
            let ancestor = codex_ancestor(&SysRoot::host(), std::os::unix::process::parent_id());
            let file = SessionFile {
                state,
                waiting_for: waiting_for.map(str::to_owned),
                cwd: hook.cwd,
                since_s,
                pid: ancestor.map(|(pid, _)| pid),
                proc_start: ancestor.map(|(_, start)| start),
            };
            let json = serde_json::to_vec(&file).map_err(io::Error::from);
            json.and_then(|json| write_atomic(&path, &json))
                .map_err(|error| format!("writing the session failed: {error}"))
        }
    }
}

fn previous(path: &Path) -> Option<SessionFile> {
    serde_json::from_slice(&fs::read(path).ok()?).ok()
}
