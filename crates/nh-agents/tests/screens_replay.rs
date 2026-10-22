//! The built-in screen manifests against the recordings of the harnesses.
//!
//! Two kinds of test. The saved screens of a recording (`screens/*.txt`) are read by the detector,
//! and the matches are checked: the states, the dialogs, the captures. The casts (`session.cast`)
//! are replayed through the engine of the panes, and the detector looks at the screen ten times a
//! second, as the core does, so the sequence of the states is checked too.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use nh_agents::screen::{
    Cell, Detector, Evaluation, Screen, ScreenManifest, ScreenState, builtin,
    engine::UNKNOWN_DIALOG_RULE,
};
use nh_proto::modes::ModeFlags;
use nh_term::{AlacrittyEngine, MIN_HISTORY, Replaying, ScreenDump, VtEngine, xterm_palette};
use serde_json::Value;

fn golden() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/harness-golden")
}

fn manifest(harness: &str) -> Arc<ScreenManifest> {
    Arc::new(builtin(harness).expect("the built-in manifest compiles"))
}

/// A saved screen, padded to the height of the recordings.
fn saved(harness: &str, version: &str, scenario: &str, name: &str) -> Screen {
    let path = golden()
        .join(harness)
        .join(version)
        .join(scenario)
        .join("screens")
        .join(format!("{name}.txt"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let mut lines: Vec<&str> = text.lines().collect();
    while lines.len() < 40 {
        lines.push("");
    }
    Screen::from_text(&lines)
}

/// What the detector says about a screen that stood still: the clock moves past every `hold_ms`.
fn read(harness: &str, screen: &Screen) -> Evaluation {
    let mut detector = Detector::new(manifest(harness));
    let start = Instant::now();
    let _ = detector.evaluate(screen, start);
    detector.evaluate(screen, start + Duration::from_secs(2))
}

fn labels(found: &Evaluation, rule: &str) -> Vec<String> {
    found
        .rule(rule)
        .and_then(|m| m.dialog.as_ref())
        .map(|d| d.options.iter().map(|o| o.label.clone()).collect())
        .unwrap_or_default()
}

// --- the saved screens ---------------------------------------------------------------------------

#[test]
fn claude_approval_dialog_has_its_title_its_command_and_its_options() {
    let screen = saved("claude", "2.1.292", "delivery-dialog-while-held", "dialog");
    let found = read("claude", &screen);
    let approval = found.rule("approval").expect("the approval rule");
    assert_eq!(approval.captures["title"], "proceed");
    assert_eq!(approval.captures["command"], "touch note.txt");
    let dialog = approval.dialog.as_ref().expect("a dialog");
    assert_eq!(dialog.options.len(), 4);
    assert_eq!(dialog.options[0].label, "Yes");
    assert_eq!(dialog.options[3].label, "No");
    assert_eq!(dialog.pointer, Some(0));
    assert!(dialog.accelerators);
    assert_eq!(dialog.choices["allow"], 0);
    assert_eq!(dialog.choices["allow_always"], 1);
    assert_eq!(dialog.choices["deny"], 3);
    assert!(approval.keystroke_answerable);
    assert!(found.rule(UNKNOWN_DIALOG_RULE).is_none());
    // The dialog is not the idle prompt, and it is not a working screen.
    assert!(found.state().is_none(), "{:?}", found.matches);
}

#[test]
fn claude_question_dialog_is_a_question_with_its_choices() {
    let screen = saved("claude", "2.1.292", "question-native", "dialog");
    let found = read("claude", &screen);
    let question = found.rule("question").expect("the question rule");
    assert_eq!(question.captures["title"], "Color");
    assert_eq!(
        labels(&found, "question"),
        ["Red", "Blue", "Type something.", "Chat about this"]
    );
    let dialog = question.dialog.as_ref().expect("a dialog");
    assert_eq!(dialog.pointer, Some(0));
    assert_eq!(dialog.multi_toggle.as_deref(), Some("space"));
    assert!(found.rule("approval").is_none());
}

#[test]
fn claude_plan_dialog_is_a_plan_review() {
    let screen = saved("claude", "2.1.292", "plan-allow", "dialog");
    let found = read("claude", &screen);
    let plan = found.rule("plan").expect("the plan rule");
    assert_eq!(
        labels(&found, "plan"),
        [
            "Yes, and use auto mode",
            "Yes, manually approve edits",
            "Tell Claude what to change"
        ]
    );
    // "Allow" is the choice that keeps the approval of edits, and not the one that turns on the
    // auto mode: a person who allows a plan does not allow every edit that follows.
    assert_eq!(plan.dialog.as_ref().expect("a dialog").choices["allow"], 1);
    assert!(found.rule("approval").is_none());
}

#[test]
fn claude_screens_without_a_dialog_are_idle_or_say_nothing() {
    for (scenario, name) in [
        ("delivery-dialog-while-held", "done"),
        ("permission-allow", "allowed"),
        ("elicitation-native", "answered"),
    ] {
        let found = read("claude", &saved("claude", "2.1.292", scenario, name));
        assert_eq!(
            found.state().map(|(s, _)| s),
            Some(ScreenState::Idle),
            "{scenario}/{name}: {:?}",
            found.matches
        );
        assert!(found.dialog().is_none(), "{scenario}/{name}");
    }
    // The final screen is the shell after the exit.
    let found = read("claude", &saved("claude", "2.1.292", "startup", "final"));
    assert!(found.matches.is_empty(), "{:?}", found.matches);
}

#[test]
fn codex_approval_dialog_has_its_command_and_its_options() {
    let screen = saved("codex", "0.156.1", "delivery-release", "dialog");
    let found = read("codex", &screen);
    let approval = found.rule("approval").expect("the approval rule");
    assert_eq!(approval.captures["command"], "touch note.txt");
    assert_eq!(approval.captures["title"], "run the following command");
    let dialog = approval.dialog.as_ref().expect("a dialog");
    assert_eq!(dialog.options.len(), 3);
    assert_eq!(dialog.options[0].label, "Yes, proceed");
    assert_eq!(dialog.pointer, Some(0));
    assert_eq!(dialog.choices["allow"], 0);
    assert_eq!(dialog.choices["allow_always"], 1);
    assert_eq!(dialog.choices["deny"], 2);
    assert!(found.state().is_none(), "{:?}", found.matches);
}

#[test]
fn codex_prompt_is_idle_and_the_trust_dialog_is_not_read_as_an_interaction() {
    let found = read(
        "codex",
        &saved("codex", "0.156.1", "delivery-release", "done"),
    );
    assert_eq!(found.state().map(|(s, _)| s), Some(ScreenState::Idle));
    let prompt = read(
        "codex",
        &saved("codex", "0.156.1", "delivery-release", "prompt"),
    );
    assert_eq!(prompt.state().map(|(s, _)| s), Some(ScreenState::Idle));
    let review = read(
        "codex",
        &saved("codex", "0.156.1", "delivery-release", "hooks-review"),
    );
    assert!(review.dialog().is_none(), "{:?}", review.matches);
    assert!(review.state().is_none(), "{:?}", review.matches);
}

// --- the casts ------------------------------------------------------------------------------------

fn screen_of(engine: &AlacrittyEngine) -> Screen {
    let dump = ScreenDump::of(engine, &xterm_palette());
    let rows = dump
        .lines
        .iter()
        .map(|runs| {
            runs.iter()
                .flat_map(|run| {
                    (0..run.count).map(|_| Cell {
                        ch: run.cell.ch,
                        style: run.cell.style,
                    })
                })
                .collect()
        })
        .collect();
    Screen {
        total_rows: dump.rows,
        first_row: 0,
        rows,
        cursor: (dump.cursor.y, dump.cursor.x),
        alt_screen: dump.modes.contains(ModeFlags::ALT_SCREEN),
        title: dump.title.clone(),
    }
}

/// What the detector says about a cast, when it looks every 100 ms.
struct Replay {
    /// For each look: the time and the matches.
    looks: Vec<(f64, Evaluation)>,
}

impl Replay {
    fn first(&self, rule: &str) -> Option<f64> {
        self.looks
            .iter()
            .find(|(_, e)| e.rule(rule).is_some())
            .map(|(t, _)| *t)
    }

    fn ever(&self, rule: &str) -> bool {
        self.first(rule).is_some()
    }

    fn last_state(&self) -> Option<ScreenState> {
        self.looks
            .iter()
            .rev()
            .find_map(|(_, e)| e.state().map(|(s, _)| s))
    }
}

fn replay(harness: &str, version: &str, scenario: &str) -> Replay {
    let path = golden()
        .join(harness)
        .join(version)
        .join(scenario)
        .join("session.cast");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let mut lines = text.lines();
    let header: Value = serde_json::from_str(lines.next().expect("a header")).expect("JSON");
    let cols = header["width"].as_u64().unwrap_or(120) as u16;
    let rows = header["height"].as_u64().unwrap_or(40) as u16;
    let mut engine = AlacrittyEngine::new(cols, rows, MIN_HISTORY, xterm_palette());
    let mut detector = Detector::new(manifest(harness));
    let start = Instant::now();
    let mut looks = Vec::new();
    let mut next_look = 0.0_f64;
    for line in lines {
        let event: Value = serde_json::from_str(line).expect("an event");
        let t = event[0].as_f64().unwrap_or(0.0);
        if event[1] == "o" {
            // A look at the screen for each 100 ms that passed before this output.
            while next_look <= t {
                let now = start + Duration::from_secs_f64(next_look);
                looks.push((next_look, detector.evaluate(&screen_of(&engine), now)));
                next_look += 0.1;
            }
            engine.advance(
                event[2].as_str().unwrap_or("").as_bytes(),
                &mut Replaying::default(),
            );
        }
    }
    looks.push((
        next_look,
        detector.evaluate(
            &screen_of(&engine),
            start + Duration::from_secs_f64(next_look),
        ),
    ));
    Replay { looks }
}

#[test]
fn claude_cast_shows_the_dialog_and_ends_at_the_prompt() {
    let r = replay("claude", "2.1.292", "delivery-dialog-while-held");
    assert!(r.ever("approval"), "the dialog shows");
    assert_eq!(r.last_state(), Some(ScreenState::Idle));
    assert!(!r.ever(UNKNOWN_DIALOG_RULE));
    assert!(!r.ever("question") && !r.ever("plan"));
}

#[test]
fn claude_casts_show_a_dialog_only_where_the_recording_has_one() {
    let table = [
        ("permission-allow", false, false, false),
        ("delivery-dialog-while-held", true, false, false),
        ("question-native", false, true, false),
        ("plan-allow", false, false, true),
        ("startup", false, false, false),
    ];
    for (scenario, approval, question, plan) in table {
        let r = replay("claude", "2.1.292", scenario);
        assert_eq!(r.ever("approval"), approval, "{scenario}: approval");
        assert_eq!(r.ever("question"), question, "{scenario}: question");
        assert_eq!(r.ever("plan"), plan, "{scenario}: plan");
        assert!(!r.ever(UNKNOWN_DIALOG_RULE), "{scenario}: unknown dialog");
    }
}

#[test]
fn codex_cast_shows_work_the_dialog_and_the_prompt_again() {
    let r = replay("codex", "0.156.1", "delivery-release");
    let working = r
        .looks
        .iter()
        .find(|(_, e)| e.state().map(|(s, _)| s) == Some(ScreenState::Working))
        .map(|(t, _)| *t)
        .expect("a working screen");
    let dialog = r.first("approval").expect("the dialog shows");
    assert!(working < dialog, "work comes before the dialog");
    assert_eq!(r.last_state(), Some(ScreenState::Idle));
    assert!(!r.ever(UNKNOWN_DIALOG_RULE));
}
