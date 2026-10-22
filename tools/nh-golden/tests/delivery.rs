//! The delivery cases of an approval against live Claude Code and Codex.
//!
//! Each test records a scenario with a core (`[core]` in the scenario) and checks what became of
//! the interaction: its end state, the attempts, who decided, and how often the tool ran. The
//! comparison with the committed recordings is in `record.rs`. These tests state what each case
//! must show, so that a change of the delivery code or of a harness fails a named case.
//!
//! They run only when `NEOHAR_HARNESS=1` (`mise run test:harness`). A harness that is not
//! installed fails its test.

#![cfg(target_os = "macos")]

mod common;

use std::path::{Path, PathBuf};

use common::{enabled, record_once, save};
use nh_golden::recipe::Harness;
use serde_json::Value;

/// A recording of a delivery scenario, read from the files that the recorder writes.
struct Case {
    dir: PathBuf,
    interactions: Vec<Value>,
    events: Vec<Value>,
    hooks: Vec<String>,
}

impl Drop for Case {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn read(dir: &Path, name: &str) -> String {
    std::fs::read_to_string(dir.join(name)).unwrap_or_else(|e| panic!("read {name}: {e}"))
}

fn record(harness: Harness, name: &str) -> Case {
    let recording = record_once(harness, name);
    let meta = &recording.meta;
    assert_eq!(meta.exit, "code 0", "{name}: the harness exits by itself");
    assert_eq!(meta.unmatched_main, 0, "{name}: every request got a turn");
    assert_eq!(
        meta.hook_calls_api, meta.hook_calls,
        "{name}: every hook reached the core"
    );
    let dir = save(&recording, &format!("{}-{name}", harness.name()));
    let interactions: Vec<Value> =
        serde_json::from_str(&read(&dir, "core/interactions.json")).expect("interactions.json");
    let events = read(&dir, "core/events.jsonl")
        .lines()
        .map(|line| serde_json::from_str(line).expect("an event"))
        .collect();
    let hooks = read(&dir, "signals.jsonl")
        .lines()
        .map(|line| {
            let signal: Value = serde_json::from_str(line).expect("a signal");
            signal["event"].as_str().expect("the event name").to_owned()
        })
        .collect();
    Case {
        dir,
        interactions,
        events,
        hooks,
    }
}

impl Case {
    /// The only interaction of the recording.
    fn interaction(&self) -> &Value {
        assert_eq!(self.interactions.len(), 1, "one interaction");
        &self.interactions[0]
    }

    fn attempts(&self) -> Vec<&str> {
        self.interaction()["attempts"]
            .as_array()
            .expect("attempts")
            .iter()
            .map(|a| a.as_str().expect("an attempt"))
            .collect()
    }

    fn count(&self, hook: &str) -> usize {
        self.hooks.iter().filter(|h| *h == hook).count()
    }

    /// The tool ran once and the harness asked for it once.
    fn ran_once(&self) {
        assert_eq!(self.count("PermissionRequest"), 1, "one PermissionRequest");
        assert_eq!(self.count("PostToolUse"), 1, "the tool ran once");
    }

    fn screen(&self, name: &str) -> String {
        read(&self.dir, &format!("screens/{name}.txt"))
    }

    /// The events of one operation of the recorder, oldest first.
    fn ops(&self, op: &str) -> Vec<&Value> {
        self.events.iter().filter(|e| e["op"] == op).collect()
    }
}

/// Case 1: the core ends while the hook waits. The shim connects again, the core
/// remembers the interaction, the answer goes to the new connection, and the tool runs once.
#[test]
fn claude_core_killed_while_the_hook_waits() {
    if !enabled() {
        return;
    }
    let case = record(Harness::Claude, "delivery-core-killed");
    let interaction = case.interaction();
    assert_eq!(interaction["state"], "delivered");
    assert_eq!(interaction["mode"], "gate");
    assert_eq!(interaction["decision"]["behavior"], "allow");
    assert_eq!(interaction["acked"], true);
    assert_eq!(case.attempts(), ["sent", "acked"]);
    assert_eq!(case.ops("kill").len(), 1, "the core ended once");
    let start = case.ops("start");
    assert_eq!(start.len(), 1, "the core started again");
    assert_eq!(
        start[0]["detail"]["interactions"], 1,
        "the new core remembers the interaction"
    );
    case.ran_once();
    assert!(
        case.screen("done")
            .contains("Allowed by PermissionRequest hook"),
        "Claude Code took the answer through the new connection"
    );
}

/// Case 2: the process of the shim ends after the answer was recorded and before it was
/// delivered. Nobody holds the hook, so the core sends nothing and does not guess. The decision
/// waits, the dialog of the harness takes over, and the interaction ends as answered elsewhere.
#[test]
fn claude_shim_killed_after_the_answer() {
    if !enabled() {
        return;
    }
    let case = record(Harness::Claude, "delivery-shim-killed");
    let interaction = case.interaction();
    assert_eq!(interaction["state"], "resolved_elsewhere");
    assert_eq!(interaction["decision"]["behavior"], "allow");
    assert_eq!(
        interaction["acked"], false,
        "nothing acknowledged the answer"
    );
    assert!(case.attempts().is_empty(), "nothing was sent");
    assert_eq!(case.ops("kill_shim").len(), 1);
    case.ran_once();
    assert!(
        case.screen("dialog").contains("Do you want to proceed?"),
        "the dialog of Claude Code appeared when the shim ended"
    );
}

/// Case 3: two clients answer at the same time. One answer wins, the other gets
/// `already_answered`, and the harness gets one decision.
#[test]
fn claude_two_answers_at_once() {
    if !enabled() {
        return;
    }
    let case = record(Harness::Claude, "delivery-two-answers");
    let interaction = case.interaction();
    assert_eq!(interaction["state"], "delivered");
    assert_eq!(interaction["acked"], true);
    assert_eq!(case.attempts(), ["sent", "acked"]);
    let winner = interaction["decided_by"].as_str().expect("decided_by");
    assert!(
        ["first", "second"].contains(&winner),
        "a client of the race won"
    );
    let race = case.ops("race");
    assert_eq!(race.len(), 1);
    let results: Vec<&Value> = ["first", "second"]
        .iter()
        .map(|who| &race[0]["detail"][*who]["result"])
        .collect();
    let won = results.iter().filter(|r| ***r == "ok").count();
    let late = results
        .iter()
        .filter(|r| r["error"] == "already_answered")
        .count();
    assert_eq!((won, late), (1, 1), "one answer won and one was too late");
    assert_eq!(
        race[0]["detail"][winner]["result"], "ok",
        "the winner in the store is the answer that the core accepted"
    );
    case.ran_once();
    assert!(
        case.screen("done")
            .contains("Allowed by PermissionRequest hook")
    );
}

/// Case 4: the user answers in the dialog of the harness while the core holds the hook. The core
/// releases the hold, and the interaction ends as answered elsewhere without an answer from the
/// core.
#[test]
fn claude_answered_in_the_harness() {
    if !enabled() {
        return;
    }
    let case = record(Harness::Claude, "delivery-answered-in-tui");
    let interaction = case.interaction();
    assert_eq!(interaction["state"], "resolved_elsewhere");
    assert_eq!(
        interaction["mode"], "observe",
        "the release changed the mode"
    );
    assert!(
        interaction["decision"].is_null(),
        "the core decided nothing"
    );
    assert_eq!(case.attempts(), ["released"]);
    case.ran_once();
    assert!(
        case.screen("dialog").contains("Do you want to proceed?"),
        "the dialog of Claude Code is visible after the release"
    );
}

/// Case 5: nobody answers before the deadline. The interaction fails with the reason `deadline`,
/// a late answer gets `interaction_closed`, and the dialog of the harness takes over.
#[test]
fn claude_deadline_passes() {
    if !enabled() {
        return;
    }
    let case = record(Harness::Claude, "delivery-deadline");
    let interaction = case.interaction();
    assert_eq!(interaction["state"], "failed");
    assert_eq!(interaction["fail_reason"], "deadline");
    assert!(interaction["decision"].is_null());
    assert_eq!(case.attempts(), ["deadline"]);
    let answers = case.ops("answer");
    assert_eq!(answers.len(), 1, "one late answer");
    assert_eq!(answers[0]["detail"]["error"], "interaction_closed");
    case.ran_once();
    assert!(case.screen("dialog").contains("Do you want to proceed?"));
}

/// Not one of the five cases: Claude Code shows its own dialog while a hook still waits. The user
/// can answer there, and the held interaction ends as answered elsewhere.
#[test]
fn claude_shows_the_dialog_while_the_hook_waits() {
    if !enabled() {
        return;
    }
    let case = record(Harness::Claude, "delivery-dialog-while-held");
    let interaction = case.interaction();
    assert_eq!(interaction["state"], "resolved_elsewhere");
    assert_eq!(interaction["mode"], "gate", "the core did not release");
    assert!(interaction["decision"].is_null());
    assert!(case.attempts().is_empty(), "the core sent nothing");
    case.ran_once();
    assert!(
        case.screen("held").contains("Do you want to proceed?"),
        "the dialog is on the screen while the hook waits"
    );
}

/// Release on focus: the pane gets the focus while the hook waits. The core releases
/// the hook after the debounce, the shim prints nothing, and the dialog of Claude Code is the
/// only one. The user answers there, and the interaction ends as answered elsewhere.
#[test]
fn claude_is_released_when_the_pane_gets_the_focus() {
    if !enabled() {
        return;
    }
    let case = record(Harness::Claude, "delivery-released-on-focus");
    let interaction = case.interaction();
    assert_eq!(interaction["state"], "resolved_elsewhere");
    assert_eq!(interaction["mode"], "observe", "the core released the hook");
    assert_eq!(case.attempts(), ["released"]);
    assert!(interaction["decision"].is_null());
    assert_eq!(case.ops("focus").len(), 1, "one report of the focus");
    case.ran_once();
    assert!(case.screen("held").contains("Do you want to proceed?"));
    assert_eq!(
        case.screen("dialog")
            .matches("Do you want to proceed?")
            .count(),
        1,
        "the dialog is on the screen once"
    );
}

/// Codex: the core answers a held hook. The answer reaches the harness and the command runs once.
#[test]
fn codex_answer_to_a_held_hook() {
    if !enabled() {
        return;
    }
    let case = record(Harness::Codex, "delivery-held");
    let interaction = case.interaction();
    assert_eq!(interaction["state"], "delivered");
    assert_eq!(interaction["decision"]["behavior"], "allow");
    assert_eq!(interaction["acked"], true);
    assert_eq!(case.attempts(), ["sent", "acked"]);
    case.ran_once();
    let held = case.screen("held");
    assert!(
        !held.contains("Would you like to run the following command?"),
        "Codex shows no dialog while a hook waits"
    );
    assert!(held.contains("Running hook"));
    assert!(case.screen("done").contains("Ran touch note.txt"));
}

/// Codex: the dialog appears only after the hook ends. The core releases the hold, which is the
/// way to show the dialog of Codex.
#[test]
fn codex_release_shows_the_dialog() {
    if !enabled() {
        return;
    }
    let case = record(Harness::Codex, "delivery-release");
    let interaction = case.interaction();
    assert_eq!(interaction["state"], "resolved_elsewhere");
    assert_eq!(interaction["mode"], "observe");
    assert_eq!(case.attempts(), ["released"]);
    case.ran_once();
    assert!(
        case.screen("dialog")
            .contains("Would you like to run the following command?"),
        "the dialog of Codex appears after the release"
    );
}
