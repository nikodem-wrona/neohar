use nh_proto::api::{
    calls::agent::QuestionAnswer,
    common::{Risk, Via},
    ids::InteractionId,
    model::{AnswerChannel, GateMode, InteractionKind, PlanPayload, Question, QuestionOption},
    rich::Command,
    time::Timestamp,
};
use serde_json::json;
use ulid::Ulid;

use super::*;
use crate::interaction::model::{Interaction, OpenSpec, Source};

fn made(payload: InteractionPayload, decision: Decision) -> Interaction {
    let kind = match &payload {
        InteractionPayload::Approval(_) => InteractionKind::Approval,
        InteractionPayload::Question(_) => InteractionKind::Question,
        InteractionPayload::PlanReview(_) => InteractionKind::PlanReview,
        InteractionPayload::Notice(_) => InteractionKind::Notice,
    };
    let (mut interaction, _) = Interaction::open(
        InteractionId::new(Ulid::from(1u128)),
        "i1".to_owned(),
        OpenSpec {
            kind,
            run: None,
            pane: None,
            native_ref: None,
            title: "t".to_owned(),
            risk: Risk::Low,
            payload,
            mode: GateMode::Gate,
            answerable: true,
            channels: vec![AnswerChannel::Native, AnswerChannel::Keystroke],
            source: Source::Hook,
            confidence: 1.0,
            deadline: None,
            facts: None,
            by_policy: false,
            keystroke: false,
        },
        Timestamp(1_000),
    );
    interaction
        .answer(decision, "ui", Via::Card)
        .expect("a valid decision");
    interaction
}

fn bash(command: &str, argv: &[&str]) -> InteractionPayload {
    InteractionPayload::Approval(Box::new(ApprovalPayload {
        tool: "Bash".to_owned(),
        command: Some(Command {
            argv: argv.iter().map(|s| (*s).to_owned()).collect(),
            display: nh_proto::api::rich::Spans {
                text: command.to_owned(),
                spans: Vec::new(),
            },
            cwd: "/w".to_owned(),
            risk: Risk::Low,
            reasons: Vec::new(),
            policy_hint: None,
        }),
        cwd: None,
        paths: Vec::new(),
        diff: None,
        input: None,
        peek: Vec::new(),
        policy_note: None,
    }))
}

fn approve(
    behavior: Behavior,
    always: bool,
    persist: bool,
    message: Option<&str>,
    stop: bool,
) -> Decision {
    Decision::Approval(ApprovalDecision {
        behavior,
        always,
        persist,
        message: message.map(str::to_owned),
        stop,
    })
}

fn out(harness: Harness, interaction: &Interaction, persist: bool) -> Value {
    serde_json::from_str(&hook_output(harness, interaction, persist).expect("an output"))
        .expect("json")
}

#[test]
fn allow_is_a_plain_decision() {
    let i = made(
        bash("ls", &["ls"]),
        approve(Behavior::Allow, false, false, None, false),
    );
    assert_eq!(
        out(Harness::Claude, &i, false),
        json!({"hookSpecificOutput": {"hookEventName": "PermissionRequest", "decision": {"behavior": "allow"}}})
    );
}

#[test]
fn allow_always_adds_a_rule_for_the_session() {
    let i = made(
        bash("pnpm test:unit", &["pnpm", "test:unit"]),
        approve(Behavior::Allow, true, false, None, false),
    );
    let value = out(Harness::Claude, &i, true);
    let rules = &value["hookSpecificOutput"]["decision"]["updatedPermissions"][0];
    assert_eq!(rules["type"], "addRules");
    assert_eq!(rules["behavior"], "allow");
    assert_eq!(rules["destination"], "session");
    // The fields are `toolName` and `ruleContent`, as the golden recordings show.
    assert_eq!(
        rules["rules"][0],
        json!({"toolName": "Bash", "ruleContent": "pnpm test:*"})
    );
}

#[test]
fn a_persistent_rule_needs_both_the_choice_and_the_setting() {
    let i = made(
        bash("git status", &["git", "status"]),
        approve(Behavior::Allow, true, true, None, false),
    );
    let destination = |persist_allowed| {
        out(Harness::Claude, &i, persist_allowed)["hookSpecificOutput"]["decision"]
            ["updatedPermissions"][0]["destination"]
            .clone()
    };
    assert_eq!(destination(true), "localSettings");
    assert_eq!(destination(false), "session");
}

#[test]
fn a_rule_for_a_tool_without_a_command_has_no_content() {
    let payload = InteractionPayload::Approval(Box::new(ApprovalPayload {
        tool: "Edit".to_owned(),
        command: None,
        cwd: None,
        paths: vec!["a.rs".to_owned()],
        diff: None,
        input: None,
        peek: Vec::new(),
        policy_note: None,
    }));
    let i = made(payload, approve(Behavior::Allow, true, false, None, false));
    let value = out(Harness::Claude, &i, false);
    assert_eq!(
        value["hookSpecificOutput"]["decision"]["updatedPermissions"][0]["rules"][0],
        json!({"toolName": "Edit"})
    );
}

#[test]
fn codex_takes_no_rule_with_the_answer() {
    let i = made(
        bash("ls", &["ls"]),
        approve(Behavior::Allow, true, false, None, false),
    );
    assert_eq!(
        out(Harness::Codex, &i, true)["hookSpecificOutput"]["decision"],
        json!({"behavior": "allow"})
    );
}

#[test]
fn a_deny_has_a_message_and_may_stop_the_turn() {
    let i = made(
        bash("ls", &["ls"]),
        approve(Behavior::Deny, false, false, Some("not now"), false),
    );
    assert_eq!(
        out(Harness::Claude, &i, false)["hookSpecificOutput"]["decision"],
        json!({"behavior": "deny", "message": "not now"})
    );
    let i = made(
        bash("ls", &["ls"]),
        approve(Behavior::Deny, false, false, None, true),
    );
    assert_eq!(
        out(Harness::Claude, &i, false)["hookSpecificOutput"]["decision"],
        json!({"behavior": "deny", "message": "Denied from neohar", "interrupt": true})
    );
    let blank = made(
        bash("ls", &["ls"]),
        approve(Behavior::Deny, false, false, Some("  "), false),
    );
    assert_eq!(
        out(Harness::Codex, &blank, false)["hookSpecificOutput"]["decision"]["message"],
        "Denied from neohar"
    );
}

#[test]
fn a_plan_is_rejected_with_the_feedback_and_its_approval_has_no_output() {
    let plan_payload = || {
        InteractionPayload::PlanReview(PlanPayload {
            plan: nh_proto::api::rich::Markdown {
                html: String::new(),
                truncated: false,
            },
            plan_path: None,
        })
    };
    let rejected = made(
        plan_payload(),
        Decision::PlanReview(PlanDecision {
            approve: false,
            feedback: Some("use a map".to_owned()),
        }),
    );
    assert_eq!(
        out(Harness::Claude, &rejected, false)["hookSpecificOutput"]["decision"],
        json!({"behavior": "deny", "message": "use a map"})
    );
    let approved = made(
        plan_payload(),
        Decision::PlanReview(PlanDecision {
            approve: true,
            feedback: None,
        }),
    );
    assert!(hook_output(Harness::Claude, &approved, false).is_none());
}

fn questions() -> InteractionPayload {
    InteractionPayload::Question(QuestionPayload {
        questions: vec![
            Question {
                question: "Which library?".to_owned(),
                header: Some("Library".to_owned()),
                options: vec![
                    QuestionOption {
                        label: "A".to_owned(),
                        description: Some("first".to_owned()),
                    },
                    QuestionOption {
                        label: "B".to_owned(),
                        description: None,
                    },
                ],
                multi_select: false,
            },
            Question {
                question: "Which flags?".to_owned(),
                header: None,
                options: vec![
                    QuestionOption {
                        label: "x".to_owned(),
                        description: None,
                    },
                    QuestionOption {
                        label: "y".to_owned(),
                        description: None,
                    },
                ],
                multi_select: true,
            },
        ],
        context: None,
    })
}

#[test]
fn a_question_is_answered_through_the_updated_input() {
    let i = made(
        questions(),
        Decision::Question(QuestionDecision {
            answers: vec![
                QuestionAnswer {
                    question: "Which library?".to_owned(),
                    selected: vec!["B".to_owned()],
                    text: None,
                },
                QuestionAnswer {
                    question: "Which flags?".to_owned(),
                    selected: vec!["x".to_owned(), "y".to_owned()],
                    text: None,
                },
            ],
        }),
    );
    let value = out(Harness::Claude, &i, false);
    let specific = &value["hookSpecificOutput"];
    assert_eq!(specific["hookEventName"], "PreToolUse");
    assert_eq!(specific["permissionDecision"], "allow");
    // The original questions are in the input, with the answers next to them.
    let input = &specific["updatedInput"];
    assert_eq!(input["questions"][0]["question"], "Which library?");
    assert_eq!(input["questions"][0]["header"], "Library");
    assert_eq!(
        input["questions"][0]["options"][0],
        json!({"label": "A", "description": "first"})
    );
    assert_eq!(input["questions"][1]["multiSelect"], true);
    assert_eq!(
        input["answers"],
        json!({"Which library?": "B", "Which flags?": "x, y"})
    );
}

#[test]
fn free_text_is_the_answer_when_it_is_given() {
    let i = made(
        questions(),
        Decision::Question(QuestionDecision {
            answers: vec![QuestionAnswer {
                question: "Which library?".to_owned(),
                selected: Vec::new(),
                text: Some("something else".to_owned()),
            }],
        }),
    );
    assert_eq!(
        out(Harness::Claude, &i, false)["hookSpecificOutput"]["updatedInput"]["answers"],
        json!({"Which library?": "something else"})
    );
}

#[test]
fn an_elicitation_has_no_format_yet() {
    let payload = InteractionPayload::Question(QuestionPayload {
        questions: vec![Question {
            question: "Which table?".to_owned(),
            header: Some("db".to_owned()),
            options: Vec::new(),
            multi_select: false,
        }],
        context: None,
    });
    let i = made(
        payload,
        Decision::Question(QuestionDecision {
            answers: vec![QuestionAnswer {
                question: "Which table?".to_owned(),
                selected: Vec::new(),
                text: Some("users".to_owned()),
            }],
        }),
    );
    assert!(hook_output(Harness::Claude, &i, false).is_none());
}

#[test]
fn no_decision_means_no_output() {
    let (interaction, _) = Interaction::open(
        InteractionId::new(Ulid::from(2u128)),
        "i2".to_owned(),
        OpenSpec {
            kind: InteractionKind::Approval,
            run: None,
            pane: None,
            native_ref: None,
            title: "t".to_owned(),
            risk: Risk::Low,
            payload: bash("ls", &["ls"]),
            mode: GateMode::Gate,
            answerable: true,
            channels: vec![AnswerChannel::Native],
            source: Source::Hook,
            confidence: 1.0,
            deadline: None,
            facts: None,
            by_policy: false,
            keystroke: false,
        },
        Timestamp(1_000),
    );
    assert!(hook_output(Harness::Claude, &interaction, false).is_none());
}
