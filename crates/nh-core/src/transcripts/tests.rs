use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use nh_agents::transcript::Billing;
use nh_proto::api::{common::Harness, model::TimelineBody, time::Timestamp};
use serde_json::{Value, json};

use super::{
    calls::{ITEM_CAP, render_item},
    day, prices,
    tail::{Batch, RING, Tail, TimelineUpdate, all_items, item_text},
};

const NOW: Timestamp = Timestamp(1_791_500_800_000);

fn dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("nh-transcripts-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("a directory");
    dir
}

fn append(path: &PathBuf, text: &str) {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .expect("a file");
    file.write_all(text.as_bytes()).expect("a write");
}

fn line(value: Value) -> String {
    format!("{value}\n")
}

fn prompt(uuid: &str, text: &str) -> String {
    line(
        json!({"type": "user", "uuid": uuid, "timestamp": "2026-10-08T23:05:05.000Z",
        "message": {"content": text}}),
    )
}

fn message(uuid: &str, id: &str, input: u64, output: u64, blocks: Value) -> String {
    line(
        json!({"type": "assistant", "uuid": uuid, "timestamp": "2026-10-08T23:05:06.000Z",
        "message": {"id": id, "model": "claude-opus-5-5", "content": blocks,
            "usage": {"input_tokens": input, "output_tokens": output}}}),
    )
}

fn tail(path: &Path) -> Tail {
    Tail::new(
        Harness::Claude,
        path.to_path_buf(),
        0,
        Tail::ledger_from(None),
        prices(),
    )
}

fn tokens(batch: &Batch) -> (u64, u64) {
    batch.deltas.iter().fold((0, 0), |sum, d| {
        (sum.0 + d.tokens.input, sum.1 + d.tokens.output)
    })
}

#[test]
fn a_file_that_is_not_there_yet_gives_nothing() {
    let path = dir("missing").join("t.jsonl");
    assert!(tail(&path).poll(false, NOW).is_none());
}

#[test]
fn a_poll_counts_the_new_lines_and_a_line_that_is_not_finished_waits() {
    let path = dir("partial").join("t.jsonl");
    let mut tail = tail(&path);
    append(&path, &prompt("u1", "hi"));
    append(
        &path,
        &message("a1", "m1", 100, 10, json!([{"type": "text", "text": "ok"}])),
    );
    // The harness is in the middle of a line.
    let half = message("a2", "m2", 50, 5, json!([]));
    let (first, rest) = half.split_at(half.len() / 2);
    append(&path, first);
    let batch = tail.poll(false, NOW).expect("a batch");
    assert_eq!(tokens(&batch), (100, 10));
    assert!(batch.deltas[0].cost_usd.is_some());
    let end_of_two_lines = fs::read_to_string(&path)
        .expect("text")
        .find(first)
        .expect("at");
    assert_eq!(batch.offset, end_of_two_lines as u64);
    // The poll again sees no change.
    assert!(tail.poll(false, NOW).is_none());
    append(&path, rest);
    let batch = tail.poll(false, NOW).expect("a batch");
    assert_eq!(tokens(&batch), (50, 5));
}

#[test]
fn a_tail_that_goes_on_from_the_saved_state_does_not_count_a_message_again() {
    let path = dir("restart").join("t.jsonl");
    let mut one = tail(&path);
    append(
        &path,
        &message("a1", "m1", 100, 1, json!([{"type": "text", "text": "a"}])),
    );
    let batch = one.poll(false, NOW).expect("a batch");
    let (offset, ledger) = (batch.offset, batch.ledger_json.expect("a state"));
    drop(one);
    // The core restarted. The harness writes the next block of the same message, and a new one.
    append(
        &path,
        &message("a2", "m1", 100, 30, json!([{"type": "text", "text": "b"}])),
    );
    append(&path, &message("a3", "m2", 7, 7, json!([])));
    let mut two = Tail::new(
        Harness::Claude,
        path.clone(),
        offset,
        Tail::ledger_from(Some(&ledger)),
        prices(),
    );
    let batch = two.poll(false, NOW).expect("a batch");
    // The message m1 grew by 29 tokens of output and has no new input.
    assert_eq!(tokens(&batch), (7, 36));
}

#[test]
fn a_file_that_gets_shorter_is_read_again_and_the_count_does_not_double() {
    let path = dir("replaced").join("t.jsonl");
    let mut tail = tail(&path);
    append(&path, &message("a1", "m1", 100, 10, json!([])));
    assert_eq!(tokens(&tail.poll(false, NOW).expect("a batch")), (100, 10));
    fs::write(&path, message("b1", "m1", 100, 10, json!([]))).expect("a new file");
    let batch = tail.poll(false, NOW);
    // It is the same message in a new file. The ledger remembers it.
    assert!(batch.is_none_or(|b| b.deltas.is_empty()));
}

#[test]
fn the_cost_of_the_harness_ends_in_the_batch_as_a_correction() {
    let path = dir("cost").join("t.jsonl");
    let mut tail = tail(&path);
    append(&path, &message("a1", "m1", 1_000_000, 0, json!([])));
    append(
        &path,
        &line(json!({"type": "cost-state", "sessionId": "s",
            "modelUsage": {"claude-opus-5-5": {"costUSD": 4.5}}})),
    );
    let batch = tail.poll(false, NOW).expect("a batch");
    let cost: f64 = batch.deltas.iter().filter_map(|d| d.cost_usd).sum();
    assert!((cost - 4.5).abs() < 1e-9, "{cost}");
}

#[test]
fn a_subscription_billing_gives_tokens_and_no_dollars() {
    let path = dir("subscription").join("t.jsonl");
    let mut tail = tail(&path);
    tail.set_billing(Billing::Subscription);
    append(&path, &message("a1", "m1", 100, 10, json!([])));
    let batch = tail.poll(false, NOW).expect("a batch");
    assert!(batch.deltas.iter().all(|d| d.cost_usd.is_none()));
}

#[test]
fn a_tool_result_is_an_end_of_a_call_with_its_outcome() {
    let path = dir("ends").join("t.jsonl");
    let mut tail = tail(&path);
    append(
        &path,
        &line(
            json!({"type": "user", "uuid": "r1", "timestamp": "2026-10-08T23:05:07.000Z",
            "message": {"content": [
                {"type": "tool_result", "tool_use_id": "t1", "content": "fine"},
                {"type": "tool_result", "tool_use_id": "t2", "content": "bad", "is_error": true}]}}),
        ),
    );
    let batch = tail.poll(false, NOW).expect("a batch");
    let ends: Vec<_> = batch
        .tool_ends
        .iter()
        .map(|e| (e.call.as_str(), e.ok))
        .collect();
    assert_eq!(ends, vec![("t1", true), ("t2", false)]);
}

#[test]
fn codex_rate_limits_come_with_the_batch() {
    let path = dir("limits").join("rollout.jsonl");
    let mut tail = Tail::new(
        Harness::Codex,
        path.clone(),
        0,
        Tail::ledger_from(None),
        prices(),
    );
    append(
        &path,
        &line(
            json!({"type": "event_msg", "ordinal": 3, "payload": {"type": "token_count",
            "rate_limits": {"primary": {"used_percent": 61.0, "window_minutes": 300}}}}),
        ),
    );
    let batch = tail.poll(false, NOW).expect("a batch");
    let windows = batch.limits.expect("windows");
    assert_eq!(
        (windows[0].name.as_str(), windows[0].used_percent),
        ("5h", 61.0)
    );
}

fn ids(update: &TimelineUpdate) -> Vec<String> {
    match update {
        TimelineUpdate::Set { order, .. } => order.clone(),
        TimelineUpdate::Drop => Vec::new(),
    }
}

#[test]
fn the_timeline_is_built_from_the_file_when_a_window_follows_it_and_dropped_after() {
    let path = dir("timeline").join("t.jsonl");
    let mut tail = tail(&path);
    append(&path, &prompt("u1", "first"));
    append(
        &path,
        &message(
            "a1",
            "m1",
            1,
            1,
            json!([{"type": "tool_use", "id": "t1", "name": "Bash", "input": {"command": "ls"}}]),
        ),
    );
    // Nobody follows the timeline: nothing is kept.
    let batch = tail.poll(false, NOW).expect("a batch");
    assert!(batch.timeline.is_none());
    // A window follows it. The file up to the place of the count is read again.
    let batch = tail.poll(true, NOW).expect("a batch");
    let Some(TimelineUpdate::Set { reset, upserts, .. }) = &batch.timeline else {
        panic!("{:?}", batch.timeline);
    };
    assert!(*reset);
    assert_eq!(upserts.len(), 2, "a prompt and a tool");
    // New lines add to it and change the item of the call.
    append(
        &path,
        &line(
            json!({"type": "user", "uuid": "r1", "timestamp": "2026-10-08T23:05:09.000Z",
            "message": {"content": [{"type": "tool_result", "tool_use_id": "t1", "content": "a.txt"}]}}),
        ),
    );
    let batch = tail.poll(true, NOW).expect("a batch");
    let Some(TimelineUpdate::Set {
        reset,
        upserts,
        order,
        ..
    }) = &batch.timeline
    else {
        panic!("{:?}", batch.timeline);
    };
    assert!(!*reset);
    assert_eq!(
        order,
        &vec!["u1".to_owned(), "t1".to_owned(), "t1.out".to_owned()]
    );
    assert!(upserts.iter().any(|i| i.id == "t1"
        && matches!(
            &i.body,
            TimelineBody::Tool {
                duration_ms: Some(_),
                ..
            }
        )));
    // The window goes.
    let batch = tail.poll(false, NOW).expect("a batch");
    assert_eq!(batch.timeline, Some(TimelineUpdate::Drop));
}

#[test]
fn the_timeline_keeps_the_latest_five_hundred_items() {
    let path = dir("ring").join("t.jsonl");
    let mut tail = tail(&path);
    let mut text = String::new();
    for index in 0..(RING + 40) {
        text += &prompt(&format!("u{index}"), "p");
    }
    append(&path, &text);
    let batch = tail.poll(true, NOW).expect("a batch");
    let update = batch.timeline.expect("an update");
    let order = ids(&update);
    assert_eq!(order.len(), RING);
    assert_eq!(order[0], "u40");
    let TimelineUpdate::Set { evicted, .. } = &update else {
        panic!("a set");
    };
    assert_eq!(evicted.len(), 40);
}

#[test]
fn all_items_reads_the_whole_file_in_order_for_the_pages() {
    let path = dir("all").join("t.jsonl");
    for index in 0..30 {
        append(&path, &prompt(&format!("u{index}"), "p"));
    }
    let items = all_items(&path, nh_agents::transcript::Format::Claude);
    assert_eq!(items.len(), 30);
    assert_eq!(items[0].id, "u0");
    assert_eq!(items[29].turn, 30);
    assert!(
        all_items(
            &path.with_extension("none"),
            nh_agents::transcript::Format::Claude
        )
        .is_empty()
    );
}

// --- timeline.item ------------------------------------------------------------

#[test]
fn an_item_gives_the_full_text_of_a_prompt_or_of_a_text_of_the_model() {
    let path = dir("item").join("t.jsonl");
    let long = "word ".repeat(400);
    append(&path, &prompt("u1", &long));
    append(
        &path,
        &message(
            "a1",
            "m1",
            1,
            1,
            json!([
                {"type": "tool_use", "id": "t1", "name": "Bash", "input": {"command": "ls"}},
                {"type": "text", "text": long.clone()},
            ]),
        ),
    );
    let format = nh_agents::transcript::Format::Claude;
    // The timeline has the ids, and a preview only.
    let items = all_items(&path, format);
    let reply = items
        .iter()
        .find(|item| matches!(item.body, TimelineBody::Assistant { .. }))
        .expect("a text of the model");
    assert_eq!(reply.id, "a1.1");
    assert_eq!(
        item_text(&path, format, "u1").as_deref(),
        Some(long.as_str())
    );
    assert_eq!(
        item_text(&path, format, "a1.1").as_deref(),
        Some(long.as_str())
    );
    // The call of a tool and an id that is not there have no text.
    assert_eq!(item_text(&path, format, "a1.0"), None);
    assert_eq!(item_text(&path, format, "a1.x"), None);
    assert_eq!(item_text(&path, format, "u9"), None);
    assert_eq!(item_text(&path.with_extension("none"), format, "u1"), None);
}

#[test]
fn the_html_of_an_item_is_sanitized_and_cut_at_the_cap() {
    let shown = render_item("**done** <script>alert(1)</script> [x](javascript:alert(1))");
    assert!(
        shown.html.contains("<strong>done</strong>"),
        "{}",
        shown.html
    );
    assert!(!shown.html.contains("<script"), "{}", shown.html);
    assert!(!shown.html.contains("javascript:"), "{}", shown.html);
    assert!(!shown.truncated);
    // A cut never splits a character.
    let long = "é".repeat(ITEM_CAP);
    let cut = render_item(&long);
    assert!(cut.truncated);
    assert!(cut.html.len() < long.len());
}

#[test]
fn the_day_is_the_utc_date_of_the_moment() {
    assert_eq!(day(Timestamp(1_791_500_705_581)), "2026-10-08");
    assert_eq!(day(Timestamp(0)), "1970-01-01");
}
