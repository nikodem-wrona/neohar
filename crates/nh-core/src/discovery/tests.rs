//! The discovery with fake harness programs on the `PATH` of the login environment (the
//! row "Harnesses").

use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf};

use nh_proto::api::model::LoginMethod;

use super::*;
use crate::ErrorCode;
use crate::host::{HostCall, RecordingHost};

const CLAUDE_IN: &str = r#"case "$1 $2" in
  "--version "*) echo '2.1.292 (Claude Code)' ;;
  "auth status") echo '{"loggedIn": true, "authMethod": "claude.ai", "email": "<email>"}' ;;
  *) exit 2 ;;
esac"#;

const CODEX_OUT: &str = r#"case "$1 $2" in
  "--version "*) echo 'codex-cli 0.162.1' ;;
  "login status") echo 'Not logged in' >&2; exit 1 ;;
  *) exit 2 ;;
esac"#;

const CODEX_IN: &str = r#"case "$1 $2" in
  "--version "*) echo 'codex-cli 0.162.1' ;;
  "login status") echo 'Logged in using ChatGPT' >&2 ;;
  *) exit 2 ;;
esac"#;

/// A machine for a test: a directory of programs, the manifests, the health and a host.
struct Machine {
    dir: PathBuf,
    hub: Arc<ProjectionHub>,
    health: Arc<Health>,
    host: Arc<RecordingHost>,
    discovery: Arc<Discovery>,
}

impl Machine {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("nh-discovery-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("bin")).expect("a directory");
        let hub = ProjectionHub::new();
        let health = Health::new(Arc::clone(&hub));
        let harnesses = Harnesses::load(dir.join("config"), None, Arc::clone(&health));
        let host = Arc::new(RecordingHost::new(dir.join("nh"), false));
        let env = vec![
            (
                "PATH".to_owned(),
                format!("{}:/usr/bin:/bin", dir.join("bin").display()),
            ),
            ("HOME".to_owned(), dir.display().to_string()),
        ];
        let discovery = Discovery::with_limit(
            Handle::current(),
            env,
            harnesses,
            Arc::clone(&health),
            Arc::clone(&hub),
            Arc::clone(&host) as Arc<dyn Host>,
            Duration::from_secs(5),
        );
        Self {
            dir,
            hub,
            health,
            host,
            discovery,
        }
    }

    /// A harness program. A second call replaces it with a new modification time.
    fn program(&self, name: &str, body: &str) {
        let file = self.dir.join("bin").join(name);
        let _ = fs::remove_file(&file);
        fs::write(&file, format!("#!/bin/sh\n{body}\n")).expect("a script");
        fs::set_permissions(&file, fs::Permissions::from_mode(0o755)).expect("permissions");
    }

    fn row(&self, id: &str) -> HarnessValue {
        self.discovery.rows().remove(id).expect("a row")
    }

    fn warnings(&self) -> Vec<(String, String)> {
        self.health
            .value()
            .warnings
            .into_iter()
            .filter(|warning| warning.id.starts_with(WARNING))
            .map(|warning| (warning.id, warning.text))
            .collect()
    }

    fn notifications(&self) -> Vec<String> {
        self.host
            .calls()
            .into_iter()
            .filter_map(|call| match call {
                HostCall::Notify(_, notification) => Some(notification.title),
                _ => None,
            })
            .collect()
    }
}

impl Drop for Machine {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_machine_with_no_harness_says_so_and_refuses_a_start() {
    let m = Machine::new("none");
    m.discovery.refresh(None).await.expect("the probes run");
    for id in ["claude", "codex"] {
        let row = m.row(id);
        assert!(!row.installed && !row.available, "{row:?}");
        assert_eq!(row.login, LoginState::Unknown);
        assert!(row.checked_at.is_some());
        let hint = row.hint.expect("a hint");
        assert!(
            hint.contains(&format!("`{id}` is not on the PATH")),
            "{hint}"
        );
    }
    m.hub.flush();
    assert_eq!(
        m.hub.snapshot(Projection::Harnesses).order,
        ["claude", "codex"]
    );
    assert_eq!(
        m.warnings(),
        [
            (
                "harnesses:claude".to_owned(),
                "Claude Code is not installed".to_owned()
            ),
            (
                "harnesses:codex".to_owned(),
                "Codex is not installed".to_owned()
            ),
        ]
    );
    // Not installed is no notification: only a change to `logged_out` is.
    assert!(m.notifications().is_empty());
    let refused = m
        .discovery
        .ensure(Harness::Codex)
        .await
        .expect_err("no codex");
    assert_eq!(refused.code, ErrorCode::HarnessUnavailable);
    assert_eq!(refused.details["harness"], "codex");
    assert_eq!(refused.details["reason"], "not_installed");
    assert!(refused.details["hint"].as_str().is_some());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_harness_that_is_not_logged_in_has_one_warning_and_one_notification() {
    let m = Machine::new("out");
    m.program("claude", CLAUDE_IN);
    m.program("codex", CODEX_OUT);
    m.discovery.refresh(None).await.expect("the probes run");
    let codex = m.row("codex");
    assert!(codex.installed && !codex.available, "{codex:?}");
    assert_eq!(codex.login, LoginState::LoggedOut);
    assert_eq!(codex.login_source, Some(LoginSource::Probe));
    assert_eq!(codex.version.as_deref(), Some("0.162.1"));
    assert!(
        codex
            .hint
            .as_deref()
            .is_some_and(|h| h.contains("codex login"))
    );
    assert_eq!(
        m.warnings(),
        [(
            "harnesses:codex".to_owned(),
            "Codex is not logged in".to_owned()
        )]
    );
    assert_eq!(m.notifications(), ["Codex is not logged in"]);
    // The same state again posts nothing.
    m.discovery.refresh(Some("codex")).await.expect("a probe");
    assert_eq!(m.notifications().len(), 1);
    let refused = m
        .discovery
        .ensure(Harness::Codex)
        .await
        .expect_err("logged out");
    assert_eq!(refused.details["reason"], "logged_out");
    // The person logs in: the warning goes, and the next logout is a new notification.
    m.program("codex", CODEX_IN);
    m.discovery.ensure(Harness::Codex).await.expect("logged in");
    assert_eq!(m.row("codex").login, LoginState::LoggedIn);
    assert!(m.warnings().is_empty());
    m.program("codex", CODEX_OUT);
    m.discovery.refresh(None).await.expect("the probes run");
    assert_eq!(m.notifications().len(), 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn both_harnesses_logged_in_have_their_method_and_their_version() {
    let m = Machine::new("both");
    m.program("claude", CLAUDE_IN);
    m.program("codex", CODEX_IN);
    m.discovery.refresh(None).await.expect("the probes run");
    let claude = m.row("claude");
    assert_eq!(claude.login, LoginState::LoggedIn);
    assert_eq!(claude.login_method, Some(LoginMethod::Subscription));
    assert_eq!(claude.version.as_deref(), Some("2.1.292"));
    // The recordings cover 2.1.x, and not Codex 0.162.
    assert!(claude.validated);
    assert!(!m.row("codex").validated);
    assert!(claude.available && claude.hint.is_none(), "{claude:?}");
    assert!(
        claude
            .path
            .as_deref()
            .is_some_and(|path| path.ends_with("/bin/claude"))
    );
    // The row has no word of the output: not the address of the account.
    let text = serde_json::to_string(&claude).expect("json");
    assert!(!text.contains("email"), "{text}");
    assert!(m.warnings().is_empty());
    assert!(m.notifications().is_empty());
    m.discovery.ensure(Harness::Claude).await.expect("ready");
    assert!(
        m.discovery.refresh(Some("gemini")).await.is_err(),
        "no such manifest"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_run_that_shows_a_login_wins_until_a_probe_gives_logged_in() {
    let m = Machine::new("run");
    m.program("claude", CLAUDE_IN);
    m.discovery.refresh(Some("claude")).await.expect("a probe");
    m.discovery.run_logged_out("claude");
    let row = m.row("claude");
    assert_eq!(row.login, LoginState::LoggedOut);
    assert_eq!(row.login_source, Some(LoginSource::Run));
    assert!(!row.available);
    assert_eq!(m.notifications(), ["Claude Code is not logged in"]);
    // A probe that says nothing keeps what the run showed, and a start goes on with `unknown`.
    m.program(
        "claude",
        "case \"$1\" in --version) echo 2.1.292 ;; *) echo '?' ;; esac",
    );
    m.discovery
        .ensure(Harness::Claude)
        .await
        .expect("unknown goes on");
    assert_eq!(m.row("claude").login_source, Some(LoginSource::Run));
    // The next probe that gives `logged_in` wins.
    m.program("claude", CLAUDE_IN);
    m.discovery.refresh(None).await.expect("the probes run");
    let row = m.row("claude");
    assert_eq!(row.login, LoginState::LoggedIn);
    assert_eq!(row.login_source, Some(LoginSource::Probe));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_wrapper_whose_child_keeps_the_output_open_does_not_hold_the_round() {
    let m = Machine::new("wrapper");
    // The npm `codex` is a script that starts the native program. A child that outlives the
    // script keeps the pipes open.
    m.program(
        "claude",
        r#"case "$1 $2" in
  "--version "*) sleep 60 & echo '2.1.292 (Claude Code)' ;;
  "auth status") sleep 60 & echo '{"loggedIn": true, "authMethod": "claude.ai"}' ;;
  *) exit 2 ;;
esac"#,
    );
    tokio::time::timeout(Duration::from_secs(4), m.discovery.refresh(Some("claude")))
        .await
        .expect("the round ends")
        .expect("a probe");
    let row = m.row("claude");
    assert_eq!(row.login, LoginState::LoggedIn, "{row:?}");
    assert_eq!(row.version.as_deref(), Some("2.1.292"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_first_ask_for_a_row_takes_the_round_that_runs() {
    let m = Machine::new("first");
    let count = m.dir.join("count");
    m.program(
        "claude",
        &format!(
            r#"case "$1 $2" in
  "--version "*) echo '2.1.292 (Claude Code)' ;;
  "auth status") echo run >> {}; sleep 1; echo '{{"loggedIn": true, "authMethod": "claude.ai"}}' ;;
  *) exit 2 ;;
esac"#,
            count.display()
        ),
    );
    let runs = || {
        fs::read_to_string(&count)
            .map(|text| text.lines().count())
            .unwrap_or(0)
    };
    // The round of the start of the core asks the harness, and `agent.choices` comes in the
    // meantime.
    let discovery = Arc::clone(&m.discovery);
    let round = tokio::spawn(async move { discovery.refresh(None).await });
    let until = Instant::now() + Duration::from_secs(10);
    while runs() == 0 {
        assert!(Instant::now() < until, "the round does not start");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let row = m.discovery.row_or_probe("claude").await.expect("a row");
    assert_eq!(row.login, LoginState::LoggedIn);
    assert_eq!(runs(), 1, "the call probed the harness again");
    round.await.expect("the round").expect("the probes run");
    // A row that is there is the answer, with no probe.
    m.discovery.row_or_probe("claude").await.expect("a row");
    assert_eq!(runs(), 1);
}
