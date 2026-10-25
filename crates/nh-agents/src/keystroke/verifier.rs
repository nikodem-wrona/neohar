//! The state machine of the verifier. See the module above.

use crate::screen::Dialog;

use super::{MIN_SIMILARITY, MOVE_RETRIES};

/// A key that the verifier sends. The core encodes it for the modes of the pane.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Up,
    Down,
    Enter,
    Space,
    Char(char),
}

/// What the user wants in the dialog.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Choice {
    /// One option, by its label.
    Option(String),
    /// One option, by the name that the screen manifest gave it (`allow`, `allow_always`,
    /// `deny`). The labels of the dialog on the screen say which option that is.
    Named(String),
    /// These labels checked, the others not (a list with many choices).
    Toggle(Vec<String>),
    /// The text goes in the option with this label.
    Text { option: String, text: String },
}

/// What the dialog on the screen must ask for, so that the keys answer the request of the
/// interaction and no other request of the harness.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Asks {
    /// The request has nothing that the dialog shows (a plan). Other checks must make sure that
    /// the dialog is the one of the request.
    Anything,
    /// A command. The dialog must show the same command: the line breaks of the screen and the
    /// runs of white space do not count ([`shows_command`]).
    Command(String),
    /// Each text, the name of a file, must be in the title or in the box of the dialog.
    Mentions(Vec<String>),
    /// The title of the dialog, the header of a question, must be like this text.
    Title(String),
}

/// What to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    /// The labels that the dialog must have, in order. Empty when the interaction does not know
    /// them (an approval that a hook opened). The labels of the choice must be there either way.
    pub expected: Vec<String>,
    /// What the dialog must ask for. The verifier checks it before the first key.
    pub asks: Asks,
    pub choice: Choice,
}

/// What the core read from the screen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reading {
    /// The dialog on the screen now, or `None` when the screen shows none.
    pub dialog: Option<Dialog>,
    /// The screen changed since the last key. The first reading has it true.
    pub damaged: bool,
}

/// What the core must do next.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Next {
    /// Send these keys, wait up to [`super::DAMAGE_WAIT_MS`] for the screen to change, and read.
    Send(Vec<Key>),
    /// Paste the text, wait for the screen to change, and read.
    Paste(String),
    /// Send Enter, then wait up to [`super::CONFIRM_WAIT_MS`] for the dialog to go, and read.
    Commit,
    /// Send nothing. Wait for the dialog to go (the time of the wait is not over), and read.
    Wait,
    /// The sequence ended.
    Done(Result<Outcome, Failure>),
}

/// How a sequence that sent its last key ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The dialog went after the last key.
    DialogGone,
    /// The last key was sent and the dialog stays. The answer is unknown.
    DialogStays,
}

/// Why no key was sent after the last reading (the sequence "aborted before Enter").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    /// The screen shows no dialog.
    NoDialog,
    /// The dialog does not match the request, or it changed during the sequence.
    DialogChanged,
    /// The pointer is not where the keys put it.
    PointerLost,
    /// The screen did not answer a move key, also after the retries.
    StepFailed,
    /// The dialog has no pointer, or no checkbox, or the option is not there.
    Unsupported,
    /// The text did not show in the option.
    TextNotShown,
    /// The person typed in the pane. The core ends the sequence and reports this.
    UserTyped,
    /// The dialog on the screen asks for something other than the request: another command,
    /// another file or another question. No key was sent, and the dialog stays for the person.
    OtherRequest,
}

impl Failure {
    /// The reason of `delivery_failed`.
    pub fn reason(self) -> &'static str {
        match self {
            Self::NoDialog => "no_dialog",
            Self::DialogChanged => "dialog_changed",
            Self::PointerLost => "pointer_lost",
            Self::StepFailed => "step_failed",
            Self::Unsupported => "keystrokes_unsupported",
            Self::TextNotShown => "text_not_shown",
            Self::UserTyped => "user_typed",
            Self::OtherRequest => "request_not_on_screen",
        }
    }
}

/// How alike two labels are, from 0 to 1: one minus the edit distance divided by the longer
/// length. The case and the white space do not count.
pub fn similarity(a: &str, b: &str) -> f32 {
    let norm = |s: &str| -> Vec<char> {
        s.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase()
            .chars()
            .collect()
    };
    let (a, b) = (norm(a), norm(b));
    let longest = a.len().max(b.len());
    if longest == 0 {
        return 1.0;
    }
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut row = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            row.push(
                (previous[j] + cost)
                    .min(previous[j + 1] + 1)
                    .min(row[j] + 1),
            );
        }
        previous = row;
    }
    1.0 - previous[b.len()] as f32 / longest as f32
}

/// The text without the runs of white space: each run is one space, and none is at the ends.
fn collapse(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Whether `shown`, the rows of a dialog that hold a command, show the command `want`. A terminal
/// breaks a long line at a space or in a word, so a row break counts as one space or as nothing.
/// The runs of white space count as one space. Any other difference, also a row that is cut or
/// missing, is another command.
pub fn shows_command(shown: &str, want: &str) -> bool {
    let want: Vec<char> = collapse(want).chars().collect();
    let rows: Vec<Vec<char>> = shown
        .lines()
        .map(|row| collapse(row).chars().collect::<Vec<char>>())
        .filter(|row| !row.is_empty())
        .collect();
    if want.is_empty() || rows.is_empty() {
        return false;
    }
    let mut at = 0;
    for (index, row) in rows.iter().enumerate() {
        // A row starts with a character that is not a space. A space of the command here is the
        // place where the terminal broke the line.
        if index > 0 && want.get(at) == Some(&' ') {
            at += 1;
        }
        if want.get(at..at + row.len()) != Some(row.as_slice()) {
            return false;
        }
        at += row.len();
    }
    at == want.len()
}

/// Whether the dialog asks for what the request is about.
fn asks_for(asks: &Asks, dialog: &Dialog) -> bool {
    match asks {
        Asks::Anything => true,
        Asks::Command(want) => dialog
            .command
            .as_deref()
            .is_some_and(|shown| shows_command(shown, want)),
        Asks::Mentions(names) => {
            let text = collapse(&format!(
                "{}\n{}",
                dialog.title.as_deref().unwrap_or(""),
                dialog.command.as_deref().unwrap_or("")
            ));
            !names.is_empty()
                && names
                    .iter()
                    .all(|name| !name.trim().is_empty() && text.contains(name.trim()))
        }
        Asks::Title(want) => dialog
            .title
            .as_deref()
            .is_some_and(|title| similarity(title, want) >= MIN_SIMILARITY),
    }
}

/// The option that a request names: `@allow` is the option that the screen manifest named
/// `allow`, and any other text is a label.
fn locate(dialog: &Dialog, wanted: &str) -> Option<usize> {
    match wanted.strip_prefix('@') {
        Some(name) => dialog.choices.get(name).copied(),
        None => find(dialog, wanted),
    }
}

/// The index of the option whose label is the most like `wanted`, with at least
/// [`MIN_SIMILARITY`].
fn find(dialog: &Dialog, wanted: &str) -> Option<usize> {
    dialog
        .options
        .iter()
        .enumerate()
        .map(|(i, o)| (i, similarity(&o.label, wanted)))
        .filter(|(_, s)| *s >= MIN_SIMILARITY)
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(i, _)| i)
}

#[derive(Clone, Debug)]
enum Phase {
    Start,
    /// A move key is out. `from` is the pointer before the key; `tries` the keys sent for this
    /// step.
    Moving {
        from: usize,
        tries: u8,
    },
    /// An accelerator is out.
    Accelerated,
    /// A space is out to toggle option `index`, which was `was` before.
    Toggling {
        index: usize,
        was: bool,
    },
    /// The text is out.
    Pasted,
    /// Enter is out.
    Committed,
    Ended,
}

/// The sequence for one request.
#[derive(Debug)]
pub struct Verifier {
    request: Request,
    phase: Phase,
    /// The options that still must be toggled, the last first.
    pending: Vec<usize>,
    /// The text was pasted.
    pasted: bool,
    /// The labels the dialog had at the start. A change of them ends the sequence.
    labels: Vec<String>,
    /// The title and the command of the dialog at the start. A dialog with others is the dialog
    /// of another request.
    asked: (Option<String>, Option<String>),
}

impl Verifier {
    pub fn new(request: Request) -> Self {
        Self {
            request,
            phase: Phase::Start,
            pending: Vec::new(),
            pasted: false,
            labels: Vec::new(),
            asked: (None, None),
        }
    }

    fn fail(&mut self, failure: Failure) -> Next {
        self.phase = Phase::Ended;
        Next::Done(Err(failure))
    }

    fn done(&mut self, outcome: Outcome) -> Next {
        self.phase = Phase::Ended;
        Next::Done(Ok(outcome))
    }

    /// Whether the dialog on the screen is the dialog of another request than the dialog of the
    /// start: its title or its command differs. Without a command, the dialogs of two requests can
    /// look the same, and this says no.
    fn other_request(&self, dialog: &Dialog) -> bool {
        let (title, command) = &self.asked;
        (command.is_some() || title.is_some())
            && (dialog.command != *command || dialog.title != *title)
    }

    /// Whether the dialog on the screen is the dialog of the start.
    fn same_dialog(&self, dialog: &Dialog) -> bool {
        !self.other_request(dialog)
            && dialog.options.len() == self.labels.len()
            && dialog
                .options
                .iter()
                .zip(&self.labels)
                .all(|(o, l)| similarity(&o.label, l) >= MIN_SIMILARITY)
    }

    /// The first reading: checks the dialog and plans.
    fn begin(&mut self, dialog: &Dialog) -> Next {
        // The dialog must ask for the request of the interaction. Else the keys would answer
        // another request: a later one of the harness, or a text that only looks like a dialog.
        if !asks_for(&self.request.asks, dialog) {
            return self.fail(Failure::OtherRequest);
        }
        self.asked = (dialog.title.clone(), dialog.command.clone());
        // The labels that the interaction names must be there, in order.
        if !self.request.expected.is_empty() {
            let fits = dialog.options.len() == self.request.expected.len()
                && dialog
                    .options
                    .iter()
                    .zip(&self.request.expected)
                    .all(|(o, l)| similarity(&o.label, l) >= MIN_SIMILARITY);
            if !fits {
                return self.fail(Failure::DialogChanged);
            }
        }
        self.labels = dialog.options.iter().map(|o| o.label.clone()).collect();
        if dialog.pointer.is_none() {
            return self.fail(Failure::Unsupported);
        }
        match self.request.choice.clone() {
            Choice::Option(label) => {
                let Some(target) = locate(dialog, &label) else {
                    return self.fail(Failure::DialogChanged);
                };
                // From now on the choice is the label of that option.
                self.request.choice = Choice::Option(dialog.options[target].label.clone());
                self.go_to(dialog, target)
            }
            Choice::Named(name) => {
                let Some(target) = dialog.choices.get(&name).copied() else {
                    return self.fail(Failure::Unsupported);
                };
                // From now on the choice is the label of that option.
                self.request.choice = Choice::Option(dialog.options[target].label.clone());
                self.go_to(dialog, target)
            }
            Choice::Toggle(labels) => {
                let mut wanted = Vec::new();
                for label in &labels {
                    let Some(index) = find(dialog, label) else {
                        return self.fail(Failure::DialogChanged);
                    };
                    if dialog.options[index].checked.is_none() {
                        return self.fail(Failure::Unsupported);
                    }
                    wanted.push(index);
                }
                // The options whose checkbox differs from the wish. The smallest index is last, so
                // that the pointer only moves down.
                let mut pending: Vec<usize> = dialog
                    .options
                    .iter()
                    .enumerate()
                    .filter(|(i, o)| {
                        o.checked.is_some() && (o.checked == Some(true)) != wanted.contains(i)
                    })
                    .map(|(i, _)| i)
                    .collect();
                pending.sort_unstable_by(|a, b| b.cmp(a));
                self.pending = pending;
                self.next_toggle(dialog)
            }
            Choice::Text { option, text } => {
                let Some(target) = locate(dialog, &option) else {
                    return self.fail(Failure::DialogChanged);
                };
                self.request.choice = Choice::Text {
                    option: dialog.options[target].label.clone(),
                    text,
                };
                self.go_to(dialog, target)
            }
        }
    }

    /// The next move toward `target`, or the step that comes when the pointer is there.
    fn go_to(&mut self, dialog: &Dialog, target: usize) -> Next {
        let Some(pointer) = dialog.pointer else {
            return self.fail(Failure::Unsupported);
        };
        if pointer == target {
            return self.at_target(dialog, target);
        }
        // An accelerator is one key that cannot go wrong halfway. It is used
        // for the single choice of an option that has a digit.
        if matches!(self.request.choice, Choice::Option(_) | Choice::Named(_))
            && dialog.accelerators
            && let Some(number) = dialog.options[target].number
            && number <= 9
            && let Some(digit) = char::from_digit(number, 10)
        {
            self.phase = Phase::Accelerated;
            return Next::Send(vec![Key::Char(digit)]);
        }
        let key = if target > pointer { Key::Down } else { Key::Up };
        self.phase = Phase::Moving {
            from: pointer,
            tries: 1,
        };
        Next::Send(vec![key])
    }

    /// The pointer is on `target`.
    fn at_target(&mut self, dialog: &Dialog, target: usize) -> Next {
        match self.request.choice.clone() {
            Choice::Option(_) | Choice::Named(_) => {
                self.phase = Phase::Committed;
                Next::Commit
            }
            Choice::Text { text, .. } => {
                if self.pasted {
                    self.phase = Phase::Committed;
                    return Next::Commit;
                }
                self.pasted = true;
                self.phase = Phase::Pasted;
                Next::Paste(text)
            }
            Choice::Toggle(_) => self.toggle_here(dialog, target),
        }
    }

    fn toggle_here(&mut self, dialog: &Dialog, index: usize) -> Next {
        let was = dialog.options[index].checked.unwrap_or(false);
        self.phase = Phase::Toggling { index, was };
        Next::Send(vec![Key::Space])
    }

    /// The next option to toggle, or the end of the list.
    fn next_toggle(&mut self, dialog: &Dialog) -> Next {
        match self.pending.last().copied() {
            Some(index) => self.go_to_toggle(dialog, index),
            None => {
                self.phase = Phase::Committed;
                Next::Commit
            }
        }
    }

    fn go_to_toggle(&mut self, dialog: &Dialog, index: usize) -> Next {
        let Some(pointer) = dialog.pointer else {
            return self.fail(Failure::Unsupported);
        };
        if pointer == index {
            return self.toggle_here(dialog, index);
        }
        let key = if index > pointer { Key::Down } else { Key::Up };
        self.phase = Phase::Moving {
            from: pointer,
            tries: 1,
        };
        Next::Send(vec![key])
    }

    /// The target of the move in progress.
    fn target_index(&self, dialog: &Dialog) -> Option<usize> {
        match &self.request.choice {
            Choice::Option(label) => find(dialog, label),
            Choice::Text { option, .. } => find(dialog, option),
            Choice::Toggle(_) => self.pending.last().copied(),
            Choice::Named(_) => None,
        }
    }

    /// The reading that follows the last thing that the core did. The first call starts the
    /// sequence.
    pub fn observe(&mut self, reading: &Reading) -> Next {
        let phase = self.phase.clone();
        match (phase, &reading.dialog) {
            (Phase::Ended, _) => Next::Done(Err(Failure::DialogChanged)),
            (Phase::Start, None) => self.fail(Failure::NoDialog),
            (Phase::Start, Some(dialog)) => self.begin(dialog),
            (Phase::Committed, None) | (Phase::Accelerated, None) => self.done(Outcome::DialogGone),
            // The dialog of the next request came after the key: the dialog of this one went.
            (Phase::Committed | Phase::Accelerated, Some(dialog)) if self.other_request(dialog) => {
                self.done(Outcome::DialogGone)
            }
            (Phase::Committed, Some(_)) => {
                // The dialog stays after the last key. A reading without damage is the end of the
                // wait.
                if reading.damaged {
                    Next::Wait
                } else {
                    self.done(Outcome::DialogStays)
                }
            }
            (_, None) => self.fail(Failure::DialogChanged),
            (Phase::Moving { from, tries }, Some(dialog)) => self.after_move(dialog, from, tries),
            (Phase::Accelerated, Some(dialog)) => {
                // The key moved the pointer and did not commit: check, then Enter.
                if !self.same_dialog(dialog) {
                    return self.fail(Failure::DialogChanged);
                }
                match (self.target_index(dialog), dialog.pointer) {
                    (Some(target), Some(pointer)) if target == pointer => {
                        self.phase = Phase::Committed;
                        Next::Commit
                    }
                    _ => self.fail(Failure::PointerLost),
                }
            }
            (Phase::Toggling { index, was }, Some(dialog)) => {
                if !self.same_dialog(dialog) {
                    return self.fail(Failure::DialogChanged);
                }
                if dialog.options[index].checked == Some(!was) {
                    self.pending.pop();
                    self.next_toggle(dialog)
                } else {
                    self.fail(Failure::StepFailed)
                }
            }
            (Phase::Pasted, Some(dialog)) => {
                if self.other_request(dialog) {
                    return self.fail(Failure::DialogChanged);
                }
                let shown = match (&self.request.choice, dialog.pointer) {
                    (Choice::Text { text, .. }, Some(pointer)) => {
                        let head: String = text.chars().take(16).collect();
                        dialog.options[pointer].label.contains(&head)
                    }
                    _ => false,
                };
                if shown {
                    self.phase = Phase::Committed;
                    Next::Commit
                } else {
                    self.fail(Failure::TextNotShown)
                }
            }
        }
    }

    fn after_move(&mut self, dialog: &Dialog, from: usize, tries: u8) -> Next {
        // The labels may change in a text (the option that shows typed text), but not their
        // number or their order.
        if dialog.options.len() != self.labels.len() {
            return self.fail(Failure::DialogChanged);
        }
        let Some(pointer) = dialog.pointer else {
            return self.fail(Failure::PointerLost);
        };
        let Some(target) = self.target_index(dialog) else {
            return self.fail(Failure::DialogChanged);
        };
        if !self.same_dialog(dialog) {
            return self.fail(Failure::DialogChanged);
        }
        let towards = if target > from {
            from + 1
        } else {
            from.saturating_sub(1)
        };
        if pointer == from {
            // The screen did not answer the key.
            if tries > MOVE_RETRIES {
                return self.fail(Failure::StepFailed);
            }
            let key = if target > pointer { Key::Down } else { Key::Up };
            self.phase = Phase::Moving {
                from,
                tries: tries + 1,
            };
            return Next::Send(vec![key]);
        }
        if pointer != towards {
            // Moved, but not by one toward the target: a wrap, a jump, or another key.
            return self.fail(Failure::PointerLost);
        }
        if pointer == target {
            return match &self.request.choice {
                Choice::Toggle(_) => {
                    let dialog = dialog.clone();
                    self.toggle_here(&dialog, pointer)
                }
                _ => self.at_target(dialog, pointer),
            };
        }
        let key = if target > pointer { Key::Down } else { Key::Up };
        self.phase = Phase::Moving {
            from: pointer,
            tries: 1,
        };
        Next::Send(vec![key])
    }
}
