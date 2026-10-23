use std::path::Path;

use nh_proto::paths::{Env, Platform};
use serde_json::{Value, json};

use super::{
    testing::{FakeCore, Reply},
    *,
};

fn env(session: Option<&str>, socket: Option<&str>) -> Environment {
    Environment {
        paths: Env {
            platform: Platform::current(),
            uid: 501,
            home: Some(PathBuf::from("/home/u")),
            config_override: None,
            data_override: None,
            runtime_override: Some(PathBuf::from("/tmp/r")),
            xdg_config_home: None,
            xdg_data_home: None,
            xdg_runtime_dir: None,
        },
        session: session.map(str::to_owned),
        socket: socket.map(PathBuf::from),
    }
}

struct Ran {
    code: u8,
    out: String,
    err: String,
}

fn run_with(args: &[&str], env: &Environment, stdin: &str, tty: bool) -> Ran {
    let args: Vec<String> = args.iter().map(|a| (*a).to_owned()).collect();
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let code = run(&args, env, &mut out, &mut err, &mut stdin.as_bytes(), tty);
    Ran {
        code,
        out: String::from_utf8(out).expect("UTF-8"),
        err: String::from_utf8(err).expect("UTF-8"),
    }
}

fn socket_arg(core: &FakeCore) -> String {
    core.socket.display().to_string()
}

fn info() -> Value {
    json!({
        "name": "default", "version": "0.0.0", "api_version": "1.0", "pid": 4242,
        "started_at": "2026-10-09T10:00:00.000Z",
        "session_uuid": "s-uuid", "machine_uuid": "m-uuid", "log_epoch": 1,
        "protocols": {"frame": 3, "holder": 2},
    })
}

fn socket_for(args: &[&str], env: &Environment) -> Result<PathBuf, CliError> {
    let doc = nh_proto::api::schema::document();
    let matches = args::command(&doc)
        .try_get_matches_from(std::iter::once("nh").chain(args.iter().copied()))
        .expect("the arguments parse");
    socket_path(&args::globals(&matches), env)
}

#[test]
fn the_socket_comes_from_the_flag_then_the_session_then_the_environment() {
    let both = env(Some("from-env"), Some("/tmp/env.sock"));
    let base = ["session", "info"];
    let with = |extra: &[&str]| {
        let mut args: Vec<&str> = extra.to_vec();
        args.extend(base);
        socket_for(&args, &both).expect("a path")
    };
    // `--socket` wins over everything.
    assert_eq!(
        with(&["--socket", "/tmp/flag.sock", "--session", "x"]),
        Path::new("/tmp/flag.sock")
    );
    // `--session` wins over `$NEOHAR_SOCKET`.
    assert_eq!(with(&["--session", "x"]), Path::new("/tmp/r/x/api.sock"));
    // Without flags: the socket of the environment, which every pane has.
    assert_eq!(with(&[]), Path::new("/tmp/env.sock"));
    // Else the session of the environment, and else `default`.
    let session_only = env(Some("from-env"), None);
    assert_eq!(
        socket_for(&base, &session_only).expect("a path"),
        Path::new("/tmp/r/from-env/api.sock")
    );
    assert_eq!(
        socket_for(&base, &env(None, None)).expect("a path"),
        Path::new("/tmp/r/default/api.sock")
    );
    // The flags may come after the command, too.
    assert_eq!(
        socket_for(&["session", "info", "--session", "x"], &both).expect("a path"),
        Path::new("/tmp/r/x/api.sock")
    );
}

#[test]
fn a_session_name_that_cannot_make_a_socket_is_a_usage_error() {
    let error = socket_for(
        &["--session", "Bad Name!", "session", "info"],
        &env(None, None),
    )
    .expect_err("a bad name");
    assert_eq!(error.exit_code(), exit::USAGE);
}

#[test]
fn help_and_version_are_not_errors() {
    let help = run_with(&["--help"], &env(None, None), "", false);
    assert_eq!(help.code, 0);
    assert!(
        help.out.contains("Usage: nh")
            && help.out.contains("session")
            && help.out.contains("--socket")
    );
    assert_eq!(help.err, "");
    let version = run_with(&["--version"], &env(None, None), "", false);
    assert_eq!(version.code, 0);
    assert!(version.out.starts_with("nh "), "{}", version.out);
    let sub = run_with(&["api", "call", "--help"], &env(None, None), "", false);
    assert_eq!(sub.code, 0);
    assert!(sub.out.contains("METHOD") && sub.out.contains("PARAMS"));
}

#[test]
fn wrong_arguments_are_a_usage_error_on_stderr() {
    for args in [
        &[][..],
        &["nothing"],
        &["session"],
        &["session", "info", "extra"],
        &["api", "call"],
        &["--timeout", "soon", "session", "info"],
        &["--bogus"],
    ] {
        let ran = run_with(args, &env(None, None), "", false);
        assert_eq!(ran.code, 2, "{args:?}: {}", ran.err);
        assert_eq!(ran.out, "", "{args:?}");
        assert!(!ran.err.is_empty(), "{args:?}");
    }
}

#[test]
fn session_info_prints_a_table_on_a_terminal() {
    let core = FakeCore::start(vec![Reply::Result(info())]);
    let ran = run_with(
        &["--socket", &socket_arg(&core), "session", "info"],
        &env(None, None),
        "",
        true,
    );
    assert_eq!(ran.code, 0, "{}", ran.err);
    assert!(ran.out.contains("session          default"), "{}", ran.out);
    assert!(ran.out.contains("pid              4242"));
    assert!(ran.out.contains("holder protocol  2"));
    assert!(
        ran.out
            .contains("started          2026-10-09T10:00:00.000Z")
    );
    assert_eq!(core.requests()[0]["method"], "session.info");
}

#[test]
fn session_info_prints_json_when_stdout_is_not_a_terminal_or_with_the_flag() {
    let core = FakeCore::start(vec![Reply::Result(info())]);
    let ran = run_with(
        &["--socket", &socket_arg(&core), "session", "info"],
        &env(None, None),
        "",
        false,
    );
    assert_eq!(ran.code, 0, "{}", ran.err);
    assert_eq!(ran.out.lines().count(), 1, "one line: {}", ran.out);
    assert_eq!(
        serde_json::from_str::<Value>(&ran.out).expect("JSON"),
        info()
    );
    drop(core);

    let core = FakeCore::start(vec![Reply::Result(info())]);
    let ran = run_with(
        &["--json", "--socket", &socket_arg(&core), "session", "info"],
        &env(None, None),
        "",
        true,
    );
    assert_eq!(ran.code, 0, "{}", ran.err);
    assert!(
        ran.out.lines().count() > 5,
        "indented on a terminal: {}",
        ran.out
    );
    assert_eq!(
        serde_json::from_str::<Value>(&ran.out).expect("JSON"),
        info()
    );
}

#[test]
fn api_call_sends_the_params_and_prints_the_result() {
    let core = FakeCore::start(vec![Reply::Result(json!({"lines": ["a", "b"]}))]);
    let ran = run_with(
        &[
            "api",
            "call",
            "pane.read",
            r#"{"pane":"w1:p2","lines":2}"#,
            "--socket",
            &socket_arg(&core),
        ],
        &env(None, None),
        "",
        false,
    );
    assert_eq!(ran.code, 0, "{}", ran.err);
    assert_eq!(
        serde_json::from_str::<Value>(&ran.out).expect("JSON"),
        json!({"lines": ["a", "b"]})
    );
    let request = &core.requests()[0];
    assert_eq!(request["method"], "pane.read");
    assert_eq!(request["params"], json!({"pane": "w1:p2", "lines": 2}));
}

#[test]
fn api_call_reads_params_from_stdin_and_defaults_to_an_empty_object() {
    let core = FakeCore::start(vec![Reply::Result(json!({})), Reply::Result(json!({}))]);
    let socket = socket_arg(&core);
    let ran = run_with(
        &["--socket", &socket, "api", "call", "config.get", "-"],
        &env(None, None),
        r#"{"section":"events"}"#,
        false,
    );
    assert_eq!(ran.code, 0, "{}", ran.err);
    assert_eq!(core.requests()[0]["params"], json!({"section": "events"}));
}

#[test]
fn api_call_without_params_sends_an_empty_object() {
    let core = FakeCore::start(vec![Reply::Result(json!({"ok": true}))]);
    let ran = run_with(
        &[
            "--socket",
            &socket_arg(&core),
            "api",
            "call",
            "session.info",
        ],
        &env(None, None),
        "",
        false,
    );
    assert_eq!(ran.code, 0, "{}", ran.err);
    assert_eq!(core.requests()[0]["params"], json!({}));
}

#[test]
fn bad_params_are_a_usage_error_before_any_connection() {
    // Nothing listens at this socket, so a connection attempt would be exit code 1.
    let ran = run_with(
        &[
            "--socket",
            "/tmp/nhcli-nothing.sock",
            "api",
            "call",
            "x.y",
            "{oops",
        ],
        &env(None, None),
        "",
        false,
    );
    assert_eq!(ran.code, 2, "{}", ran.err);
    assert!(ran.err.contains("not JSON"), "{}", ran.err);
}

#[test]
fn not_found_and_already_answered_have_their_own_exit_codes() {
    // The fake core serves one connection, so each run gets its own.
    for (string, code) in [
        ("not_found", 3),
        ("already_answered", 4),
        ("degraded_storage", 1),
        ("internal", 1),
    ] {
        let core = FakeCore::start(vec![Reply::Error(-32000, string, "the message")]);
        let ran = run_with(
            &["--socket", &socket_arg(&core), "api", "call", "x.y"],
            &env(None, None),
            "",
            true,
        );
        assert_eq!(ran.code, code, "{string}: {}", ran.err);
        assert_eq!(ran.out, "");
        assert_eq!(ran.err, format!("nh: the message ({string})\n"));
    }
}

#[test]
fn errors_are_json_on_stderr_when_the_output_is_json() {
    let core = FakeCore::start(vec![Reply::Error(-32000, "not_found", "no pane")]);
    let ran = run_with(
        &["--socket", &socket_arg(&core), "api", "call", "pane.read"],
        &env(None, None),
        "",
        false,
    );
    assert_eq!(ran.code, 3);
    assert_eq!(ran.out, "");
    let error: Value = serde_json::from_str(&ran.err).expect("JSON on stderr");
    assert_eq!(error["error"]["code"], "not_found");
    assert_eq!(error["error"]["message"], "no pane");
}

#[test]
fn no_core_is_exit_code_1_with_a_hint() {
    let ran = run_with(
        &["--socket", "/tmp/nhcli-nothing.sock", "session", "info"],
        &env(None, None),
        "",
        true,
    );
    assert_eq!(ran.code, 1);
    assert_eq!(ran.out, "");
    assert!(
        ran.err.contains("not running") && ran.err.contains("/tmp/nhcli-nothing.sock"),
        "{}",
        ran.err
    );
}

#[test]
fn a_core_that_does_not_answer_is_exit_code_1_after_the_timeout() {
    let core = FakeCore::start(vec![Reply::Silent(Duration::from_millis(2500))]);
    let started = std::time::Instant::now();
    let ran = run_with(
        &[
            "--timeout",
            "1",
            "--socket",
            &socket_arg(&core),
            "session",
            "info",
        ],
        &env(None, None),
        "",
        false,
    );
    assert_eq!(ran.code, 1);
    assert!(
        started.elapsed() < Duration::from_millis(2400),
        "the CLI gave up at the timeout, not when the core did"
    );
    assert!(ran.err.contains("did not answer"), "{}", ran.err);
}

#[test]
fn the_schema_needs_no_core() {
    let ran = run_with(&["api", "schema"], &env(None, None), "", false);
    assert_eq!(ran.code, 0, "{}", ran.err);
    let document: Value = serde_json::from_str(&ran.out).expect("one JSON document");
    assert!(document["methods"].is_object() || document["methods"].is_array());
    assert!(ran.out.ends_with("}\n"));
}
