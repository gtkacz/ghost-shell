use std::{
    fs,
    fs::File,
    time::{Duration, UNIX_EPOCH},
};

use ghost_proto::{AgentState, UsageWindow};
use ghostd::feeds::agents::{FIVE_HOURS_S, SEVEN_DAYS_S, claude, codex};

const NOW_S: u64 = 1_792_797_010;

#[test]
fn a_waiting_claude_session_maps_to_waiting_with_its_reason() {
    let parsed =
        claude::parse_session(include_str!("fixtures/agents/claude-session-waiting.json")).unwrap();
    assert_eq!(parsed.proc_start, 123_456);
    let s = parsed.session;
    assert_eq!(
        (s.provider.as_str(), s.pid, s.state),
        ("claude", Some(4242), AgentState::Waiting)
    );
    assert_eq!(s.waiting_for.as_deref(), Some("permission prompt"));
    assert_eq!(
        (s.name.as_str(), s.cwd.as_str(), s.since_s),
        ("proj-68", "/home/u/Codes/proj", 1_792_797_000)
    );
    assert_eq!(s.tmux.as_deref(), Some("unsnooze-6:@12.%12"));
}

#[test]
fn daemon_kinds_and_garbage_are_not_sessions() {
    assert!(
        claude::parse_session(include_str!("fixtures/agents/claude-session-daemon.json")).is_none()
    );
    assert!(claude::parse_session("{not json").is_none());
}

#[test]
fn shell_status_reads_as_idle_and_an_empty_name_is_built() {
    let json = include_str!("fixtures/agents/claude-session-waiting.json")
        .replace("\"waiting\"", "\"shell\"")
        .replace("\"proj-68\"", "\"\"");
    let s = claude::parse_session(&json).unwrap().session;
    assert_eq!(s.state, AgentState::Idle);
    assert_eq!(s.name, "proj-01234567");
}

#[test]
fn the_statusline_gives_both_windows() {
    let usage = claude::parse_statusline(
        include_str!("fixtures/agents/claude-statusline.json"),
        NOW_S,
        7,
    );
    assert!(usage.available && usage.reason.is_none() && usage.plan.is_none());
    assert_eq!(usage.read_at_s, 7);
    assert_eq!(
        usage.windows,
        [
            UsageWindow {
                length_s: FIVE_HOURS_S,
                used_pct: 42.0,
                resets_at_s: Some(1_792_805_040)
            },
            UsageWindow {
                length_s: SEVEN_DAYS_S,
                used_pct: 61.0,
                resets_at_s: Some(1_793_091_600)
            },
        ]
    );
}

#[test]
fn a_passed_or_dropped_window_reads_zero_with_no_reset() {
    // five_hour's reset is in the past; seven_day is dropped from the JSON.
    let json = r#"{"rate_limits":{"five_hour":{"used_percentage":42,"resets_at":1792797010}}}"#;
    let usage = claude::parse_statusline(json, NOW_S, 0);
    assert_eq!(
        usage.windows,
        [
            UsageWindow {
                length_s: FIVE_HOURS_S,
                used_pct: 0.0,
                resets_at_s: None
            },
            UsageWindow {
                length_s: SEVEN_DAYS_S,
                used_pct: 0.0,
                resets_at_s: None
            },
        ]
    );
}

#[test]
fn a_statusline_without_limits_is_unavailable_not_zero() {
    let usage = claude::parse_statusline(
        include_str!("fixtures/agents/claude-statusline-no-limits.json"),
        NOW_S,
        0,
    );
    assert!(!usage.available && usage.windows.is_empty());
    assert_eq!(usage.reason.as_deref(), Some("no reading yet"));
    assert!(!claude::parse_statusline("nope", NOW_S, 0).available);
}

#[test]
fn the_tap_is_recognised_by_its_command_suffix() {
    assert!(claude::tap_installed(include_str!(
        "fixtures/agents/claude-settings-tapped.json"
    )));
    assert!(!claude::tap_installed(include_str!(
        "fixtures/agents/claude-settings-hud.json"
    )));
    assert!(!claude::tap_installed("{}"));
}

#[test]
fn the_newest_token_count_line_gives_both_codex_windows() {
    let usage = codex::parse_rollout_tail(
        include_bytes!("fixtures/agents/codex-rollout-tail.jsonl"),
        NOW_S,
        9,
    );
    assert!(usage.available);
    assert_eq!(usage.plan.as_deref(), Some("plus"));
    assert_eq!(
        usage.windows[0],
        UsageWindow {
            length_s: FIVE_HOURS_S,
            used_pct: 0.0,
            resets_at_s: Some(1_792_814_950)
        }
    );
    assert_eq!(
        usage.windows[1],
        UsageWindow {
            length_s: SEVEN_DAYS_S,
            used_pct: 14.0,
            resets_at_s: Some(1_793_018_780)
        }
    );
}

#[test]
fn a_tail_cut_mid_line_still_finds_the_last_complete_reading() {
    let whole = include_bytes!("fixtures/agents/codex-rollout-tail.jsonl");
    let cut = &whole[whole.len() / 3..];
    assert_eq!(
        codex::parse_rollout_tail(cut, NOW_S, 0).windows,
        codex::parse_rollout_tail(whole, NOW_S, 0).windows
    );
    assert!(
        !codex::parse_rollout_tail(
            b"{\"type\":\"event_msg\",\"payload\":{\"type\":\"agent_message\"}}\n",
            NOW_S,
            0
        )
        .available
    );
}

#[test]
fn thread_names_take_the_last_line_per_id_and_skip_garbage() {
    let names = codex::thread_names(include_str!("fixtures/agents/codex-session-index.jsonl"));
    assert_eq!(names.len(), 3);
    assert_eq!(names["aaaaaaaa-0000"], "AGENTS.md draft");
}

#[test]
fn a_codex_session_is_named_from_the_index_or_its_cwd() {
    let names = codex::thread_names(include_str!("fixtures/agents/codex-session-index.jsonl"));
    let file = codex::SessionFile {
        state: AgentState::Busy,
        waiting_for: None,
        cwd: "/home/u/Codes/content-platform".into(),
        since_s: 5,
        pid: Some(7),
        proc_start: Some(8),
    };
    assert_eq!(
        codex::session("aaaaaaaa-0000", &file, &names).name,
        "AGENTS.md draft"
    );
    let unnamed = codex::session("zzzzzzzz-9999-1234", &file, &names);
    assert_eq!(
        (
            unnamed.name.as_str(),
            unnamed.provider.as_str(),
            unnamed.pid
        ),
        ("content-platform-zzzzzzzz", "codex", Some(7))
    );
}

#[test]
fn hook_events_map_to_states() {
    use codex::HookEffect::*;
    assert!(matches!(
        codex::hook_effect("PermissionRequest"),
        Some(Set {
            state: AgentState::Waiting,
            waiting_for: Some("permission prompt")
        })
    ));
    assert!(matches!(
        codex::hook_effect("PostToolUse"),
        Some(Set {
            state: AgentState::Busy,
            waiting_for: None
        })
    ));
    assert!(matches!(
        codex::hook_effect("Interrupt"),
        Some(Set {
            state: AgentState::Idle,
            ..
        })
    ));
    assert!(matches!(codex::hook_effect("SessionEnd"), Some(Remove)));
    assert!(codex::hook_effect("PreCompact").is_none());
}

#[test]
fn the_newest_rollout_is_found_across_day_dirs() {
    let dir = tempfile::tempdir().unwrap();
    let old = dir.path().join("sessions/2026/10/22/rollout-a.jsonl");
    let new = dir.path().join("sessions/2026/10/23/rollout-b.jsonl");
    for (path, mtime) in [(&old, 1_700_000_000), (&new, 1_700_000_100)] {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "").unwrap();
        File::open(path)
            .unwrap()
            .set_modified(UNIX_EPOCH + Duration::from_secs(mtime))
            .unwrap();
    }
    assert_eq!(codex::newest_rollout(dir.path()), Some(new));
    assert_eq!(codex::newest_rollout(&dir.path().join("nowhere")), None);
}
