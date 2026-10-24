use ghost_proto::{AgentState, UsageWindow};
use ghostd::feeds::agents::{FIVE_HOURS_S, SEVEN_DAYS_S, claude};

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
