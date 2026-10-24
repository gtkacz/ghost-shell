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

mod sessions {
    use std::{collections::BTreeSet, fs, path::PathBuf, sync::Arc, time::Duration};

    use ghost_proto::AgentState::{self, Busy, Idle, Waiting};
    use ghost_proto::{AgentProvider, AgentsSessions, ResolvedConfig, topics};
    use ghostd::collect::identity;
    use ghostd::feeds::agents::{SessionPaths, codex, sessions_feed_at};
    use ghostd::server::{FeedContext, Publisher};
    use ghostd::sysroot::SysRoot;
    use serde_json::Value;
    use tempfile::TempDir;
    use tokio::sync::{mpsc, watch};
    use tokio::time::timeout;

    use super::NOW_S;

    const WAIT: Duration = Duration::from_secs(2);
    // Offsets from here keep test times inside the 24 h a pid-less Codex file lives.
    const BASE_S: u64 = NOW_S - 1000;

    struct Sandbox {
        dir: TempDir,
    }

    impl Sandbox {
        fn new() -> Self {
            let sb = Self {
                dir: tempfile::tempdir().unwrap(),
            };
            for sub in ["root", "claude/sessions", "run/codex-sessions", "codex"] {
                fs::create_dir_all(sb.dir.path().join(sub)).unwrap();
            }
            fs::write(sb.dir.path().join("codex/session_index.jsonl"), "").unwrap();
            sb
        }

        fn root(&self) -> PathBuf {
            self.dir.path().join("root")
        }

        fn proc(&self, pid: u32, comm: &str, start: u64) {
            let dir = self.root().join(format!("proc/{pid}"));
            fs::create_dir_all(&dir).unwrap();
            fs::write(
                dir.join("stat"),
                format!("{pid} ({comm}) S 7 0 0 0 0 0 0 0 0 0 1 2 0 0 0 0 0 0 {start} 0 8 0\n"),
            )
            .unwrap();
        }

        /// Replaces by rename, as Claude does, so a watcher never sees a torn file.
        fn claude_session(&self, pid: u32, status: &str, proc_start: u64, since_s: u64) {
            let waiting = if status == "waiting" {
                r#","waitingFor":"permission prompt""#
            } else {
                ""
            };
            let json = format!(
                r#"{{"pid":{pid},"procStart":{proc_start},"sessionId":"{pid}","cwd":"/home/u/p","name":"s{pid}","kind":"interactive","status":"{status}","statusUpdatedAt":{}{waiting}}}"#,
                (BASE_S + since_s) * 1000
            );
            let dir = self.dir.path().join("claude/sessions");
            let staged = dir.join(format!("{pid}.json.tmp"));
            fs::write(&staged, json).unwrap();
            fs::rename(staged, dir.join(format!("{pid}.json"))).unwrap();
        }

        fn codex_session(&self, id: &str, state: AgentState, since_s: u64, pid: Option<u32>) {
            let file = codex::SessionFile {
                state,
                waiting_for: None,
                cwd: "/home/u/p".into(),
                since_s: BASE_S + since_s,
                pid,
                proc_start: None,
            };
            fs::write(
                self.dir
                    .path()
                    .join(format!("run/codex-sessions/{id}.json")),
                serde_json::to_string(&file).unwrap(),
            )
            .unwrap();
        }

        async fn start(&self, providers: &[AgentProvider]) -> Running {
            let (sender, published) = mpsc::unbounded_channel();
            let (demand, demand_receiver) =
                watch::channel(BTreeSet::from([topics::AGENTS_SESSIONS.to_owned()]));
            let (config, config_receiver) = watch::channel(ResolvedConfig::default());
            let context = FeedContext {
                publisher: Publisher::new(move |topic, data| {
                    sender.send((topic.to_owned(), data)).unwrap();
                }),
                demand: demand_receiver,
                config: config_receiver,
                calls: tokio::sync::broadcast::channel(64).1,
            };
            let paths = SessionPaths {
                claude_sessions_dirs: vec![self.dir.path().join("claude/sessions")],
                codex_sessions_dir: self.dir.path().join("run/codex-sessions"),
                codex_home: self.dir.path().join("codex"),
            };
            let feed = sessions_feed_at(
                SysRoot::new(self.root()),
                providers.to_vec(),
                paths,
                Arc::new(|| NOW_S),
            );
            Running {
                published,
                _demand: demand,
                _config: config,
                _task: tokio::spawn(feed.run(context)),
            }
        }
    }

    struct Running {
        published: mpsc::UnboundedReceiver<(String, Value)>,
        _demand: watch::Sender<BTreeSet<String>>,
        _config: watch::Sender<ResolvedConfig>,
        _task: tokio::task::JoinHandle<()>,
    }

    impl Running {
        async fn next(&mut self) -> AgentsSessions {
            let (topic, data) = timeout(WAIT, self.published.recv())
                .await
                .expect("nothing was published in time")
                .unwrap();
            assert_eq!(topic, topics::AGENTS_SESSIONS);
            serde_json::from_value(data).unwrap()
        }
    }

    #[test]
    fn identity_reads_name_parent_and_start_time() {
        let sb = Sandbox::new();
        sb.proc(42, "codex", 555);
        let root = SysRoot::new(sb.root());
        let id = identity(&root, 42).unwrap();
        assert_eq!(
            (id.name.as_str(), id.ppid, id.start_time),
            ("codex", 7, 555)
        );
        assert!(identity(&root, 43).is_none());
    }

    #[tokio::test]
    async fn live_sessions_are_sorted_and_dead_ones_dropped() {
        let sb = Sandbox::new();
        sb.claude_session(1, "idle", 100, 10);
        sb.claude_session(2, "busy", 200, 20);
        sb.claude_session(3, "waiting", 300, 5);
        sb.claude_session(4, "busy", 400, 30);
        sb.proc(1, "claude", 100);
        sb.proc(2, "claude", 200);
        sb.proc(3, "claude", 300);
        sb.proc(4, "claude", 999);
        sb.codex_session("abc", Busy, 25, None);
        let mut feed = sb
            .start(&[AgentProvider::Claude, AgentProvider::Codex])
            .await;
        assert!(
            feed.next().await.sessions.is_empty(),
            "an empty snapshot comes first"
        );
        let snapshot = feed.next().await;
        let order: Vec<(String, AgentState)> = snapshot
            .sessions
            .iter()
            .map(|s| (s.id.clone(), s.state))
            .collect();
        assert_eq!(
            order,
            [
                ("3".into(), Waiting),
                ("abc".into(), Busy),
                ("2".into(), Busy),
                ("1".into(), Idle)
            ]
        );
    }

    #[tokio::test]
    async fn a_rewritten_session_file_publishes_and_an_unreadable_stat_is_dead() {
        let sb = Sandbox::new();
        sb.claude_session(1, "busy", 100, 10);
        sb.proc(1, "claude", 100);
        let mut feed = sb.start(&[AgentProvider::Claude]).await;
        feed.next().await;
        feed.next().await;
        sb.claude_session(1, "waiting", 100, 11);
        assert_eq!(feed.next().await.sessions[0].state, Waiting);
        fs::remove_file(sb.root().join("proc/1/stat")).unwrap();
        sb.claude_session(1, "busy", 100, 12);
        assert!(feed.next().await.sessions.is_empty());
    }

    #[tokio::test]
    async fn a_pidless_codex_file_a_day_behind_the_clock_is_dropped_and_removed() {
        let sb = Sandbox::new();
        sb.codex_session("fresh", Busy, 5, None);
        sb.codex_session("old", Busy, 5, None);
        let old = sb.dir.path().join("run/codex-sessions/old.json");
        let text = fs::read_to_string(&old).unwrap();
        fs::write(&old, text.replace(&(BASE_S + 5).to_string(), "1")).unwrap();
        let mut feed = sb.start(&[AgentProvider::Codex]).await;
        feed.next().await;
        let ids: Vec<_> = feed
            .next()
            .await
            .sessions
            .into_iter()
            .map(|s| s.id)
            .collect();
        assert_eq!(ids, ["fresh"]);
        assert!(!old.exists());
    }

    #[tokio::test]
    async fn only_enabled_providers_are_read() {
        let sb = Sandbox::new();
        sb.claude_session(1, "busy", 100, 10);
        sb.proc(1, "claude", 100);
        sb.codex_session("abc", Busy, 25, None);
        let mut feed = sb.start(&[AgentProvider::Codex]).await;
        feed.next().await;
        let providers: Vec<_> = feed
            .next()
            .await
            .sessions
            .into_iter()
            .map(|s| s.provider)
            .collect();
        assert_eq!(providers, ["codex"]);
    }
}
