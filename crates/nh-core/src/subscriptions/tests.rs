use nh_proto::api::{common::Empty, time::Timestamp};
use serde_json::json;

use super::*;

fn event(seq: u64, kind: &str, subject: Option<&str>) -> Event {
    Event {
        seq,
        at: Timestamp(0),
        kind: kind.to_owned(),
        v: 1,
        subject: subject.map(str::to_owned),
        payload: json!({}),
    }
}

#[test]
fn an_empty_filter_takes_every_event() {
    let filter = EventFilter::default();
    assert!(matches(&filter, &event(1, "agent.state_changed", None)));
    assert!(matches(&filter, &event(2, "x", Some("s"))));
}

#[test]
fn a_kind_is_exact_and_a_star_makes_a_prefix() {
    let filter = |kinds: &[&str]| EventFilter {
        kinds: kinds.iter().map(|k| (*k).to_owned()).collect(),
        subject: None,
    };
    let e = event(1, "interaction.opened", None);
    assert!(matches(&filter(&["interaction.opened"]), &e));
    assert!(!matches(&filter(&["interaction.open"]), &e));
    assert!(matches(&filter(&["interaction.*"]), &e));
    assert!(matches(&filter(&["agent.*", "interaction.*"]), &e));
    assert!(!matches(&filter(&["agent.*"]), &e));
    assert!(matches(&filter(&["*"]), &e), "a lone star takes all");
    // `_` is not a wildcard, as in the database.
    assert!(!matches(&filter(&["interaction_*"]), &e));
}

#[test]
fn a_subject_must_be_the_same() {
    let filter = EventFilter {
        kinds: Vec::new(),
        subject: Some("run-1".to_owned()),
    };
    assert!(matches(&filter, &event(1, "k", Some("run-1"))));
    assert!(!matches(&filter, &event(2, "k", Some("run-2"))));
    assert!(!matches(&filter, &event(3, "k", None)));
}

#[test]
fn the_filter_here_agrees_with_the_filter_of_the_database() {
    let mut store = nh_store::Store::open_in_memory().expect("a store");
    let kinds = [
        "agent.state_changed",
        "agent.detected",
        "interaction.opened",
        "pane_x.y",
    ];
    let subjects = [None, Some("a"), Some("b")];
    let (_, appended) = store
        .tx(1_000, |tx| {
            for n in 0..24 {
                tx.append_raw(kinds[n % 4], 1, subjects[n % 3], json!({"n": n}))?;
            }
            Ok(())
        })
        .expect("a transaction");
    assert_eq!(appended.len(), 24);
    let start = store.cursor_now().expect("a cursor");
    let from_zero = nh_proto::api::events::Cursor { seq: 0, ..start };
    let filters = [
        EventFilter::default(),
        EventFilter {
            kinds: vec!["agent.*".into()],
            subject: None,
        },
        EventFilter {
            kinds: vec!["agent.detected".into(), "pane_x.y".into()],
            subject: None,
        },
        EventFilter {
            kinds: vec!["pane_*".into()],
            subject: None,
        },
        EventFilter {
            kinds: Vec::new(),
            subject: Some("a".into()),
        },
        EventFilter {
            kinds: vec!["interaction.*".into()],
            subject: Some("b".into()),
        },
    ];
    for filter in &filters {
        let nh_store::EventsRead::Events { events, .. } = store
            .events_since(&from_zero, filter, 1_000)
            .expect("a read")
        else {
            panic!("events");
        };
        let from_db: Vec<u64> = events.iter().map(|e| e.seq).collect();
        let here: Vec<u64> = appended
            .iter()
            .filter(|e| matches(filter, e))
            .map(|e| e.seq)
            .collect();
        assert_eq!(here, from_db, "{filter:?}");
    }
    let _ = Empty {};
}

#[test]
fn ids_are_numbered_per_connection_and_a_wrong_prefix_is_not_found() {
    let table = Table::default();
    assert_eq!(table.new_id("p"), "p-1");
    assert_eq!(table.new_id("e"), "e-2");
    assert_eq!(table.new_id("p"), "p-3");
    assert!(table.take("p-1").is_none());
    assert_eq!(table.len(), 0);
}

#[test]
fn a_notification_is_one_line_without_an_id() {
    let line = notification("events.event", &json!({"subscription": "e-1"}));
    assert_eq!(line.last(), Some(&b'\n'));
    let value: Value = serde_json::from_slice(&line).expect("JSON");
    assert_eq!(
        value,
        json!({"jsonrpc": "2.0", "method": "events.event", "params": {"subscription": "e-1"}})
    );
}

#[tokio::test]
async fn a_full_queue_drops_the_sink_and_says_so() {
    let (queue, mut messages) = mpsc::channel(2);
    let overflow = Arc::new(AtomicBool::new(false));
    let sink = QueueSink {
        queue,
        overflow: Arc::clone(&overflow),
    };
    assert!(sink.send(json!(1)));
    assert!(sink.send(json!(2)));
    assert!(!sink.send(json!(3)), "the third does not fit");
    assert!(overflow.load(Ordering::SeqCst));
    assert_eq!(messages.recv().await, Some(json!(1)));
    assert_eq!(messages.recv().await, Some(json!(2)));
    drop(sink);
    assert_eq!(messages.recv().await, None);
}
