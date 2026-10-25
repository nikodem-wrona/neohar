use nh_proto::api::model::{
    ApprovalPayload, NoticePayload, PlanPayload, Question, QuestionOption, QuestionPayload,
};
use proptest::prelude::*;
use ulid::Ulid;

use super::*;

fn id(n: u128) -> InteractionId {
    InteractionId::new(Ulid::from(n))
}

fn at(ms: i64) -> Timestamp {
    Timestamp(ms)
}

fn approval_payload() -> InteractionPayload {
    InteractionPayload::Approval(Box::new(ApprovalPayload {
        tool: "Bash".to_owned(),
        command: None,
        cwd: None,
        paths: Vec::new(),
        diff: None,
        input: None,
        peek: Vec::new(),
        policy_note: None,
    }))
}

fn question_payload() -> InteractionPayload {
    InteractionPayload::Question(QuestionPayload {
        questions: vec![
            Question {
                question: "Which?".to_owned(),
                header: None,
                options: vec![
                    QuestionOption {
                        label: "A".to_owned(),
                        description: None,
                    },
                    QuestionOption {
                        label: "B".to_owned(),
                        description: None,
                    },
                ],
                multi_select: false,
            },
            Question {
                question: "Which many?".to_owned(),
                header: None,
                options: vec![
                    QuestionOption {
                        label: "X".to_owned(),
                        description: None,
                    },
                    QuestionOption {
                        label: "Y".to_owned(),
                        description: None,
                    },
                ],
                multi_select: true,
            },
        ],
        context: None,
    })
}

fn spec(kind: InteractionKind, payload: InteractionPayload) -> OpenSpec {
    OpenSpec {
        kind,
        run: None,
        pane: None,
        native_ref: Some("toolu_1".to_owned()),
        title: "title".to_owned(),
        risk: Risk::Low,
        payload,
        mode: GateMode::Gate,
        answerable: true,
        channels: vec![AnswerChannel::Native],
        source: Source::Hook,
        confidence: 1.0,
        deadline: Some(at(1_801_000)),
        facts: None,
        by_policy: false,
        keystroke: false,
    }
}

fn open_approval() -> Interaction {
    Interaction::open(
        id(1),
        "i1".to_owned(),
        spec(InteractionKind::Approval, approval_payload()),
        at(1_000),
    )
    .0
}

fn allow() -> Decision {
    Decision::Approval(ApprovalDecision {
        behavior: Behavior::Allow,
        always: false,
        persist: false,
        message: None,
        stop: false,
    })
}

fn deny() -> Decision {
    Decision::Approval(ApprovalDecision {
        behavior: Behavior::Deny,
        always: false,
        persist: false,
        message: Some("no".to_owned()),
        stop: false,
    })
}

fn lease(conn: u64) -> Lease {
    Lease {
        holder: Holder::Shim,
        conn,
        pid: None,
        expires_at: at(2_000_000),
    }
}

fn names(step: &Step) -> Vec<&'static str> {
    step.events
        .iter()
        .map(|event| match event {
            EventPayload::InteractionOpened(_) => "opened",
            EventPayload::InteractionDecided(_) => "decided",
            EventPayload::InteractionDeliveryStarted(_) => "delivery_started",
            EventPayload::InteractionDelivered(_) => "delivered",
            EventPayload::InteractionDeliveryUnknown(_) => "delivery_unknown",
            EventPayload::InteractionDeliveryFailed(_) => "delivery_failed",
            EventPayload::InteractionResolvedElsewhere(_) => "resolved_elsewhere",
            EventPayload::InteractionDismissed(_) => "dismissed",
            EventPayload::InteractionCancelled(_) => "cancelled",
            _ => "other",
        })
        .collect()
}

#[test]
fn opening_gives_the_event_and_an_open_interaction() {
    let (interaction, step) = Interaction::open(
        id(1),
        "i1".to_owned(),
        spec(InteractionKind::Approval, approval_payload()),
        at(1_000),
    );
    assert_eq!(interaction.state, InteractionState::Open);
    assert_eq!(names(&step), ["opened"]);
    assert!(step.effects.is_empty());
    let value = interaction.value();
    assert_eq!(value.handle, "i1");
    assert!(value.answerable);
    assert_eq!(value.delivery.rev, 0);
}

#[test]
fn the_first_decision_is_recorded_and_asks_for_delivery() {
    let mut interaction = open_approval();
    let step = interaction
        .answer(allow(), "ui", Via::Card)
        .expect("recorded");
    assert_eq!(interaction.state, InteractionState::DecisionRecorded);
    assert_eq!(names(&step), ["decided"]);
    assert_eq!(step.effects, [Effect::Deliver]);
    assert_eq!(interaction.delivery.rev, 1);
    assert_eq!(
        interaction.delivery.key.as_deref(),
        Some(idempotency_key(id(1), 1).as_str())
    );
    assert_eq!(interaction.delivery.decided_by.as_deref(), Some("ui"));
    assert_eq!(interaction.delivery.channel, Some(AnswerChannel::Native));
}

#[test]
fn a_decision_can_be_replaced_until_it_is_sent_and_gets_a_new_key() {
    let mut interaction = open_approval();
    interaction.answer(allow(), "ui", Via::Card).expect("first");
    let first_key = interaction.delivery.key.clone();
    let step = interaction
        .answer(deny(), "ui", Via::Palette)
        .expect("second");
    assert_eq!(interaction.delivery.rev, 2);
    assert_ne!(interaction.delivery.key, first_key);
    assert_eq!(names(&step), ["decided"]);
    assert_eq!(interaction.delivery.via, Some(Via::Palette));
    assert_eq!(interaction.state, InteractionState::DecisionRecorded);
}

#[test]
fn another_client_cannot_replace_a_recorded_decision() {
    let mut interaction = open_approval();
    interaction.answer(allow(), "ui", Via::Card).expect("first");
    let error = interaction
        .answer(deny(), "cli", Via::Cli)
        .expect_err("late");
    assert_eq!(
        error,
        Refusal::AlreadyAnswered {
            by: Some("ui".to_owned()),
            via: Some(Via::Card),
            decision: Some(allow()),
        }
    );
    assert_eq!(interaction.delivery.rev, 1);
}

#[test]
fn once_the_decision_is_sent_a_second_answer_is_refused_with_who_decided() {
    let mut interaction = open_approval();
    interaction.answer(allow(), "ui", Via::Card).expect("first");
    interaction.hold(lease(1));
    interaction.sent(at(1_100));
    assert_eq!(interaction.state, InteractionState::Delivering);
    let error = interaction
        .answer(deny(), "cli", Via::Cli)
        .expect_err("refused");
    assert_eq!(
        error,
        Refusal::AlreadyAnswered {
            by: Some("ui".to_owned()),
            via: Some(Via::Card),
            decision: Some(allow()),
        }
    );
    assert_eq!(interaction.delivery.rev, 1, "nothing changed");
}

#[test]
fn an_interaction_that_ended_refuses_with_its_state() {
    let mut interaction = open_approval();
    interaction.cancel("process_exited", at(2_000));
    assert_eq!(
        interaction.answer(allow(), "ui", Via::Card),
        Err(Refusal::Closed(InteractionState::Cancelled))
    );
}

#[test]
fn an_interaction_that_is_not_answerable_refuses_every_decision() {
    let mut open = spec(InteractionKind::Approval, approval_payload());
    open.answerable = false;
    open.channels = Vec::new();
    let (mut interaction, _) = Interaction::open(id(1), "i1".to_owned(), open, at(1_000));
    assert_eq!(
        interaction.answer(allow(), "ui", Via::Card),
        Err(Refusal::NotAnswerable)
    );
    assert!(!interaction.value().answerable);
}

#[test]
fn the_normal_course_is_decided_sent_acked_resolved_and_delivered() {
    let mut interaction = open_approval();
    interaction.hold(lease(7));
    interaction
        .answer(allow(), "ui", Via::Card)
        .expect("decided");
    let sent = interaction.sent(at(1_100));
    assert_eq!(names(&sent), ["delivery_started"]);
    let key = interaction.delivery.key.clone().expect("a key");
    assert!(interaction.acked(&key, true, at(1_150)));
    let done = interaction.native_resolved(&Resolution::Allowed, at(1_300));
    assert_eq!(names(&done), ["delivered"]);
    assert_eq!(interaction.state, InteractionState::Delivered);
    assert_eq!(interaction.resolved_at, Some(at(1_300)));
    assert!(interaction.delivery.lease.is_none());
    let outcomes: Vec<&str> = interaction
        .delivery
        .attempts
        .iter()
        .map(|a| a.outcome)
        .collect();
    assert_eq!(outcomes, ["sent", "acked"]);
}

#[test]
fn an_ack_with_another_key_or_in_another_state_does_nothing() {
    let mut interaction = open_approval();
    assert!(!interaction.acked("k", true, at(1_000)), "not delivering");
    interaction
        .answer(allow(), "ui", Via::Card)
        .expect("decided");
    interaction.sent(at(1_100));
    assert!(!interaction.acked("another-key", true, at(1_150)));
    assert!(!interaction.delivery.acked);
}

#[test]
fn a_resolution_without_an_ack_means_someone_else_answered() {
    let mut interaction = open_approval();
    interaction
        .answer(allow(), "ui", Via::Card)
        .expect("decided");
    interaction.sent(at(1_100));
    let step = interaction.native_resolved(&Resolution::Denied, at(1_200));
    assert_eq!(names(&step), ["resolved_elsewhere"]);
    assert_eq!(interaction.state, InteractionState::ResolvedElsewhere);
}

#[test]
fn a_resolution_before_any_decision_is_resolved_elsewhere_and_releases_the_shim() {
    let mut interaction = open_approval();
    interaction.hold(lease(7));
    let step = interaction.native_resolved(&Resolution::Allowed, at(1_200));
    assert_eq!(names(&step), ["resolved_elsewhere"]);
    assert_eq!(
        step.effects,
        [Effect::Release {
            conn: 7,
            reason: "resolved_elsewhere"
        }]
    );
    assert_eq!(interaction.state, InteractionState::ResolvedElsewhere);
}

#[test]
fn a_lost_connection_before_the_ack_makes_the_delivery_unknown() {
    let mut interaction = open_approval();
    interaction.hold(lease(7));
    interaction
        .answer(allow(), "ui", Via::Card)
        .expect("decided");
    interaction.sent(at(1_100));
    let step = interaction.lease_lost(at(1_200));
    assert_eq!(names(&step), ["delivery_unknown"]);
    assert_eq!(interaction.state, InteractionState::DeliveryUnknown);
    assert!(!interaction.is_final());
}

#[test]
fn a_lost_connection_after_the_ack_waits_for_the_resolution() {
    let mut interaction = open_approval();
    interaction.hold(lease(7));
    interaction
        .answer(allow(), "ui", Via::Card)
        .expect("decided");
    interaction.sent(at(1_100));
    let key = interaction.delivery.key.clone().expect("a key");
    interaction.acked(&key, true, at(1_150));
    let step = interaction.lease_lost(at(1_200));
    assert!(step.is_empty());
    assert_eq!(interaction.state, InteractionState::Delivering);
    let done = interaction.native_resolved(&Resolution::Allowed, at(1_300));
    assert_eq!(names(&done), ["delivered"]);
}

#[test]
fn a_lost_connection_of_an_open_interaction_leaves_it_open_for_a_resume() {
    let mut interaction = open_approval();
    interaction.hold(lease(7));
    let step = interaction.lease_lost(at(1_200));
    assert!(step.is_empty());
    assert_eq!(interaction.state, InteractionState::Open);
    assert!(interaction.delivery.lease.is_none());
}

#[test]
fn a_resume_after_an_unknown_delivery_sends_the_same_decision_with_the_same_key() {
    let mut interaction = open_approval();
    interaction.hold(lease(7));
    interaction
        .answer(allow(), "ui", Via::Card)
        .expect("decided");
    interaction.sent(at(1_100));
    let key = interaction.delivery.key.clone();
    interaction.lease_lost(at(1_200));
    let step = interaction.resumed(lease(8), at(1_300));
    assert_eq!(step.effects, [Effect::Deliver]);
    assert_eq!(interaction.delivery.key, key);
    assert_eq!(interaction.delivery.rev, 1);
    let sent = interaction.sent(at(1_310));
    assert_eq!(names(&sent), ["delivery_started"]);
    assert_eq!(interaction.state, InteractionState::Delivering);
}

#[test]
fn a_resume_of_an_interaction_without_a_decision_only_holds_the_lease() {
    let mut interaction = open_approval();
    let step = interaction.resumed(lease(9), at(1_300));
    assert!(step.is_empty());
    assert_eq!(interaction.delivery.lease.map(|l| l.conn), Some(9));
}

#[test]
fn an_unknown_delivery_is_settled_by_a_consistent_resolution() {
    let settle = |resolution: Resolution| {
        let mut interaction = open_approval();
        interaction.hold(lease(7));
        interaction
            .answer(allow(), "ui", Via::Card)
            .expect("decided");
        interaction.sent(at(1_100));
        interaction.lease_lost(at(1_200));
        let step = interaction.native_resolved(&resolution, at(1_300));
        (interaction.state, names(&step))
    };
    assert_eq!(
        settle(Resolution::Allowed),
        (InteractionState::Delivered, vec!["delivered"])
    );
    assert_eq!(
        settle(Resolution::Denied),
        (
            InteractionState::ResolvedElsewhere,
            vec!["resolved_elsewhere"]
        )
    );
    assert_eq!(
        settle(Resolution::Unknown),
        (InteractionState::DeliveryUnknown, vec![])
    );
}

#[test]
fn the_deadline_fails_an_open_interaction_and_releases_the_shim() {
    let mut interaction = open_approval();
    interaction.hold(lease(7));
    let step = interaction.deadline_passed(at(1_801_000));
    assert_eq!(names(&step), ["delivery_failed"]);
    assert_eq!(
        step.effects,
        [Effect::Release {
            conn: 7,
            reason: "deadline"
        }]
    );
    assert_eq!(interaction.state, InteractionState::Failed);
    assert_eq!(
        interaction.delivery.fail_reason.as_deref(),
        Some("deadline")
    );
}

#[test]
fn the_deadline_in_the_middle_of_a_delivery_makes_it_unknown() {
    let mut interaction = open_approval();
    interaction.hold(lease(7));
    interaction
        .answer(allow(), "ui", Via::Card)
        .expect("decided");
    interaction.sent(at(1_100));
    let step = interaction.deadline_passed(at(1_801_000));
    assert_eq!(names(&step), ["delivery_unknown"]);
    assert_eq!(interaction.state, InteractionState::DeliveryUnknown);
}

#[test]
fn a_cancel_ends_the_interaction_once() {
    let mut interaction = open_approval();
    interaction.hold(lease(7));
    let step = interaction.cancel("process_exited", at(1_500));
    assert_eq!(names(&step), ["cancelled"]);
    assert_eq!(
        step.effects,
        [Effect::Release {
            conn: 7,
            reason: "cancelled"
        }]
    );
    assert!(interaction.cancel("again", at(1_600)).is_empty());
    assert_eq!(interaction.resolved_at, Some(at(1_500)));
}

#[test]
fn dismissal_is_for_an_interaction_nobody_answered() {
    let mut interaction = open_approval();
    interaction.hold(lease(7));
    let step = interaction.dismiss(at(1_500)).expect("dismissed");
    assert_eq!(names(&step), ["dismissed"]);
    assert_eq!(
        step.effects,
        [Effect::Release {
            conn: 7,
            reason: "dismissed"
        }]
    );
    assert_eq!(interaction.state, InteractionState::Dismissed);
    assert_eq!(
        interaction.dismiss(at(1_600)),
        Err(Refusal::Closed(InteractionState::Dismissed))
    );
    let mut decided = open_approval();
    decided.answer(allow(), "ui", Via::Card).expect("decided");
    assert!(matches!(
        decided.dismiss(at(1_500)),
        Err(Refusal::AlreadyAnswered { .. })
    ));
}

#[test]
fn a_release_to_observe_drops_the_native_channel_and_the_deadline() {
    let mut interaction = open_approval();
    interaction.hold(lease(7));
    let step = interaction.release_to_observe(at(1_500));
    assert_eq!(
        step.effects,
        [Effect::Release {
            conn: 7,
            reason: "focused"
        }]
    );
    assert_eq!(interaction.mode, GateMode::Observe);
    assert_eq!(interaction.state, InteractionState::Open);
    assert!(interaction.channels.is_empty());
    assert!(!interaction.value().answerable);
    assert_eq!(interaction.deadline, None);
    assert_eq!(
        interaction.answer(allow(), "ui", Via::Card),
        Err(Refusal::NotAnswerable)
    );
    // A second release has nothing to do.
    assert!(interaction.release_to_observe(at(1_600)).is_empty());
}

/// The facts of an approval that a rule may cover.
fn eligible_facts() -> Facts {
    Facts {
        fingerprint: "0123456789abcdef".to_owned(),
        harness: nh_proto::api::common::Harness::Claude,
        tool: "Bash".to_owned(),
        pattern: Some("pnpm test:*".to_owned()),
        workspace: "/w".to_owned(),
        eligible: true,
    }
}

#[test]
fn allow_always_needs_a_rule_that_covers_only_the_request() {
    let always = Decision::Approval(ApprovalDecision {
        behavior: Behavior::Allow,
        always: true,
        persist: false,
        message: None,
        stop: false,
    });
    let try_always = |facts: Option<Facts>, channels: Vec<AnswerChannel>| {
        let mut interaction = open_approval();
        interaction.facts = facts;
        interaction.channels = channels;
        let rule = interaction.always_rule().map(str::to_owned);
        let shown = interaction.value().always_rule;
        assert_eq!(rule, shown, "the API shows the rule that the core takes");
        (rule, interaction.answer(always.clone(), "ui", Via::Card))
    };
    // No facts, or facts that no rule may cover.
    let (rule, refused) = try_always(None, vec![AnswerChannel::Native]);
    assert_eq!(rule, None);
    assert!(matches!(refused, Err(Refusal::Invalid(_))));
    let mut wide = eligible_facts();
    wide.eligible = false;
    wide.pattern = None;
    let (_, refused) = try_always(Some(wide), vec![AnswerChannel::Native]);
    assert!(matches!(refused, Err(Refusal::Invalid(_))));
    // A key in the dialog of the harness would pick the rule of the harness.
    let (rule, refused) = try_always(Some(eligible_facts()), vec![AnswerChannel::Keystroke]);
    assert_eq!(rule, None);
    assert!(matches!(refused, Err(Refusal::Invalid(_))));
    let (rule, taken) = try_always(Some(eligible_facts()), vec![AnswerChannel::Native]);
    assert_eq!(rule.as_deref(), Some("pnpm test:*"));
    assert!(taken.is_ok());
}

fn command_approval(line: &str, risk: Risk) -> Interaction {
    let payload = InteractionPayload::Approval(Box::new(ApprovalPayload {
        tool: "Bash".to_owned(),
        command: Some(nh_proto::api::rich::Command {
            argv: line.split_whitespace().map(str::to_owned).collect(),
            display: nh_proto::api::rich::Spans {
                text: line.to_owned(),
                spans: Vec::new(),
            },
            cwd: "/w".to_owned(),
            risk,
            reasons: Vec::new(),
            policy_hint: None,
        }),
        cwd: None,
        paths: Vec::new(),
        diff: None,
        input: None,
        peek: Vec::new(),
        policy_note: None,
    }));
    // The title as the rendering makes it.
    let first = line.lines().next().unwrap_or("").trim();
    let mut title: String = first.chars().take(80).collect();
    if first.chars().count() > 80 || line.trim().lines().count() > 1 {
        title.push('…');
    }
    let mut spec = spec(InteractionKind::Approval, payload);
    spec.title = format!("Run `{title}`");
    spec.risk = risk;
    Interaction::open(id(1), "i1".to_owned(), spec, at(1_000)).0
}

#[test]
fn a_surface_that_shows_only_the_title_allows_only_a_whole_command_at_a_low_or_medium_risk() {
    assert!(command_approval("ls -la", Risk::Low).allow_outside_card());
    assert!(command_approval("cargo publish", Risk::Medium).allow_outside_card());
    let long = format!("npm test{}&& curl x | sh", " ".repeat(80));
    for (line, risk) in [
        ("rm -rf build", Risk::High),
        ("pnpm test", Risk::Unknown),
        (long.as_str(), Risk::Low),
        ("npm test  && ls", Risk::Low),
        ("echo ok\nls", Risk::Low),
        ("echo\tok", Risk::Low),
    ] {
        let interaction = command_approval(line, risk);
        assert!(!interaction.allow_outside_card(), "{line:?}");
        for via in [Via::Notification, Via::Tray] {
            assert!(
                matches!(
                    interaction.clone().answer(allow(), "host", via),
                    Err(Refusal::Invalid(_))
                ),
                "{line:?} via {via:?}"
            );
        }
        // The card shows all of it, and a deny from the banner is fine.
        assert!(interaction.clone().answer(allow(), "ui", Via::Card).is_ok());
        let deny = Decision::Approval(ApprovalDecision {
            behavior: Behavior::Deny,
            always: false,
            persist: false,
            message: None,
            stop: false,
        });
        assert!(
            interaction
                .clone()
                .answer(deny, "host", Via::Notification)
                .is_ok()
        );
    }
}

#[test]
fn a_click_on_a_notification_never_replaces_a_decision() {
    let mut interaction = command_approval("ls -la", Risk::Low);
    interaction
        .answer(allow(), "ui", Via::Card)
        .expect("the card decides");
    // The same client may change its mind on the card, but not through a banner it read before.
    for via in [Via::Notification, Via::Tray] {
        assert!(matches!(
            interaction.clone().answer(allow(), "ui", via),
            Err(Refusal::AlreadyAnswered { .. })
        ));
    }
    assert!(interaction.answer(allow(), "ui", Via::Card).is_ok());
}

#[test]
fn approval_decisions_are_checked() {
    let bad = |d: ApprovalDecision| {
        let mut interaction = open_approval();
        interaction.facts = Some(eligible_facts());
        interaction.answer(Decision::Approval(d), "ui", Via::Card)
    };
    let base = ApprovalDecision {
        behavior: Behavior::Allow,
        always: false,
        persist: false,
        message: None,
        stop: false,
    };
    assert!(matches!(
        bad(ApprovalDecision {
            behavior: Behavior::Deny,
            always: true,
            ..base.clone()
        }),
        Err(Refusal::Invalid(_))
    ));
    assert!(matches!(
        bad(ApprovalDecision {
            persist: true,
            ..base.clone()
        }),
        Err(Refusal::Invalid(_))
    ));
    assert!(matches!(
        bad(ApprovalDecision {
            stop: true,
            ..base.clone()
        }),
        Err(Refusal::Invalid(_))
    ));
    assert!(matches!(
        bad(ApprovalDecision {
            message: Some("x".to_owned()),
            ..base.clone()
        }),
        Err(Refusal::Invalid(_))
    ));
    assert!(
        bad(ApprovalDecision {
            always: true,
            ..base.clone()
        })
        .is_ok()
    );
    assert!(
        bad(ApprovalDecision {
            always: true,
            persist: true,
            ..base
        })
        .is_ok()
    );
}

#[test]
fn a_decision_of_the_wrong_kind_is_refused() {
    let mut interaction = open_approval();
    let wrong = Decision::PlanReview(PlanDecision {
        approve: false,
        feedback: Some("no".to_owned()),
    });
    assert!(matches!(
        interaction.answer(wrong, "ui", Via::Card),
        Err(Refusal::Invalid(_))
    ));
    let (mut notice, _) = Interaction::open(
        id(2),
        "i2".to_owned(),
        spec(
            InteractionKind::Notice,
            InteractionPayload::Notice(NoticePayload {
                text: "limit".to_owned(),
                resets_at: None,
            }),
        ),
        at(1_000),
    );
    assert!(matches!(
        notice.answer(allow(), "ui", Via::Card),
        Err(Refusal::Invalid(_))
    ));
    assert!(notice.dismiss(at(1_100)).is_ok());
}

#[test]
fn question_answers_are_checked_against_the_options() {
    let answer = |answers: Vec<nh_proto::api::calls::agent::QuestionAnswer>| {
        let (mut interaction, _) = Interaction::open(
            id(3),
            "i3".to_owned(),
            spec(InteractionKind::Question, question_payload()),
            at(1_000),
        );
        interaction.answer(
            Decision::Question(QuestionDecision { answers }),
            "ui",
            Via::Card,
        )
    };
    use nh_proto::api::calls::agent::QuestionAnswer as A;
    let a = |question: &str, selected: &[&str], text: Option<&str>| A {
        question: question.to_owned(),
        selected: selected.iter().map(|s| (*s).to_owned()).collect(),
        text: text.map(str::to_owned),
    };
    assert!(answer(vec![a("Which?", &["A"], None)]).is_ok());
    assert!(answer(vec![a("Which many?", &["X", "Y"], None)]).is_ok());
    assert!(answer(vec![a("Which?", &[], Some("something else"))]).is_ok());
    assert!(matches!(answer(vec![]), Err(Refusal::Invalid(_))));
    assert!(matches!(
        answer(vec![a("Nope?", &["A"], None)]),
        Err(Refusal::Invalid(_))
    ));
    assert!(matches!(
        answer(vec![a("Which?", &["C"], None)]),
        Err(Refusal::Invalid(_))
    ));
    assert!(matches!(
        answer(vec![a("Which?", &["A", "B"], None)]),
        Err(Refusal::Invalid(_))
    ));
    assert!(matches!(
        answer(vec![a("Which?", &[], None)]),
        Err(Refusal::Invalid(_))
    ));
    assert!(matches!(
        answer(vec![a("Which?", &[], Some(""))]),
        Err(Refusal::Invalid(_))
    ));
}

#[test]
fn a_plan_is_approved_only_with_keystrokes_and_rejected_with_feedback() {
    let plan = |channels: Vec<AnswerChannel>| {
        let mut open = spec(
            InteractionKind::PlanReview,
            InteractionPayload::PlanReview(PlanPayload {
                plan: nh_proto::api::rich::Markdown {
                    html: String::new(),
                    truncated: false,
                },
                plan_path: None,
            }),
        );
        open.channels = channels;
        Interaction::open(id(4), "i4".to_owned(), open, at(1_000)).0
    };
    let approve = Decision::PlanReview(PlanDecision {
        approve: true,
        feedback: None,
    });
    let reject = |feedback: Option<&str>| {
        Decision::PlanReview(PlanDecision {
            approve: false,
            feedback: feedback.map(str::to_owned),
        })
    };
    let mut native = plan(vec![AnswerChannel::Native]);
    assert!(matches!(
        native.answer(approve.clone(), "ui", Via::Card),
        Err(Refusal::Invalid(_))
    ));
    assert!(matches!(
        native.answer(reject(None), "ui", Via::Card),
        Err(Refusal::Invalid(_))
    ));
    assert!(
        native
            .answer(reject(Some("use a map")), "ui", Via::Card)
            .is_ok()
    );
    let mut both = plan(vec![AnswerChannel::Native, AnswerChannel::Keystroke]);
    assert!(both.answer(approve, "ui", Via::Card).is_ok());
}

#[test]
fn only_keystrokes_means_the_keystroke_channel() {
    let mut open = spec(InteractionKind::Approval, approval_payload());
    open.channels = vec![AnswerChannel::Keystroke];
    open.mode = GateMode::Observe;
    let (mut interaction, _) = Interaction::open(id(5), "i5".to_owned(), open, at(1_000));
    interaction
        .answer(allow(), "ui", Via::Card)
        .expect("decided");
    assert_eq!(interaction.delivery.channel, Some(AnswerChannel::Keystroke));
}

// --- the property test ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
enum Op {
    Answer(bool),
    Hold(u64),
    Sent,
    Ack,
    AckWrongKey,
    LeaseLost,
    Resume(u64),
    Resolved(u8),
    Deadline,
    Fail,
    Cancel,
    Dismiss,
    Release,
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        3 => any::<bool>().prop_map(Op::Answer),
        2 => (1u64..4).prop_map(Op::Hold),
        3 => Just(Op::Sent),
        3 => Just(Op::Ack),
        1 => Just(Op::AckWrongKey),
        2 => Just(Op::LeaseLost),
        2 => (1u64..4).prop_map(Op::Resume),
        3 => (0u8..4).prop_map(Op::Resolved),
        1 => Just(Op::Deadline),
        1 => Just(Op::Fail),
        1 => Just(Op::Cancel),
        1 => Just(Op::Dismiss),
        1 => Just(Op::Release),
    ]
}

fn resolution(n: u8) -> Resolution {
    match n {
        0 => Resolution::Allowed,
        1 => Resolution::Denied,
        2 => Resolution::Answered,
        _ => Resolution::Unknown,
    }
}

/// Plays the operations and checks the rules of the state machine after each step.
fn play(ops: &[Op]) {
    let mut interaction = open_approval();
    let mut events: Vec<&'static str> = vec!["opened"];
    let mut now = 2_000;
    let mut delivered = 0;
    for op in ops {
        now += 10;
        let before = interaction.state;
        let rev_before = interaction.delivery.rev;
        let step = match op {
            Op::Answer(allowed) => interaction
                .answer(if *allowed { allow() } else { deny() }, "ui", Via::Card)
                .unwrap_or_default(),
            Op::Hold(conn) => {
                if !interaction.is_final() {
                    interaction.hold(lease(*conn));
                }
                Step::default()
            }
            Op::Sent => interaction.sent(at(now)),
            Op::Ack => {
                let key = interaction.delivery.key.clone().unwrap_or_default();
                interaction.acked(&key, true, at(now));
                Step::default()
            }
            Op::AckWrongKey => {
                interaction.acked("wrong", true, at(now));
                Step::default()
            }
            Op::LeaseLost => interaction.lease_lost(at(now)),
            Op::Resume(conn) => interaction.resumed(lease(*conn), at(now)),
            Op::Resolved(n) => interaction.native_resolved(&resolution(*n), at(now)),
            Op::Deadline => interaction.deadline_passed(at(now)),
            Op::Fail => interaction.failed("rejected", at(now)),
            Op::Cancel => interaction.cancel("process_exited", at(now)),
            Op::Dismiss => interaction.dismiss(at(now)).unwrap_or_default(),
            Op::Release => interaction.release_to_observe(at(now)),
        };
        let after = interaction.state;
        let kinds = names(&step);
        events.extend(&kinds);
        // No transition out of a final state.
        if is_final(before) {
            assert_eq!(before, after, "{op:?} left {before:?}");
            assert!(step.events.is_empty(), "{op:?} made events in {before:?}");
        }
        // A step that ends the interaction has the event of its ending, once.
        if !is_final(before) && is_final(after) {
            let ending = match after {
                InteractionState::Delivered => "delivered",
                InteractionState::Failed => "delivery_failed",
                InteractionState::ResolvedElsewhere => "resolved_elsewhere",
                InteractionState::Dismissed => "dismissed",
                InteractionState::Cancelled => "cancelled",
                other => panic!("final state {other:?}"),
            };
            assert_eq!(kinds, [ending], "{op:?}: {before:?} -> {after:?}");
            assert!(interaction.resolved_at.is_some());
        }
        if after == InteractionState::Delivered {
            delivered += usize::from(before != after);
        }
        // The revision only grows, one at a time, and only with a decision.
        assert!(interaction.delivery.rev >= rev_before);
        assert!(interaction.delivery.rev <= rev_before + 1);
        if interaction.delivery.rev != rev_before {
            assert_eq!(kinds, ["decided"]);
        }
        // A decision exists exactly when the revision is above 0, and the key follows the revision.
        assert_eq!(
            interaction.delivery.decision.is_some(),
            interaction.delivery.rev > 0
        );
        if interaction.delivery.rev > 0 {
            assert_eq!(
                interaction.delivery.key,
                Some(idempotency_key(interaction.id, interaction.delivery.rev))
            );
        }
        // The states that need a decision have one.
        if matches!(
            after,
            InteractionState::DecisionRecorded
                | InteractionState::Delivering
                | InteractionState::DeliveryUnknown
        ) {
            assert!(
                interaction.delivery.decision.is_some(),
                "{after:?} without a decision"
            );
        }
        // A final interaction holds no lease.
        if is_final(after) {
            assert!(interaction.delivery.lease.is_none());
        }
        // A delivery is `delivered` straight from `delivering` only after the ack.
        if before == InteractionState::Delivering && after == InteractionState::Delivered {
            assert!(interaction.delivery.acked);
        }
    }
    // One delivered decision at most, and the events of the life are well formed.
    assert!(delivered <= 1);
    let delivered_events = events.iter().filter(|e| **e == "delivered").count();
    assert!(delivered_events <= 1, "{events:?}");
    let endings = events
        .iter()
        .filter(|e| {
            matches!(
                **e,
                "delivered" | "delivery_failed" | "resolved_elsewhere" | "dismissed" | "cancelled"
            )
        })
        .count();
    assert!(endings <= 1, "{events:?}");
    assert_eq!(events[0], "opened");
    assert_eq!(events.iter().filter(|e| **e == "opened").count(), 1);
    assert_eq!(
        endings == 1,
        interaction.is_final(),
        "{events:?} {:?}",
        interaction.state
    );
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2000))]

    #[test]
    fn no_order_of_events_breaks_the_rules_of_delivery(ops in prop::collection::vec(op(), 0..40)) {
        play(&ops);
    }
}
