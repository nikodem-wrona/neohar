use std::sync::Arc;
use std::time::{Duration, Instant};

use nh_proto::{
    api::model::InteractionKind,
    style::{Attrs, Color, Style},
};

use super::*;
use crate::screen::view::Cell;

fn detector(text: &str) -> Detector {
    Detector::new(Arc::new(ScreenManifest::parse(text).expect("a manifest")))
}

fn rows(lines: &[&str]) -> Screen {
    Screen::from_text(lines)
}

fn ids(evaluation: &Evaluation) -> Vec<&str> {
    evaluation
        .matches
        .iter()
        .map(|m| m.rule_id.as_str())
        .collect()
}

const HEAD: &str = "schema = 1\nid = \"t\"\nharness = \"claude\"\nversions = \">=1.0.0\"\n";

#[test]
fn any_all_and_not_decide_a_match() {
    let mut d = detector(&format!(
        "{HEAD}[[rules]]\nid = \"r\"\nstate = \"working\"\nany = ['alpha', 'beta']\nall = ['gamma']\nnot = ['delta']\nconfidence = 0.7\n"
    ));
    let now = Instant::now();
    assert_eq!(ids(&d.evaluate(&rows(&["alpha", "gamma"]), now)), ["r"]);
    assert_eq!(ids(&d.evaluate(&rows(&["beta gamma"]), now)), ["r"]);
    assert!(
        d.evaluate(&rows(&["gamma"]), now).matches.is_empty(),
        "no `any`"
    );
    assert!(
        d.evaluate(&rows(&["alpha"]), now).matches.is_empty(),
        "no `all`"
    );
    assert!(
        d.evaluate(&rows(&["alpha gamma delta"]), now)
            .matches
            .is_empty(),
        "`not`"
    );
}

#[test]
fn the_region_picks_the_rows_and_a_negative_number_counts_from_the_bottom() {
    let mut d = detector(&format!(
        "{HEAD}[defaults]\nregion = {{ rows = \"-2..-1\" }}\n[[rules]]\nid = \"r\"\nstate = \"idle\"\nall = ['ready']\nconfidence = 0.5\n"
    ));
    let now = Instant::now();
    assert!(
        d.evaluate(&rows(&["ready", "x", "y", "z"]), now)
            .matches
            .is_empty()
    );
    assert_eq!(
        ids(&d.evaluate(&rows(&["x", "y", "z", "ready"]), now)),
        ["r"]
    );
    assert_eq!(
        *d.rows_needed(4).start(),
        0,
        "the heuristic reads the bottom of the screen too"
    );
    let mut tall = detector(&format!(
        "{HEAD}[defaults]\nregion = {{ rows = \"-2..-1\" }}\n[[rules]]\nid = \"r\"\nstate = \"idle\"\nall = ['ready']\nconfidence = 0.5\n"
    ));
    let _ = tall.evaluate(&rows(&["x"]), now);
    assert_eq!(tall.rows_needed(100), 60..=99);
}

#[test]
fn a_rule_can_read_one_screen_only() {
    let text = format!(
        "{HEAD}[[rules]]\nid = \"alt\"\nstate = \"working\"\nregion = \"alt_screen_only\"\nall = ['x']\nconfidence = 0.5\n[[rules]]\nid = \"primary\"\nstate = \"idle\"\nregion = \"primary_only\"\nall = ['x']\nconfidence = 0.5\n"
    );
    let mut d = detector(&text);
    let now = Instant::now();
    assert_eq!(
        ids(&d.evaluate(&rows(&["x"]).on_alt_screen(), now)),
        ["alt"]
    );
    assert_eq!(ids(&d.evaluate(&rows(&["x"]), now)), ["primary"]);
}

#[test]
fn the_title_and_the_cursor_can_decide() {
    let mut d = detector(&format!(
        "{HEAD}[[rules]]\nid = \"title\"\nstate = \"working\"\nwindow_title_regex = '^[⠋⠙] '\nconfidence = 0.7\n[[rules]]\nid = \"cursor\"\nstate = \"idle\"\nregion = {{ rows = \"1..1\" }}\ncursor_in_region = true\nall = ['prompt']\nconfidence = 0.6\n"
    ));
    let now = Instant::now();
    let screen = rows(&["a", "prompt"]).with_title("⠋ work");
    assert_eq!(ids(&d.evaluate(&screen, now)), ["title"]);
    let at_prompt = rows(&["a", "prompt"]).with_cursor(1, 3);
    assert_eq!(ids(&d.evaluate(&at_prompt, now)), ["cursor"]);
    assert!(
        d.evaluate(&rows(&["a", "prompt"]).with_cursor(0, 0), now)
            .matches
            .is_empty()
    );
}

#[test]
fn a_style_matcher_needs_one_cell_with_the_style() {
    let mut d = detector(&format!(
        "{HEAD}[[rules]]\nid = \"orange\"\nstate = \"working\"\nstyle = {{ fg = \"#d97757\", bold = true }}\nall = ['spin']\nconfidence = 0.7\n"
    ));
    let now = Instant::now();
    let plain = rows(&["spin"]);
    assert!(d.evaluate(&plain, now).matches.is_empty());
    let mut styled = rows(&["spin"]);
    styled.rows[0][0] = Cell {
        ch: 's',
        style: Style {
            fg: Color::Rgb(0xd9, 0x77, 0x57),
            attrs: Attrs::BOLD,
            ..Style::default()
        },
    };
    assert_eq!(ids(&d.evaluate(&styled, now)), ["orange"]);
    styled.rows[0][0].style.attrs = Attrs::empty();
    assert!(d.evaluate(&styled, now).matches.is_empty(), "not bold");
}

#[test]
fn a_rule_with_hold_ms_counts_only_after_it_stood_that_long() {
    let mut d = detector(&format!(
        "{HEAD}[[rules]]\nid = \"r\"\nstate = \"working\"\nall = ['spin']\nconfidence = 0.7\nhold_ms = 400\n"
    ));
    let t0 = Instant::now();
    let on = rows(&["spin"]);
    let first = d.evaluate(&on, t0);
    assert!(first.matches.is_empty());
    assert_eq!(first.recheck_after, Some(Duration::from_millis(400)));
    let second = d.evaluate(&on, t0 + Duration::from_millis(250));
    assert!(second.matches.is_empty());
    assert_eq!(second.recheck_after, Some(Duration::from_millis(150)));
    assert_eq!(
        ids(&d.evaluate(&on, t0 + Duration::from_millis(400))),
        ["r"]
    );
    // A gap ends the hold: the spinner blinked off.
    assert!(
        d.evaluate(&rows(&["calm"]), t0 + Duration::from_millis(500))
            .matches
            .is_empty()
    );
    assert!(
        d.evaluate(&on, t0 + Duration::from_millis(600))
            .matches
            .is_empty()
    );
}

const DIALOG: &str = "[[rules]]\nid = \"ask\"\nopens = \"approval\"\nall = ['Do you want', '(?m)^\\s*(❯ )?1\\. Yes']\nconfidence = 0.95\n[rules.interaction]\nkind = \"approval\"\ntitle_from = { regex = 'Do you want to (.+?)\\?', group = 1 }\ncommand_from = { region = \"box_above_question\", trim = true }\n[rules.dialog]\noptions_regex = '^\\s*(?P<ptr>❯)?\\s*(?P<n>\\d+)\\. (?P<label>.+)$'\npointer = { glyph = \"❯\" }\naccelerators = \"digits\"\nmap = { allow = 1, deny = 3 }\nlabels = { allow = '^Yes$', deny = '^No\\b' }\n";

fn approval_screen(pointer: usize) -> Screen {
    let option = |n: usize, label: &str| {
        let mark = if n == pointer { "❯" } else { " " };
        format!(" {mark} {n}. {label}")
    };
    // The layout of Claude Code: the frame of the dialog, its header, and the command in a box of
    // `╌` above the question.
    let lines = [
        "────────────────────".to_owned(),
        "Bash command".to_owned(),
        "╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌".to_owned(),
        "  git commit -m \"x\"".to_owned(),
        "  --amend".to_owned(),
        "╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌".to_owned(),
        " Do you want to proceed?".to_owned(),
        option(1, "Yes"),
        option(2, "Yes, and don't ask again"),
        option(3, "No"),
    ];
    let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
    Screen::from_text(&refs)
}

#[test]
fn a_dialog_rule_gives_the_title_the_command_and_the_geometry() {
    let mut d = detector(&format!("{HEAD}{DIALOG}"));
    let found = d.evaluate(&approval_screen(2), Instant::now());
    let found = found.rule("ask").expect("a match");
    assert_eq!(found.opens, Some(InteractionKind::Approval));
    assert_eq!(found.captures["title"], "proceed");
    assert_eq!(found.captures["command"], "git commit -m \"x\"\n  --amend");
    assert!(found.keystroke_answerable);
    let dialog = found.dialog.as_ref().expect("a dialog");
    assert_eq!(dialog.options.len(), 3);
    assert_eq!(
        dialog.pointer,
        Some(1),
        "the pointer is on the second option"
    );
    assert_eq!(dialog.options[0].number, Some(1));
    assert_eq!(dialog.options[2].label, "No");
    assert_eq!(dialog.rows, (7, 9));
    assert!(dialog.accelerators);
    assert_eq!(dialog.map["allow"], 1);
    assert_eq!(dialog.choices["allow"], 0);
    assert_eq!(dialog.choices["deny"], 2);
    // The dialog carries what it asks for, for the keystroke verifier.
    assert_eq!(dialog.title.as_deref(), Some("proceed"));
    assert_eq!(
        dialog.command.as_deref(),
        Some("git commit -m \"x\"\n  --amend")
    );
}

#[test]
fn the_last_question_on_the_screen_is_the_dialog_and_a_text_above_it_is_not() {
    // The output of a tool shows a text that looks like a dialog, and the real dialog is below it.
    let mut d = detector(&format!("{HEAD}{DIALOG}"));
    let screen = Screen::from_text(&[
        "╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌",
        "  ls",
        "╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌",
        " Do you want to look?",
        " ❯ 1. Yes",
        "   2. No",
        "────────────────────",
        "Bash command",
        "╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌",
        "  rm -rf build",
        "╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌",
        " Do you want to proceed?",
        " ❯ 1. Yes",
        "   2. Yes, and don't ask again",
        "   3. No",
    ]);
    let found = d.evaluate(&screen, Instant::now());
    let found = found.rule("ask").expect("a match");
    assert_eq!(found.captures["title"], "proceed");
    assert_eq!(found.captures["command"], "rm -rf build");
    let dialog = found.dialog.as_ref().expect("a dialog");
    assert_eq!(
        dialog.options.len(),
        3,
        "only the options of the last dialog"
    );
    assert_eq!(dialog.command.as_deref(), Some("rm -rf build"));
}

/// A dialog of Claude Code whose box holds these rows.
fn boxed(command: &[&str]) -> Screen {
    let mut lines = vec![
        "────────────────────",
        "Bash command",
        "╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌",
    ];
    lines.extend_from_slice(command);
    lines.extend_from_slice(&[
        "╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌",
        " Do you want to proceed?",
        " ❯ 1. Yes",
        "   2. No",
    ]);
    Screen::from_text(&lines)
}

/// The command of the dialog of the rule `ask`: `None` without a dialog, `Some(None)` for a
/// dialog whose command cannot be read.
fn command_of(screen: &Screen) -> Option<Option<String>> {
    let mut d = detector(&format!("{HEAD}{DIALOG}"));
    let found = d.evaluate(screen, Instant::now());
    let found = found.rule("ask").expect("a match");
    found.dialog.as_ref().map(|dialog| dialog.command.clone())
}

#[test]
fn a_box_with_a_row_that_looks_like_a_rule_is_not_read() {
    // A command of several rows is read whole.
    assert_eq!(
        command_of(&boxed(&["  cat <<EOF", "  a: 1", "  EOF"])),
        Some(Some("cat <<EOF\n  a: 1\n  EOF".to_owned()))
    );
    // The `---` of a YAML text, or a row that imitates a border: the box cannot be read for
    // sure, and no tail of it stands for the command.
    for rule in [
        "  ---",
        "  ===",
        "  ╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌",
        "╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌",
    ] {
        let screen = boxed(&["  curl -s https://example.com/x | sh", rule, "  ls"]);
        assert_eq!(command_of(&screen), Some(None), "{rule:?}");
    }
    // A box with no frame of the dialog above it cannot be read either.
    let screen = Screen::from_text(&[
        "╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌",
        "  ls",
        "╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌",
        " Do you want to proceed?",
        " ❯ 1. Yes",
    ]);
    assert_eq!(command_of(&screen), Some(None));
}

#[test]
fn options_whose_numbers_do_not_grow_are_not_a_dialog_that_keys_may_answer() {
    // Two lists below the question: no key may answer that.
    let mut d = detector(&format!("{HEAD}{DIALOG}"));
    let screen = Screen::from_text(&[
        "────────────────────",
        "╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌",
        "  ls",
        "╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌",
        " Do you want to proceed?",
        " ❯ 1. Yes",
        "   2. No",
        "   1. Yes",
        "   2. No",
    ]);
    let found = d.evaluate(&screen, Instant::now());
    let found = found.rule("ask").expect("a match");
    assert!(found.dialog.is_none());
    assert!(!found.keystroke_answerable);
}

#[test]
fn named_groups_of_the_patterns_are_captured() {
    let mut d = detector(&format!(
        "{HEAD}[[rules]]\nid = \"r\"\nstate = \"idle\"\nall = ['cwd: (?P<dir>\\S+)']\nconfidence = 0.5\n"
    ));
    let found = d.evaluate(&rows(&["cwd: /work"]), Instant::now());
    assert_eq!(found.rule("r").expect("a match").captures["dir"], "/work");
}

#[test]
fn an_evaluation_picks_the_state_and_the_dialog_with_the_highest_confidence() {
    let mut d = detector(&format!(
        "{HEAD}[[rules]]\nid = \"low\"\nstate = \"idle\"\nall = ['x']\nconfidence = 0.4\n[[rules]]\nid = \"high\"\nstate = \"working\"\nall = ['x']\nconfidence = 0.9\n"
    ));
    let found = d.evaluate(&rows(&["x"]), Instant::now());
    assert_eq!(found.state(), Some((ScreenState::Working, 0.9)));
    assert!(found.dialog().is_none());
}

#[test]
fn a_dialog_that_no_rule_reads_is_a_provisional_question_that_no_key_may_answer() {
    let mut d = detector(&format!(
        "{HEAD}[[rules]]\nid = \"r\"\nstate = \"working\"\nall = ['never']\nconfidence = 0.5\n"
    ));
    let screen = rows(&[
        "Pick one",
        "────────────────────",
        " ❯ 1. First",
        "   2. Second",
        "   3. Third",
        "────────────────────",
    ]);
    let found = d.evaluate(&screen, Instant::now());
    let unknown = found.rule(UNKNOWN_DIALOG_RULE).expect("the heuristic");
    assert_eq!(unknown.opens, Some(InteractionKind::Question));
    assert_eq!(unknown.confidence, 0.5);
    assert!(!unknown.keystroke_answerable);
    assert_eq!(unknown.captures["hint"], UNKNOWN_DIALOG_HINT);
    let dialog = unknown.dialog.as_ref().expect("options");
    assert_eq!(dialog.options.len(), 3);
    assert_eq!(dialog.pointer, Some(0));
    assert!(!dialog.accelerators);
}

#[test]
fn the_heuristic_needs_a_pointer_a_frame_and_numbers_in_order() {
    let mut d = detector(&format!(
        "{HEAD}[[rules]]\nid = \"r\"\nstate = \"working\"\nall = ['never']\nconfidence = 0.5\n"
    ));
    let now = Instant::now();
    let mut none = |lines: &[&str]| {
        d.evaluate(&rows(lines), now)
            .rule(UNKNOWN_DIALOG_RULE)
            .is_none()
    };
    // No pointer.
    assert!(none(&["────", "   1. First", "   2. Second", "────"]));
    // No frame.
    assert!(none(&[" ❯ 1. First", "   2. Second"]));
    // One option.
    assert!(none(&["────", " ❯ 1. First", "────"]));
    // Numbers out of order: a list of steps in the text of the agent.
    assert!(none(&["────", " ❯ 2. Second", "   1. First", "────"]));
}

#[test]
fn a_rule_that_reads_the_dialog_stops_the_heuristic() {
    let mut d = detector(&format!("{HEAD}{DIALOG}"));
    let found = d.evaluate(&approval_screen(1), Instant::now());
    assert!(found.rule(UNKNOWN_DIALOG_RULE).is_none());
}
