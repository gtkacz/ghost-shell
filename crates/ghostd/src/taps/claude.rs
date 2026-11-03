use std::io::Write;
use std::path::Path;
use std::process::{Command, ExitCode, Stdio};
use std::{env, fs, io};

use super::{read_stdin, write_atomic};
use crate::config::config_dir;
use crate::feeds::agents::capture_path;

/// Under `config_dir()`; holds the status-line command the tap hands the same input on to.
pub const INNER_FILE: &str = "claude-statusline-inner";
const CONFIG_DIR_ENV: &str = "CLAUDE_CONFIG_DIR";
const HOME_ENV: &str = "HOME";
const DEFAULT_CONFIG_DIR: &str = ".claude";

/// Writes stdin to the capture atomically, then runs the saved inner command with the same stdin, stdout passed through.
/// Always succeeds: a failing tap must never blank the user's status line.
pub fn claude_statusline_tap() -> ExitCode {
    let input = match read_stdin() {
        Ok(input) => input,
        Err(error) => {
            eprintln!("ghostd claude-statusline-tap: reading stdin failed: {error}");
            return ExitCode::SUCCESS;
        }
    };
    // A session that has not had its first API response renders without limits, and would otherwise
    // overwrite the reading every other session captured.
    if is_default_profile()
        && has_rate_limits(&input)
        && let Err(error) = write_atomic(&capture_path(), &input)
    {
        eprintln!("ghostd claude-statusline-tap: writing the capture failed: {error}");
    }
    if let Err(error) = run_inner(&input) {
        eprintln!("ghostd claude-statusline-tap: the inner command failed: {error}");
    }
    ExitCode::SUCCESS
}

// Another profile can share the default's settings.json by symlink, and with it this tap. Usage is read for the default
// profile only, so another account's limits must not overwrite its reading.
fn is_default_profile() -> bool {
    let Some(config_dir) = env::var_os(CONFIG_DIR_ENV).filter(|dir| !dir.is_empty()) else {
        return true;
    };
    let Some(home) = env::var_os(HOME_ENV) else {
        return false;
    };
    let config_dir = Path::new(&config_dir);
    let default_dir = Path::new(&home).join(DEFAULT_CONFIG_DIR);
    // Canonicalized too, since a default profile reached by another spelling would lose its readings without a word.
    config_dir == default_dir
        || matches!(
            (fs::canonicalize(config_dir), fs::canonicalize(&default_dir)),
            (Ok(named), Ok(default)) if named == default
        )
}

fn has_rate_limits(input: &[u8]) -> bool {
    serde_json::from_slice::<serde_json::Value>(input).is_ok_and(|status| {
        status
            .get("rate_limits")
            .is_some_and(serde_json::Value::is_object)
    })
}

fn run_inner(input: &[u8]) -> io::Result<()> {
    let saved = match fs::read_to_string(config_dir().join(INNER_FILE)) {
        Ok(saved) => saved,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    let command = saved.trim();
    if command.is_empty() {
        return Ok(());
    }
    let mut child = Command::new("bash")
        .args(["-c", command])
        .stdin(Stdio::piped())
        .spawn()?;
    if let Some(mut stdin) = child.stdin.take() {
        // An inner command that ignores its input closes the pipe early; that is not a failure.
        let _ = stdin.write_all(input);
    }
    child.wait().map(drop)
}
