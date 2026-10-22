use serde_json::json;

use super::*;

fn id(n: u128) -> Ulid {
    Ulid::from(n)
}

fn run(n: u128) -> RunRow {
    RunRow {
        id: id(n),
        pane: None,
        harness: "claude".to_owned(),
        manifest: "claude".to_owned(),
        name: None,
        session_id: None,
        previous_session_ids: Vec::new(),
        transcript_path: None,
        binding: "suggested".to_owned(),
        binding_confidence: 0.9,
        launch_mode: "tui".to_owned(),
        version: Some("2.1.292".to_owned()),
        capabilities: json!({"observe": true}),
        facets: json!({"execution": {"value": {"state": "starting"}}}),
        yolo: false,
        resume: None,
        model: None,
        task: None,
        process: Some((4242, 1_760_000_000_123)),
        started_at: 1_000,
        ended_at: None,
    }
}

#[test]
fn a_run_comes_back_as_it_was_written() {
    let mut store = Store::open_in_memory().expect("a store");
    let mut written = run(1);
    written.session_id = Some("s-1".to_owned());
    written.previous_session_ids = vec!["s-0".to_owned()];
    written.resume = Some(json!({"argv": ["claude", "--resume", "s-1"]}));
    written.model = Some("opus".to_owned());
    written.yolo = true;
    store.tx(2_000, |tx| tx.put_run(&written)).expect("a run");
    assert_eq!(store.run(id(1)).expect("a read"), Some(written));
    assert_eq!(store.run(id(9)).expect("a read"), None);
}

#[test]
fn a_run_without_a_process_has_none() {
    let mut store = Store::open_in_memory().expect("a store");
    let mut written = run(2);
    written.process = None;
    store.tx(2_000, |tx| tx.put_run(&written)).expect("a run");
    assert_eq!(
        store.run(id(2)).expect("a read").expect("a run").process,
        None
    );
}

#[test]
fn writing_a_run_again_changes_it_and_keeps_the_start() {
    let mut store = Store::open_in_memory().expect("a store");
    store.tx(2_000, |tx| tx.put_run(&run(1))).expect("a run");
    let mut changed = run(1);
    changed.binding = "bound".to_owned();
    changed.binding_confidence = 1.0;
    changed.facets = json!({"execution": {"value": {"state": "idle"}}});
    changed.started_at = 9_999;
    changed.ended_at = Some(5_000);
    store.tx(3_000, |tx| tx.put_run(&changed)).expect("a run");
    let read = store.run(id(1)).expect("a read").expect("a run");
    assert_eq!(read.binding, "bound");
    assert_eq!(read.facets, changed.facets);
    assert_eq!(read.ended_at, Some(5_000));
    assert_eq!(read.started_at, 1_000, "the start does not change");
}

#[test]
fn the_open_runs_are_the_ones_that_have_not_ended() {
    let mut store = Store::open_in_memory().expect("a store");
    store
        .tx(2_000, |tx| {
            let mut late = run(3);
            late.started_at = 3_000;
            let mut ended = run(2);
            ended.ended_at = Some(2_500);
            tx.put_run(&late)?;
            tx.put_run(&ended)?;
            tx.put_run(&run(1))
        })
        .expect("runs");
    let open = store.open_runs().expect("a read");
    assert_eq!(
        open.iter().map(|r| r.id).collect::<Vec<_>>(),
        [id(1), id(3)]
    );
}

#[test]
fn the_open_runs_come_in_the_order_of_the_board_and_a_run_with_no_place_comes_last() {
    let mut store = Store::open_in_memory().expect("a store");
    store
        .tx(2_000, |tx| {
            for n in 1..=4 {
                let mut row = run(n);
                row.started_at = 1_000 + i64::try_from(n).expect("a small number");
                tx.put_run(&row)?;
            }
            tx.put_board_order(&[id(3), id(1)])
        })
        .expect("runs");
    // A write of the row again keeps its place.
    store.tx(3_000, |tx| tx.put_run(&run(1))).expect("a run");
    let open = store.open_runs().expect("a read");
    assert_eq!(
        open.iter().map(|r| r.id).collect::<Vec<_>>(),
        [id(3), id(1), id(2), id(4)]
    );
}

#[test]
fn the_pane_of_a_run_that_goes_leaves_the_run() {
    // `ON DELETE SET NULL`: the history of a run stays when its pane closes.
    use crate::layout::{PaneSlot, TabRow, WorkspaceRow};
    let mut store = Store::open_in_memory().expect("a store");
    store
        .tx(1, |tx| {
            tx.put_workspace(&WorkspaceRow {
                id: id(10),
                handle: "w1".to_owned(),
                name: "One".to_owned(),
                root: None,
                machine: "local".to_owned(),
                group: None,
                position: 1,
                selected_tab: None,
            })?;
            tx.put_tab(&TabRow {
                id: id(20),
                workspace: id(10),
                handle: "w1:t1".to_owned(),
                name: "Tab".to_owned(),
                position: 1,
                layout: json!({"type": "pane", "pane": id(30).to_string()}),
                floating: json!([]),
                zoomed: None,
            })?;
            tx.put_pane_slot(&PaneSlot {
                id: id(30),
                render_id: 1,
                workspace: id(10),
                tab: id(20),
                handle: "w1:p1".to_owned(),
                cols: 80,
                rows: 24,
                spec: None,
            })?;
            let mut with_pane = run(1);
            with_pane.pane = Some(id(30));
            tx.put_run(&with_pane)
        })
        .expect("a layout and a run");
    assert_eq!(
        store.run(id(1)).expect("a read").expect("a run").pane,
        Some(id(30))
    );
    store.tx(2, |tx| tx.delete_pane(id(30))).expect("a delete");
    assert_eq!(store.run(id(1)).expect("a read").expect("a run").pane, None);
}
