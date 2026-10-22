use nh_proto::api::{
    calls::agent::{ApprovalDecision, Behavior},
    model::{ApprovalPayload, InteractionPayload},
};
use ulid::Ulid;

use super::*;
use crate::interaction::model::{Holder, Lease, OpenSpec};

fn interaction() -> Interaction {
    let (mut interaction, _) = Interaction::open(
        InteractionId::new(Ulid::from(7u128)),
        "i7".to_owned(),
        OpenSpec {
            kind: InteractionKind::Approval,
            run: Some(RunId::new(Ulid::from(1u128))),
            pane: None,
            native_ref: Some("toolu_1".to_owned()),
            title: "Run `ls`".to_owned(),
            risk: Risk::Medium,
            payload: InteractionPayload::Approval(Box::new(ApprovalPayload {
                tool: "Bash".to_owned(),
                command: None,
                cwd: Some("/w".to_owned()),
                paths: vec!["a".to_owned()],
                diff: None,
                input: None,
                peek: Vec::new(),
                policy_note: None,
            })),
            mode: GateMode::Gate,
            answerable: true,
            channels: vec![AnswerChannel::Native, AnswerChannel::Keystroke],
            source: Source::Hook,
            confidence: 0.9,
            deadline: Some(Timestamp(1_801_000)),
            facts: None,
            by_policy: false,
            keystroke: false,
        },
        Timestamp(1_000),
    );
    interaction.hold(Lease {
        holder: Holder::Shim,
        conn: 3,
        pid: Some(4242),
        expires_at: Timestamp(9_000),
    });
    interaction
        .answer(
            Decision::Approval(ApprovalDecision {
                behavior: Behavior::Deny,
                always: false,
                persist: false,
                message: Some("no".to_owned()),
                stop: true,
            }),
            "ui",
            Via::Notification,
        )
        .expect("decided");
    interaction.sent(Timestamp(1_100));
    interaction
}

#[test]
fn an_interaction_comes_back_from_its_rows_without_a_lease() {
    let original = interaction();
    let (row, record) = to_rows(&original);
    let mut expected = original.clone();
    // A lease does not outlive the core.
    expected.delivery.lease = None;
    let read = from_rows(&row, Some(&record)).expect("readable rows");
    assert_eq!(read, expected);
    assert_eq!(read.delivery.attempts.len(), 1);
    assert_eq!(read.delivery.attempts[0].outcome, "sent");
}

#[test]
fn the_rows_use_the_words_of_the_schema() {
    let (row, record) = to_rows(&interaction());
    assert_eq!(row.kind, "approval");
    assert_eq!(row.risk, "medium");
    assert_eq!(row.state, "delivering");
    assert_eq!(row.gate_mode, "gate");
    assert_eq!(row.source, "hook");
    assert_eq!(record.state, "delivering");
    assert_eq!(record.via.as_deref(), Some("notification"));
    assert_eq!(record.channel.as_deref(), Some("native"));
    assert_eq!(record.rev, 1);
    assert_eq!(
        record.lease.as_ref().map(|l| l["holder"].clone()),
        Some("shim".into())
    );
    // The database accepts the words: `CHECK` constraints hold.
    let mut store = nh_store::Store::open_in_memory().expect("a store");
    store
        .tx(2_000, |tx| {
            tx.put_run(&nh_store::RunRow {
                id: Ulid::from(1u128),
                pane: None,
                harness: "claude".to_owned(),
                manifest: "claude".to_owned(),
                name: None,
                session_id: None,
                previous_session_ids: Vec::new(),
                transcript_path: None,
                binding: "bound".to_owned(),
                binding_confidence: 1.0,
                launch_mode: "tui".to_owned(),
                version: None,
                capabilities: serde_json::json!({}),
                facets: serde_json::json!({}),
                yolo: false,
                resume: None,
                model: None,
                task: None,
                process: None,
                started_at: 1,
                ended_at: None,
            })?;
            tx.put_interaction(&row)?;
            tx.put_delivery(&record)
        })
        .expect("rows");
}

#[test]
fn rows_with_words_we_do_not_know_are_not_read() {
    let (mut row, record) = to_rows(&interaction());
    row.kind = "nonsense".to_owned();
    assert!(from_rows(&row, Some(&record)).is_none());
    let (mut row, record) = to_rows(&interaction());
    row.payload = serde_json::json!({"kind": "approval", "tool": 5});
    assert!(from_rows(&row, Some(&record)).is_none());
    let (mut row, record) = to_rows(&interaction());
    row.source = "telepathy".to_owned();
    assert!(from_rows(&row, Some(&record)).is_none());
}

#[test]
fn an_interaction_without_a_delivery_record_has_a_blank_one() {
    let (row, _) = to_rows(&interaction());
    let read = from_rows(&row, None).expect("readable");
    assert_eq!(read.delivery.rev, 0);
    assert!(read.delivery.decision.is_none());
}

#[test]
fn the_words_for_enums_come_from_the_api() {
    assert_eq!(name(&Risk::High), "high");
    assert_eq!(
        name(&InteractionState::DecisionRecorded),
        "decision_recorded"
    );
    assert_eq!(name(&Via::Cli), "cli");
    assert_eq!(parse::<GateMode>("observe"), Some(GateMode::Observe));
    assert_eq!(parse::<GateMode>("nope"), None);
}
