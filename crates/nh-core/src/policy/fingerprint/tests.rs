use nh_proto::api::common::{Harness, Risk};

use super::*;

fn command(line: &str, workspace: &str) -> (Subject, Vec<String>) {
    let argv = line.split_whitespace().map(str::to_owned).collect();
    (
        Subject {
            harness: Harness::Claude,
            tool: "Bash".to_owned(),
            command: Some(line.to_owned()),
            paths: Vec::new(),
            workspace: workspace.to_owned(),
        },
        argv,
    )
}

fn files(paths: &[&str]) -> Subject {
    Subject {
        harness: Harness::Claude,
        tool: "Edit".to_owned(),
        command: None,
        paths: paths.iter().map(|p| (*p).to_owned()).collect(),
        workspace: "/work".to_owned(),
    }
}

fn of_command(line: &str, risk: Risk) -> Facts {
    let (subject, argv) = command(line, "/work");
    facts_of(&subject, risk, &argv)
}

#[test]
fn a_plain_command_gets_the_prefix_of_a_rule() {
    let facts = of_command("pnpm test --watch", Risk::Low);
    assert!(facts.eligible);
    assert_eq!(facts.pattern.as_deref(), Some("pnpm test:*"));
    assert_eq!(facts.fingerprint.len(), 16);
}

#[test]
fn the_same_kind_of_command_in_the_same_workspace_has_one_fingerprint() {
    let a = of_command("pnpm test", Risk::Low);
    let b = of_command("pnpm test --watch", Risk::Low);
    assert_eq!(a.fingerprint, b.fingerprint);
    let c = of_command("pnpm lint", Risk::Low);
    assert_ne!(a.fingerprint, c.fingerprint);
    let (other_workspace, argv) = command("pnpm test", "/else");
    assert_ne!(
        facts_of(&other_workspace, Risk::Low, &argv).fingerprint,
        a.fingerprint
    );
    let (mut codex, argv) = command("pnpm test", "/work");
    codex.harness = Harness::Codex;
    assert_ne!(
        facts_of(&codex, Risk::Low, &argv).fingerprint,
        a.fingerprint
    );
}

#[test]
fn no_rule_is_suggested_for_a_high_risk() {
    assert!(!of_command("pnpm test", Risk::High).eligible);
    assert!(of_command("pnpm test", Risk::Medium).eligible);
}

#[test]
fn no_rule_is_suggested_for_a_line_with_many_commands() {
    for line in [
        "pnpm test; pnpm lint",
        "pnpm test | tee out",
        "pnpm test && pnpm lint",
        "pnpm test > out",
        "pnpm test < in",
        "pnpm test $(date)",
        "pnpm test `date`",
        "pnpm test\npnpm lint",
    ] {
        let facts = of_command(line, Risk::Low);
        assert!(!facts.eligible, "{line:?}");
        assert!(facts.pattern.is_none(), "{line:?}");
    }
}

#[test]
fn no_rule_is_suggested_for_a_destructive_program() {
    for line in [
        "rm -rf build",
        "sudo ls",
        "chmod -R 777 .",
        "dd if=a of=b",
        "mkfs /dev/x",
    ] {
        assert!(!of_command(line, Risk::High).eligible, "{line}");
    }
    // Even with a lower risk, the program has no rule prefix.
    assert!(!of_command("rm file", Risk::Low).eligible);
}

#[test]
fn a_file_tool_gets_the_directory_of_its_paths_as_a_glob() {
    let facts = facts_of(&files(&["src/app/a.ts"]), Risk::Unknown, &[]);
    assert!(facts.eligible);
    assert_eq!(facts.pattern.as_deref(), Some("src/app/**"));
    let both = facts_of(
        &files(&["src/app/a.ts", "src/app/sub/b.ts"]),
        Risk::Unknown,
        &[],
    );
    assert_eq!(both.pattern.as_deref(), Some("src/app/**"));
    let root_file = facts_of(&files(&["README.md"]), Risk::Unknown, &[]);
    assert_eq!(root_file.pattern.as_deref(), Some("README.md"));
    assert!(root_file.eligible);
}

#[test]
fn no_rule_is_suggested_for_paths_without_one_directory_or_outside_the_workspace() {
    for paths in [
        &["src/a.ts", "lib/b.ts"][..],
        &["a.ts", "b.ts"][..],
        &["/etc/hosts"][..],
        &["src/a.ts", "/etc/hosts"][..],
    ] {
        let facts = facts_of(&files(paths), Risk::Unknown, &[]);
        assert!(!facts.eligible, "{paths:?}");
        assert!(facts.pattern.is_none(), "{paths:?}");
    }
}

#[test]
fn no_rule_is_suggested_for_git_env_keys_and_credentials() {
    for path in [
        ".git/config",
        "sub/.git/hooks/pre-commit",
        ".env",
        ".env.local",
        ".env.example",
        "app/.env.production",
        "keys/server.pem",
        "id_rsa",
        ".ssh/config",
        "config/credentials.json",
        ".aws/credentials",
        "terraform.tfvars",
    ] {
        let facts = facts_of(&files(&[path]), Risk::Unknown, &[]);
        assert!(!facts.eligible, "{path}");
    }
    assert!(facts_of(&files(&["src/environment.ts"]), Risk::Unknown, &[]).eligible);
}

#[test]
fn a_request_without_a_command_or_paths_has_no_pattern() {
    let mut subject = files(&[]);
    subject.tool = "WebSearch".to_owned();
    let facts = facts_of(&subject, Risk::Unknown, &[]);
    assert!(!facts.eligible);
    assert!(facts.pattern.is_none());
}

#[test]
fn the_facts_survive_the_column_of_the_store() {
    let facts = of_command("pnpm test", Risk::Low);
    let text = serde_json::to_string(&facts).expect("JSON");
    let back: Facts = serde_json::from_str(&text).expect("facts");
    assert_eq!(back, facts);
}
