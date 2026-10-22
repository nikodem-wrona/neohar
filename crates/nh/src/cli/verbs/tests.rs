use std::path::Path;

use nh_proto::api::methods::Method;
use serde_json::{Value, json};

use super::*;
use crate::cli::args;

fn doc() -> Value {
    nh_proto::api::schema::document()
}

/// The params that a line of arguments gives, and the method it calls.
fn parsed(line: &[&str]) -> Result<(String, Value), CliError> {
    let doc = doc();
    let matches = args::command(&doc)
        .try_get_matches_from(std::iter::once("nh").chain(line.iter().copied()))
        .map_err(|e| CliError::Usage(e.to_string()))?;
    let (noun, sub) = matches.subcommand().expect("a noun");
    let (name, verb_matches) = sub.subcommand().expect("a verb");
    let verb = VERBS
        .iter()
        .find(|v| v.noun == noun && v.name == name)
        .expect("a verb of the table");
    let value = params(&doc, verb, verb_matches, Path::new("/work/repo"))?;
    Ok((verb.method.to_owned(), value))
}

fn ok(line: &[&str]) -> (String, Value) {
    parsed(line).unwrap_or_else(|e| panic!("{line:?}: {e:?}"))
}

#[test]
fn every_method_that_a_person_can_call_has_a_verb_or_is_on_the_list() {
    let covered = covered();
    let missing: Vec<&str> = Method::ALL
        .iter()
        .filter(|m| m.scopes().cli)
        .map(|m| m.name())
        .filter(|name| !covered.contains(name))
        .collect();
    assert!(missing.is_empty(), "no verb for {missing:?}");
}

#[test]
fn every_verb_calls_a_method_that_exists_and_names_real_params() {
    let doc = doc();
    for verb in VERBS {
        let method = Method::from_name(verb.method)
            .unwrap_or_else(|| panic!("{} {}: no method {}", verb.noun, verb.name, verb.method));
        assert!(
            method.scopes().cli,
            "{}: a person cannot call it",
            verb.method
        );
        assert!(
            NOUNS.iter().any(|(noun, _)| *noun == verb.noun),
            "{}",
            verb.noun
        );
        if verb.custom == Custom::None {
            let all = props(&doc, verb.method);
            for key in verb.pos {
                let name = key.trim_end_matches("...");
                assert!(
                    all.iter().any(|p| p.key == name),
                    "{} {}: the param {name} is not in the schema of {}",
                    verb.noun,
                    verb.name,
                    verb.method
                );
            }
        }
    }
}

#[test]
fn the_names_of_verbs_do_not_repeat() {
    let mut seen = HashSet::new();
    for verb in VERBS {
        assert!(
            seen.insert((verb.noun, verb.name)),
            "{} {}",
            verb.noun,
            verb.name
        );
    }
}

#[test]
fn positionals_and_options_become_params() {
    assert_eq!(
        ok(&["workspace", "rename", "w1", "New name"]),
        (
            "workspace.rename".to_owned(),
            json!({"workspace": "w1", "name": "New name"})
        )
    );
    let (method, split) = ok(&["pane", "split", "w1:p1", "right", "--ratio", "0.3"]);
    assert_eq!(method, "pane.split");
    assert_eq!(split["pane"], "w1:p1");
    assert_eq!(split["direction"], "right");
    assert_eq!(split["ratio"], 0.3);
    let (_, close) = ok(&["tab", "close", "w1:t2", "--force"]);
    assert_eq!(close, json!({"tab": "w1:t2", "force": true}));
    let (_, ws) = ok(&["workspace", "create", "Work", "--root", "/work/app"]);
    assert_eq!(ws, json!({"name": "Work", "root": "/work/app"}));
}

#[test]
fn a_switch_that_is_true_by_default_has_a_flag_that_turns_it_off() {
    let (_, prompt) = ok(&["agent", "prompt", "a1", "hello", "--no-submit"]);
    assert_eq!(
        prompt,
        json!({"agent": "a1", "text": "hello", "submit": false})
    );
    let (_, kept) = ok(&["agent", "prompt", "a1", "hello"]);
    assert_eq!(kept, json!({"agent": "a1", "text": "hello"}));
}

#[test]
fn json_options_and_any_values_are_parsed() {
    let (_, tab) = ok(&["tab", "create", "w1", "--command", r#"{"argv":["htop"]}"#]);
    assert_eq!(tab["command"], json!({"argv": ["htop"]}));
    assert!(parsed(&["tab", "create", "w1", "--command", "{oops"]).is_err());
    assert_eq!(
        ok(&["config", "set", "tasks.ports", "2"]).1["value"],
        json!(2)
    );
    assert_eq!(
        ok(&["config", "set", "approvals.mode", "gate"]).1["value"],
        json!("gate")
    );
    assert_eq!(
        ok(&["config", "set", "tasks.setup", r#"["a","b"]"#]).1["value"],
        json!(["a", "b"])
    );
    assert_eq!(ok(&["config", "get"]).1, json!({}));
    assert_eq!(
        ok(&["config", "get", "tasks"]).1,
        json!({"section": "tasks"})
    );
}

#[test]
fn a_number_flag_must_be_a_number() {
    let error = parsed(&["interaction", "list", "--limit", "many"]).expect_err("not a number");
    assert!(matches!(error, CliError::Usage(_)));
    assert_eq!(
        ok(&["interaction", "list", "--limit", "5"]).1["limit"],
        json!(5)
    );
}

#[test]
fn the_rest_of_the_words_is_a_list() {
    // No verb of the table has a list of the schema as its last positional now, so the test makes
    // one.
    let doc = doc();
    let drop = verb("pane", "drop", "pane.drop_files", "", &["pane", "paths..."]);
    let matches = command_of(&doc, &drop)
        .try_get_matches_from(["drop", "w1:p1", "a.txt", "b c.txt", "d.txt"])
        .expect("the words parse");
    let value = params(&doc, &drop, &matches, Path::new("/work/repo")).expect("params");
    assert_eq!(value["pane"], "w1:p1");
    assert_eq!(value["paths"], json!(["a.txt", "b c.txt", "d.txt"]));
}

#[test]
fn a_paste_gets_an_input_id() {
    let (_, paste) = ok(&["pane", "paste", "w1:p1", "ls -l"]);
    assert_eq!(paste["text"], "ls -l");
    assert!(
        paste["input_id"].as_u64().is_some_and(|id| id > 0),
        "{paste}"
    );
}

#[test]
fn send_keys_turns_names_into_bytes() {
    let (method, keys) = ok(&["pane", "send-keys", "w1:p1", "ls", "Enter", "C-c", "Escape"]);
    assert_eq!(method, "pane.send_bytes");
    assert_eq!(keys["pane"], "w1:p1");
    // "ls", Enter, Ctrl+C, Escape: `ls\r\x03\x1b` in base64.
    assert_eq!(keys["bytes"], "bHMNAxs=");
    assert!(keys["input_id"].is_u64());
    assert_eq!(key_bytes("C-a"), Some(vec![1]));
    assert_eq!(key_bytes("M-x"), Some(vec![0x1b, b'x']));
    assert_eq!(key_bytes("Up"), Some(b"\x1b[A".to_vec()));
    assert_eq!(key_bytes("hello"), None);
    // The standard alphabet, with padding.
    for (text, encoded) in [
        ("M", "TQ=="),
        ("Ma", "TWE="),
        ("Man", "TWFu"),
        ("?>", "Pz4="),
    ] {
        let (_, keys) = ok(&["pane", "send-keys", "w1:p1", text]);
        assert_eq!(keys["bytes"], encoded, "{text}");
    }
}

#[test]
fn an_answer_is_a_word_and_goes_by_the_cli() {
    let (method, allow) = ok(&["interaction", "answer", "i42", "allow"]);
    assert_eq!(method, "interaction.answer");
    assert_eq!(
        allow,
        json!({"interaction": "i42", "via": "cli", "decision": {"kind": "approval", "behavior": "allow"}})
    );
    let (_, always) = ok(&["interaction", "answer", "i42", "allow-always"]);
    assert_eq!(
        always["decision"],
        json!({"kind": "approval", "behavior": "allow", "always": true})
    );
    let (_, deny) = ok(&[
        "interaction",
        "answer",
        "i42",
        "deny",
        "--message",
        "not that",
        "--stop",
    ]);
    assert_eq!(
        deny["decision"],
        json!({"kind": "approval", "behavior": "deny", "stop": true, "message": "not that"})
    );
    let (_, plan) = ok(&[
        "interaction",
        "answer",
        "i42",
        "x",
        "--decision",
        r#"{"kind":"plan_review","approve":true}"#,
    ]);
    assert_eq!(
        plan["decision"],
        json!({"kind": "plan_review", "approve": true})
    );
    assert!(parsed(&["interaction", "answer", "i42", "maybe"]).is_err());
}

#[test]
fn an_agent_starts_where_the_flags_say() {
    let (_, split) = ok(&[
        "agent", "start", "claude", "--split", "w1:p1", "--prompt", "go",
    ]);
    assert_eq!(
        split,
        json!({"harness": "claude", "prompt": "go", "placement": {"type": "split", "pane": "w1:p1", "direction": "right"}})
    );
    let (_, pane) = ok(&[
        "agent", "start", "codex", "--pane", "w1:p2", "--name", "worker",
    ]);
    assert_eq!(pane["placement"], json!({"type": "pane", "pane": "w1:p2"}));
    assert_eq!(pane["name"], "worker");
    let (_, tab) = ok(&[
        "agent",
        "start",
        "claude",
        "--new-tab",
        "w1",
        "--args=--verbose",
    ]);
    assert_eq!(
        tab["placement"],
        json!({"type": "new_tab", "workspace": "w1"})
    );
    assert_eq!(tab["args"], json!(["--verbose"]));
    assert!(
        parsed(&["agent", "start", "claude"]).is_err(),
        "it needs a place"
    );
    assert!(parsed(&["agent", "start", "claude", "--pane", "a", "--split", "b"]).is_err());
}

#[test]
fn the_changes_of_a_task_and_the_diff_of_a_file_take_positions() {
    assert_eq!(
        ok(&["task", "changes", "k7"]),
        ("task.changes".to_owned(), json!({"task": "k7"}))
    );
    // The path is the key of a row, from the root of the worktree: it stays as it is.
    assert_eq!(
        ok(&["task", "diff", "k7", "src/a b.rs"]),
        (
            "task.diff".to_owned(),
            json!({"task": "k7", "path": "src/a b.rs"})
        )
    );
}

#[test]
fn a_task_takes_the_repository_of_the_current_directory() {
    let (method, task) = ok(&[
        "task",
        "create",
        "Fix the login bug",
        "--harness",
        "claude",
        "--prompt",
        "go",
    ]);
    assert_eq!(method, "task.create");
    assert_eq!(
        task,
        json!({"title": "Fix the login bug", "repo": "/work/repo", "harness": "claude", "prompt": "go"})
    );
    let (_, named) = ok(&["task", "create", "Fix", "--repo", "w3"]);
    assert_eq!(named["repo"], "w3");
    // The core reads the files, so a path starts in the current directory of the person.
    let (_, files) = ok(&[
        "task",
        "create",
        "Fix",
        "--files",
        "shot.png",
        "--files",
        "/tmp/plan.md",
        "--start",
    ]);
    assert_eq!(
        files["files"],
        json!(["/work/repo/shot.png", "/tmp/plan.md"])
    );
    assert_eq!(files["start"], true);
}

#[test]
fn a_task_changes_with_flags_and_clears_a_value() {
    let (method, change) = ok(&[
        "task", "update", "k3", "--title", "Better", "--clear", "prompt", "--clear", "harness",
        "--files", "notes.md", "--remove", "01J",
    ]);
    assert_eq!(method, "task.update");
    assert_eq!(
        change,
        json!({"task": "k3", "title": "Better", "prompt": null, "harness": null,
               "files": ["/work/repo/notes.md"], "remove": ["01J"]})
    );
    let wrong = parsed(&["task", "update", "k3", "--clear", "title"]);
    assert!(matches!(wrong, Err(CliError::Usage(text)) if text.contains("--clear")));
    assert_eq!(
        ok(&["task", "start", "k3"]),
        ("task.start".into(), json!({"task": "k3"}))
    );
    assert_eq!(ok(&["task", "delete", "k3"]).0, "task.delete");
}

#[test]
fn the_session_stops_everything_with_or_without_force_and_shuts_down() {
    assert_eq!(
        ok(&["session", "stop-everything"]),
        ("session.stop_everything".into(), json!({}))
    );
    assert_eq!(
        ok(&["session", "stop-everything", "--force"]),
        ("session.stop_everything".into(), json!({"force": true}))
    );
    assert_eq!(
        ok(&["session", "shutdown"]),
        ("session.shutdown".into(), json!({}))
    );
}

#[test]
fn the_verbs_of_a_machine_take_its_label() {
    assert_eq!(ok(&["machine", "list"]), ("machine.list".into(), json!({})));
    assert_eq!(
        ok(&["machine", "connect", "studio"]),
        ("machine.connect".into(), json!({"machine": "studio"}))
    );
    assert_eq!(
        ok(&["machine", "disconnect", "studio"]),
        ("machine.disconnect".into(), json!({"machine": "studio"}))
    );
    assert_eq!(
        ok(&["machine", "open-window", "studio"]),
        ("machine.open_window".into(), json!({"machine": "studio"}))
    );
}

#[test]
fn a_refresh_of_the_harnesses_takes_one_harness_or_none() {
    assert_eq!(
        ok(&["harness", "refresh"]),
        ("harness.refresh".into(), json!({}))
    );
    assert_eq!(
        ok(&["harness", "refresh", "codex"]),
        ("harness.refresh".into(), json!({"harness": "codex"}))
    );
}

#[test]
fn a_favorite_is_the_current_directory_or_a_folder_of_it() {
    assert_eq!(
        ok(&["favorite", "add"]),
        ("favorite.add".into(), json!({"path": "/work/repo"}))
    );
    assert_eq!(
        ok(&["favorite", "remove", "/work/other"]),
        ("favorite.remove".into(), json!({"path": "/work/other"}))
    );
    assert_eq!(ok(&["favorite", "add", "w3"]).1, json!({"path": "w3"}));
    assert_eq!(
        ok(&["favorite", "list"]),
        ("favorite.list".into(), json!({}))
    );
    // A folder of the current directory becomes absolute; another word stays.
    let here = Path::new(env!("CARGO_MANIFEST_DIR"));
    assert_eq!(
        folder_of("src", here),
        here.join("src").display().to_string()
    );
    assert_eq!(folder_of("w3", here), "w3");
    assert_eq!(folder_of("~/work", here), "~/work");
}

#[test]
fn scroll_takes_a_delta_or_a_line() {
    assert_eq!(
        ok(&["pane", "scroll", "w1:p1", "--delta", "-5"]).1,
        json!({"pane": "w1:p1", "delta": -5})
    );
    assert_eq!(
        ok(&["pane", "scroll", "w1:p1", "--top-line", "120"]).1,
        json!({"pane": "w1:p1", "top_line": 120})
    );
}

#[test]
fn a_missing_required_option_is_named() {
    // `layout set-ratios` needs its three params.
    let error = parsed(&["layout", "set-ratios", "t1", "[0]"]).expect_err("ratios are missing");
    let CliError::Usage(message) = error else {
        panic!("a usage error");
    };
    assert!(message.contains("--ratios"), "{message}");
}

#[test]
fn fs_dirs_takes_no_path_for_the_home_folder_or_a_path() {
    assert_eq!(ok(&["fs", "dirs"]), ("fs.dirs".into(), json!({})));
    assert_eq!(
        ok(&["fs", "dirs", "/work"]),
        ("fs.dirs".into(), json!({"path": "/work"}))
    );
    assert_eq!(
        ok(&["fs", "dirs", "~/work"]),
        ("fs.dirs".into(), json!({"path": "~/work"}))
    );
}

#[test]
fn fs_list_takes_the_current_directory_or_a_path_that_it_makes_absolute() {
    assert_eq!(
        ok(&["fs", "list"]),
        ("fs.list".into(), json!({"path": "/work/repo"}))
    );
    assert_eq!(
        ok(&["fs", "list", "src"]),
        ("fs.list".into(), json!({"path": "/work/repo/src"}))
    );
    assert_eq!(
        ok(&["fs", "list", "/etc"]),
        ("fs.list".into(), json!({"path": "/etc"}))
    );
    let home = std::env::var("HOME").expect("a home folder");
    assert_eq!(
        ok(&["fs", "list", "~/work"]),
        ("fs.list".into(), json!({"path": format!("{home}/work")}))
    );
}
