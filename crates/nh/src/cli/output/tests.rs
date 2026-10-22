use std::path::PathBuf;

use serde_json::json;

use super::*;

fn text(bytes: Vec<u8>) -> String {
    String::from_utf8(bytes).expect("UTF-8")
}

/// Runs `f` with an output and returns what went to stdout and to stderr.
fn capture(json: bool, pretty: bool, f: impl FnOnce(&mut Output<'_>)) -> (String, String) {
    let (mut out, mut err) = (Vec::new(), Vec::new());
    f(&mut Output {
        out: &mut out,
        err: &mut err,
        json,
        pretty,
    });
    (text(out), text(err))
}

#[test]
fn json_is_one_line_unless_stdout_is_a_terminal() {
    let value = json!({"a": 1, "b": [1, 2]});
    let (out, _) = capture(true, false, |o| o.value(&value).expect("written"));
    assert_eq!(out, "{\"a\":1,\"b\":[1,2]}\n");
    let (out, _) = capture(true, true, |o| o.value(&value).expect("written"));
    assert!(out.starts_with("{\n  \"a\": 1,"), "{out}");
    assert_eq!(serde_json::from_str::<Value>(&out).expect("JSON"), value);
}

#[test]
fn pairs_line_up_their_values() {
    let (out, _) = capture(false, false, |o| {
        o.pairs(&[
            ("a", "1".into()),
            ("long label", "2".into()),
            ("bb", "3".into()),
        ])
        .expect("written");
    });
    assert_eq!(out, "a           1\nlong label  2\nbb          3\n");
}

#[test]
fn a_table_has_a_header_and_columns_as_wide_as_their_cells() {
    let (out, _) = capture(false, false, |o| {
        o.table(
            &["ID", "TITLE", "STATE"],
            &[
                vec!["w1".into(), "Build".into(), "running".into()],
                vec!["w12".into(), "A longer title".into(), "done".into()],
            ],
        )
        .expect("written");
    });
    assert_eq!(
        out,
        "ID   TITLE           STATE\nw1   Build           running\nw12  A longer title  done\n"
    );
    // A table with no rows has the header.
    let (out, _) = capture(false, false, |o| {
        o.table(&["A", "B"], &[]).expect("written")
    });
    assert_eq!(out, "A  B\n");
}

fn failure(code: &str) -> CliError {
    CliError::Call(CallError::Rpc(Failure {
        rpc_code: -32000,
        code: code.to_owned(),
        message: "the thing is gone".to_owned(),
        details: json!({"pane": "x"}),
    }))
}

#[test]
fn exit_codes_follow_the_string_code() {
    assert_eq!(failure("not_found").exit_code(), exit::NOT_FOUND);
    assert_eq!(
        failure("already_answered").exit_code(),
        exit::ALREADY_ANSWERED
    );
    for code in [
        "internal",
        "permission_denied",
        "degraded_storage",
        "invalid_params",
        "unsupported",
    ] {
        assert_eq!(failure(code).exit_code(), exit::ERROR, "{code}");
    }
    assert_eq!(CliError::Usage("x".into()).exit_code(), exit::USAGE);
    assert_eq!(
        CliError::Call(CallError::Timeout(std::time::Duration::from_secs(1))).exit_code(),
        exit::ERROR
    );
    assert_eq!(
        (
            exit::OK,
            exit::ERROR,
            exit::USAGE,
            exit::NOT_FOUND,
            exit::ALREADY_ANSWERED
        ),
        (0, 1, 2, 3, 4)
    );
}

#[test]
fn an_error_is_one_line_for_people_and_json_for_scripts() {
    let (out, err) = capture(false, true, |o| o.error(&failure("not_found")));
    assert_eq!(out, "");
    assert_eq!(err, "nh: the thing is gone (not_found)\n");
    let (_, err) = capture(true, false, |o| o.error(&failure("not_found")));
    let parsed: Value = serde_json::from_str(&err).expect("JSON");
    assert_eq!(
        parsed,
        json!({"error": {"code": "not_found", "message": "the thing is gone", "details": {"pane": "x"}}})
    );
    let (_, err) = capture(false, true, |o| o.error(&CliError::Usage("bad".into())));
    assert_eq!(err, "nh: bad\n");
}

#[test]
fn a_core_that_is_not_running_says_where_it_looked_and_what_to_do() {
    let error = CliError::Call(CallError::NotRunning(PathBuf::from(
        "/tmp/neohar-1/default/api.sock",
    )));
    let (_, err) = capture(false, true, |o| o.error(&error));
    assert!(err.contains("not running") && err.contains("/tmp/neohar-1/default/api.sock"));
    assert!(err.contains("--session") && err.contains("--socket"));
    let (_, err) = capture(true, false, |o| o.error(&error));
    assert_eq!(
        serde_json::from_str::<Value>(&err).expect("JSON")["error"]["code"],
        "not_running"
    );
}
