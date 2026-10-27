//! Scrolling and reading a pane: `pane.scroll`, the wheel, `pane.read`, and the frames
//! that follow the viewport.

mod core_support;

use std::time::{Duration, Instant};

use core_support::{Frames, Harness, TIMEOUT, client::Client, wait_for};
use serde_json::{Value, json};
use ulid::Ulid;

async fn client(h: &Harness) -> Client {
    Client::connect(&h.api_socket()).await
}

/// A pane that prints the numbers 1 to 200 and waits. The screen has 24 rows, so 176 or more
/// lines have left it when the numbers end.
async fn numbered(h: &Harness, c: &mut Client) -> (Value, std::sync::Arc<nh_core::pane::Pane>) {
    let created = c
        .ok(
            "workspace.create",
            json!({ "command": { "argv": ["/bin/sh", "-c", "seq 1 200; exec sleep 60"] } }),
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
    wait_for(&pane, "the last number", |t| {
        t.iter().any(|row| row == "200")
    })
    .await;
    (created, pane)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scrolling_moves_the_viewport_inside_the_history_and_back_to_the_end() {
    let h = Harness::start();
    let mut c = client(&h).await;
    let (created, pane) = numbered(&h, &mut c).await;
    let end = pane.viewport();
    // The rows of the screen are the lines `top_line..top_line + 24`, and line n has the number
    // n + 1.
    assert_eq!(pane.text()[0], (end.top_line + 1).to_string());

    h.call(
        "pane.scroll",
        json!({ "pane": created["pane"], "delta": -10 }),
    )
    .await;
    let up = pane.viewport();
    assert_eq!(up.top_line, end.top_line - 10);
    assert_eq!(up.total_lines, end.total_lines);
    assert_eq!(pane.text()[0], (up.top_line + 1).to_string());

    // An absolute line, and a line above the history stops at the oldest line that is in memory.
    h.call("pane.scroll", json!({ "pane": "w1:p1", "top_line": 50 }))
        .await;
    assert_eq!(pane.viewport().top_line, 50);
    assert_eq!(pane.text()[0], "51");
    h.call("pane.scroll", json!({ "pane": "w1:p1", "delta": -100000 }))
        .await;
    assert_eq!(pane.viewport().top_line, 0);

    // A move below the end shows the live screen again.
    h.call("pane.scroll", json!({ "pane": "w1:p1", "delta": 100000 }))
        .await;
    assert_eq!(pane.viewport(), end);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_wheel_scrolls_three_lines_for_each_notch_when_the_program_ignores_it() {
    let h = Harness::start();
    let mut c = client(&h).await;
    let (created, pane) = numbered(&h, &mut c).await;
    let end = pane.viewport();
    let wheel = |notches: i64| {
        json!({ "pane": created["pane"], "input_id": 1000 + notches.abs(),
            "event": { "type": "mouse", "kind": "wheel", "col": 5, "row": 5, "wheel_y": notches } })
    };
    h.call("pane.input", wheel(-2)).await;
    assert_eq!(pane.viewport().top_line, end.top_line - 6);
    h.call("pane.input", wheel(1)).await;
    assert_eq!(pane.viewport().top_line, end.top_line - 3);
    // Nothing went to the program.
    assert_eq!(pane.pending_inputs(), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_frames_of_a_window_follow_the_viewport() {
    let h = Harness::start();
    let mut c = client(&h).await;
    let (created, pane) = numbered(&h, &mut c).await;
    let window = h.call("window.open", json!({})).await["window"].clone();
    let key = window.as_str().expect("id").to_owned();
    let mut frames = Frames::open(&h.handle, &key, 120.0);
    let first = frames.next(TIMEOUT).await.expect("a first frame");
    let end = pane.viewport();
    assert_eq!(first.viewport.top_line, end.top_line);
    assert_eq!(first.viewport.total_lines, end.total_lines);
    h.call(
        "render.ack",
        json!({ "window": window, "acks": [{ "pane": first.pane, "rev": first.rev }] }),
    )
    .await;

    h.call(
        "pane.scroll",
        json!({ "pane": created["pane"], "delta": -30 }),
    )
    .await;
    let scrolled = frames
        .next(TIMEOUT)
        .await
        .expect("a frame after the scroll");
    assert_eq!(scrolled.viewport.top_line, end.top_line - 30);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pane_read_gives_the_screen_and_the_lines_of_the_archive() {
    let h = Harness::start();
    let mut c = client(&h).await;
    let (created, pane) = numbered(&h, &mut c).await;
    let end = pane.viewport();

    // Without a range it reads what the viewport shows.
    let read = c.ok("pane.read", json!({ "pane": "w1:p1" })).await;
    assert_eq!(read["from_line"], json!(end.top_line));
    let lines: Vec<&str> = read["text"].as_str().expect("text").lines().collect();
    assert_eq!(
        lines.first().copied(),
        Some((end.top_line + 1).to_string().as_str())
    );
    assert_eq!(lines.last().copied(), Some("200"));

    // A range of lines that left the screen comes from the archive, which the pane flushes first.
    let read = c
        .ok(
            "pane.read",
            json!({ "pane": created["pane"], "lines": { "from": 10, "to": 15 } }),
        )
        .await;
    assert_eq!(
        read,
        json!({ "text": "11\n12\n13\n14\n15", "from_line": 10 })
    );

    // A range over the end of the archive goes on with the rows of the screen.
    let from = end.top_line - 2;
    let read = c
        .ok(
            "pane.read",
            json!({ "pane": "w1:p1", "lines": { "from": from, "to": from + 5 } }),
        )
        .await;
    let expected: Vec<String> = (from + 1..from + 6).map(|n| n.to_string()).collect();
    assert_eq!(read["text"], json!(expected.join("\n")));
    assert_eq!(read["from_line"], json!(from));

    // A range after the last line is empty.
    let read = c
        .ok(
            "pane.read",
            json!({ "pane": "w1:p1", "lines": { "from": 5000, "to": 5010 } }),
        )
        .await;
    assert_eq!(read, json!({ "text": "", "from_line": 5000 }));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pane_read_gives_a_wide_character_once() {
    let h = Harness::start();
    let mut c = client(&h).await;
    let created = c
        .ok(
            "workspace.create",
            json!({ "command": { "argv": ["/bin/sh", "-c", "printf 'a日本é\\r\\n'; exec sleep 60"] } }),
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
    wait_for(&pane, "the text", |t| t.iter().any(|row| row == "a日本é")).await;
    let read = c.ok("pane.read", json!({ "pane": "w1:p1" })).await;
    assert_eq!(
        read["text"].as_str().expect("text").lines().next(),
        Some("a日本é")
    );
}

fn host() -> std::sync::Arc<nh_core::RecordingHost> {
    std::sync::Arc::new(nh_core::RecordingHost::new(
        env!("CARGO_BIN_EXE_nh").into(),
        false,
    ))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_selection_is_in_the_frames_and_copy_writes_its_text_to_the_clipboard() {
    let recording = host();
    let h = Harness::start_hosted(recording.clone());
    let mut c = client(&h).await;
    let created = c
        .ok(
            "workspace.create",
            json!({ "command": { "argv": ["/bin/sh", "-c",
                "printf 'hello world\\r\\nsecond line'; exec sleep 60"] } }),
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
    wait_for(&pane, "the text", |t| t.iter().any(|r| r == "second line")).await;
    let window = h.call("window.open", json!({})).await["window"].clone();
    let mut frames = Frames::open(&h.handle, window.as_str().expect("id"), 120.0);
    let first = frames.next(TIMEOUT).await.expect("a first frame");
    assert_eq!(first.selection, None);
    let ack = |frame: &nh_proto::frame::Frame| json!({ "window": window, "acks": [{ "pane": frame.pane, "rev": frame.rev }] });
    h.call("render.ack", ack(&first)).await;

    // A click in the middle of a word selects the word. The frame has the widened points.
    h.call(
        "pane.select",
        json!({ "pane": "w1:p1", "anchor": { "col": 8, "line": 0 },
            "head": { "col": 8, "line": 0 }, "mode": "word" }),
    )
    .await;
    let frame = frames
        .next(TIMEOUT)
        .await
        .expect("a frame with the selection");
    let selection = frame.selection.expect("a selection");
    assert_eq!((selection.start.col, selection.end.col), (6, 10));
    assert_eq!((selection.start.line, selection.end.line), (0, 0));
    h.call("render.ack", ack(&frame)).await;

    // `pane.copy` is a method of the UI and of the CLI. The CLI asks for the text.
    let copied = c
        .ok("pane.copy", json!({ "pane": "w1:p1", "return_text": true }))
        .await;
    assert_eq!(copied, json!({ "text": "world" }));
    let quiet = c.ok("pane.copy", json!({ "pane": "w1:p1" })).await;
    assert_eq!(quiet, json!({}));
    let writes: Vec<String> = recording
        .calls()
        .into_iter()
        .filter_map(|call| match call {
            nh_core::host::HostCall::ClipboardWrite(text) => Some(text),
            _ => None,
        })
        .collect();
    assert_eq!(writes, ["world", "world"]);

    // Over two lines, in the mode of cells.
    h.call(
        "pane.select",
        json!({ "pane": "w1:p1", "anchor": { "col": 6, "line": 0 },
            "head": { "col": 5, "line": 1 }, "mode": "simple" }),
    )
    .await;
    let copied = c
        .ok("pane.copy", json!({ "pane": "w1:p1", "return_text": true }))
        .await;
    assert_eq!(copied, json!({ "text": "world\nsecond" }));

    // Clearing ends it in the frames, and a copy has nothing.
    h.call("pane.select_clear", json!({ "pane": "w1:p1" }))
        .await;
    let mut cleared = false;
    while let Some(frame) = frames.next(Duration::from_secs(2)).await {
        h.call("render.ack", ack(&frame)).await;
        if frame.selection.is_none() {
            cleared = true;
            break;
        }
    }
    assert!(cleared, "a frame without the selection");
    let reply = c.call("pane.copy", json!({ "pane": "w1:p1" })).await;
    assert_eq!(
        core_support::client::error_of(&reply).1,
        "not_found",
        "{reply}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_restarted_pane_numbers_its_lines_after_the_archive_of_the_run_before() {
    let h = Harness::start();
    let mut c = client(&h).await;
    let created = c
        .ok(
            "workspace.create",
            json!({ "command": { "argv": ["/bin/sh", "-c", "seq 1 200; sleep 3"] } }),
        )
        .await;
    let id: Ulid = created["pane"].as_str().expect("id").parse().expect("ulid");
    let deadline = Instant::now() + TIMEOUT;
    let first = loop {
        if let Some(pane) = h.handle.pane(id) {
            break pane;
        }
        assert!(Instant::now() < deadline, "no pane");
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    wait_for(&first, "the last number", |t| {
        t.iter().any(|row| row == "200")
    })
    .await;
    // The first run archived the lines `0..scrolled`. A read flushes the archive.
    let scrolled = first.viewport().top_line;
    c.ok(
        "pane.read",
        json!({ "pane": "w1:p1", "lines": { "from": 0, "to": 1 } }),
    )
    .await;

    // The program ends after its sleep. Then the pane can start again.
    let deadline = Instant::now() + TIMEOUT;
    loop {
        if c.call("pane.restart", json!({ "pane": "w1:p1" }))
            .await
            .get("error")
            .is_none()
        {
            break;
        }
        assert!(Instant::now() < deadline, "the pane did not end");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let deadline = Instant::now() + TIMEOUT;
    let second = loop {
        if let Some(pane) = h
            .handle
            .pane(id)
            .filter(|p| !std::sync::Arc::ptr_eq(p, &first))
        {
            break pane;
        }
        assert!(Instant::now() < deadline, "no new pane");
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    wait_for(&second, "the last number", |t| {
        t.iter().any(|row| row == "200")
    })
    .await;

    // The second run continues after the lines of the first one, and both stay readable.
    assert_eq!(second.viewport().top_line, 2 * scrolled);
    let read = c
        .ok(
            "pane.read",
            json!({ "pane": "w1:p1", "lines": { "from": scrolled, "to": scrolled + 3 } }),
        )
        .await;
    assert_eq!(read, json!({ "text": "1\n2\n3", "from_line": scrolled }));
    let read = c
        .ok(
            "pane.read",
            json!({ "pane": "w1:p1", "lines": { "from": 10, "to": 13 } }),
        )
        .await;
    assert_eq!(read, json!({ "text": "11\n12\n13", "from_line": 10 }));
}
