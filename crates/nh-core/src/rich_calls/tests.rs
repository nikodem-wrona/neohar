//! The mapping from `nh-rich` to the API keeps every field, and the API types still say what the
//! UI expects.

use nh_rich::command::{Context, explain};
use serde_json::json;

use super::*;

#[test]
fn a_diff_keeps_its_lines_numbers_and_ranges() {
    let diff = diff(&nh_rich::diff::between(
        "let total = price * count;\n",
        "let total = price * quantity;\n",
        Some("a.rs"),
        None,
    ));
    let value = serde_json::to_value(&diff).expect("json");
    let file = &value["files"][0];
    assert_eq!(file["path"], "a.rs");
    assert_eq!(file["status"], "modified");
    assert_eq!(file["stats"], json!({"add": 1, "del": 1}));
    assert_eq!(file["truncated"], false);
    let lines = file["hunks"][0]["lines"].as_array().expect("lines");
    assert_eq!(lines[0]["kind"], "del");
    assert_eq!(lines[1]["kind"], "add");
    // The line numbers of a diff of two texts start at 1.
    assert_eq!(lines[0]["old"], 1);
    assert_eq!(lines[1]["new"], 1);
    assert_eq!(lines[0]["intra"], json!([[20, 25]]));
    assert_eq!(lines[1]["intra"], json!([[20, 28]]));
    // A span is `[start, end, class]`.
    let span = &lines[1]["text"]["spans"][0];
    assert!(
        span[0].is_u64() && span[1].is_u64() && span[2].is_string(),
        "{span}"
    );
}

#[test]
fn markdown_keeps_the_flag() {
    let result = markdown(&nh_rich::markdown("# T\n\n[x](https://a.example/)"));
    assert!(result.html.contains("<h1>T</h1>"));
    assert!(result.html.contains("nh://open?url="));
    assert!(!result.truncated);
}

#[test]
fn highlighted_text_has_class_spans() {
    let result = spans(&nh_rich::highlight::highlight("fn a() {}", "rust"));
    assert_eq!(result.text, "fn a() {}");
    assert_eq!(result.spans[0], (0, 2, "kw".to_owned()));
}

#[test]
fn a_command_keeps_its_risk_and_reasons() {
    let project = nh_rich::project::Project::default();
    let ctx = Context {
        workspace: std::path::Path::new("/w"),
        cwd: std::path::Path::new("/w"),
        home: None,
        project: &project,
        network_allow: &[],
    };
    let wire = command(&explain("git push --force", &ctx));
    assert_eq!(wire.risk, Risk::High);
    assert_eq!(wire.argv, ["git", "push", "--force"]);
    assert!(!wire.reasons.is_empty());
    assert_eq!(wire.display.text, "git push --force");
    assert_eq!(wire.policy_hint, None);
}

#[test]
fn every_risk_of_the_rules_has_an_api_risk() {
    use nh_rich::command::Risk as Rules;
    assert_eq!(risk(Rules::Low), Risk::Low);
    assert_eq!(risk(Rules::Unknown), Risk::Unknown);
    assert_eq!(risk(Rules::Medium), Risk::Medium);
    assert_eq!(risk(Rules::High), Risk::High);
}
