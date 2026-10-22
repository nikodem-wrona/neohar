//! The installer on `hooks.json` of Codex, and the reading of the trust.

use serde_json::{Value, json};

use super::*;
use crate::integration::hooks::Presence;

const BIN: &str = "/Users/x/.local/bin/nh";

fn parsed(text: &str) -> Value {
    serde_json::from_str(text).unwrap_or_else(|e| panic!("{e}: {text}"))
}

fn commands(text: &str) -> Vec<(String, String)> {
    let value = parsed(text);
    let mut out = Vec::new();
    for (event, groups) in value["hooks"].as_object().into_iter().flatten() {
        for group in groups.as_array().into_iter().flatten() {
            for hook in group["hooks"].as_array().into_iter().flatten() {
                out.push((
                    event.clone(),
                    hook["command"].as_str().unwrap_or("").to_owned(),
                ));
            }
        }
    }
    out
}

#[test]
fn a_missing_file_gets_the_12_hooks_without_a_marker_key() {
    let text = install(None, BIN).expect("a file");
    let value = parsed(&text);
    assert_eq!(
        value["hooks"].as_object().map(serde_json::Map::len),
        Some(12)
    );
    assert!(!text.contains("_neohar"), "no extra key in the definitions");
    let hook = &value["hooks"]["PermissionRequest"][0]["hooks"][0];
    assert_eq!(hook["type"], "command");
    assert_eq!(
        hook["command"],
        "/Users/x/.local/bin/nh hook codex PermissionRequest"
    );
    assert_eq!(hook["timeout"], 1800);
    assert!(
        value["hooks"]["PermissionRequest"][0]
            .get("matcher")
            .is_none()
    );
    assert_eq!(value["hooks"]["SessionEnd"][0]["hooks"][0]["timeout"], 3);
    assert_eq!(value["hooks"]["Interrupt"][0]["hooks"][0]["timeout"], 3);
    assert_eq!(value["hooks"]["Stop"][0]["hooks"][0]["timeout"], 10);
    assert_eq!(value["hooks"]["PreToolUse"][0]["hooks"][0]["timeout"], 1800);
    let states = inspect(Some(&text), BIN).expect("a file");
    assert!(states.iter().all(|s| s.presence == Presence::Current));
    assert_eq!(states.len(), 12);
}

#[test]
fn a_second_run_is_the_same_and_uninstall_brings_the_original_back() {
    let user = "{\n  \"hooks\": {\n    \"Stop\": [\n      { \"hooks\": [ { \"type\": \"command\", \"command\": \"say done\", \"timeout\": 5 } ] }\n    ]\n  },\n  \"other\":   1\n}\n";
    for original in [None, Some(user), Some("{}"), Some("{\n    \"a\": 1\n}\n")] {
        let first = install(original, BIN).expect("a file");
        assert_eq!(install(Some(&first), BIN).expect("a file"), first);
        let removed = uninstall(&first).expect("a file");
        match original {
            Some(original) => assert_eq!(removed, original),
            None => assert_eq!(parsed(&removed), json!({})),
        }
    }
}

#[test]
fn the_hooks_of_the_user_stay_and_only_commands_of_neohar_go() {
    let user = "{\n  \"hooks\": {\n    \"Stop\": [\n      { \"hooks\": [ { \"type\": \"command\", \"command\": \"say done\" } ] }\n    ],\n    \"PreToolUse\": [\n      { \"hooks\": [ { \"type\": \"command\", \"command\": \"/opt/check hook codex PreToolUse extra\" } ] }\n    ]\n  }\n}\n";
    let installed = install(Some(user), BIN).expect("a file");
    let all = commands(&installed);
    assert!(all.iter().any(|(e, c)| e == "Stop" && c == "say done"));
    // A command that only looks like ours is the user's.
    assert!(
        all.iter()
            .any(|(e, c)| e == "PreToolUse" && c.starts_with("/opt/check"))
    );
    assert_eq!(
        all.iter()
            .filter(|(_, c)| c.ends_with(&format!(" hook codex {}", "Stop")) && c.contains("nh"))
            .count(),
        1
    );
    assert_eq!(uninstall(&installed).expect("a file"), user);
}

#[test]
fn another_path_of_nh_makes_the_hooks_stale_and_a_new_install_moves_them() {
    let installed = install(None, BIN).expect("a file");
    let states = inspect(Some(&installed), "/other/nh").expect("a file");
    assert!(states.iter().all(|s| s.presence == Presence::Stale));
    let moved = install(Some(&installed), "/other/nh").expect("a file");
    assert!(moved.contains("/other/nh hook codex Stop"));
    assert!(!moved.contains(BIN));
    // The old path still counts as ours, so uninstall takes both out of a file that has them.
    let both = install(Some(&installed), "/other/nh").expect("a file");
    assert_eq!(commands(&both).len(), 12);
    let removed = uninstall(&moved).expect("a file");
    assert_eq!(parsed(&removed), json!({}));
}

#[test]
fn a_quoted_path_is_ours_too() {
    let text = install(None, "/Users/x y/nh").expect("a file");
    assert!(
        text.contains("\\\"/Users/x y/nh\\\" hook codex Stop"),
        "{text}"
    );
    assert!(
        inspect(Some(&text), "/Users/x y/nh")
            .expect("a file")
            .iter()
            .all(|s| s.presence == Presence::Current)
    );
    assert_eq!(parsed(&uninstall(&text).expect("a file")), json!({}));
}

#[test]
fn the_place_of_each_hook_is_known_for_the_trust() {
    let user = "{\n  \"hooks\": {\n    \"Stop\": [\n      { \"hooks\": [ { \"type\": \"command\", \"command\": \"say done\" } ] }\n    ]\n  }\n}\n";
    let installed = install(Some(user), BIN).expect("a file");
    let states = inspect(Some(&installed), BIN).expect("a file");
    let stop = states.iter().find(|s| s.name == "Stop").expect("Stop");
    assert_eq!(stop.place, Some((1, 0)), "the group of the user is first");
    let start = states
        .iter()
        .find(|s| s.name == "SessionStart")
        .expect("SessionStart");
    assert_eq!(start.place, Some((0, 0)));
}

#[test]
fn trust_is_read_from_the_table_of_codex_and_nothing_is_written() {
    let installed = install(None, BIN).expect("a file");
    let states = inspect(Some(&installed), BIN).expect("a file");
    let path = "/Users/x/.codex/hooks.json";
    let config = format!(
        "[features]\nhooks = true\n\n[hooks.state.\"{path}:session_start:0:0\"]\ntrusted_hash = \"sha256:abc\"\n\n[hooks.state.\"{path}:stop:0:0\"]\ntrusted_hash = \"\"\n\n[hooks.state.\"{path}:interrupt:0:0\"]\nother = 1\n"
    );
    let trusted = trust(&config, path, &states);
    assert_eq!(trusted.len(), 12);
    let of = |name: &str| trusted.iter().find(|(n, _)| *n == name).map(|(_, t)| *t);
    assert_eq!(of("SessionStart"), Some(true));
    assert_eq!(of("Stop"), Some(false), "an empty hash is no trust");
    assert_eq!(of("Interrupt"), Some(false));
    assert_eq!(of("PermissionRequest"), Some(false));
    // No config at all means that nothing is trusted. A hook that is not in the file is not listed.
    assert!(trust("", path, &states).iter().all(|(_, t)| !*t));
    assert!(trust("not toml [", path, &states).iter().all(|(_, t)| !*t));
    let few = inspect(Some("{}"), BIN).expect("a file");
    assert!(trust(&config, path, &few).is_empty());
}

#[test]
fn the_feature_of_hooks_is_on_unless_the_file_turns_it_off() {
    assert!(hooks_feature_on(""));
    assert!(hooks_feature_on("[features]\nother = false\n"));
    assert!(hooks_feature_on("[features]\nhooks = true\n"));
    assert!(!hooks_feature_on("[features]\nhooks = false\n"));
    assert!(hooks_feature_on("broken ["));
}
