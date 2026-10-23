use serde_json::json;

use super::*;
use crate::RunRow;

fn id(n: u128) -> Ulid {
    Ulid::from(n)
}

fn run(n: u128, harness: &str) -> RunRow {
    RunRow {
        id: id(n),
        pane: None,
        harness: harness.to_owned(),
        manifest: harness.to_owned(),
        name: None,
        session_id: None,
        previous_session_ids: Vec::new(),
        transcript_path: None,
        binding: "bound".to_owned(),
        binding_confidence: 1.0,
        launch_mode: "tui".to_owned(),
        version: None,
        capabilities: json!({}),
        facets: json!({}),
        yolo: false,
        resume: None,
        model: None,
        process: None,
        started_at: 1_000,
        ended_at: None,
    }
}

#[test]
fn the_usage_of_a_run_adds_up_for_each_day_and_model() {
    let mut store = Store::open_in_memory().expect("a store");
    store
        .tx(1, |tx| {
            tx.put_run(&run(1, "claude"))?;
            tx.add_usage(
                id(1),
                "2026-10-08",
                "claude-opus-5-5",
                100,
                10,
                5,
                2,
                Some(0.5),
            )?;
            tx.add_usage(
                id(1),
                "2026-10-08",
                "claude-opus-5-5",
                50,
                5,
                0,
                0,
                Some(0.25),
            )?;
            // No cost leaves the cost as it is.
            tx.add_usage(id(1), "2026-10-08", "claude-opus-5-5", 1, 1, 0, 0, None)?;
            tx.add_usage(id(1), "2026-10-09", "claude-opus-5-5", 7, 0, 0, 0, None)?;
            let rows = tx.usage_of_run(id(1))?;
            assert_eq!(rows.len(), 2);
            assert_eq!(rows[0].0, "2026-10-08");
            assert_eq!(
                rows[0].2,
                UsageTotals {
                    input: 151,
                    output: 16,
                    cache_read: 5,
                    cache_write: 2,
                    cost_usd: Some(0.75)
                }
            );
            assert_eq!(rows[1].2.cost_usd, None, "no cost was ever added");
            Ok(())
        })
        .expect("a transaction");
}

#[test]
fn the_totals_by_run_and_by_harness_on_a_day() {
    let mut store = Store::open_in_memory().expect("a store");
    store
        .tx(1, |tx| {
            tx.put_run(&run(1, "claude"))?;
            tx.put_run(&run(2, "claude"))?;
            tx.put_run(&run(3, "codex"))?;
            tx.add_usage(id(1), "2026-10-08", "m", 10, 1, 0, 0, Some(1.0))?;
            tx.add_usage(id(1), "2026-10-09", "m", 20, 2, 0, 0, Some(2.0))?;
            tx.add_usage(id(2), "2026-10-09", "m", 30, 3, 0, 0, None)?;
            tx.add_usage(id(3), "2026-10-09", "gpt", 40, 4, 0, 0, Some(4.0))?;
            Ok(())
        })
        .expect("a transaction");
    let by_run = store.usage_by_run().expect("totals");
    let one = by_run.iter().find(|(run, _)| *run == id(1)).expect("run 1");
    assert_eq!(
        (one.1.input, one.1.output, one.1.cost_usd),
        (30, 3, Some(3.0))
    );
    store
        .tx(2, |tx| {
            let today = tx.usage_on_day("2026-10-09")?;
            let names: Vec<_> = today.iter().map(|(harness, _)| harness.as_str()).collect();
            assert_eq!(names, vec!["claude", "codex"]);
            assert_eq!(today[0].1.input, 50);
            assert_eq!(today[0].1.cost_usd, Some(2.0));
            assert_eq!(today[1].1.input, 40);
            assert!(tx.usage_on_day("2026-01-01")?.is_empty());
            Ok(())
        })
        .expect("a transaction");
}

#[test]
fn the_state_of_a_transcript_is_kept_when_the_run_is_written_again() {
    let mut store = Store::open_in_memory().expect("a store");
    store
        .tx(1, |tx| {
            assert_eq!(tx.transcript_state(id(9))?, None);
            tx.put_run(&run(1, "claude"))?;
            assert_eq!(
                tx.transcript_state(id(1))?,
                Some(TranscriptState::default())
            );
            tx.set_transcript_state(id(1), 4096, "{\"seen\":[]}")?;
            // Writing the run again does not lose the place in the file.
            tx.put_run(&run(1, "claude"))?;
            assert_eq!(
                tx.transcript_state(id(1))?,
                Some(TranscriptState {
                    offset: 4096,
                    ledger_json: Some("{\"seen\":[]}".to_owned())
                })
            );
            tx.add_usage(id(1), "2026-10-08", "m", 1, 1, 0, 0, None)?;
            Ok(())
        })
        .expect("a transaction");
}
