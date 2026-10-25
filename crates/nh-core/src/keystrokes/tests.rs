use std::{
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};

use nh_agents::screen::{Dialog, DialogOption};
use nh_proto::api::{
    calls::agent::{
        ApprovalDecision, Behavior, Decision, PlanDecision, QuestionAnswer, QuestionDecision,
    },
    common::Risk,
    model::{
        ApprovalPayload, GateMode, InteractionKind, InteractionPayload, PlanPayload, Question,
        QuestionOption, QuestionPayload,
    },
    rich::Markdown,
    time::Timestamp,
};

use super::*;
use crate::interaction::model::{Interaction, OpenSpec, Source};

/// A terminal with a dialog that answers keys. `log` has what the program got.
struct Fake {
    labels: Vec<String>,
    pointer: usize,
    gone: bool,
    changed: bool,
    log: Vec<Key>,
    /// The person types after this many keys.
    typed_after: Option<usize>,
    /// The dialog is drawn with other labels after this many keys.
    redraw_after: Option<usize>,
    /// The command that the dialog asks for.
    command: Option<String>,
}

impl Fake {
    fn new(labels: &[&str], pointer: usize) -> Self {
        Self {
            labels: labels.iter().map(|s| (*s).to_owned()).collect(),
            pointer,
            gone: false,
            changed: true,
            log: Vec::new(),
            typed_after: None,
            redraw_after: None,
            command: Some("ls".to_owned()),
        }
    }
}

impl Terminal for Fake {
    fn dialog(&mut self) -> Option<Dialog> {
        if self.gone {
            return None;
        }
        let options = self
            .labels
            .iter()
            .enumerate()
            .map(|(i, label)| DialogOption {
                number: Some(i as u32 + 1),
                label: label.clone(),
                row: i as u16,
                pointed: i == self.pointer,
                checked: None,
            })
            .collect();
        Some(Dialog {
            options,
            pointer: Some(self.pointer),
            accelerators: false,
            map: Default::default(),
            choices: [("allow".to_owned(), 0), ("deny".to_owned(), 2)].into(),
            multi_toggle: None,
            rows: (0, 2),
            title: Some("proceed".to_owned()),
            command: self.command.clone(),
        })
    }

    fn send(&mut self, key: Key) -> Result<(), String> {
        self.log.push(key);
        self.changed = true;
        if self.redraw_after == Some(self.log.len()) {
            self.labels = vec!["A".into(), "B".into(), "C".into()];
        }
        match key {
            Key::Down => self.pointer = (self.pointer + 1).min(self.labels.len() - 1),
            Key::Up => self.pointer = self.pointer.saturating_sub(1),
            Key::Enter => self.gone = true,
            _ => {}
        }
        Ok(())
    }

    fn paste(&mut self, _: &str) -> Result<(), String> {
        Ok(())
    }

    fn interrupted(&self) -> bool {
        self.typed_after.is_some_and(|n| self.log.len() >= n)
    }

    async fn damage(&mut self, _: Duration) -> bool {
        std::mem::take(&mut self.changed)
    }
}

fn named(name: &str) -> Request {
    Request {
        expected: Vec::new(),
        asks: Asks::Command("ls".to_owned()),
        choice: Choice::Named(name.to_owned()),
    }
}

#[tokio::test]
async fn the_sequence_moves_the_pointer_and_presses_enter_once() {
    let mut term = Fake::new(&["Yes", "Yes, always", "No"], 0);
    let commits = AtomicUsize::new(0);
    let end = run(&mut term, named("deny"), || {
        commits.fetch_add(1, Ordering::Relaxed);
    })
    .await;
    assert_eq!(end, Ok(Outcome::DialogGone));
    assert_eq!(term.log, [Key::Down, Key::Down, Key::Enter]);
    assert_eq!(
        commits.load(Ordering::Relaxed),
        1,
        "told before Enter, once"
    );
}

#[tokio::test]
async fn a_person_who_types_ends_the_sequence_before_the_next_key() {
    let mut term = Fake::new(&["Yes", "Yes, always", "No"], 0);
    term.typed_after = Some(1);
    let end = run(&mut term, named("deny"), || {}).await;
    assert_eq!(end, Err(Failure::UserTyped));
    assert_eq!(term.log, [Key::Down], "no more keys, and no Enter");
}

#[tokio::test]
async fn a_dialog_that_changes_ends_the_sequence_without_enter() {
    let mut term = Fake::new(&["Yes", "Yes, always", "No"], 0);
    term.redraw_after = Some(1);
    let commits = AtomicUsize::new(0);
    let end = run(&mut term, named("deny"), || {
        commits.fetch_add(1, Ordering::Relaxed);
    })
    .await;
    assert_eq!(end, Err(Failure::DialogChanged));
    assert!(!term.log.contains(&Key::Enter));
    assert_eq!(commits.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn a_screen_without_a_dialog_gets_no_key() {
    let mut term = Fake::new(&["Yes", "No", "Maybe"], 0);
    term.gone = true;
    let end = run(&mut term, named("allow"), || {}).await;
    assert_eq!(end, Err(Failure::NoDialog));
    assert!(term.log.is_empty());
}

#[tokio::test]
async fn a_dialog_of_another_command_gets_no_key_and_nothing_is_committed() {
    // The card is for `ls`, and the dialog on the screen asks for another command.
    let mut term = Fake::new(&["Yes", "Yes, always", "No"], 0);
    term.command = Some("rm -rf build".to_owned());
    let commits = AtomicUsize::new(0);
    let end = run(&mut term, named("allow"), || {
        commits.fetch_add(1, Ordering::Relaxed);
    })
    .await;
    assert_eq!(end, Err(Failure::OtherRequest));
    assert!(term.log.is_empty(), "no key: {:?}", term.log);
    assert_eq!(commits.load(Ordering::Relaxed), 0);
}

// --- what to type for a decision -----------------------------------------------------------

fn interaction(payload: InteractionPayload, decision: Decision) -> Interaction {
    let (mut interaction, _) = Interaction::open(
        nh_proto::api::ids::InteractionId::new(ulid::Ulid::from(1u128)),
        "i1".to_owned(),
        OpenSpec {
            kind: match &payload {
                InteractionPayload::Approval(_) => InteractionKind::Approval,
                InteractionPayload::Question(_) => InteractionKind::Question,
                _ => InteractionKind::PlanReview,
            },
            run: None,
            pane: None,
            native_ref: None,
            title: "t".to_owned(),
            risk: Risk::Low,
            payload,
            mode: GateMode::Observe,
            answerable: true,
            channels: vec![nh_proto::api::model::AnswerChannel::Keystroke],
            source: Source::Hook,
            confidence: 1.0,
            deadline: None,
            facts: None,
            by_policy: false,
            keystroke: true,
        },
        Timestamp(1),
    );
    interaction.delivery.decision = Some(decision);
    interaction
}

fn command(line: &str) -> nh_proto::api::rich::Command {
    nh_proto::api::rich::Command {
        argv: line.split_whitespace().map(str::to_owned).collect(),
        display: nh_proto::api::rich::Spans {
            text: line.to_owned(),
            spans: Vec::new(),
        },
        cwd: String::new(),
        risk: Risk::Low,
        reasons: Vec::new(),
        policy_hint: None,
    }
}

fn approval_payload() -> InteractionPayload {
    approval_of(Some("ls -la"), &[])
}

fn approval_of(line: Option<&str>, paths: &[&str]) -> InteractionPayload {
    InteractionPayload::Approval(Box::new(ApprovalPayload {
        tool: "Bash".to_owned(),
        command: line.map(command),
        cwd: None,
        paths: paths.iter().map(|p| (*p).to_owned()).collect(),
        diff: None,
        input: None,
        peek: Vec::new(),
        policy_note: None,
    }))
}

fn approval(behavior: Behavior, always: bool) -> Decision {
    Decision::Approval(ApprovalDecision {
        behavior,
        always,
        persist: false,
        message: None,
        stop: false,
    })
}

fn choice(request: Result<Request, &'static str>) -> Choice {
    request.expect("a request").choice
}

#[test]
fn an_approval_is_typed_as_allow_allow_always_or_deny() {
    let p = approval_payload();
    assert_eq!(
        choice(request_for(&interaction(
            p.clone(),
            approval(Behavior::Allow, false)
        ))),
        Choice::Named("allow".into())
    );
    assert_eq!(
        choice(request_for(&interaction(
            p.clone(),
            approval(Behavior::Allow, true)
        ))),
        Choice::Named("allow_always".into())
    );
    assert_eq!(
        choice(request_for(&interaction(
            p,
            approval(Behavior::Deny, false)
        ))),
        Choice::Named("deny".into())
    );
}

#[test]
fn a_plan_is_approved_or_rejected_with_the_feedback_in_the_text_option() {
    let plan = InteractionPayload::PlanReview(PlanPayload {
        plan: Markdown {
            html: String::new(),
            truncated: false,
        },
        plan_path: None,
    });
    let approve = Decision::PlanReview(PlanDecision {
        approve: true,
        feedback: None,
    });
    assert_eq!(
        choice(request_for(&interaction(plan.clone(), approve))),
        Choice::Named("allow".into())
    );
    let reject = Decision::PlanReview(PlanDecision {
        approve: false,
        feedback: Some("use a smaller step".into()),
    });
    assert_eq!(
        choice(request_for(&interaction(plan.clone(), reject))),
        Choice::Text {
            option: "@deny".into(),
            text: "use a smaller step".into()
        }
    );
    let plain = Decision::PlanReview(PlanDecision {
        approve: false,
        feedback: None,
    });
    assert_eq!(
        choice(request_for(&interaction(plan, plain))),
        Choice::Named("deny".into())
    );
}

fn question(multi: bool) -> InteractionPayload {
    InteractionPayload::Question(QuestionPayload {
        questions: vec![Question {
            question: "Color?".into(),
            header: None,
            options: vec![
                QuestionOption {
                    label: "Red".into(),
                    description: None,
                },
                QuestionOption {
                    label: "Blue".into(),
                    description: None,
                },
            ],
            multi_select: multi,
        }],
        context: None,
    })
}

fn answer(selected: &[&str], text: Option<&str>) -> Decision {
    Decision::Question(QuestionDecision {
        answers: vec![QuestionAnswer {
            question: "Color?".into(),
            selected: selected.iter().map(|s| (*s).to_owned()).collect(),
            text: text.map(str::to_owned),
        }],
    })
}

#[test]
fn a_question_is_typed_as_an_option_a_list_of_options_or_free_text() {
    assert_eq!(
        choice(request_for(&interaction(
            question(false),
            answer(&["Blue"], None)
        ))),
        Choice::Option("Blue".into())
    );
    assert_eq!(
        choice(request_for(&interaction(
            question(true),
            answer(&["Red", "Blue"], None)
        ))),
        Choice::Toggle(vec!["Red".into(), "Blue".into()])
    );
    assert_eq!(
        choice(request_for(&interaction(
            question(false),
            answer(&[], Some("teal"))
        ))),
        Choice::Text {
            option: "@other".into(),
            text: "teal".into()
        }
    );
}

#[test]
fn an_answer_that_keys_cannot_give_is_refused_before_a_key_is_sent() {
    // Two options for a question with one choice, and nothing at all.
    assert!(
        request_for(&interaction(
            question(false),
            answer(&["Red", "Blue"], None)
        ))
        .is_err()
    );
    assert!(request_for(&interaction(question(false), answer(&[], None))).is_err());
    let mut none = interaction(approval_payload(), approval(Behavior::Allow, false));
    none.delivery.decision = None;
    assert_eq!(request_for(&none), Err("no_decision"));
}

#[test]
fn the_request_names_what_the_dialog_must_ask_for() {
    let allow = || approval(Behavior::Allow, false);
    let asks = |payload: InteractionPayload, decision: Decision| {
        request_for(&interaction(payload, decision)).map(|r| r.asks)
    };
    // An approval of a command names the command, and one of a file names the file.
    assert_eq!(
        asks(approval_of(Some("pnpm test"), &[]), allow()),
        Ok(Asks::Command("pnpm test".into()))
    );
    assert_eq!(
        asks(approval_of(None, &["/work/src/note.txt"]), allow()),
        Ok(Asks::Mentions(vec!["note.txt".into()]))
    );
    // An approval with neither cannot be told from another one: no key answers it, also a deny.
    assert_eq!(
        asks(approval_of(None, &[]), allow()),
        Err("request_not_verifiable")
    );
    assert_eq!(
        asks(approval_of(None, &[]), approval(Behavior::Deny, false)),
        Err("request_not_verifiable")
    );
    // A question names its header, when it has one.
    let mut headed = question(false);
    if let InteractionPayload::Question(q) = &mut headed {
        q.questions[0].header = Some("Color".into());
    }
    assert_eq!(
        asks(headed, answer(&["Red"], None)),
        Ok(Asks::Title("Color".into()))
    );
    assert_eq!(
        asks(question(false), answer(&["Red"], None)),
        Ok(Asks::Anything)
    );
}
