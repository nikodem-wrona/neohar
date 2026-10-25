use serde_json::json;

use super::*;

fn id(n: u128) -> Ulid {
    Ulid::from(n)
}

fn interaction(n: u128) -> InteractionRow {
    InteractionRow {
        id: id(n),
        handle: format!("i{n}"),
        run: None,
        pane: None,
        kind: "approval".to_owned(),
        native_ref: Some("toolu_1".to_owned()),
        title: "Run `pnpm test`".to_owned(),
        risk: "low".to_owned(),
        payload: json!({"kind": "approval", "tool": "Bash"}),
        state: "open".to_owned(),
        answerable: true,
        gate_mode: "gate".to_owned(),
        source: "hook".to_owned(),
        confidence: 1.0,
        opened_at: 1_000,
        deadline: Some(1_801_000),
        resolved_at: None,
        resolution: None,
        fingerprint: None,
    }
}

fn delivery(n: u128) -> DeliveryRow {
    DeliveryRow {
        interaction: id(n),
        native_ref: Some("toolu_1".to_owned()),
        rev: 0,
        decision: None,
        decided_by: None,
        via: None,
        key: None,
        channel: None,
        lease: None,
        deadline: Some(1_801_000),
        state: "open".to_owned(),
        attempts: json!([]),
    }
}

#[test]
fn an_interaction_and_its_delivery_come_back_as_they_were_written() {
    let mut store = Store::open_in_memory().expect("a store");
    let mut row = interaction(1);
    row.confidence = 0.75;
    row.fingerprint = Some("abc".to_owned());
    let mut record = delivery(1);
    record.rev = 2;
    record.decision = Some(json!({"kind": "approval", "behavior": "allow"}));
    record.decided_by = Some("ui".to_owned());
    record.via = Some("card".to_owned());
    record.key = Some("k".to_owned());
    record.channel = Some("native".to_owned());
    record.lease = Some(json!({"holder": "shim", "conn": 7, "expires_at": 2_000}));
    record.state = "delivering".to_owned();
    record.attempts = json!([{"at": 1_500, "channel": "native", "via": "card", "outcome": "sent"}]);
    store
        .tx(2_000, |tx| {
            tx.put_interaction(&row)?;
            tx.put_delivery(&record)
        })
        .expect("rows");
    assert_eq!(
        store.interaction_row(id(1)).expect("a read"),
        Some((row, Some(record)))
    );
    assert_eq!(store.interaction_row(id(9)).expect("a read"), None);
}

#[test]
fn writing_an_interaction_again_changes_it_and_keeps_the_time_of_opening() {
    let mut store = Store::open_in_memory().expect("a store");
    store
        .tx(2_000, |tx| tx.put_interaction(&interaction(1)))
        .expect("a row");
    let mut changed = interaction(1);
    changed.state = "delivered".to_owned();
    changed.resolved_at = Some(5_000);
    changed.resolution = Some(json!({"how": "delivered"}));
    changed.opened_at = 9_999;
    changed.handle = "i999".to_owned();
    store
        .tx(3_000, |tx| tx.put_interaction(&changed))
        .expect("a row");
    let (read, _) = store
        .interaction_row(id(1))
        .expect("a read")
        .expect("a row");
    assert_eq!(read.state, "delivered");
    assert_eq!(read.resolved_at, Some(5_000));
    assert_eq!(read.opened_at, 1_000, "the time of opening does not change");
    assert_eq!(read.handle, "i1", "the handle does not change");
}

#[test]
fn a_start_reads_the_interactions_that_are_not_final() {
    let mut store = Store::open_in_memory().expect("a store");
    for (n, state) in [
        (1, "open"),
        (2, "decision_recorded"),
        (3, "delivering"),
        (4, "delivery_unknown"),
        (5, "delivered"),
        (6, "failed"),
        (7, "resolved_elsewhere"),
        (8, "dismissed"),
        (9, "cancelled"),
        (10, "superseded"),
    ] {
        let mut row = interaction(n);
        row.state = state.to_owned();
        row.opened_at = i64::try_from(n).expect("small");
        let mut record = delivery(n);
        record.state = state.to_owned();
        store
            .tx(2_000, |tx| {
                tx.put_interaction(&row)?;
                tx.put_delivery(&record)
            })
            .expect("rows");
    }
    let open: Vec<String> = store
        .open_interactions()
        .expect("a read")
        .into_iter()
        .map(|(row, record)| {
            assert_eq!(record.expect("a record").state, row.state);
            row.handle
        })
        .collect();
    assert_eq!(open, ["i1", "i2", "i3", "i4"]);
}

#[test]
fn the_prune_deletes_old_ended_interactions_and_keeps_open_and_recent_ones() {
    let mut store = Store::open_in_memory().expect("a store");
    // (number, state, opened, resolved)
    for (n, state, opened, resolved) in [
        (1, "delivered", 100, Some(200)),          // old and ended: goes
        (2, "cancelled", 100, None),               // ended with no time of its end: goes
        (3, "open", 100, None),                    // old and open: stays
        (4, "delivery_unknown", 100, Some(200)),   // not final: stays
        (5, "failed", 100, Some(5_000)),           // ended after the cut: stays
        (6, "resolved_elsewhere", 100, Some(999)), // just before the cut: goes
    ] {
        let mut row = interaction(n);
        row.state = state.to_owned();
        row.opened_at = opened;
        row.resolved_at = resolved;
        let mut record = delivery(n);
        record.state = state.to_owned();
        store
            .tx(10_000, |tx| {
                tx.put_interaction(&row)?;
                tx.put_delivery(&record)
            })
            .expect("rows");
    }
    let gone = store.prune_interactions(10_000, 1_000).expect("a prune");
    assert_eq!(gone, 3);
    let left: Vec<String> = store
        .recent_interactions(100)
        .expect("a read")
        .into_iter()
        .map(|(row, _)| row.handle)
        .collect();
    assert_eq!(left.len(), 3);
    for handle in ["i3", "i4", "i5"] {
        assert!(left.contains(&handle.to_owned()), "{left:?}");
    }
    // The delivery records went with their interactions.
    for n in [1, 2, 6] {
        assert_eq!(store.delivery_row(id(n)).expect("a read"), None, "{n}");
    }
    assert!(store.delivery_row(id(3)).expect("a read").is_some());
    // A second prune finds nothing.
    assert_eq!(store.prune_interactions(10_000, 1_000).expect("a prune"), 0);
}

#[test]
fn an_interaction_without_a_delivery_record_reads_with_none() {
    let mut store = Store::open_in_memory().expect("a store");
    store
        .tx(2_000, |tx| tx.put_interaction(&interaction(1)))
        .expect("a row");
    let (_, record) = store
        .interaction_row(id(1))
        .expect("a read")
        .expect("a row");
    assert_eq!(record, None);
}

#[test]
fn the_recent_interactions_are_the_newest_first_up_to_the_limit() {
    let mut store = Store::open_in_memory().expect("a store");
    for n in 1..=5u128 {
        let mut row = interaction(n);
        row.opened_at = i64::try_from(n).expect("small") * 10;
        store
            .tx(2_000, |tx| tx.put_interaction(&row))
            .expect("a row");
    }
    let handles: Vec<String> = store
        .recent_interactions(3)
        .expect("a read")
        .into_iter()
        .map(|(row, _)| row.handle)
        .collect();
    assert_eq!(handles, ["i5", "i4", "i3"]);
}

#[test]
fn a_native_ref_is_not_unique_but_a_handle_is() {
    let mut store = Store::open_in_memory().expect("a store");
    store
        .tx(2_000, |tx| tx.put_interaction(&interaction(1)))
        .expect("a row");
    let mut same_ref = interaction(2);
    same_ref.handle = "i2".to_owned();
    store
        .tx(2_000, |tx| tx.put_interaction(&same_ref))
        .expect("a second row");
    let mut same_handle = interaction(3);
    same_handle.handle = "i1".to_owned();
    assert!(
        store
            .tx(2_000, |tx| tx.put_interaction(&same_handle))
            .is_err()
    );
}
