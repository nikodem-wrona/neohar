//! Subscriptions on the API socket: `projection.subscribe` and `events.subscribe`.

mod core_support;

use std::time::Duration;

use core_support::{
    Harness, TEST_PANE_TOKEN,
    client::{Client, error_of},
};
use serde_json::{Value, json};

async fn client(h: &Harness) -> Client {
    Client::connect(&h.api_socket()).await
}

/// Reads notifications until `pred` accepts one.
async fn until(c: &mut Client, what: &str, pred: impl Fn(&Value) -> bool) -> Value {
    for _ in 0..10_000 {
        let line = c.recv().await;
        if pred(&line) {
            return line;
        }
    }
    panic!("no line: {what}");
}

fn is_message(line: &Value, subscription: &str, kind: &str) -> bool {
    line["method"] == "projection.message"
        && line["params"]["subscription"] == subscription
        && line["params"]["message"]["type"] == kind
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_reply_comes_first_then_the_snapshot_then_patches() {
    let h = Harness::start();
    let mut c = client(&h).await;
    let id = c
        .request("projection.subscribe", json!({"name": "settings"}))
        .await;
    // The reply, and only then the notifications.
    let reply = c.recv().await;
    assert_eq!(reply["id"], json!(id));
    let subscription = reply["result"]["subscription"]
        .as_str()
        .expect("an id")
        .to_owned();
    let snapshot = c.recv().await;
    assert!(
        is_message(&snapshot, &subscription, "snapshot"),
        "{snapshot}"
    );
    let data = &snapshot["params"]["message"]["data"];
    assert_eq!(data["order"].as_array().map(Vec::len), Some(11));
    assert_eq!(
        data["items"]["approvals"]["entries"]["mode"]["value"],
        "auto"
    );
    let version = snapshot["params"]["message"]["version"]
        .as_u64()
        .expect("a version");

    // A change of the configuration is a patch.
    let mut other = client(&h).await;
    other
        .ok(
            "config.set",
            json!({"key": "approvals.mode", "value": "gate"}),
        )
        .await;
    let patch = until(&mut c, "a patch", |l| is_message(l, &subscription, "patch")).await;
    let message = &patch["params"]["message"];
    assert!(message["version"].as_u64().expect("a version") > version);
    let ops = message["ops"].as_array().expect("ops");
    assert_eq!(ops[0]["op"], "set");
    assert_eq!(ops[0]["key"], "approvals");
    assert_eq!(
        ops[0]["value"]["entries"]["mode"],
        json!({"value": "gate", "source": "ui"})
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn since_gives_patches_only_and_a_version_the_core_does_not_have_gives_resync() {
    let h = Harness::start();
    let mut c = client(&h).await;
    let reply = c
        .ok("projection.subscribe", json!({"name": "settings"}))
        .await;
    let first = reply["subscription"].as_str().expect("an id").to_owned();
    let snapshot = until(&mut c, "the snapshot", |l| {
        is_message(l, &first, "snapshot")
    })
    .await;
    let version = snapshot["params"]["message"]["version"]
        .as_u64()
        .expect("a version");
    c.ok("projection.unsubscribe", json!({"subscription": first}))
        .await;

    // With the current version, the client gets no snapshot, and the next change is a patch.
    let reply = c
        .ok(
            "projection.subscribe",
            json!({"name": "settings", "since": version}),
        )
        .await;
    let second = reply["subscription"].as_str().expect("an id").to_owned();
    assert!(
        c.try_recv(Duration::from_millis(200)).await.is_none(),
        "no snapshot with since"
    );
    c.ok("config.set", json!({"key": "agents.shims", "value": false}))
        .await;
    let patch = until(&mut c, "a patch", |l| is_message(l, &second, "patch")).await;
    assert_eq!(patch["params"]["message"]["version"], version + 1);

    // A version in the future: the core says resync, and the subscription is gone.
    let reply = c
        .ok(
            "projection.subscribe",
            json!({"name": "settings", "since": 99_999}),
        )
        .await;
    let third = reply["subscription"].as_str().expect("an id").to_owned();
    until(&mut c, "resync", |l| is_message(l, &third, "resync")).await;
    c.ok("config.set", json!({"key": "agents.shims", "value": true}))
        .await;
    // Only the second subscription gets this patch.
    let line = until(&mut c, "a patch", |l| l["method"] == "projection.message").await;
    assert_eq!(line["params"]["subscription"], second.as_str());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unsubscribe_ends_the_messages_and_wrong_ids_are_not_found() {
    let h = Harness::start();
    let mut c = client(&h).await;
    let reply = c
        .ok("projection.subscribe", json!({"name": "settings"}))
        .await;
    let id = reply["subscription"].as_str().expect("an id").to_owned();
    until(&mut c, "the snapshot", |l| is_message(l, &id, "snapshot")).await;
    c.ok("projection.unsubscribe", json!({"subscription": id}))
        .await;
    c.ok("config.set", json!({"key": "agents.shims", "value": false}))
        .await;
    assert!(
        c.try_recv(Duration::from_millis(300)).await.is_none(),
        "nothing after unsubscribe"
    );

    for (method, subscription) in [
        ("projection.unsubscribe", id.as_str()),
        ("projection.unsubscribe", "p-999"),
        ("projection.unsubscribe", "e-1"),
        ("events.unsubscribe", "p-1"),
        ("events.unsubscribe", "nothing"),
    ] {
        let reply = c.call(method, json!({"subscription": subscription})).await;
        assert_eq!(
            error_of(&reply),
            (-32000, "not_found".into()),
            "{method} {subscription}"
        );
    }
    let reply = c
        .call("projection.subscribe", json!({"name": "nothing"}))
        .await;
    assert_eq!(error_of(&reply), (-32602, "invalid_params".into()));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_connection_has_at_most_64_subscriptions_and_a_pane_has_none() {
    let h = Harness::start();
    let mut c = client(&h).await;
    for n in 0..64 {
        let reply = c
            .call("projection.subscribe", json!({"name": "settings"}))
            .await;
        assert!(reply.get("error").is_none(), "subscription {n}: {reply}");
    }
    let reply = c
        .call("projection.subscribe", json!({"name": "settings"}))
        .await;
    assert_eq!(error_of(&reply), (-32000, "busy".into()));
    let reply = c.call("events.subscribe", json!({})).await;
    assert_eq!(error_of(&reply), (-32000, "busy".into()));

    let mut p = client(&h).await;
    for method in ["projection.subscribe", "events.subscribe"] {
        p.send(
            &json!({"jsonrpc": "2.0", "id": 1, "pane_token": TEST_PANE_TOKEN,
            "method": method, "params": {"name": "settings"}}),
        )
        .await;
        assert_eq!(error_of(&p.recv().await).1, "permission_denied", "{method}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn events_start_at_the_end_of_the_log_and_arrive_in_order() {
    let h = Harness::start();
    let mut c = client(&h).await;
    let mut other = client(&h).await;
    let reply = c.ok("events.subscribe", json!({})).await;
    let subscription = reply["subscription"].as_str().expect("an id").to_owned();
    let cursor = reply["cursor"].clone();
    assert!(cursor["machine_uuid"].is_string() && cursor["session_uuid"].is_string());
    let start = cursor["seq"].as_u64().expect("a seq");

    other.ok("debug.emit_events", json!({"count": 5})).await;
    for n in 1..=5u64 {
        let line = until(&mut c, "an event", |l| l["method"] == "events.event").await;
        assert_eq!(line["params"]["subscription"], subscription.as_str());
        let event = &line["params"]["event"];
        assert_eq!(event["seq"], start + n);
        assert_eq!(event["kind"], "debug.tick");
        assert_eq!(event["v"], 1);
        assert_eq!(event["payload"]["n"], n - 1);
    }
    assert!(c.try_recv(Duration::from_millis(200)).await.is_none());

    // After unsubscribe nothing comes.
    c.ok("events.unsubscribe", json!({"subscription": subscription}))
        .await;
    other.ok("debug.emit_events", json!({"count": 2})).await;
    assert!(c.try_recv(Duration::from_millis(300)).await.is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cursor_in_the_past_gets_the_missed_events_and_then_the_live_ones() {
    let h = Harness::start();
    let mut emitter = client(&h).await;
    let mut first = client(&h).await;
    let cursor = first.ok("events.subscribe", json!({})).await["cursor"].clone();
    let start = cursor["seq"].as_u64().expect("a seq");
    emitter.ok("debug.emit_events", json!({"count": 30})).await;

    let mut late = client(&h).await;
    let reply = late.ok("events.subscribe", json!({"cursor": cursor})).await;
    assert_eq!(
        reply["cursor"]["seq"],
        start + 30,
        "the reply says where the log ends"
    );
    for n in 1..=30u64 {
        let line = until(&mut late, "a missed event", |l| {
            l["method"] == "events.event"
        })
        .await;
        assert_eq!(line["params"]["event"]["seq"], start + n);
    }
    emitter.ok("debug.emit_events", json!({"count": 3})).await;
    for n in 31..=33u64 {
        let line = until(&mut late, "a live event", |l| l["method"] == "events.event").await;
        assert_eq!(line["params"]["event"]["seq"], start + n);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_filter_keeps_kinds_and_subjects() {
    let h = Harness::start();
    let mut emitter = client(&h).await;
    let mut kinds = client(&h).await;
    let mut subject = client(&h).await;
    let k = kinds
        .ok(
            "events.subscribe",
            json!({"filter": {"kinds": ["a.*", "c.z"]}}),
        )
        .await;
    let s = subject
        .ok("events.subscribe", json!({"filter": {"subject": "run-2"}}))
        .await;
    let (k, s) = (k["subscription"].clone(), s["subscription"].clone());
    for (kind, subj) in [
        ("a.x", "run-1"),
        ("b.y", "run-2"),
        ("c.z", "run-1"),
        ("c.w", "run-2"),
        ("a.q", "run-2"),
    ] {
        emitter
            .ok(
                "debug.emit_events",
                json!({"count": 1, "kind": kind, "subject": subj}),
            )
            .await;
    }
    let mut seen = Vec::new();
    for _ in 0..3 {
        let line = until(&mut kinds, "an event", |l| l["method"] == "events.event").await;
        assert_eq!(line["params"]["subscription"], k);
        seen.push(
            line["params"]["event"]["kind"]
                .as_str()
                .expect("a kind")
                .to_owned(),
        );
    }
    assert_eq!(seen, ["a.x", "c.z", "a.q"]);
    assert!(kinds.try_recv(Duration::from_millis(200)).await.is_none());
    let mut seen = Vec::new();
    for _ in 0..3 {
        let line = until(&mut subject, "an event", |l| l["method"] == "events.event").await;
        assert_eq!(line["params"]["subscription"], s);
        seen.push(
            line["params"]["event"]["kind"]
                .as_str()
                .expect("a kind")
                .to_owned(),
        );
    }
    assert_eq!(seen, ["b.y", "c.w", "a.q"]);
    assert!(subject.try_recv(Duration::from_millis(200)).await.is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cursor_that_the_log_cannot_continue_gets_truncated() {
    let h = Harness::start();
    let mut c = client(&h).await;
    let mut emitter = client(&h).await;
    let end = c.ok("events.subscribe", json!({})).await["cursor"].clone();
    emitter.ok("debug.emit_events", json!({"count": 2})).await;

    // Another epoch, another session, and a position the log has not reached.
    for (round, (what, cursor)) in [
        (
            "epoch",
            json!({"log_epoch": end["log_epoch"].as_u64().unwrap_or(1) + 1}),
        ),
        (
            "session",
            json!({"session_uuid": "00000000-0000-0000-0000-000000000000"}),
        ),
        ("ahead", json!({"seq": 1_000_000})),
    ]
    .into_iter()
    .enumerate()
    {
        let mut stale = end.clone();
        for (key, value) in cursor.as_object().expect("an object") {
            stale[key] = value.clone();
        }
        let mut client = client(&h).await;
        let reply = client
            .ok("events.subscribe", json!({"cursor": stale}))
            .await;
        let subscription = reply["subscription"].clone();
        let line = until(&mut client, "truncated", |l| {
            l["method"] == "events.truncated"
        })
        .await;
        assert_eq!(line["params"]["subscription"], subscription, "{what}");
        // The cursor to continue from is the end of the log. It grew by two events before the
        // loop, and by one in each round before this one.
        assert_eq!(
            line["params"]["cursor"]["seq"],
            end["seq"].as_u64().unwrap_or(0) + 2 + round as u64,
            "{what}"
        );
        assert_eq!(
            line["params"]["cursor"]["log_epoch"], end["log_epoch"],
            "{what}"
        );
        // The subscription is over.
        emitter.ok("debug.emit_events", json!({"count": 1})).await;
        assert!(
            client.try_recv(Duration::from_millis(200)).await.is_none(),
            "{what}"
        );
    }
}

/// Reads events until `count` of them came, and checks that they are the numbers `first`,
/// `first + 1`, and so on.
async fn expect_run(c: &mut Client, subscription: &str, first: u64, count: u64) {
    let mut next = first;
    while next < first + count {
        let line = c.recv().await;
        if line["method"] != "events.event" {
            continue;
        }
        assert_eq!(line["params"]["subscription"], subscription);
        assert_eq!(line["params"]["event"]["seq"], next, "a gap or a repeat");
        next += 1;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_burst_of_10000_events_has_no_gap_and_no_repeat() {
    let h = Harness::start();
    let mut emitter = client(&h).await;
    let mut live = client(&h).await;
    let mut slow = client(&h).await;
    let mut mid = client(&h).await;

    let reply = live.ok("events.subscribe", json!({})).await;
    let (live_id, cursor) = (
        reply["subscription"].as_str().expect("an id").to_owned(),
        reply["cursor"].clone(),
    );
    let start = cursor["seq"].as_u64().expect("a seq");
    let reply = slow
        .ok("events.subscribe", json!({"cursor": cursor.clone()}))
        .await;
    let slow_id = reply["subscription"].as_str().expect("an id").to_owned();

    // The burst. `live` and `slow` do not read while it runs, so their queues fill, the tasks wait,
    // and the broadcast channel (4,096 events) loses events. The tasks must read the rest from the
    // database.
    let burst = tokio::spawn(async move {
        emitter
            .ok("debug.emit_events", json!({"count": 10_000, "batch": 50}))
            .await;
        emitter
    });
    // A third client subscribes in the middle of the burst, with the old cursor.
    tokio::time::sleep(Duration::from_millis(20)).await;
    let reply = mid.ok("events.subscribe", json!({"cursor": cursor})).await;
    let mid_id = reply["subscription"].as_str().expect("an id").to_owned();
    let emitter = burst.await.expect("the burst ends");

    expect_run(&mut live, &live_id, start + 1, 10_000).await;
    expect_run(&mut slow, &slow_id, start + 1, 10_000).await;
    expect_run(&mut mid, &mid_id, start + 1, 10_000).await;

    // After the burst the live subscriptions go on.
    let mut emitter = emitter;
    emitter.ok("debug.emit_events", json!({"count": 5})).await;
    expect_run(&mut live, &live_id, start + 10_001, 5).await;
    expect_run(&mut mid, &mid_id, start + 10_001, 5).await;
    assert!(
        live.try_recv(Duration::from_millis(200)).await.is_none(),
        "no repeat after the end"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn projection_versions_gives_the_version_of_each_projection_that_exists() {
    let h = Harness::start();
    let mut c = client(&h).await;
    c.request("projection.subscribe", json!({"name": "settings"}))
        .await;
    let _reply = c.recv().await;
    let snapshot = c.recv().await;
    let version = snapshot["params"]["message"]["version"]
        .as_u64()
        .expect("a version");
    // The version of a projection that exists, and nothing for one that the core has not made.
    let result = c
        .ok(
            "projection.versions",
            json!({"names": ["settings", "timeline:01ARZ3NDEKTSV4RRFFQ69G5FAV"]}),
        )
        .await;
    assert_eq!(result["versions"]["settings"], json!(version));
    assert!(
        result["versions"]
            .get("timeline:01ARZ3NDEKTSV4RRFFQ69G5FAV")
            .is_none(),
        "{result}"
    );
    // A change moves the version on.
    c.ok("config.set", json!({"key": "agents.shims", "value": false}))
        .await;
    let patch = until(&mut c, "a patch of settings", |line| {
        line["method"] == "projection.message" && line["params"]["message"]["type"] == "patch"
    })
    .await;
    let after = patch["params"]["message"]["version"]
        .as_u64()
        .expect("a version");
    let result = c
        .ok("projection.versions", json!({"names": ["settings"]}))
        .await;
    assert_eq!(result["versions"]["settings"], json!(after));
    // A name that is not a projection is a bad param.
    let reply = c
        .call("projection.versions", json!({"names": ["nope"]}))
        .await;
    assert_eq!(error_of(&reply).1, "invalid_params", "{reply}");
}
