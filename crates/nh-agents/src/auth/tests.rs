use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf, time::Instant};

use serde::Deserialize;

use super::*;

/// The outputs that the installed harnesses gave.
const PROBES: &str = include_str!("../../tests/fixtures/auth/probes.toml");

#[derive(Deserialize)]
struct Fixtures {
    probe: Vec<Fixture>,
}

#[derive(Deserialize)]
struct Fixture {
    harness: String,
    case: String,
    state: LoginState,
    #[serde(default)]
    method: Option<LoginMethod>,
    exit: i32,
    stdout: String,
    stderr: String,
}

fn auth_of(harness: &str) -> Auth {
    crate::builtin()
        .get(harness)
        .and_then(|manifest| manifest.auth.clone())
        .expect("the built-in manifest has [auth]")
}

#[test]
fn the_built_in_rules_read_the_recorded_outputs() {
    let fixtures: Fixtures = toml::from_str(PROBES).expect("the fixtures parse");
    assert_eq!(fixtures.probe.len(), 8);
    for fixture in fixtures.probe {
        let output = Output {
            exit: Some(fixture.exit),
            stdout: fixture.stdout,
            stderr: fixture.stderr,
        };
        let verdict = evaluate(&auth_of(&fixture.harness), &output);
        assert_eq!(
            verdict,
            Verdict {
                state: fixture.state,
                method: fixture.method,
            },
            "{} {}",
            fixture.harness,
            fixture.case
        );
    }
}

#[test]
fn a_rule_reads_the_exit_status_the_text_or_a_json_field() {
    let output = |exit: i32, stdout: &str| Output {
        exit: Some(exit),
        stdout: stdout.to_owned(),
        stderr: String::new(),
    };
    let exit = AuthRule {
        exit: Some(0),
        ..AuthRule::default()
    };
    assert!(matches(&exit, &output(0, ""), None));
    assert!(!matches(&exit, &output(2, ""), None));
    let text = AuthRule {
        regex: Some("(?m)^ok$".to_owned()),
        exit: Some(0),
        ..AuthRule::default()
    };
    assert!(matches(&text, &output(0, "x\nok\n"), None));
    assert!(!matches(&text, &output(1, "ok"), None), "every key matches");
    let json = serde_json::json!({"a": {"b": "yes"}, "off": false, "none": null});
    let field = |pointer: &str| AuthRule {
        json: Some(pointer.to_owned()),
        ..AuthRule::default()
    };
    assert!(matches(&field("/a/b"), &output(0, ""), Some(&json)));
    assert!(!matches(&field("/off"), &output(0, ""), Some(&json)));
    assert!(!matches(&field("/none"), &output(0, ""), Some(&json)));
    assert!(!matches(&field("/gone"), &output(0, ""), Some(&json)));
    assert!(!matches(&field("/a/b"), &output(0, ""), None), "no JSON");
    let equals = AuthRule {
        equals: Some(serde_json::json!("yes")),
        ..field("/a/b")
    };
    assert!(matches(&equals, &output(0, ""), Some(&json)));
    // A rule without a key says nothing.
    assert!(!matches(&AuthRule::default(), &output(0, ""), None));
}

#[test]
fn output_that_no_rule_matches_is_unknown() {
    let claude = auth_of("claude");
    let output = Output {
        exit: Some(0),
        stdout: "Usage: claude [options]".to_owned(),
        stderr: String::new(),
    };
    assert_eq!(evaluate(&claude, &output), Verdict::UNKNOWN);
    // A method that `values` does not have gives no method, and the state stays.
    let output = Output {
        exit: Some(0),
        stdout: r#"{"loggedIn": true, "authMethod": "something_new"}"#.to_owned(),
        stderr: String::new(),
    };
    assert_eq!(
        evaluate(&claude, &output),
        Verdict {
            state: LoginState::LoggedIn,
            method: None,
        }
    );
}

/// A directory with programs for a test, removed at the end.
struct Bin(PathBuf);

impl Bin {
    fn new(name: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("nh-agents-auth-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("a directory");
        Self(path)
    }

    fn program(&self, name: &str, body: &str) {
        let file = self.0.join(name);
        fs::write(&file, format!("#!/bin/sh\n{body}\n")).expect("a script");
        fs::set_permissions(&file, fs::Permissions::from_mode(0o755)).expect("permissions");
    }

    fn env(&self) -> Vec<(String, String)> {
        vec![(
            "PATH".to_owned(),
            format!("{}:/usr/bin:/bin", self.0.display()),
        )]
    }
}

impl Drop for Bin {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn a_probe_runs_the_program_of_the_path_with_no_stdin() {
    let bin = Bin::new("runs");
    // `read` fails at once when stdin is closed, so the probe does not wait for a person.
    bin.program(
        "codex",
        "if read -r line; then echo 'stdin is open'; exit 3; fi\n\
         [ \"$1 $2\" = 'login status' ] || exit 4\n\
         echo 'Logged in using ChatGPT' >&2",
    );
    let verdict = probe(&auth_of("codex"), &bin.env());
    assert_eq!(
        verdict,
        Verdict {
            state: LoginState::LoggedIn,
            method: Some(LoginMethod::Subscription),
        }
    );
    // A program that is not on the PATH is unknown: discovery tells that it is not installed.
    assert_eq!(probe(&auth_of("claude"), &bin.env()), Verdict::UNKNOWN);
}

#[test]
fn a_probe_that_takes_too_long_is_unknown() {
    let bin = Bin::new("slow");
    bin.program("codex", "sleep 5\necho 'Logged in using ChatGPT' >&2");
    let started = Instant::now();
    let verdict = probe_with_limit(&auth_of("codex"), &bin.env(), Duration::from_millis(200));
    assert_eq!(verdict, Verdict::UNKNOWN);
    assert!(started.elapsed() < Duration::from_secs(3));
}
