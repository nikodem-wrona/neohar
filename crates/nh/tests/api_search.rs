//! `search.query` against the real core: the lines that left the screen of a pane
//! are in the archive, and the search finds them with the range of the match.

mod core_support;

use std::time::{Duration, Instant};

use core_support::{Harness, TIMEOUT, client::Client, wait_for};
use nh_core::ErrorCode;
use serde_json::{Value, json};
use ulid::Ulid;

/// A pane that prints lines, so that most of them leave the screen, and waits.
async fn printing(h: &Harness, name: &str, script: &str) -> Value {
    let mut c = Client::connect(&h.api_socket()).await;
    let created = c
        .ok(
            "workspace.create",
            json!({ "name": name, "command": { "argv": ["/bin/sh", "-c", script] } }),
        )
        .await;
    let id: Ulid = created["pane"].as_str().expect("id").parse().expect("ulid");
    let deadline = Instant::now() + TIMEOUT;
    let pane = loop {
        if let Some(pane) = h.handle.pane(id) {
            break pane;
        }
        assert!(Instant::now() < deadline, "no pane");
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    wait_for(&pane, "the end", |t| t.iter().any(|row| row == "done")).await;
    created
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_search_finds_lines_that_left_the_screen_with_the_range_of_the_match() {
    let h = Harness::start();
    let a = printing(
        &h,
        "A",
        "echo 'migration failed: no such column [a]'; seq 1 120; echo 'the deploy [ok]'; seq 1 60; echo done; exec sleep 60",
    )
    .await;
    let b = printing(
        &h,
        "B",
        "seq 1 50; echo 'second migration failed: timeout'; seq 1 60; echo done; exec sleep 60",
    )
    .await;

    // All panes: the best match first, each with the pane and the absolute line.
    let all = h
        .call("search.query", json!({ "query": "migration failed" }))
        .await;
    let hits = all["hits"].as_array().expect("hits");
    assert_eq!(hits.len(), 2, "{all}");
    assert_eq!(all["truncated"], false);
    let in_a = hits
        .iter()
        .find(|hit| hit["pane"] == a["pane"])
        .expect("a hit in A");
    assert_eq!(in_a["line"], 0);
    // The text is the snippet without marks, and the ranges are the matches.
    let text = in_a["text"]["text"].as_str().expect("text");
    assert!(
        text.starts_with("migration failed: no such column [a]"),
        "{text}"
    );
    let first = &in_a["text"]["spans"][0];
    assert_eq!(first[2], "match");
    let (from, to) = (
        first[0].as_u64().expect("from"),
        first[1].as_u64().expect("to"),
    );
    assert_eq!(&text[from as usize..to as usize], "migration");

    // A bracket in the text is only text.
    let bracket = h.call("search.query", json!({ "query": "deploy" })).await;
    let text = bracket["hits"][0]["text"]["text"].as_str().expect("text");
    assert!(text.contains("[ok]"), "{text}");

    // A scope of one pane, and of one workspace.
    let only_b = h
        .call(
            "search.query",
            json!({ "query": "migration", "pane": b["pane"] }),
        )
        .await;
    assert_eq!(only_b["hits"].as_array().expect("hits").len(), 1);
    assert_eq!(only_b["hits"][0]["pane"], b["pane"]);
    let workspace = h
        .call(
            "search.query",
            json!({ "query": "migration", "workspace": a["workspace"] }),
        )
        .await;
    assert_eq!(workspace["hits"].as_array().expect("hits").len(), 1);
    assert_eq!(workspace["hits"][0]["pane"], a["pane"]);

    // A limit cuts the answer and says so.
    let cut = h
        .call("search.query", json!({ "query": "migration", "limit": 1 }))
        .await;
    assert_eq!(cut["hits"].as_array().expect("hits").len(), 1);
    assert_eq!(cut["truncated"], true);

    // No match is an empty answer, and a query with no words is too.
    let none = h
        .call("search.query", json!({ "query": "nothing like it" }))
        .await;
    assert_eq!(none["hits"], json!([]));
    let blank = h.call("search.query", json!({ "query": "   " })).await;
    assert_eq!(blank["hits"], json!([]));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_scope_that_does_not_exist_or_two_scopes_are_errors() {
    let h = Harness::start();
    let a = printing(&h, "A", "seq 1 60; echo done; exec sleep 60").await;
    let missing = h
        .handle
        .rpc("search.query", json!({ "query": "x", "pane": "w9:p9" }))
        .await
        .expect_err("no such pane");
    assert_eq!(missing.code, ErrorCode::NotFound);
    let both = h
        .handle
        .rpc(
            "search.query",
            json!({ "query": "x", "pane": a["pane"], "workspace": a["workspace"] }),
        )
        .await
        .expect_err("two scopes");
    assert_eq!(both.code, ErrorCode::InvalidParams);
}
