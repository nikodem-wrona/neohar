use std::sync::{Arc, Mutex, PoisonError};

use serde_json::{Value, json};

use super::*;

#[derive(Clone, Default)]
struct Collector(Arc<Mutex<Vec<Value>>>);

impl Collector {
    fn messages(&self) -> Vec<Value> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl MessageSink for Collector {
    fn send(&self, message: Value) -> bool {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(message);
        true
    }
}

/// A sink that stops taking messages after `limit`.
struct Limited(Collector, usize);

impl MessageSink for Limited {
    fn send(&self, message: Value) -> bool {
        let mut list = self.0.0.lock().unwrap_or_else(PoisonError::into_inner);
        if list.len() >= self.1 {
            return false;
        }
        list.push(message);
        true
    }
}

fn subscribe(
    hub: &ProjectionHub,
    projection: Projection,
    since: Option<u64>,
) -> (Collector, SubscriptionId, Start) {
    let sink = Collector::default();
    let (id, start) = hub.subscribe(projection, since, Box::new(sink.clone()));
    (sink, id, start)
}

#[test]
fn a_subscriber_gets_the_snapshot_and_then_one_patch_per_flush() {
    let hub = ProjectionHub::new();
    hub.set(Projection::Panes, "a", json!({"title": "one"}));
    hub.flush();
    let (sink, _, start) = subscribe(&hub, Projection::Panes, None);
    assert_eq!(start, Start::Snapshot);
    hub.set(Projection::Panes, "a", json!({"title": "two"}));
    hub.set(Projection::Panes, "b", json!({"title": "three"}));
    assert_eq!(
        sink.messages().len(),
        1,
        "nothing goes out before the flush"
    );
    assert_eq!(hub.flush(), 1);
    let got = sink.messages();
    assert_eq!(got.len(), 2);
    assert_eq!(
        got[0],
        json!({"type": "snapshot", "version": 1, "data": {"items": {"a": {"title": "one"}}, "order": ["a"]}})
    );
    assert_eq!(
        got[1],
        json!({"type": "patch", "version": 2, "ops": [
            {"op": "set", "key": "a", "value": {"title": "two"}},
            {"op": "set", "key": "b", "value": {"title": "three"}},
            {"op": "order", "keys": ["a", "b"]},
        ]})
    );
}

#[test]
fn many_changes_of_one_window_make_one_patch_with_the_last_value_of_each_key() {
    let hub = ProjectionHub::new();
    let (sink, _, _) = subscribe(&hub, Projection::Agents, None);
    for n in 0..100 {
        hub.set(Projection::Agents, "run", json!({"n": n}));
    }
    hub.set(Projection::Agents, "other", json!(1));
    hub.remove(Projection::Agents, "other");
    hub.set(Projection::Agents, "third", json!(3));
    hub.flush();
    let got = sink.messages();
    assert_eq!(got.len(), 2, "the snapshot and one patch");
    assert_eq!(got[1]["version"], 1);
    let ops = got[1]["ops"].as_array().expect("ops");
    assert_eq!(
        ops[0],
        json!({"op": "set", "key": "run", "value": {"n": 99}})
    );
    assert_eq!(ops[1], json!({"op": "remove", "key": "other"}));
    assert_eq!(ops[2], json!({"op": "set", "key": "third", "value": 3}));
    assert_eq!(ops[3], json!({"op": "order", "keys": ["run", "third"]}));
    assert_eq!(ops.len(), 4);
}

#[test]
fn a_flush_without_changes_makes_no_patch_and_keeps_the_version() {
    let hub = ProjectionHub::new();
    hub.set(Projection::Tree, "w1", json!({}));
    assert_eq!(hub.flush(), 1);
    assert_eq!(hub.flush(), 0);
    assert_eq!(hub.snapshot(Projection::Tree).version, 1);
    hub.set(Projection::Tree, "w1", json!({}));
    assert_eq!(hub.flush(), 0, "the same value again is no change");
    assert_eq!(hub.snapshot(Projection::Tree).version, 1);
    hub.set(Projection::Tree, "w1", json!({"a": 1}));
    assert_eq!(hub.flush(), 1);
    assert_eq!(hub.snapshot(Projection::Tree).version, 2);
}

#[test]
fn setting_all_rows_again_patches_only_the_rows_that_changed() {
    let hub = ProjectionHub::new();
    for key in ["a", "b", "c"] {
        hub.set(Projection::Panes, key, json!({"v": 1}));
    }
    hub.flush();
    let sink = Collector::default();
    hub.subscribe(Projection::Panes, None, Box::new(sink.clone()));
    for key in ["a", "b", "c"] {
        let v = if key == "b" { 2 } else { 1 };
        hub.set(Projection::Panes, key, json!({"v": v}));
    }
    hub.flush();
    let messages = sink.messages();
    let ops = messages[1]["ops"].as_array().expect("ops");
    assert_eq!(ops.len(), 1, "{ops:?}");
    assert_eq!(ops[0], json!({"op": "set", "key": "b", "value": {"v": 2}}));
}

#[test]
fn a_remove_and_a_set_again_move_the_key_to_the_end() {
    let hub = ProjectionHub::new();
    for key in ["a", "b", "c"] {
        hub.set(Projection::Panes, key, json!(key));
    }
    hub.flush();
    hub.remove(Projection::Panes, "a");
    hub.set(Projection::Panes, "a", json!("again"));
    hub.flush();
    assert_eq!(hub.snapshot(Projection::Panes).order, ["b", "c", "a"]);
    hub.remove(Projection::Panes, "nothing");
    assert_eq!(hub.flush(), 0, "a key that does not exist is ignored");
}

#[test]
fn the_version_is_the_one_of_the_last_flush_and_none_for_a_projection_nothing_made() {
    let hub = ProjectionHub::new();
    assert_eq!(hub.version(Projection::Panes), None);
    hub.set(Projection::Panes, "a", json!(1));
    assert_eq!(
        hub.version(Projection::Panes),
        Some(0),
        "nothing flushed yet"
    );
    hub.flush();
    assert_eq!(hub.version(Projection::Panes), Some(1));
    hub.set(Projection::Panes, "a", json!(2));
    hub.flush();
    assert_eq!(hub.version(Projection::Panes), Some(2));
    // Asking does not make the projection.
    assert_eq!(hub.version(Projection::Tree), None);
    assert_eq!(hub.subscriber_count(Projection::Tree), 0);
    hub.forget(Projection::Panes);
    assert_eq!(hub.version(Projection::Panes), None);
}

#[test]
fn set_order_puts_known_keys_first_and_ignores_unknown_ones() {
    let hub = ProjectionHub::new();
    for key in ["a", "b", "c", "d"] {
        hub.set(Projection::Panes, key, json!(0));
    }
    hub.flush();
    hub.set_order(
        Projection::Panes,
        &["c".into(), "x".into(), "a".into(), "c".into()],
    );
    hub.flush();
    assert_eq!(hub.snapshot(Projection::Panes).order, ["c", "a", "b", "d"]);
    hub.set_order(Projection::Panes, &["c".into(), "a".into()]);
    assert_eq!(hub.flush(), 0, "the same order is no change");
}

#[test]
fn a_reader_sees_the_state_of_the_last_flush_and_not_the_changes_that_wait() {
    let hub = ProjectionHub::new();
    hub.set(Projection::Jobs, "j1", json!("running"));
    assert_eq!(hub.snapshot(Projection::Jobs).version, 0);
    assert!(hub.snapshot(Projection::Jobs).items.is_empty());
    hub.flush();
    let published = hub.snapshot(Projection::Jobs);
    assert_eq!(
        (published.version, published.order.clone()),
        (1, vec!["j1".to_owned()])
    );
    hub.set(Projection::Jobs, "j1", json!("done"));
    assert_eq!(hub.snapshot(Projection::Jobs).items["j1"], json!("running"));
    hub.flush();
    assert_eq!(hub.snapshot(Projection::Jobs).items["j1"], json!("done"));
}

#[test]
fn a_new_subscriber_sees_the_current_state_and_loses_nothing() {
    let hub = ProjectionHub::new();
    let (early, _, _) = subscribe(&hub, Projection::Panes, None);
    hub.set(Projection::Panes, "a", json!(1));
    hub.flush();
    hub.set(Projection::Panes, "a", json!(2));
    // The change waits for the window. The new subscriber still gets it in the snapshot.
    let (late, _, _) = subscribe(&hub, Projection::Panes, None);
    let got = late.messages();
    assert_eq!(got.len(), 1);
    assert_eq!(got[0]["data"]["items"]["a"], json!(2));
    assert_eq!(got[0]["version"], 2);
    // The subscriber from before got that change as a patch when the new one came.
    assert_eq!(early.messages().len(), 3);
    hub.set(Projection::Panes, "a", json!(3));
    hub.flush();
    assert_eq!(late.messages()[1]["version"], 3);
    assert_eq!(early.messages()[3]["version"], 3);
}

#[test]
fn since_inside_the_ring_returns_the_patches_after_it() {
    let hub = ProjectionHub::new();
    for n in 1..=5 {
        hub.set(Projection::Panes, "a", json!(n));
        hub.flush();
    }
    let (sink, _, start) = subscribe(&hub, Projection::Panes, Some(3));
    assert_eq!(start, Start::Patches(2));
    let versions: Vec<u64> = sink
        .messages()
        .iter()
        .filter_map(|m| m["version"].as_u64())
        .collect();
    assert_eq!(versions, [4, 5]);
    // The subscriber is live afterwards.
    hub.set(Projection::Panes, "a", json!(6));
    hub.flush();
    assert_eq!(sink.messages().len(), 3);
    // A client that is at the version already gets nothing and is live.
    let (sink, _, start) = subscribe(&hub, Projection::Panes, Some(6));
    assert_eq!((start, sink.messages().len()), (Start::Patches(0), 0));
    hub.set(Projection::Panes, "a", json!(7));
    hub.flush();
    assert_eq!(sink.messages().len(), 1);
}

#[test]
fn since_older_than_the_ring_or_ahead_of_the_projection_is_a_resync() {
    let hub = ProjectionHub::new();
    for n in 0..(RING_PATCHES + 100) {
        hub.set(Projection::Panes, "a", json!(n));
        hub.flush();
    }
    let latest = hub.snapshot(Projection::Panes).version;
    assert_eq!(latest as usize, RING_PATCHES + 100);

    let (old, old_id, start) = subscribe(&hub, Projection::Panes, Some(50));
    assert_eq!(start, Start::Resync);
    assert_eq!(old.messages(), [json!({"type": "resync"})]);
    assert!(
        !hub.unsubscribe(Projection::Panes, old_id),
        "a resync does not subscribe"
    );

    // The oldest version that the ring can serve is one below its first patch.
    let first = latest - RING_PATCHES as u64 + 1;
    let (edge, _, start) = subscribe(&hub, Projection::Panes, Some(first - 1));
    assert_eq!(start, Start::Patches(RING_PATCHES));
    assert_eq!(edge.messages().len(), RING_PATCHES);
    let (_, _, start) = subscribe(&hub, Projection::Panes, Some(first - 2));
    assert_eq!(start, Start::Resync);

    let (ahead, _, start) = subscribe(&hub, Projection::Panes, Some(latest + 5));
    assert_eq!(
        (start, ahead.messages()),
        (Start::Resync, vec![json!({"type": "resync"})])
    );
}

#[test]
fn a_sink_that_refuses_a_message_is_dropped() {
    let hub = ProjectionHub::new();
    let sink = Collector::default();
    hub.subscribe(Projection::Panes, None, Box::new(Limited(sink.clone(), 2)));
    for n in 0..4 {
        hub.set(Projection::Panes, "a", json!(n));
        hub.flush();
    }
    assert_eq!(
        sink.messages().len(),
        2,
        "the snapshot and one patch, then it was dropped"
    );
}

#[test]
fn unsubscribe_stops_the_messages() {
    let hub = ProjectionHub::new();
    let (sink, id, _) = subscribe(&hub, Projection::Panes, None);
    hub.set(Projection::Panes, "a", json!(1));
    hub.flush();
    assert!(hub.unsubscribe(Projection::Panes, id));
    assert!(!hub.unsubscribe(Projection::Panes, id));
    hub.set(Projection::Panes, "a", json!(2));
    hub.flush();
    assert_eq!(sink.messages().len(), 2);
}

#[test]
fn a_singleton_has_one_key() {
    let hub = ProjectionHub::new();
    hub.set_singleton(Projection::Health, json!({"storage": {"state": "ok"}}));
    hub.flush();
    let snapshot = hub.snapshot(Projection::Health);
    assert_eq!(snapshot.order, ["_"]);
    assert!(snapshot.items.contains_key("_"));
}

#[test]
fn projections_do_not_touch_each_other() {
    let hub = ProjectionHub::new();
    let run = nh_proto::api::ids::RunId::new(ulid::Ulid::from_parts(1, 1));
    hub.set(Projection::Timeline(run), "item", json!(1));
    hub.set(Projection::Panes, "pane", json!(2));
    assert_eq!(hub.flush(), 2);
    assert_eq!(hub.snapshot(Projection::Timeline(run)).order, ["item"]);
    assert_eq!(hub.snapshot(Projection::Panes).order, ["pane"]);
    assert!(hub.snapshot(Projection::Agents).order.is_empty());
}

#[test]
fn threads_that_change_one_projection_lose_nothing() {
    let hub = ProjectionHub::new();
    let sink = Collector::default();
    hub.subscribe(Projection::Panes, None, Box::new(sink.clone()));
    let threads: Vec<_> = (0..8)
        .map(|t| {
            let hub = Arc::clone(&hub);
            std::thread::spawn(move || {
                for n in 0..500 {
                    hub.set(Projection::Panes, &format!("k{t}-{n}"), json!(n));
                    if n % 50 == 0 {
                        hub.flush();
                    }
                }
            })
        })
        .collect();
    for thread in threads {
        thread.join().expect("a thread");
    }
    hub.flush();
    assert_eq!(hub.snapshot(Projection::Panes).items.len(), 4_000);
    // The client rebuilds the state from the messages and gets the same state.
    let mut items = std::collections::HashMap::new();
    let mut last = 0;
    for message in sink.messages() {
        let version = message["version"].as_u64().expect("a version");
        if message["type"] == "snapshot" {
            last = version;
            continue;
        }
        assert!(version > last, "versions grow");
        last = version;
        for op in message["ops"].as_array().expect("ops") {
            if op["op"] == "set" {
                items.insert(
                    op["key"].as_str().expect("a key").to_owned(),
                    op["value"].clone(),
                );
            }
        }
    }
    assert_eq!(items.len(), 4_000);
}

#[tokio::test(start_paused = true)]
async fn the_ticker_flushes_once_for_each_window() {
    let hub = ProjectionHub::new();
    let (sink, _, _) = subscribe(&hub, Projection::Panes, None);
    let ticker = hub.spawn_ticker(&tokio::runtime::Handle::current());
    for n in 0..50 {
        hub.set(Projection::Panes, "a", json!(n));
    }
    tokio::time::sleep(COALESCE + Duration::from_millis(1)).await;
    assert_eq!(sink.messages().len(), 2, "50 changes made one patch");
    hub.set(Projection::Panes, "b", json!(1));
    hub.set(Projection::Panes, "b", json!(2));
    tokio::time::sleep(COALESCE).await;
    assert_eq!(sink.messages().len(), 3);
    tokio::time::sleep(COALESCE * 5).await;
    assert_eq!(sink.messages().len(), 3, "no change, no patch");
    ticker.abort();
}

#[test]
fn the_hook_learns_of_each_subscription_after_its_snapshot() {
    let hub = ProjectionHub::new();
    let seen = Arc::new(Mutex::new(Vec::<(Projection, usize)>::new()));
    let (sink, of_hub) = (Arc::clone(&seen), Arc::clone(&hub));
    hub.on_subscribe(Box::new(move |projection| {
        // The subscriber counts already when the hook runs.
        let count = of_hub.subscriber_count(projection);
        sink.lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((projection, count));
    }));
    let window: nh_proto::api::ids::WindowId = "01M4F1W7EM2GN3EV6NHXZJEGGD".parse().expect("an id");
    let (_a, _, _) = subscribe(&hub, Projection::ChangesFor(window), None);
    let (_b, _, _) = subscribe(&hub, Projection::Jobs, None);
    let seen = seen.lock().unwrap_or_else(PoisonError::into_inner).clone();
    assert_eq!(
        seen,
        [(Projection::ChangesFor(window), 1), (Projection::Jobs, 1)]
    );
}
