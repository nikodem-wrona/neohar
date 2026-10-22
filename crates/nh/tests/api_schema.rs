//! `nh api schema` prints the contract as JSON.

use std::process::Command;

fn run(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_nh"))
        .args(args)
        .output()
        .expect("nh runs")
}

#[test]
fn the_schema_is_one_json_document_on_stdout() {
    let output = run(&["api", "schema"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let doc: serde_json::Value = serde_json::from_slice(&output.stdout).expect("JSON");
    assert_eq!(doc["api_version"], "neohar/1");
    let methods = doc["methods"].as_array().expect("methods");
    assert!(methods.iter().any(|m| m["name"] == "pane.split"));
    assert!(output.stdout.ends_with(b"}\n"), "one line break at the end");
}

#[test]
fn two_runs_print_the_same_bytes() {
    assert_eq!(
        run(&["api", "schema"]).stdout,
        run(&["api", "schema"]).stdout
    );
}

#[test]
fn an_unknown_subcommand_is_a_usage_error() {
    let output = run(&["api", "nothing"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
}
