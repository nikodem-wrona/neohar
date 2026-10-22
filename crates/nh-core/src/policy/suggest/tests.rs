use nh_store::FingerprintRow;

use super::*;

const NOW: i64 = 100 * 86_400_000;
const DAY: i64 = 86_400_000;

fn row(fingerprint: &str, decision: &str, ago_days: i64) -> FingerprintRow {
    FingerprintRow {
        fingerprint: fingerprint.to_owned(),
        decision: decision.to_owned(),
        interaction: None,
        at: NOW - ago_days * DAY,
        harness: Some("claude".to_owned()),
        tool: Some("Bash".to_owned()),
        pattern: Some("pnpm test:*".to_owned()),
        workspace: Some("/work".to_owned()),
        eligible: true,
    }
}

fn many(count: usize, decision: &str, ago_days: i64) -> Vec<FingerprintRow> {
    (0..count).map(|_| row("f1", decision, ago_days)).collect()
}

#[test]
fn five_approvals_and_no_denial_make_a_suggestion() {
    let found = suggest(&many(5, "allow", 1), NOW);
    assert_eq!(found.len(), 1);
    let s = &found[0];
    assert_eq!(s.approvals, 5);
    assert_eq!(s.fingerprint, "f1");
    assert_eq!(s.rule.command.as_deref(), Some("pnpm test:*"));
    assert_eq!(s.rule.harness, [Harness::Claude]);
    assert_eq!(s.rule.tool, "Bash");
    assert_eq!(s.rule.workspace.as_deref(), Some("/work"));
    assert_eq!(s.rule.decision, PolicyDecision::Allow);
    assert!(s.rule.paths.is_empty());
    assert_eq!(s.last_at, Timestamp(NOW - DAY));
}

#[test]
fn four_approvals_are_not_enough() {
    assert!(suggest(&many(4, "allow", 1), NOW).is_empty());
}

#[test]
fn one_denial_in_the_window_ends_the_suggestion() {
    let mut rows = many(9, "allow", 1);
    rows.push(row("f1", "deny", 2));
    assert!(suggest(&rows, NOW).is_empty());
}

#[test]
fn only_the_last_thirty_days_count() {
    // Four recent approvals and one old one: four.
    let mut rows = many(4, "allow", 1);
    rows.push(row("f1", "allow", 31));
    assert!(suggest(&rows, NOW).is_empty());
    // An old denial does not stop a rule.
    let mut rows = many(5, "allow", 1);
    rows.push(row("f1", "deny", 45));
    assert_eq!(suggest(&rows, NOW).len(), 1);
}

#[test]
fn a_fingerprint_that_no_rule_may_cover_never_comes_out() {
    let rows: Vec<FingerprintRow> = (0..20)
        .map(|_| FingerprintRow {
            eligible: false,
            ..row("f1", "allow", 1)
        })
        .collect();
    assert!(suggest(&rows, NOW).is_empty());
    let mut mixed = many(10, "allow", 1);
    mixed[3].eligible = false;
    assert!(suggest(&mixed, NOW).is_empty());
    let no_pattern: Vec<FingerprintRow> = (0..5)
        .map(|_| FingerprintRow {
            pattern: None,
            ..row("f1", "allow", 1)
        })
        .collect();
    assert!(suggest(&no_pattern, NOW).is_empty());
}

#[test]
fn the_most_approved_comes_first() {
    let mut rows = many(5, "allow", 1);
    rows.extend((0..8).map(|_| row("f2", "allow", 2)));
    let found = suggest(&rows, NOW);
    assert_eq!(
        found
            .iter()
            .map(|s| s.fingerprint.as_str())
            .collect::<Vec<_>>(),
        ["f2", "f1"]
    );
}

#[test]
fn a_path_pattern_is_a_rule_for_paths() {
    let rows: Vec<FingerprintRow> = (0..5)
        .map(|_| FingerprintRow {
            tool: Some("Edit".to_owned()),
            pattern: Some("src/app/**".to_owned()),
            ..row("f3", "allow", 1)
        })
        .collect();
    let found = suggest(&rows, NOW);
    assert_eq!(found[0].rule.paths, ["src/app/**"]);
    assert!(found[0].rule.command.is_none());
}

#[test]
fn the_block_is_ready_to_paste_and_reads_back_as_the_same_rule() {
    let found = suggest(&many(5, "allow", 1), NOW);
    let block = &found[0].toml;
    assert!(block.starts_with("[[policy.rule]]\n"), "{block}");
    assert!(block.contains("command = \"pnpm test:*\""));
    assert!(block.contains("harness = [\"claude\"]"));
    let parsed: toml::Table = block.parse().expect("TOML");
    let rules = parsed["policy"]["rule"].as_array().expect("an array");
    assert_eq!(rules.len(), 1);
    let rule: PolicyRule =
        serde_json::from_value(serde_json::to_value(&rules[0]).expect("JSON")).expect("a rule");
    assert_eq!(rule, found[0].rule);
}

#[test]
fn text_with_quotes_is_escaped_in_the_block() {
    let rule = PolicyRule {
        id: Some("a\"b".to_owned()),
        reason: Some("line \"one\"\nline two".to_owned()),
        ..rule_for(Harness::Codex, "exec_command", "git status:*", None, None)
    };
    let block = toml_block(&rule);
    let parsed: toml::Table = block.parse().expect("TOML");
    assert_eq!(parsed["policy"]["rule"][0]["id"].as_str(), Some("a\"b"));
    assert_eq!(
        parsed["policy"]["rule"][0]["reason"].as_str(),
        Some("line \"one\"\nline two")
    );
}
