//! The matcher on synthetic trees: a direct program, `npx`, `mise exec`, `bash -c`, a path
//! in the Nix store, `exec -a`, and a wrapper of the user at priority 200.

use std::{collections::BTreeMap, fs, path::PathBuf};

use super::*;
use crate::manifest::loader::{builtin, load};

/// A process of a test.
fn process(pid: i32, ppid: i32, exe: &str, argv: &[&str]) -> Process {
    Process {
        pid,
        ppid,
        start: 1_000 + u64::try_from(pid).unwrap_or_default(),
        exe: Some(PathBuf::from(exe)),
        argv: argv.iter().map(|arg| (*arg).to_owned()).collect(),
        env: None,
    }
}

fn tree(processes: Vec<Process>) -> Tree {
    Tree::new(processes)
}

fn built_in() -> Matcher {
    Matcher::new(&builtin())
}

fn best(matcher: &Matcher, tree: &Tree) -> Option<(String, Harness, i32)> {
    matcher
        .find(tree)
        .best
        .map(|found| (found.manifest, found.harness, found.pid))
}

/// A directory with the manifests of a user, removed at the end.
struct Dir(PathBuf);

impl Dir {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("nh-matcher-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("a directory");
        Self(path)
    }

    fn write(&self, file: &str, text: &str) {
        fs::write(self.0.join(file), text).expect("a file");
    }

    fn matcher(&self) -> Matcher {
        let loaded = load(Some(&self.0));
        assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
        Matcher::new(&loaded)
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn a_program_with_the_name_of_the_harness_matches() {
    let matcher = built_in();
    let found = tree(vec![process(10, 1, "/usr/local/bin/claude", &["claude"])]);
    assert_eq!(
        best(&matcher, &found),
        Some(("claude".to_owned(), Harness::Claude, 10))
    );
    let found = tree(vec![process(
        11,
        1,
        "/opt/homebrew/Caskroom/codex/0.162.0/bin/codex",
        &["codex", "--yolo"],
    )]);
    assert_eq!(
        best(&matcher, &found),
        Some(("codex".to_owned(), Harness::Codex, 11))
    );
}

#[test]
fn a_link_to_a_file_with_the_number_of_the_version_matches_by_its_name() {
    // This is the native install of Claude Code: `claude` links to `versions/2.1.292`, and
    // the system reports the file, so only `argv[0]` has the name.
    let matcher = built_in();
    let found = tree(vec![process(
        10377,
        10374,
        "/Users/x/.local/share/claude/versions/2.1.292",
        &["claude"],
    )]);
    assert_eq!(
        best(&matcher, &found).map(|(id, ..)| id),
        Some("claude".to_owned())
    );
}

#[test]
fn a_shell_does_not_match() {
    let matcher = built_in();
    let found = tree(vec![
        process(1, 0, "/bin/zsh", &["-zsh"]),
        process(2, 1, "/usr/bin/git", &["git", "status"]),
    ]);
    assert_eq!(matcher.find(&found), Found::default());
    assert_eq!(matcher.find(&Tree::default()), Found::default());
}

#[test]
fn npx_and_bunx_match_on_the_node_that_runs_the_script() {
    let matcher = built_in();
    let found = tree(vec![
        process(
            20,
            1,
            "/usr/local/bin/node",
            &[
                "node",
                "/usr/lib/node_modules/npm/bin/npx-cli.js",
                "@anthropic-ai/claude-code",
            ],
        ),
        process(
            21,
            20,
            "/usr/local/bin/node",
            &[
                "node",
                "--no-warnings",
                "/home/u/.npm/_npx/9f/node_modules/@anthropic-ai/claude-code/cli.js",
            ],
        ),
    ]);
    let hit = matcher.find(&found).best.expect("a match");
    assert_eq!((hit.harness, hit.pid, hit.place), (Harness::Claude, 21, 1));
    // Bun runs the same script.
    let found = tree(vec![
        process(
            30,
            1,
            "/home/u/.bun/bin/bunx",
            &["bunx", "@anthropic-ai/claude-code"],
        ),
        process(
            31,
            30,
            "/home/u/.bun/bin/bun",
            &[
                "bun",
                "/home/u/.bun/install/cache/@anthropic-ai/claude-code@2.1.0/cli.js",
            ],
        ),
    ]);
    assert_eq!(best(&matcher, &found).map(|(_, _, pid)| pid), Some(31));
    // A node that runs another script does not match.
    let found = tree(vec![process(
        40,
        1,
        "/usr/local/bin/node",
        &["node", "server.js"],
    )]);
    assert_eq!(matcher.find(&found), Found::default());
}

#[test]
fn a_wrapper_that_starts_the_program_is_unwrapped() {
    let matcher = built_in();
    // `mise exec -- claude`.
    let found = tree(vec![
        process(
            50,
            1,
            "/opt/homebrew/bin/mise",
            &["mise", "exec", "--", "claude"],
        ),
        process(
            51,
            50,
            "/Users/x/.local/share/claude/versions/2.1.292",
            &["claude"],
        ),
    ]);
    let hit = matcher.find(&found).best.expect("a match");
    assert_eq!((hit.harness, hit.pid, hit.place), (Harness::Claude, 51, 1));
    // `bash -c 'cd app && codex'`, two levels down.
    let found = tree(vec![
        process(60, 1, "/bin/bash", &["bash", "-c", "cd app && codex"]),
        process(61, 60, "/usr/bin/direnv", &["direnv", "exec", ".", "codex"]),
        process(62, 61, "/usr/local/bin/codex", &["codex"]),
    ]);
    let hit = matcher.find(&found).best.expect("a match");
    assert_eq!((hit.harness, hit.pid, hit.place), (Harness::Codex, 62, 2));
}

#[test]
fn a_path_in_the_nix_store_matches() {
    let matcher = built_in();
    let found = tree(vec![process(
        70,
        1,
        "/nix/store/abc123-claude-code-2.1.0/bin/.claude-wrapped",
        &[".claude-wrapped"],
    )]);
    assert_eq!(
        best(&matcher, &found).map(|(id, ..)| id),
        Some("claude".to_owned())
    );
}

#[test]
fn the_marker_in_the_environment_matches_only_when_the_walk_read_the_environment() {
    let matcher = built_in();
    assert!(matcher.wants_env());
    let mut child = process(80, 1, "/usr/bin/sh", &["sh", "-c", "tool"]);
    assert_eq!(matcher.find(&tree(vec![child.clone()])), Found::default());
    child.env = Some(BTreeMap::from([("CLAUDECODE".to_owned(), "1".to_owned())]));
    assert_eq!(
        best(&matcher, &tree(vec![child.clone()])).map(|(id, ..)| id),
        Some("claude".to_owned())
    );
    child.env = Some(BTreeMap::from([("CLAUDECODE".to_owned(), "0".to_owned())]));
    assert_eq!(matcher.find(&tree(vec![child])), Found::default());
}

#[test]
fn codex_with_the_marker_of_claude_code_in_its_environment_is_codex() {
    // An app that the Bash tool of Claude Code started gave `CLAUDECODE=1` to its panes.
    let matcher = built_in();
    let marker = || Some(BTreeMap::from([("CLAUDECODE".to_owned(), "1".to_owned())]));
    let mut codex = process(120, 1, "/opt/homebrew/bin/codex", &["codex"]);
    codex.env = marker();
    let result = matcher.find(&tree(vec![codex]));
    assert_eq!(
        result
            .best
            .map(|hit| (hit.manifest, hit.harness, hit.pid, hit.place)),
        Some(("codex".to_owned(), Harness::Codex, 120, 0))
    );
    assert!(result.tied.is_empty());
    // The launcher of npm is nearer to the leader and matches only by the marker.
    let mut node = process(
        121,
        1,
        "/usr/local/bin/node",
        &["node", "/usr/local/bin/codex"],
    );
    node.env = marker();
    let mut native = process(122, 121, "/usr/local/lib/codex/codex", &["codex"]);
    native.env = marker();
    assert_eq!(
        best(&matcher, &tree(vec![node, native])),
        Some(("codex".to_owned(), Harness::Codex, 122))
    );
}

#[test]
fn codex_that_claude_code_started_leaves_claude_code_the_run_of_the_pane() {
    // The Bash tool of Claude Code runs codex: both names match, and Claude Code is nearer.
    let matcher = built_in();
    let mut bash = process(131, 130, "/bin/bash", &["bash", "-c", "codex exec hi"]);
    bash.env = Some(BTreeMap::from([("CLAUDECODE".to_owned(), "1".to_owned())]));
    let mut codex = process(
        132,
        131,
        "/opt/homebrew/bin/codex",
        &["codex", "exec", "hi"],
    );
    codex.env = bash.env.clone();
    let found = tree(vec![
        process(130, 1, "/Users/me/.local/bin/claude", &["claude"]),
        bash,
        codex,
    ]);
    let result = matcher.find(&found);
    assert_eq!(
        result.best.map(|hit| (hit.manifest, hit.pid)),
        Some(("claude".to_owned(), 130))
    );
    assert_eq!(result.tied, vec!["codex".to_owned()]);
}

#[test]
fn exec_a_is_found_by_the_name_in_argv0() {
    let dir = Dir::new("execa");
    dir.write(
        "mine.toml",
        r#"
schema = 1
id = "mine"
extends = "codex"
[detect]
priority = 150
[[detect.process]]
argv0_basename = ["my-agent"]
"#,
    );
    let matcher = dir.matcher();
    let found = tree(vec![process(
        90,
        1,
        "/opt/bin/some-binary",
        &["my-agent", "--x"],
    )]);
    let hit = matcher.find(&found).best.expect("a match");
    assert_eq!(
        (hit.manifest.as_str(), hit.harness),
        ("mine", Harness::Codex)
    );
    // The program with its own name does not match this rule.
    let found = tree(vec![process(91, 1, "/opt/bin/some-binary", &["other"])]);
    assert_eq!(matcher.find(&found), Found::default());
}

#[test]
fn a_wrapper_of_the_user_at_priority_200_beats_the_built_in() {
    let dir = Dir::new("wrapper");
    dir.write(
        "cc.toml",
        r#"
schema = 1
id = "cc"
extends = "claude"
[detect]
priority = 200
[[detect.process]]
argv_regex = '(^|/)cc( |$)'
"#,
    );
    let matcher = dir.matcher();
    // `cc` is a shell script, so the wrapper process is a shell and claude is its child.
    let found = tree(vec![
        process(
            100,
            1,
            "/bin/bash",
            &["bash", "/home/u/bin/cc", "--model", "opus"],
        ),
        process(
            101,
            100,
            "/usr/local/bin/claude",
            &["claude", "--model", "opus"],
        ),
    ]);
    let result = matcher.find(&found);
    let hit = result.best.expect("a match");
    assert_eq!(
        (hit.manifest.as_str(), hit.harness, hit.priority, hit.pid),
        ("cc", Harness::Claude, 200, 100)
    );
    assert!(result.tied.is_empty());
    // Without the script, the built-in finds claude.
    let found = tree(vec![process(102, 1, "/usr/local/bin/claude", &["claude"])]);
    assert_eq!(
        best(&matcher, &found).map(|(id, ..)| id),
        Some("claude".to_owned())
    );
}

#[test]
fn equal_priorities_tie_and_the_nearer_process_wins() {
    let dir = Dir::new("tie");
    dir.write(
        "a.toml",
        "schema = 1\nid = \"a\"\nextends = \"claude\"\n[detect]\npriority = 150\n[[detect.process]]\nexe_basename = [\"shared\"]\n",
    );
    dir.write(
        "b.toml",
        "schema = 1\nid = \"b\"\nextends = \"codex\"\n[detect]\npriority = 150\n[[detect.process]]\nexe_basename = [\"shared\"]\n",
    );
    let matcher = dir.matcher();
    let found = tree(vec![process(110, 1, "/usr/bin/shared", &["shared"])]);
    let result = matcher.find(&found);
    assert_eq!(result.best.map(|hit| hit.manifest), Some("a".to_owned()));
    assert_eq!(result.tied, vec!["b".to_owned()]);
}

#[test]
fn a_rule_without_keys_and_a_manifest_of_no_harness_match_nothing() {
    let dir = Dir::new("empty");
    dir.write(
        "loose.toml",
        "schema = 1\nid = \"loose\"\nextends = \"claude\"\n[detect]\npriority = 300\n[[detect.process]]\n",
    );
    dir.write(
        "aider.toml",
        "schema = 1\nid = \"aider\"\n[detect]\npriority = 300\n[[detect.process]]\nexe_basename = [\"aider\"]\n",
    );
    let matcher = dir.matcher();
    let found = tree(vec![process(120, 1, "/usr/bin/aider", &["aider"])]);
    assert_eq!(matcher.find(&found), Found::default());
    let found = tree(vec![process(121, 1, "/bin/zsh", &["zsh"])]);
    assert_eq!(matcher.find(&found), Found::default());
}

#[test]
fn the_process_that_matched_is_named_by_its_pid_and_start_time() {
    let matcher = built_in();
    let found = tree(vec![process(130, 1, "/usr/local/bin/claude", &["claude"])]);
    let hit = matcher.find(&found).best.expect("a match");
    assert_eq!(hit.key(), (130, 1_130));
    assert_eq!(hit.remote_loop, None);
    let found = tree(vec![process(131, 1, "/usr/local/bin/codex", &["codex"])]);
    let hit = matcher.find(&found).best.expect("a match");
    assert_eq!(hit.remote_loop.as_deref(), Some("codex_daemon"));
}

fn words(text: &str) -> Vec<String> {
    text.split_whitespace().map(str::to_owned).collect()
}

#[test]
fn a_flag_or_a_sequence_of_arguments_makes_a_run_yolo() {
    let loaded = builtin();
    let claude = loaded
        .get("claude")
        .and_then(|m| m.yolo.clone())
        .expect("yolo");
    assert!(is_yolo(
        &claude,
        &words("claude --dangerously-skip-permissions")
    ));
    assert!(is_yolo(
        &claude,
        &words("claude --model opus --dangerously-skip-permissions -c")
    ));
    assert!(!is_yolo(&claude, &words("claude --model opus")));
    // The program name is not an argument.
    assert!(!is_yolo(&claude, &words("--dangerously-skip-permissions")));
    let codex = loaded
        .get("codex")
        .and_then(|m| m.yolo.clone())
        .expect("yolo");
    assert!(is_yolo(
        &codex,
        &words("codex --dangerously-bypass-approvals-and-sandbox")
    ));
    assert!(is_yolo(&codex, &words("codex -a never")));
    assert!(is_yolo(
        &codex,
        &words("codex --sandbox danger-full-access -m x")
    ));
    assert!(is_yolo(
        &codex,
        &words("codex --sandbox=danger-full-access")
    ));
    assert!(!is_yolo(&codex, &words("codex -a on-request")));
    assert!(!is_yolo(&codex, &words("codex never -a")));
    assert!(!is_yolo(&Yolo::default(), &words("codex -a never")));
}
