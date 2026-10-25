use super::*;
use crate::screen::{Dialog, DialogOption};

/// A dialog that reacts to keys the way a terminal program does. Tests change its behavior.
struct Sim {
    labels: Vec<String>,
    checked: Vec<Option<bool>>,
    pointer: usize,
    wrap: bool,
    accelerators: bool,
    /// A digit commits at once.
    accelerator_commits: bool,
    /// The next move keys that the program ignores.
    ignore: u8,
    /// After this many keys the dialog is drawn again with other labels.
    redraw_after: Option<(usize, Vec<String>)>,
    /// The pointer jumps by this much on the next move (a person typed in the pane).
    jump: Option<usize>,
    gone: bool,
    /// What the dialog had when the key came, for the journal.
    keys: Vec<Key>,
    pasted: Option<String>,
    /// The program shows the dialog again after Enter (it did not take the key).
    deaf_to_enter: bool,
    changed: bool,
    /// The title and the command of the dialog.
    title: Option<String>,
    command: Option<String>,
    /// After Enter, the dialog of the next request comes with this command.
    next_command: Option<String>,
}

impl Sim {
    fn new(labels: &[&str], pointer: usize) -> Self {
        Self {
            labels: labels.iter().map(|s| (*s).to_owned()).collect(),
            checked: vec![None; labels.len()],
            pointer,
            wrap: true,
            accelerators: false,
            accelerator_commits: true,
            ignore: 0,
            redraw_after: None,
            jump: None,
            gone: false,
            keys: Vec::new(),
            pasted: None,
            deaf_to_enter: false,
            changed: true,
            title: None,
            command: None,
            next_command: None,
        }
    }

    fn dialog(&self) -> Option<Dialog> {
        if self.gone {
            return None;
        }
        let options = self
            .labels
            .iter()
            .enumerate()
            .map(|(i, label)| DialogOption {
                number: Some(i as u32 + 1),
                label: if self.pointer == i {
                    match &self.pasted {
                        Some(text) => format!("{label} {text}"),
                        None => label.clone(),
                    }
                } else {
                    label.clone()
                },
                row: 10 + i as u16,
                pointed: i == self.pointer,
                checked: self.checked[i],
            })
            .collect();
        Some(Dialog {
            options,
            pointer: Some(self.pointer),
            accelerators: self.accelerators,
            map: Default::default(),
            choices: Default::default(),
            multi_toggle: None,
            rows: (10, 10 + self.labels.len() as u16),
            title: self.title.clone(),
            command: self.command.clone(),
        })
    }

    fn press(&mut self, key: Key) {
        self.keys.push(key);
        self.changed = false;
        if let Some((after, labels)) = self.redraw_after.clone()
            && self.keys.len() > after
        {
            self.labels = labels;
            self.redraw_after = None;
            self.changed = true;
        }
        let last = self.labels.len() - 1;
        match key {
            Key::Up | Key::Down => {
                if self.ignore > 0 {
                    self.ignore -= 1;
                    return;
                }
                if let Some(by) = self.jump.take() {
                    self.pointer = (self.pointer + by) % self.labels.len();
                    self.changed = true;
                    return;
                }
                let before = self.pointer;
                self.pointer = match (key, self.pointer) {
                    (Key::Up, 0) if self.wrap => last,
                    (Key::Up, 0) => 0,
                    (Key::Up, p) => p - 1,
                    (_, p) if p == last && self.wrap => 0,
                    (_, p) if p == last => last,
                    (_, p) => p + 1,
                };
                self.changed = self.pointer != before;
            }
            Key::Char(c) if self.accelerators && c.is_ascii_digit() => {
                let n = c.to_digit(10).unwrap_or(0) as usize;
                if n >= 1 && n <= self.labels.len() {
                    self.pointer = n - 1;
                    self.changed = true;
                    if self.accelerator_commits {
                        self.gone = true;
                    }
                }
            }
            Key::Space => {
                if let Some(Some(on)) = self.checked.get(self.pointer).copied() {
                    self.checked[self.pointer] = Some(!on);
                    self.changed = true;
                }
            }
            Key::Enter => {
                if let Some(command) = self.next_command.take() {
                    self.command = Some(command);
                    self.pointer = 0;
                    self.changed = true;
                } else if !self.deaf_to_enter {
                    self.gone = true;
                    self.changed = true;
                }
            }
            Key::Char(_) => {}
        }
    }

    fn paste(&mut self, text: &str) {
        self.pasted = Some(text.to_owned());
        self.changed = true;
    }
}

/// Runs the verifier against the simulator, as the core does. Gives the end and the keys that
/// reached the program.
fn drive(sim: &mut Sim, request: Request) -> Result<Outcome, Failure> {
    let mut verifier = Verifier::new(request);
    let mut reading = Reading {
        dialog: sim.dialog(),
        damaged: true,
    };
    for _ in 0..40 {
        match verifier.observe(&reading) {
            Next::Done(end) => return end,
            Next::Send(keys) => {
                for key in keys {
                    sim.press(key);
                }
            }
            Next::Paste(text) => sim.paste(&text),
            Next::Commit => sim.press(Key::Enter),
            Next::Wait => {}
        }
        reading = Reading {
            dialog: sim.dialog(),
            damaged: sim.changed,
        };
        sim.changed = false;
    }
    panic!("the sequence never ended");
}

fn pick(label: &str) -> Request {
    Request {
        expected: Vec::new(),
        asks: Asks::Anything,
        choice: Choice::Option(label.to_owned()),
    }
}

const LABELS: [&str; 4] = [
    "Yes",
    "Yes, and don't ask again",
    "Switch to auto mode",
    "No",
];

#[test]
fn the_pointer_already_on_the_target_gets_only_enter() {
    let mut sim = Sim::new(&LABELS, 0);
    assert_eq!(drive(&mut sim, pick("Yes")), Ok(Outcome::DialogGone));
    assert_eq!(sim.keys, [Key::Enter]);
}

#[test]
fn the_pointer_moves_down_one_key_at_a_time_and_then_enter() {
    let mut sim = Sim::new(&LABELS, 0);
    assert_eq!(drive(&mut sim, pick("No")), Ok(Outcome::DialogGone));
    assert_eq!(sim.keys, [Key::Down, Key::Down, Key::Down, Key::Enter]);
}

#[test]
fn the_pointer_moves_up_one_key_at_a_time_and_then_enter() {
    let mut sim = Sim::new(&LABELS, 3);
    assert_eq!(
        drive(&mut sim, pick("Yes, and don't ask again")),
        Ok(Outcome::DialogGone)
    );
    assert_eq!(sim.keys, [Key::Up, Key::Up, Key::Enter]);
}

#[test]
fn a_dialog_that_wraps_is_crossed_without_the_wrap() {
    // From the last option to the first, a menu that wraps would take one key. The plan never
    // relies on it: a wrap is a jump that the verifier would take for an error.
    let mut sim = Sim::new(&LABELS, 3);
    assert_eq!(drive(&mut sim, pick("Yes")), Ok(Outcome::DialogGone));
    assert_eq!(
        sim.keys,
        [Key::Up, Key::Up, Key::Up, Key::Enter],
        "linear moves"
    );
}

#[test]
fn a_pointer_that_wraps_by_surprise_ends_the_sequence_without_enter() {
    // Up at the top wraps to the last option. Here the first key finds the pointer on option 1
    // (index 1) and a hidden extra move puts it at the end.
    let mut sim = Sim::new(&LABELS, 2);
    sim.jump = Some(2);
    let end = drive(&mut sim, pick("Yes"));
    assert_eq!(end, Err(Failure::PointerLost));
    assert!(!sim.keys.contains(&Key::Enter), "no Enter after a jump");
}

#[test]
fn a_dialog_that_is_drawn_again_with_other_labels_ends_the_sequence_without_enter() {
    let mut sim = Sim::new(&LABELS, 0);
    sim.redraw_after = Some((
        1,
        [
            "Allow once",
            "Allow always",
            "Switch to auto mode",
            "Reject",
        ]
        .map(str::to_owned)
        .to_vec(),
    ));
    let end = drive(&mut sim, pick("No"));
    assert_eq!(end, Err(Failure::DialogChanged));
    assert!(!sim.keys.contains(&Key::Enter));
}

#[test]
fn a_key_that_the_program_ignores_is_sent_again_twice_and_then_the_sequence_fails() {
    let mut sim = Sim::new(&LABELS, 0);
    sim.ignore = 2;
    assert_eq!(
        drive(&mut sim, pick("Yes, and don't ask again")),
        Ok(Outcome::DialogGone)
    );
    assert_eq!(sim.keys, [Key::Down, Key::Down, Key::Down, Key::Enter]);
    let mut stuck = Sim::new(&LABELS, 0);
    stuck.ignore = 10;
    assert_eq!(drive(&mut stuck, pick("No")), Err(Failure::StepFailed));
    assert_eq!(
        stuck.keys,
        [Key::Down, Key::Down, Key::Down],
        "one key and two retries"
    );
}

#[test]
fn an_accelerator_is_one_key() {
    let mut sim = Sim::new(&LABELS, 0);
    sim.accelerators = true;
    assert_eq!(drive(&mut sim, pick("No")), Ok(Outcome::DialogGone));
    assert_eq!(sim.keys, [Key::Char('4')]);
}

#[test]
fn an_accelerator_that_only_moves_the_pointer_is_followed_by_enter() {
    let mut sim = Sim::new(&LABELS, 0);
    sim.accelerators = true;
    sim.accelerator_commits = false;
    assert_eq!(drive(&mut sim, pick("No")), Ok(Outcome::DialogGone));
    assert_eq!(sim.keys, [Key::Char('4'), Key::Enter]);
}

#[test]
fn the_labels_may_differ_a_little_but_not_much() {
    let mut sim = Sim::new(&LABELS, 0);
    // One character off in a long label is a match; the label of another option is not.
    assert_eq!(
        drive(&mut sim, pick("Yes, and dont ask again")),
        Ok(Outcome::DialogGone)
    );
    let mut other = Sim::new(&LABELS, 0);
    assert_eq!(
        drive(&mut other, pick("Maybe")),
        Err(Failure::DialogChanged)
    );
    assert!(other.keys.is_empty(), "nothing was sent");
    assert!(similarity("Yes", "yes ") > 0.99);
    assert!(similarity("Yes", "No") < 0.5);
}

#[test]
fn the_expected_labels_of_the_interaction_must_be_the_labels_on_the_screen() {
    let mut sim = Sim::new(&["Red", "Blue"], 0);
    let request = Request {
        expected: vec!["Red".to_owned(), "Green".to_owned()],
        asks: Asks::Anything,
        choice: Choice::Option("Red".to_owned()),
    };
    assert_eq!(drive(&mut sim, request), Err(Failure::DialogChanged));
    assert!(sim.keys.is_empty());
    let mut fine = Sim::new(&["Red", "Blue"], 0);
    let request = Request {
        expected: vec!["Red".to_owned(), "Blue".to_owned()],
        asks: Asks::Anything,
        choice: Choice::Option("Blue".to_owned()),
    };
    assert_eq!(drive(&mut fine, request), Ok(Outcome::DialogGone));
    assert_eq!(fine.keys, [Key::Down, Key::Enter]);
}

#[test]
fn no_dialog_and_no_pointer_send_nothing() {
    let mut gone = Sim::new(&LABELS, 0);
    gone.gone = true;
    assert_eq!(drive(&mut gone, pick("Yes")), Err(Failure::NoDialog));
    // A dialog without a pointer cannot be answered with keys.
    let mut verifier = Verifier::new(pick("Yes"));
    let mut dialog = Sim::new(&LABELS, 0).dialog().expect("a dialog");
    dialog.pointer = None;
    let next = verifier.observe(&Reading {
        dialog: Some(dialog),
        damaged: true,
    });
    assert_eq!(next, Next::Done(Err(Failure::Unsupported)));
}

#[test]
fn a_dialog_that_stays_after_enter_is_not_confirmed() {
    let mut sim = Sim::new(&LABELS, 0);
    sim.deaf_to_enter = true;
    assert_eq!(drive(&mut sim, pick("Yes")), Ok(Outcome::DialogStays));
    assert_eq!(sim.keys, [Key::Enter]);
}

#[test]
fn a_list_with_many_choices_is_toggled_one_option_at_a_time() {
    let mut sim = Sim::new(&["Red", "Green", "Blue", "Other"], 0);
    sim.checked = vec![Some(false), Some(true), Some(false), None];
    // Green must go off, Blue must go on.
    let request = Request {
        expected: Vec::new(),
        asks: Asks::Anything,
        choice: Choice::Toggle(vec!["Red".to_owned(), "Blue".to_owned()]),
    };
    // Red is off and must be on; Green is on and must be off; Blue is off and must be on.
    assert_eq!(drive(&mut sim, request), Ok(Outcome::DialogGone));
    assert_eq!(&sim.checked[..3], [Some(true), Some(false), Some(true)]);
    assert_eq!(
        sim.keys,
        [
            Key::Space,
            Key::Down,
            Key::Space,
            Key::Down,
            Key::Space,
            Key::Enter
        ]
    );
}

#[test]
fn a_toggle_that_does_not_show_ends_the_sequence() {
    let mut sim = Sim::new(&["Red", "Green"], 0);
    // The program has no checkbox in the reading.
    sim.checked = vec![None, None];
    let request = Request {
        expected: Vec::new(),
        asks: Asks::Anything,
        choice: Choice::Toggle(vec!["Red".to_owned()]),
    };
    assert_eq!(drive(&mut sim, request), Err(Failure::Unsupported));
    assert!(sim.keys.is_empty());
}

#[test]
fn text_is_pasted_in_the_option_and_read_back_before_enter() {
    let mut sim = Sim::new(&["Red", "Type something.", "Chat about this"], 0);
    let request = Request {
        expected: Vec::new(),
        asks: Asks::Anything,
        choice: Choice::Text {
            option: "Type something.".to_owned(),
            text: "teal".to_owned(),
        },
    };
    assert_eq!(drive(&mut sim, request), Ok(Outcome::DialogGone));
    assert_eq!(sim.keys, [Key::Down, Key::Enter]);
    assert_eq!(sim.pasted.as_deref(), Some("teal"));
}

#[test]
fn text_that_does_not_show_ends_the_sequence_without_enter() {
    let mut verifier = Verifier::new(Request {
        expected: Vec::new(),
        asks: Asks::Anything,
        choice: Choice::Text {
            option: "Type something.".to_owned(),
            text: "teal".to_owned(),
        },
    });
    let sim = Sim::new(&["Red", "Type something."], 1);
    let first = verifier.observe(&Reading {
        dialog: sim.dialog(),
        damaged: true,
    });
    assert_eq!(first, Next::Paste("teal".to_owned()));
    // The screen after the paste shows nothing new.
    let next = verifier.observe(&Reading {
        dialog: sim.dialog(),
        damaged: false,
    });
    assert_eq!(next, Next::Done(Err(Failure::TextNotShown)));
}

#[test]
fn the_failures_have_the_reasons_of_the_delivery_record() {
    let reasons: Vec<&str> = [
        Failure::NoDialog,
        Failure::DialogChanged,
        Failure::PointerLost,
        Failure::StepFailed,
        Failure::Unsupported,
        Failure::TextNotShown,
        Failure::UserTyped,
        Failure::OtherRequest,
    ]
    .iter()
    .map(|f| f.reason())
    .collect();
    assert_eq!(
        reasons,
        [
            "no_dialog",
            "dialog_changed",
            "pointer_lost",
            "step_failed",
            "keystrokes_unsupported",
            "text_not_shown",
            "user_typed",
            "request_not_on_screen"
        ]
    );
}

#[test]
fn a_choice_by_name_uses_the_label_that_the_screen_manifest_gave_the_option() {
    let mut sim = Sim::new(&LABELS, 0);
    // The dialog is read with the names of the manifest: deny is the last option here.
    let mut verifier = Verifier::new(Request {
        expected: Vec::new(),
        asks: Asks::Anything,
        choice: Choice::Named("deny".to_owned()),
    });
    let mut dialog = sim.dialog().expect("a dialog");
    dialog.choices.insert("deny".to_owned(), 3);
    let mut reading = Reading {
        dialog: Some(dialog.clone()),
        damaged: true,
    };
    let mut sent = Vec::new();
    for _ in 0..10 {
        match verifier.observe(&reading) {
            Next::Done(end) => {
                assert_eq!(end, Ok(Outcome::DialogGone));
                break;
            }
            Next::Send(keys) => {
                for key in keys {
                    sim.press(key);
                    sent.push(key);
                }
            }
            Next::Commit => {
                sim.press(Key::Enter);
                sent.push(Key::Enter);
            }
            other => panic!("unexpected {other:?}"),
        }
        let mut next = sim.dialog();
        if let Some(d) = next.as_mut() {
            d.choices.insert("deny".to_owned(), 3);
        }
        reading = Reading {
            dialog: next,
            damaged: sim.changed,
        };
    }
    assert_eq!(sent, [Key::Down, Key::Down, Key::Down, Key::Enter]);
    // A name that the dialog does not have cannot be answered.
    let mut other = Verifier::new(Request {
        expected: Vec::new(),
        asks: Asks::Anything,
        choice: Choice::Named("allow_always".to_owned()),
    });
    let next = other.observe(&Reading {
        dialog: Some(dialog),
        damaged: true,
    });
    assert_eq!(next, Next::Done(Err(Failure::Unsupported)));
}

// --- the request that the dialog asks for ----------------------------------------------------------

fn allow_command(command: &str) -> Request {
    Request {
        expected: Vec::new(),
        asks: Asks::Command(command.to_owned()),
        choice: Choice::Option("Yes".to_owned()),
    }
}

#[test]
fn a_dialog_that_asks_for_another_command_gets_no_key() {
    // The card is for `ls`. The harness shows the dialog of a later request.
    let mut sim = Sim::new(&LABELS, 0);
    sim.command = Some("rm -rf build".to_owned());
    assert_eq!(
        drive(&mut sim, allow_command("ls")),
        Err(Failure::OtherRequest)
    );
    assert!(sim.keys.is_empty(), "nothing was sent: {:?}", sim.keys);
    // The same command is answered.
    let mut same = Sim::new(&LABELS, 0);
    same.command = Some("ls".to_owned());
    assert_eq!(
        drive(&mut same, allow_command("ls")),
        Ok(Outcome::DialogGone)
    );
    assert_eq!(same.keys, [Key::Enter]);
}

#[test]
fn a_dialog_whose_command_the_screen_did_not_read_gets_no_key() {
    let mut sim = Sim::new(&LABELS, 0);
    assert_eq!(
        drive(&mut sim, allow_command("ls")),
        Err(Failure::OtherRequest)
    );
    assert!(sim.keys.is_empty());
}

#[test]
fn a_dialog_that_changes_to_another_command_during_the_moves_ends_the_sequence() {
    // The first reading shows the command of the card. Before the second move key, the dialog of
    // another request with the same labels takes its place.
    let mut verifier = Verifier::new(Request {
        expected: Vec::new(),
        asks: Asks::Command("ls".to_owned()),
        choice: Choice::Option("No".to_owned()),
    });
    let mut sim = Sim::new(&LABELS, 0);
    sim.command = Some("ls".to_owned());
    let first = verifier.observe(&Reading {
        dialog: sim.dialog(),
        damaged: true,
    });
    assert_eq!(first, Next::Send(vec![Key::Down]));
    sim.press(Key::Down);
    sim.command = Some("rm -rf build".to_owned());
    let next = verifier.observe(&Reading {
        dialog: sim.dialog(),
        damaged: true,
    });
    assert_eq!(next, Next::Done(Err(Failure::DialogChanged)));
}

#[test]
fn the_dialog_of_the_next_request_after_enter_means_that_this_dialog_went() {
    let mut sim = Sim::new(&LABELS, 0);
    sim.command = Some("ls".to_owned());
    sim.next_command = Some("pwd".to_owned());
    assert_eq!(
        drive(&mut sim, allow_command("ls")),
        Ok(Outcome::DialogGone)
    );
    assert_eq!(
        sim.keys,
        [Key::Enter],
        "one key, and none for the next dialog"
    );
}

#[test]
fn a_command_on_several_rows_is_the_same_command() {
    assert!(shows_command("touch note.txt", "touch note.txt"));
    assert!(shows_command("  touch   note.txt ", "touch note.txt"));
    // The terminal broke the line at a space, or in a word.
    assert!(shows_command("printf %s\n\"a b\"", "printf %s \"a b\""));
    assert!(shows_command(
        "cat /very/long/pa\nth/file",
        "cat /very/long/path/file"
    ));
    // A command with lines of its own.
    assert!(shows_command(
        "cat <<EOF\n  hello\nEOF",
        "cat <<EOF\n  hello\nEOF"
    ));
    // Another command, a part of it, or more than it.
    assert!(!shows_command("touch note.txt", "touch other.txt"));
    assert!(!shows_command("rm -rf / tmp", "rm -rf /tmp"));
    assert!(!shows_command("rm -rf /tmp", "rm -rf / tmp"));
    assert!(!shows_command("touch note", "touch note.txt"));
    assert!(!shows_command("touch note.txt && rm x", "touch note.txt"));
    assert!(!shows_command("", "ls"));
    assert!(!shows_command("ls", ""));
}

#[test]
fn a_file_or_a_question_must_be_the_one_of_the_dialog() {
    let mut sim = Sim::new(&LABELS, 0);
    sim.title = Some("make this edit to note.txt".to_owned());
    let edit = |name: &str| Request {
        expected: Vec::new(),
        asks: Asks::Mentions(vec![name.to_owned()]),
        choice: Choice::Option("Yes".to_owned()),
    };
    assert_eq!(
        drive(&mut sim, edit("other.txt")),
        Err(Failure::OtherRequest)
    );
    assert!(sim.keys.is_empty());
    assert_eq!(drive(&mut sim, edit("note.txt")), Ok(Outcome::DialogGone));
    let mut question = Sim::new(&["Red", "Blue"], 0);
    question.title = Some("Color".to_owned());
    let header = |text: &str| Request {
        expected: Vec::new(),
        asks: Asks::Title(text.to_owned()),
        choice: Choice::Option("Blue".to_owned()),
    };
    assert_eq!(
        drive(&mut question, header("Size")),
        Err(Failure::OtherRequest)
    );
    assert!(question.keys.is_empty());
    assert_eq!(
        drive(&mut question, header("Color")),
        Ok(Outcome::DialogGone)
    );
}
