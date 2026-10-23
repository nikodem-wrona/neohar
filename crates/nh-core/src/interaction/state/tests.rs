use nh_proto::api::{
    ids::Handle,
    model::{
        AnswerChannel, ApprovalPayload, GateMode, InteractionKind, InteractionPayload,
        InteractionState,
    },
};
use ulid::Ulid;

use super::*;
use crate::interaction::model::{OpenSpec, Source};

fn id(n: u128) -> InteractionId {
    InteractionId::new(Ulid::from(n))
}

fn run(n: u128) -> RunId {
    RunId::new(Ulid::from(n))
}

fn make(n: u128, run_n: u128, risk: Risk, opened: i64) -> Interaction {
    Interaction::open(
        id(n),
        format!("i{n}"),
        OpenSpec {
            kind: InteractionKind::Approval,
            run: Some(run(run_n)),
            pane: None,
            native_ref: Some(format!("ref{n}")),
            title: format!("t{n}"),
            risk,
            payload: InteractionPayload::Approval(Box::new(ApprovalPayload {
                tool: "Bash".to_owned(),
                command: None,
                cwd: None,
                paths: Vec::new(),
                diff: None,
                input: None,
                peek: Vec::new(),
                policy_note: None,
            })),
            mode: GateMode::Gate,
            answerable: true,
            channels: vec![AnswerChannel::Native],
            source: Source::Hook,
            confidence: 1.0,
            deadline: Some(Timestamp(opened + 1_800_000)),
            facts: None,
            by_policy: false,
            keystroke: false,
        },
        Timestamp(opened),
    )
    .0
}

/// A row for a run, because the interactions point to it.
fn run_row(n: u128) -> nh_store::RunRow {
    nh_store::RunRow {
        id: Ulid::from(n),
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
        process: None,
        started_at: 1,
        ended_at: None,
    }
}

fn handle(number: u32) -> Ref<InteractionId> {
    Ref(RefValue::Handle(Handle {
        machine: None,
        target: Target::Interaction(number),
    }))
}

#[test]
fn a_reference_is_an_id_or_a_handle() {
    let mut table = Interactions::new();
    table.put(make(5, 1, Risk::Low, 1_000));
    assert_eq!(table.resolve(&Ref::id(id(5))), Some(id(5)));
    assert_eq!(table.resolve(&Ref::id(id(6))), None);
    assert_eq!(table.resolve(&handle(5)), Some(id(5)));
    assert_eq!(table.resolve(&handle(6)), None);
    let remote = Ref(RefValue::Handle(Handle {
        machine: Some("box".to_owned()),
        target: Target::Interaction(5),
    }));
    assert_eq!(table.resolve(&remote), None);
    let wrong_kind = Ref(RefValue::Handle(Handle {
        machine: None,
        target: Target::Preview(5),
    }));
    assert_eq!(table.resolve(&wrong_kind), None);
}

#[test]
fn the_open_interactions_come_in_the_order_of_urgency() {
    let mut table = Interactions::new();
    table.put(make(1, 1, Risk::Low, 1_000));
    table.put(make(2, 1, Risk::High, 3_000));
    table.put(make(3, 1, Risk::High, 2_000));
    table.put(make(4, 1, Risk::Medium, 500));
    table.put(make(5, 1, Risk::Unknown, 600));
    let order: Vec<u128> = table.open().iter().map(|i| i.id.ulid().0).collect();
    // High first (the older first), then unknown, medium, low.
    assert_eq!(order, [3, 2, 5, 4, 1]);
}

#[test]
fn next_goes_through_the_answerable_interactions_and_wraps() {
    let mut table = Interactions::new();
    table.put(make(1, 1, Risk::High, 1_000));
    table.put(make(2, 1, Risk::Medium, 2_000));
    let mut observed = make(3, 1, Risk::High, 500);
    observed.answerable = false;
    table.put(observed);
    assert_eq!(table.next(None).map(|i| i.id), Some(id(1)));
    assert_eq!(table.next(Some(id(1))).map(|i| i.id), Some(id(2)));
    assert_eq!(table.next(Some(id(2))).map(|i| i.id), Some(id(1)), "wraps");
    assert_eq!(
        table.next(Some(id(9))).map(|i| i.id),
        Some(id(1)),
        "unknown start"
    );
    assert_eq!(Interactions::new().next(None).map(|i| i.id), None);
}

#[test]
fn an_interaction_with_a_decision_is_not_next() {
    let mut table = Interactions::new();
    let mut decided = make(1, 1, Risk::High, 1_000);
    decided.state = InteractionState::DecisionRecorded;
    table.put(decided);
    assert!(table.next(None).is_none());
}

#[test]
fn an_open_interaction_of_a_run_is_found_by_its_native_ref() {
    let mut table = Interactions::new();
    table.put(make(1, 1, Risk::Low, 1_000));
    assert_eq!(table.open_by_native(run(1), "ref1"), Some(id(1)));
    assert_eq!(table.open_by_native(run(2), "ref1"), None);
    assert_eq!(table.open_by_native(run(1), "other"), None);
    let mut ended = make(1, 1, Risk::Low, 1_000);
    ended.cancel("x", Timestamp(2_000));
    table.put(ended);
    assert_eq!(table.open_by_native(run(1), "ref1"), None, "it ended");
    assert!(table.open_of_run(run(1)).is_empty());
}

#[test]
fn the_oldest_interactions_that_ended_leave_the_table() {
    let mut table = Interactions::new();
    let mut gone = Vec::new();
    for n in 1..=u128::try_from(KEEP_FINAL + 3).expect("small") {
        let mut interaction = make(n, 1, Risk::Low, 1_000);
        interaction.cancel("x", Timestamp(2_000));
        gone.extend(table.put(interaction));
    }
    assert_eq!(gone, [id(1), id(2), id(3)]);
    assert_eq!(table.len(), KEEP_FINAL);
    assert!(table.get(id(1)).is_none());
    assert!(table.get(id(4)).is_some());
    // The newest first.
    assert_eq!(
        table.ended().first().map(|i| i.id),
        Some(id(u128::try_from(KEEP_FINAL + 3).expect("small")))
    );
    // Putting a final interaction again does not count it twice.
    let again = table.get(id(10)).expect("kept").clone();
    assert!(table.put(again).is_empty());
    assert_eq!(table.len(), KEEP_FINAL);
}

#[test]
fn a_load_ends_what_cannot_go_on() {
    let mut store = nh_store::Store::open_in_memory().expect("a store");
    // Three open interactions: of a live run, of a run that is gone, and one past its deadline.
    let live = make(1, 1, Risk::Low, 1_000);
    let orphan = make(2, 2, Risk::Low, 1_000);
    let mut late = make(3, 1, Risk::Low, 1_000);
    late.deadline = Some(Timestamp(5_000));
    store
        .tx(1_000, |tx| {
            tx.put_run(&run_row(1))?;
            tx.put_run(&run_row(2))?;
            for interaction in [&live, &orphan, &late] {
                rows::write(tx, interaction, &[])?;
            }
            Ok(())
        })
        .expect("rows");
    let table = Interactions::load(&mut store, Timestamp(10_000), &|r| r == run(1)).expect("load");
    assert_eq!(
        table.get(id(1)).map(|i| i.state),
        Some(InteractionState::Open)
    );
    assert_eq!(
        table.get(id(2)).map(|i| i.state),
        Some(InteractionState::Cancelled)
    );
    assert_eq!(
        table.get(id(3)).map(|i| i.state),
        Some(InteractionState::Failed)
    );
    // The store learned it, with the events.
    let (row, _) = store
        .interaction_row(id(2).ulid())
        .expect("read")
        .expect("row");
    assert_eq!(row.state, "cancelled");
    let open = store.open_interactions().expect("read");
    assert_eq!(open.len(), 1);
    let start = store.cursor_now().expect("a cursor");
    let from_zero = nh_proto::api::events::Cursor { seq: 0, ..start };
    let nh_store::EventsRead::Events { events, .. } = store
        .events_since(
            &from_zero,
            &nh_proto::api::events::EventFilter::default(),
            100,
        )
        .expect("events")
    else {
        panic!("events");
    };
    let kinds: Vec<String> = events.iter().map(|e| e.kind.clone()).collect();
    assert!(
        kinds.contains(&"interaction.cancelled".to_owned()),
        "{kinds:?}"
    );
    assert!(
        kinds.contains(&"interaction.delivery_failed".to_owned()),
        "{kinds:?}"
    );
}

#[test]
fn a_row_that_cannot_be_read_ends_at_the_load() {
    let mut store = nh_store::Store::open_in_memory().expect("a store");
    let interaction = make(1, 1, Risk::Low, 1_000);
    let (mut row, record) = rows::to_rows(&interaction);
    row.source = "telepathy".to_owned();
    store
        .tx(1_000, |tx| {
            tx.put_run(&run_row(1))?;
            tx.put_interaction(&row)?;
            tx.put_delivery(&record)
        })
        .expect("rows");
    let table = Interactions::load(&mut store, Timestamp(2_000), &|_| true).expect("load");
    assert_eq!(table.len(), 0);
    assert!(store.open_interactions().expect("read").is_empty());
}
