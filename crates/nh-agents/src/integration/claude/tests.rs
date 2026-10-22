//! The installer on settings files of several shapes.

use serde_json::{Value, json};

use super::*;
use crate::integration::{
    hooks,
    json::{self, push},
};

const BIN: &str = "/Users/x/.local/bin/nh";

fn parsed(text: &str) -> Value {
    serde_json::from_str(text).unwrap_or_else(|e| panic!("{e}: {text}"))
}

/// The hooks of neohar in a file, by event: those with the command of neohar.
fn ours(text: &str) -> Vec<(String, Value)> {
    let value = parsed(text);
    let mut out = Vec::new();
    if let Some(hooks) = value.get("hooks").and_then(Value::as_object) {
        for (event, groups) in hooks {
            for group in groups.as_array().into_iter().flatten() {
                for hook in group["hooks"].as_array().into_iter().flatten() {
                    if hook["command"]
                        .as_str()
                        .is_some_and(|command| command.contains(" hook claude "))
                    {
                        out.push((event.clone(), hook.clone()));
                    }
                }
            }
        }
    }
    out
}

fn all_installed(text: &str) -> bool {
    inspect(Some(text), BIN)
        .expect("a file")
        .iter()
        .all(|state| state.presence == Presence::Current)
}

/// A settings file of a person who has used Claude Code for a while.
const USER_FILE: &str = r#"{
  "model": "opus",
  "permissions": {
    "allow": ["Bash(git status)", "Bash(ls:*)"],
    "deny": []
  },
  "env": { "FOO":   "bar" },
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "Bash",
        "hooks": [ { "type": "command", "command": "~/bin/check-bash.sh" } ]
      }
    ],
    "Stop": [{"hooks":[{"type":"command","command":"say done"}]}]
  },
  "statusLine": { "type": "command", "command": "~/bin/status" }
}
"#;

#[test]
fn a_missing_file_gets_a_file_with_the_17_hooks() {
    let text = install(None, BIN).expect("a file");
    assert!(text.ends_with("}\n"));
    let hooks = ours(&text);
    assert_eq!(hooks.len(), 17);
    assert!(all_installed(&text));
    let (event, hook) = hooks
        .iter()
        .find(|(event, _)| event == "PermissionRequest")
        .expect("the hook");
    assert_eq!(event, "PermissionRequest");
    assert_eq!(
        hook["command"],
        "/Users/x/.local/bin/nh hook claude PermissionRequest"
    );
    assert_eq!(hook["timeout"], 1800);
    assert_eq!(hook["type"], "command");
    // Claude Code drops an unknown key when it saves the file, so the hooks have none.
    assert!(!text.contains("_neohar"), "{text}");
    // An async event has no timeout, and the groups have their matchers.
    let value = parsed(&text);
    assert!(
        value["hooks"]["Stop"][0]["hooks"][0]
            .get("timeout")
            .is_none()
    );
    assert!(value["hooks"]["Stop"][0].get("matcher").is_none());
    assert_eq!(value["hooks"]["PreToolUse"][0]["matcher"], "*");
    assert_eq!(
        value["hooks"]["Elicitation"][0]["hooks"][0]["timeout"],
        1800
    );
}

#[test]
fn a_second_run_gives_the_same_bytes() {
    for original in [
        None,
        Some(USER_FILE),
        Some("{}"),
        Some("{\n}\n"),
        Some("{\"a\":1}"),
    ] {
        let first = install(original, BIN).expect("a file");
        let second = install(Some(&first), BIN).expect("a file");
        assert_eq!(first, second, "{original:?}");
    }
}

#[test]
fn the_content_of_the_user_stays_byte_for_byte() {
    let installed = install(Some(USER_FILE), BIN).expect("a file");
    // Everything the file had before is still in it, in the same place, and in the same bytes: the
    // text before the new members is a prefix, apart from the insertions in two arrays and the
    // object of the hooks.
    for line in USER_FILE.lines() {
        if line.contains("\"Stop\"") || line.contains("\"PreToolUse\": [") {
            continue;
        }
        assert!(installed.contains(line), "lost: {line:?}");
    }
    assert!(
        installed.contains(r#""env": { "FOO":   "bar" },"#),
        "the odd spacing stays"
    );
    let value = parsed(&installed);
    assert_eq!(value["model"], "opus");
    assert_eq!(value["permissions"]["allow"][1], "Bash(ls:*)");
    assert_eq!(value["statusLine"]["command"], "~/bin/status");
    // The hooks of the user are first in their arrays, and ours come after them.
    assert_eq!(
        value["hooks"]["PreToolUse"][0]["hooks"][0]["command"],
        "~/bin/check-bash.sh"
    );
    assert_eq!(
        value["hooks"]["PreToolUse"][1]["hooks"][0]["command"],
        "/Users/x/.local/bin/nh hook claude PreToolUse"
    );
    assert_eq!(value["hooks"]["Stop"][0]["hooks"][0]["command"], "say done");
    assert_eq!(value["hooks"]["Stop"].as_array().map(Vec::len), Some(2));
    assert_eq!(ours(&installed).len(), 17);
}

#[test]
fn uninstall_gives_the_original_text_back() {
    for original in [
        USER_FILE,
        "{}",
        "{\"a\": 1}",
        "{\n    \"a\": 1,\n    \"b\": [1, 2]\n}\n",
        "{\n\t\"a\": 1\n}",
        "{\n  \"hooks\": {\n    \"Stop\": [{\"hooks\": [{\"type\": \"command\", \"command\": \"x\"}]}]\n  }\n}\n",
        "{\n  \"hooks\": {\n    \"Other\": 1\n  },\n  \"z\": 2\n}",
    ] {
        let installed = install(Some(original), BIN).expect("a file");
        assert_ne!(installed, original);
        let removed = uninstall(&installed).expect("a file");
        assert_eq!(removed, original, "{original:?}");
    }
}

#[test]
fn uninstalling_a_file_made_by_the_installer_leaves_an_empty_object() {
    let installed = install(None, BIN).expect("a file");
    let removed = uninstall(&installed).expect("a file");
    assert_eq!(parsed(&removed), json!({}));
    assert!(ours(&removed).is_empty());
}

#[test]
fn a_user_manifest_with_four_spaces_or_tabs_gets_hooks_in_the_same_style() {
    let four = "{\n    \"a\": 1\n}\n";
    let installed = install(Some(four), BIN).expect("a file");
    assert!(
        installed.contains("\n    \"hooks\": {\n        \"SessionStart\": ["),
        "{installed}"
    );
    assert!(all_installed(&installed));
    let tabs = "{\n\t\"a\": 1\n}\n";
    let installed = install(Some(tabs), BIN).expect("a file");
    assert!(
        installed.contains("\n\t\"hooks\": {\n\t\t\"SessionStart\": ["),
        "{installed}"
    );
    // A file in one line gets one line of hooks in the same style.
    let inline = install(Some("{\"a\":1}"), BIN).expect("a file");
    assert!(all_installed(&inline));
    assert!(parsed(&inline)["a"] == 1);
}

#[test]
fn an_old_hook_of_neohar_is_written_again_and_the_rest_stays() {
    let installed = install(Some(USER_FILE), BIN).expect("a file");
    // A hand-edited timeout.
    let old = installed.replacen("\"timeout\": 1800", "\"timeout\": 5", 1);
    assert!(!all_installed(&old));
    let states = inspect(Some(&old), BIN).expect("a file");
    assert!(states.iter().any(|s| s.presence == Presence::Stale));
    let again = install(Some(&old), BIN).expect("a file");
    assert_eq!(again, installed);
}

/// Settings as Claude Code 2.1.301 saves them after three installs of an older neohar. Claude Code
/// dropped the marker of the first two copies, so each install added one more.
const SAVED_BY_CLAUDE: &str = r#"{
  "model": "opus",
  "hooks": {
    "SessionStart": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "bash ~/.claude/hooks/state.sh session"
          }
        ]
      },
      {
        "matcher": "*",
        "hooks": [
          {
            "type": "command",
            "command": "/Users/x/.local/bin/nh hook claude SessionStart"
          }
        ]
      },
      {
        "matcher": "*",
        "hooks": [
          {
            "type": "command",
            "command": "/Users/x/.local/bin/nh hook claude SessionStart"
          }
        ]
      },
      {
        "matcher": "*",
        "hooks": [
          {
            "type": "command",
            "command": "/Users/x/.local/bin/nh hook claude SessionStart",
            "_neohar": "neohar-integration=claude@1"
          }
        ]
      }
    ],
    "PreToolUse": [
      {
        "matcher": "*",
        "hooks": [
          {
            "type": "command",
            "command": "/Users/x/.local/bin/nh hook claude PreToolUse",
            "timeout": 1800
          }
        ]
      },
      {
        "matcher": "*",
        "hooks": [
          {
            "type": "command",
            "command": "/Users/x/.local/bin/nh hook claude PreToolUse",
            "timeout": 1800,
            "_neohar": "neohar-integration=claude@1"
          }
        ]
      }
    ]
  }
}
"#;

#[test]
fn copies_that_lost_the_marker_are_ours_and_the_install_keeps_one() {
    let states = inspect(Some(SAVED_BY_CLAUDE), BIN).expect("a file");
    let presence = |name: &str| {
        states
            .iter()
            .find(|s| s.name == name)
            .expect("the event")
            .presence
    };
    // A copy too many makes the event stale: the install changes it.
    assert_eq!(presence("SessionStart"), Presence::Stale);
    assert_eq!(presence("PreToolUse"), Presence::Stale);
    assert_eq!(presence("Stop"), Presence::Missing);

    let installed = install(Some(SAVED_BY_CLAUDE), BIN).expect("a file");
    assert!(all_installed(&installed));
    assert!(!installed.contains("_neohar"), "{installed}");
    assert_eq!(ours(&installed).len(), 17);
    let value = parsed(&installed);
    assert_eq!(value["model"], "opus");
    // The hook of the user stays first, and one copy of neohar stays after it.
    assert_eq!(
        value["hooks"]["SessionStart"].as_array().map(Vec::len),
        Some(2)
    );
    assert_eq!(
        value["hooks"]["SessionStart"][0]["hooks"][0]["command"],
        "bash ~/.claude/hooks/state.sh session"
    );
    assert_eq!(
        value["hooks"]["PreToolUse"].as_array().map(Vec::len),
        Some(1)
    );
    assert_eq!(install(Some(&installed), BIN).expect("a file"), installed);

    // Uninstall takes every copy out, with the marker and without it.
    let removed = parsed(&uninstall(SAVED_BY_CLAUDE).expect("a file"));
    assert_eq!(
        removed["hooks"]["SessionStart"].as_array().map(Vec::len),
        Some(1)
    );
    assert!(removed["hooks"].get("PreToolUse").is_none());
}

#[test]
fn another_path_of_the_binary_changes_the_hooks() {
    let installed = install(Some(USER_FILE), BIN).expect("a file");
    assert!(
        inspect(Some(&installed), "/other/nh")
            .expect("a file")
            .iter()
            .all(|s| s.presence == Presence::Stale)
    );
    let moved = install(Some(&installed), "/other/nh").expect("a file");
    assert!(moved.contains("/other/nh hook claude Stop"));
    assert!(!moved.contains(BIN));
    assert_eq!(ours(&moved).len(), 17);
}

#[test]
fn a_path_with_a_space_is_quoted_in_the_command() {
    let text = install(None, "/Users/x y/nh").expect("a file");
    let hooks = ours(&text);
    let start = hooks
        .iter()
        .find(|(event, _)| event == "SessionStart")
        .expect("the hook");
    assert_eq!(
        start.1["command"],
        "\"/Users/x y/nh\" hook claude SessionStart"
    );
    assert!(all_installed_for(&text, "/Users/x y/nh"));
}

fn all_installed_for(text: &str, bin: &str) -> bool {
    inspect(Some(text), bin)
        .expect("a file")
        .iter()
        .all(|state| state.presence == Presence::Current)
}

#[test]
fn a_hook_of_neohar_beside_a_hook_of_the_user_in_one_group_is_taken_out_alone() {
    let text = r#"{
  "hooks": {
    "Stop": [
      {
        "hooks": [
          { "type": "command", "command": "say done" },
          { "type": "command", "command": "nh hook claude Stop", "_neohar": "neohar-integration=claude@1" }
        ]
      }
    ]
  }
}
"#;
    let removed = uninstall(text).expect("a file");
    let value = parsed(&removed);
    assert_eq!(
        value["hooks"]["Stop"][0]["hooks"].as_array().map(Vec::len),
        Some(1)
    );
    assert_eq!(value["hooks"]["Stop"][0]["hooks"][0]["command"], "say done");
    // The install adds a group of its own and leaves the user's hook where it is.
    let installed = install(Some(text), BIN).expect("a file");
    let value = parsed(&installed);
    assert_eq!(value["hooks"]["Stop"][0]["hooks"][0]["command"], "say done");
    assert_eq!(
        value["hooks"]["Stop"][0]["hooks"].as_array().map(Vec::len),
        Some(1)
    );
    assert_eq!(
        value["hooks"]["Stop"][1]["hooks"][0]["command"],
        "/Users/x/.local/bin/nh hook claude Stop"
    );
    assert_eq!(ours(&installed).len(), 17);
}

#[test]
fn a_duplicate_of_neohar_is_removed() {
    let installed = install(None, BIN).expect("a file");
    // A second group of neohar in the array of `SessionStart`.
    let root = json::parse(&installed).expect("JSON");
    let array = &root
        .get("hooks")
        .and_then(|h| h.value.get("SessionStart"))
        .expect("the event")
        .value;
    let doubled = push(
        &installed,
        array,
        &hooks::group_text(&SPEC, &EVENTS[0], BIN, "  "),
        "  ",
    )
    .apply(&installed);
    assert_eq!(
        parsed(&doubled)["hooks"]["SessionStart"]
            .as_array()
            .map(Vec::len),
        Some(2)
    );
    assert!(!all_installed(&doubled), "a duplicate is stale");
    let fixed = install(Some(&doubled), BIN).expect("a file");
    assert_eq!(fixed, installed);
}

#[test]
fn a_file_that_is_not_what_the_installer_expects_is_refused_and_untouched() {
    for bad in [
        "[]",
        "not json",
        "{\"hooks\": []}",
        "{\"hooks\": {\"Stop\": 1}}",
        "{ // a comment\n}",
        "",
    ] {
        assert!(install(Some(bad), BIN).is_err(), "{bad:?}");
    }
    assert!(uninstall("[]").is_err());
    // A file with other hooks and no hooks of neohar is returned as it is.
    let plain = "{\n  \"hooks\": {\n    \"Stop\": []\n  }\n}\n";
    assert_eq!(uninstall(plain).expect("a file"), plain);
}

#[test]
fn inspect_names_the_events() {
    let missing = inspect(None, BIN).expect("no file");
    assert_eq!(missing.len(), 17);
    assert!(missing.iter().all(|s| s.presence == Presence::Missing));
    let partial = install(Some(USER_FILE), BIN)
        .and_then(|t| uninstall(&t))
        .and_then(|t| install(Some(&t), BIN))
        .expect("a file");
    assert!(all_installed(&partial));
    let user = inspect(Some(USER_FILE), BIN).expect("a file");
    assert!(
        user.iter().all(|s| s.presence == Presence::Missing),
        "the hooks of the user are not ours"
    );
    assert_eq!(EVENTS.iter().filter(|e| e.timeout.is_some()).count(), 3);
}
