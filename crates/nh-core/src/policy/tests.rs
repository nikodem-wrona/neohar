use std::path::Path;

use nh_proto::api::{
    calls::system::{PolicyDecision, PolicyRule, PolicyScope},
    common::Harness,
    time::Timestamp,
};
use serde_json::json;

use super::*;

fn spec() -> PolicyRule {
    PolicyRule {
        id: None,
        decision: PolicyDecision::Allow,
        harness: Vec::new(),
        tool: "Bash".to_owned(),
        command: Some("pnpm test:*".to_owned()),
        paths: Vec::new(),
        workspace: None,
        reason: None,
    }
}

fn build(spec: &PolicyRule) -> Result<Rule, String> {
    compile(spec, PolicyScope::Config, "config.toml", Timestamp(7), None)
}

#[test]
fn a_rule_without_an_id_gets_the_same_id_every_time() {
    let a = build(&spec()).expect("a rule");
    let b = build(&spec()).expect("a rule");
    assert_eq!(a.id, b.id);
    assert!(a.id.starts_with("r-"), "{}", a.id);
    let mut other = spec();
    other.command = Some("pnpm lint:*".to_owned());
    assert_ne!(build(&other).expect("a rule").id, a.id);
    let mut named = spec();
    named.id = Some("pnpm-test".to_owned());
    assert_eq!(build(&named).expect("a rule").id, "pnpm-test");
}

#[test]
fn a_rule_that_is_not_valid_says_why() {
    let mut cases: Vec<(PolicyRule, &str)> = Vec::new();
    let mut empty_tool = spec();
    empty_tool.tool = "  ".to_owned();
    cases.push((empty_tool, "tool"));
    let mut both = spec();
    both.paths = vec!["src/**".to_owned()];
    cases.push((both, "not both"));
    let mut star = spec();
    star.command = Some("pnpm * test".to_owned());
    cases.push((star, "only at the end"));
    let mut absolute = spec();
    absolute.command = None;
    absolute.paths = vec!["/etc/**".to_owned()];
    cases.push((absolute, "relative"));
    let mut up = spec();
    up.command = None;
    up.paths = vec!["../x".to_owned()];
    cases.push((up, "no `..`"));
    let mut everything = spec();
    everything.tool = "*".to_owned();
    everything.command = None;
    cases.push((everything, "every tool"));
    let mut bad_id = spec();
    bad_id.id = Some("has space".to_owned());
    cases.push((bad_id, "id"));
    for (spec, expected) in cases {
        let error = build(&spec).expect_err("refused");
        assert!(error.contains(expected), "{error} should say {expected}");
    }
}

#[test]
fn a_deny_for_every_tool_is_valid() {
    let mut deny_all = spec();
    deny_all.decision = PolicyDecision::Deny;
    deny_all.tool = "*".to_owned();
    deny_all.command = None;
    assert!(build(&deny_all).is_ok());
}

#[test]
fn a_workspace_starting_with_a_tilde_is_the_home_of_the_user() {
    let mut s = spec();
    s.workspace = Some("~/code/app".to_owned());
    let rule = compile(
        &s,
        PolicyScope::Config,
        "config.toml",
        Timestamp(0),
        Some(Path::new("/home/me")),
    )
    .expect("a rule");
    assert_eq!(rule.workspace.as_deref(), Some("/home/me/code/app"));
}

#[test]
fn the_spec_of_a_rule_comes_back_as_it_was_written_with_its_id() {
    let mut s = spec();
    s.harness = vec![Harness::Claude];
    s.reason = Some("tests are safe".to_owned());
    let rule = build(&s).expect("a rule");
    let back = rule.spec();
    assert_eq!(back.id.as_deref(), Some(rule.id.as_str()));
    assert_eq!(back.command.as_deref(), Some("pnpm test:*"));
    assert_eq!(back.harness, [Harness::Claude]);
    assert_eq!(back.reason.as_deref(), Some("tests are safe"));
}

#[test]
fn a_row_of_the_table_is_a_rule_and_back() {
    let rule = build(&spec()).expect("a rule");
    let row = to_row(&rule);
    assert_eq!(row.scope, "config");
    assert_eq!(row.created_at, 7);
    let back = from_row(&row, None).expect("a rule");
    assert_eq!(back.id, rule.id);
    assert_eq!(back.command, rule.command);
    let mut session = row.clone();
    session.scope = "session".to_owned();
    assert_eq!(
        from_row(&session, None).expect("a rule").scope,
        PolicyScope::Session
    );
}

#[test]
fn the_rules_of_the_file_that_are_not_valid_are_reported_and_the_others_work() {
    let mut config = Config::default();
    config.policy.rule = vec![
        json!({"decision": "allow", "tool": "Bash", "command": "pnpm test:*"}),
        json!({"decision": "maybe", "tool": "Bash"}),
        json!({"decision": "allow", "tool": "Bash", "command": "a * b"}),
        json!({"decision": "deny", "tool": "Bash", "command": "rm:*", "unknown_key": 1}),
    ];
    let (rules, problems) = compile_config(&config, None);
    assert_eq!(rules.len(), 1);
    assert_eq!(
        rules[0]
            .command
            .as_ref()
            .map(CommandPattern::text)
            .as_deref(),
        Some("pnpm test:*")
    );
    assert_eq!(problems.len(), 3, "{problems:?}");
    assert!(problems.iter().all(|p| p.starts_with("config.toml: ")));
}

#[test]
fn the_table_and_the_file_make_one_list_and_a_change_of_the_config_is_seen() {
    let mut policy = Policy::default();
    let first = Arc::new(Config::default());
    assert!(policy.rules(&first, None).is_empty());
    policy.put(build(&spec()).expect("a rule"));
    assert_eq!(policy.rules(&first, None).len(), 1);
    let mut changed = Config::default();
    changed.policy.rule = vec![json!({"decision": "deny", "tool": "Bash", "command": "rm:*"})];
    let second = Arc::new(changed);
    let rules = policy.rules(&second, None);
    assert_eq!(rules.len(), 2);
    assert_eq!(
        rules[0].decision,
        PolicyDecision::Deny,
        "the file comes first"
    );
    let id = policy.table()[0].id.clone();
    assert!(policy.remove(&id));
    assert!(!policy.remove(&id));
    assert_eq!(policy.rules(&second, None).len(), 1);
}

#[test]
fn allow_always_makes_a_session_rule_for_the_pattern_of_the_approval() {
    let facts = fingerprint::Facts {
        fingerprint: "0123456789abcdef".to_owned(),
        harness: Harness::Claude,
        tool: "Bash".to_owned(),
        pattern: Some("pnpm test:*".to_owned()),
        workspace: "/work".to_owned(),
        eligible: true,
    };
    let rule = session_rule(&facts, Timestamp(5)).expect("a rule");
    assert_eq!(rule.id, "session-01234567");
    assert_eq!(rule.scope, PolicyScope::Session);
    assert_eq!(rule.harness, [Harness::Claude]);
    assert_eq!(rule.workspace.as_deref(), Some("/work"));
    assert_eq!(
        rule.command.as_ref().map(CommandPattern::text).as_deref(),
        Some("pnpm test:*")
    );
    let mut not_eligible = facts.clone();
    not_eligible.eligible = false;
    assert!(session_rule(&not_eligible, Timestamp(5)).is_none());
    let mut no_pattern = facts;
    no_pattern.pattern = None;
    assert!(session_rule(&no_pattern, Timestamp(5)).is_none());
}
