//! The `nh` CLI against a real core: the binary runs as a child process and calls the
//! API socket of a core that runs in the test.

mod core_support;

use std::{
    io::Write,
    process::{Command, Stdio},
};

use core_support::{Harness, TEST_PANE_TOKEN};
use serde_json::{Value, json};

const NH: &str = env!("CARGO_BIN_EXE_nh");

struct Ran {
    code: i32,
    out: String,
    err: String,
}

impl Ran {
    fn json(&self) -> Value {
        serde_json::from_str(&self.out)
            .unwrap_or_else(|e| panic!("stdout is JSON: {e}: {}", self.out))
    }
}

/// Runs `nh` with a clean environment, the given extra variables and stdin.
fn nh(args: &[&str], env: &[(&str, &str)], stdin: &str) -> Ran {
    let mut command = Command::new(NH);
    command
        .args(args)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (name, value) in env {
        command.env(name, value);
    }
    let mut child = command.spawn().expect("nh starts");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(stdin.as_bytes())
        .expect("stdin is written");
    let output = child.wait_with_output().expect("nh ends");
    Ran {
        code: output.status.code().unwrap_or(-1),
        out: String::from_utf8_lossy(&output.stdout).into_owned(),
        err: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

fn socket(h: &Harness) -> String {
    h.api_socket().display().to_string()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn session_info_asks_the_core() {
    let h = Harness::start();
    let ran = nh(&["--socket", &socket(&h), "session", "info"], &[], "");
    assert_eq!(ran.code, 0, "{}", ran.err);
    let info = ran.json();
    assert_eq!(info["name"], "test");
    assert_eq!(info["pid"], std::process::id());
    assert_eq!(
        ran.out.lines().count(),
        1,
        "JSON on one line when stdout is a pipe"
    );
    assert_eq!(ran.err, "");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn api_call_sends_params_from_the_argument_and_from_stdin() {
    let h = Harness::start();
    let socket = socket(&h);
    let ran = nh(
        &[
            "--socket",
            &socket,
            "api",
            "call",
            "config.get",
            r#"{"section":"events"}"#,
        ],
        &[],
        "",
    );
    assert_eq!(ran.code, 0, "{}", ran.err);
    assert_eq!(
        ran.json()["sections"]["events"]["entries"]["sync_retention"]["value"],
        "7d"
    );

    let ran = nh(
        &["--socket", &socket, "api", "call", "config.get", "-"],
        &[],
        r#"{"section":"agents"}"#,
    );
    assert_eq!(ran.code, 0, "{}", ran.err);
    assert!(ran.json()["sections"]["agents"].is_object());

    // No params at all.
    let ran = nh(
        &["--socket", &socket, "api", "call", "machine.info"],
        &[],
        "",
    );
    assert_eq!(ran.code, 0, "{}", ran.err);
    assert_eq!(ran.json()["label"], "local");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_change_through_the_cli_is_seen_by_the_next_call() {
    let h = Harness::start();
    let socket = socket(&h);
    let set = nh(
        &[
            "--socket",
            &socket,
            "api",
            "call",
            "config.set",
            r#"{"key":"approvals.mode","value":"gate"}"#,
        ],
        &[],
        "",
    );
    assert_eq!(set.code, 0, "{}", set.err);
    let get = nh(
        &[
            "--socket",
            &socket,
            "api",
            "call",
            "config.get",
            r#"{"section":"approvals"}"#,
        ],
        &[],
        "",
    );
    assert_eq!(
        get.json()["sections"]["approvals"]["entries"]["mode"],
        json!({"value": "gate", "source": "ui"})
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn errors_have_exit_codes_and_json_on_stderr() {
    let h = Harness::start();
    let socket = socket(&h);
    let call = |method: &str, params: &str| {
        nh(
            &["--socket", &socket, "api", "call", method, params],
            &[],
            "",
        )
    };

    // Not found: exit 3.
    let ran = call("config.get", r#"{"section":"nothing"}"#);
    assert_eq!(ran.code, 3, "{}", ran.err);
    assert_eq!(ran.out, "");
    let error: Value = serde_json::from_str(&ran.err).expect("JSON on stderr");
    assert_eq!(error["error"]["code"], "not_found");

    // Other errors: exit 1.
    for (method, params, code) in [
        ("nothing.here", "{}", "method_not_found"),
        ("config.get", r#"{"section":5}"#, "invalid_params"),
        ("pane.input", "{}", "permission_denied"),
    ] {
        let ran = call(method, params);
        assert_eq!(ran.code, 1, "{method}: {}", ran.err);
        let error: Value = serde_json::from_str(&ran.err).expect("JSON on stderr");
        assert_eq!(error["error"]["code"], code, "{method}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn usage_errors_are_exit_code_2_and_need_no_core() {
    for args in [
        &["nothing"][..],
        &["session"],
        &["api", "call"],
        &["api", "call", "x.y", "{not json"],
        &[
            "--socket",
            "/tmp/nhcli-nothing.sock",
            "api",
            "call",
            "x.y",
            "42",
        ],
    ] {
        let ran = nh(args, &[], "");
        assert_eq!(ran.code, 2, "{args:?}: {}", ran.err);
        assert_eq!(ran.out, "", "{args:?}");
        assert!(!ran.err.is_empty(), "{args:?}");
    }
    let ran = nh(&[], &[], "");
    assert_eq!(ran.code, 2);
    assert!(ran.err.contains("Usage"), "{}", ran.err);
    let help = nh(&["--help"], &[], "");
    assert_eq!(help.code, 0);
    assert!(help.out.contains("Usage: nh"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn no_core_is_exit_code_1_and_says_so() {
    let ran = nh(
        &["--socket", "/tmp/nhcli-nothing.sock", "session", "info"],
        &[],
        "",
    );
    assert_eq!(ran.code, 1);
    assert_eq!(ran.out, "");
    assert!(ran.err.contains("not running"), "{}", ran.err);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_socket_comes_from_the_environment_of_a_pane_or_from_the_session() {
    let h = Harness::start();
    let socket = socket(&h);

    // Inside a pane: `$NEOHAR_SOCKET`.
    let ran = nh(&["session", "info"], &[("NEOHAR_SOCKET", &socket)], "");
    assert_eq!(ran.code, 0, "{}", ran.err);
    assert_eq!(ran.json()["name"], "test");

    // By session name, under the runtime directory of the test.
    let run_dir = h.dir.join("run").display().to_string();
    let ran = nh(
        &["--session", "test", "session", "info"],
        &[("NEOHAR_RUNTIME_DIR", &run_dir)],
        "",
    );
    assert_eq!(ran.code, 0, "{}", ran.err);
    let ran = nh(
        &["session", "info"],
        &[("NEOHAR_SESSION", "test"), ("NEOHAR_RUNTIME_DIR", &run_dir)],
        "",
    );
    assert_eq!(ran.code, 0, "{}", ran.err);
    // Another session does not exist.
    let ran = nh(
        &["--session", "other", "session", "info"],
        &[("NEOHAR_RUNTIME_DIR", &run_dir)],
        "",
    );
    assert_eq!(ran.code, 1);
    assert!(ran.err.contains("not running"), "{}", ran.err);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_cli_does_not_send_the_token_of_the_pane_it_runs_in() {
    let h = Harness::start();
    // `session.info` is not a method for panes. With the token it would be `permission_denied`.
    let ran = nh(
        &["--socket", &socket(&h), "session", "info"],
        &[("NEOHAR_PANE_TOKEN", TEST_PANE_TOKEN)],
        "",
    );
    assert_eq!(ran.code, 0, "{}", ran.err);
}
