//! The scrollback stream, the peek stream and the `pane_text` projection.

mod core_support;

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use core_support::{Harness, TIMEOUT, client::Client, wait_for};
use nh_core::{ErrorCode, FrameSink};
use nh_proto::{
    api::streams::ScrollbackPage,
    cells::unpack_cell,
    frame::{Frame, RowUpdate},
};
use nh_term::client_grid::ClientGrid;
use serde_json::{Value, json};
use tokio::{sync::mpsc, time::timeout};
use ulid::Ulid;

async fn client(h: &Harness) -> Client {
    Client::connect(&h.api_socket()).await
}

/// A sink that keeps the bodies, and says that its receiver is gone when asked.
struct Bytes {
    tx: mpsc::UnboundedSender<Vec<u8>>,
    gone: AtomicBool,
}

impl FrameSink for Bytes {
    fn send(&self, body: Vec<u8>) -> bool {
        !self.gone.load(Ordering::Relaxed) && self.tx.send(body).is_ok()
    }

    fn is_closed(&self) -> bool {
        self.gone.load(Ordering::Relaxed) || self.tx.is_closed()
    }
}

fn new_sink() -> (Arc<Bytes>, mpsc::UnboundedReceiver<Vec<u8>>) {
    let (tx, rx) = mpsc::unbounded_channel();
    (
        Arc::new(Bytes {
            tx,
            gone: AtomicBool::new(false),
        }),
        rx,
    )
}

/// The text of a row of a page: the glyphs of its cells.
fn row_text(row: &RowUpdate) -> String {
    row.cells
        .as_chunks::<6>()
        .0
        .iter()
        .map(|cell| char::from_u32(unpack_cell(*cell).0).unwrap_or('?'))
        .collect::<String>()
        .trim_end()
        .to_owned()
}

fn ulid(value: &Value) -> Ulid {
    value.as_str().expect("an id").parse().expect("a ULID")
}

/// A pane that prints the numbers 1 to `last` and waits.
async fn numbered(h: &Harness, last: u32) -> (Value, Arc<nh_core::pane::Pane>) {
    let mut c = client(h).await;
    let created = c
        .ok(
            "workspace.create",
            json!({ "command": { "argv": ["/bin/sh", "-c", format!("seq 1 {last}; exec sleep 60")] } }),
        )
        .await;
    let id = ulid(&created["pane"]);
    let deadline = Instant::now() + TIMEOUT;
    let pane = loop {
        if let Some(pane) = h.handle.pane(id) {
            break pane;
        }
        assert!(Instant::now() < deadline, "no pane");
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    wait_for(&pane, "the last number", |t| {
        t.iter().any(|row| *row == last.to_string())
    })
    .await;
    (created, pane)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_scrollback_stream_sends_the_lines_of_the_archive_in_pages_and_an_empty_page_ends_it() {
    let h = Harness::start();
    let (created, pane) = numbered(&h, 600).await;
    let (sink, mut rx) = new_sink();
    h.handle
        .stream(
            "scrollback",
            json!({ "pane": created["pane"], "from_line": 10, "count": 250 }),
            sink,
        )
        .expect("the stream opens");

    let mut pages: Vec<ScrollbackPage> = Vec::new();
    loop {
        let body = timeout(TIMEOUT, rx.recv())
            .await
            .expect("a page in time")
            .expect("the stream stays open");
        let page: ScrollbackPage = postcard::from_bytes(&body).expect("a page");
        let last = page.lines.is_empty();
        pages.push(page);
        if last {
            break;
        }
    }
    // 250 lines come in a page of 200 and a page of 50, then the empty page.
    let sizes: Vec<usize> = pages.iter().map(|p| p.lines.len()).collect();
    assert_eq!(sizes, [200, 50, 0]);
    assert_eq!(
        pages.iter().map(|p| p.from_line).collect::<Vec<_>>(),
        [10, 210, 260]
    );
    assert!(pages.iter().all(|p| p.pane == pane.render_id()));
    // Line n has the number n + 1, and the row number is relative to the page.
    for page in &pages {
        for row in &page.lines {
            let line = page.from_line + u64::from(row.y);
            assert_eq!(row_text(row), (line + 1).to_string());
        }
    }

    // A range that goes beyond the archive stops where the archive stops, and the empty page says
    // where that is.
    let total = pane.viewport().total_lines - u64::from(pane.size().1);
    let (sink, mut rx) = new_sink();
    h.handle
        .stream(
            "scrollback",
            json!({ "pane": created["pane"], "from_line": total - 5, "count": 100 }),
            sink,
        )
        .expect("the stream opens");
    let body = timeout(TIMEOUT, rx.recv())
        .await
        .expect("in time")
        .expect("a page");
    let page: ScrollbackPage = postcard::from_bytes(&body).expect("a page");
    assert_eq!(page.lines.len(), 5);
    let body = timeout(TIMEOUT, rx.recv())
        .await
        .expect("in time")
        .expect("a page");
    let end: ScrollbackPage = postcard::from_bytes(&body).expect("a page");
    assert!(end.lines.is_empty());
    assert_eq!(end.from_line, total);

    // A pane that does not exist, and a handle where an id is needed.
    let (sink, _rx) = new_sink();
    let error = h
        .handle
        .stream(
            "scrollback",
            json!({ "pane": "01ARZ3NDEKTSV4RRFFQ69G5FAV", "from_line": 0, "count": 1 }),
            sink.clone(),
        )
        .expect_err("no such pane");
    assert_eq!(error.code, ErrorCode::NotFound);
    let error = h
        .handle
        .stream(
            "scrollback",
            json!({ "pane": "w1:p1", "from_line": 0, "count": 1 }),
            sink,
        )
        .expect_err("a handle");
    assert_eq!(error.code, ErrorCode::InvalidParams);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_peek_stream_sends_the_bottom_rows_and_left_columns_at_a_low_rate() {
    let h = Harness::start();
    let mut c = client(&h).await;
    // A program that changes the screen about a hundred times a second for a few seconds.
    let counter =
        "i=0; while [ $i -lt 400 ]; do echo number-$i; i=$((i+1)); sleep 0.01; done; sleep 60";
    let created = c
        .ok(
            "workspace.create",
            json!({ "command": { "argv": ["/bin/sh", "-c", counter] } }),
        )
        .await;
    let pane = loop {
        if let Some(pane) = h.handle.pane(ulid(&created["pane"])) {
            break pane;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    let (sink, mut rx) = new_sink();
    h.handle
        .stream(
            "peek",
            json!({ "pane": created["pane"], "max_hz": 100.0, "max_cols": 8, "max_rows": 3 }),
            sink.clone(),
        )
        .expect("the stream opens");

    // The frames of 1.3 seconds: at most 4 a second, never a queue, each cropped.
    let started = Instant::now();
    let mut frames: Vec<Frame> = Vec::new();
    while started.elapsed() < Duration::from_millis(1300) {
        if let Ok(Some(body)) = timeout(Duration::from_millis(100), rx.recv()).await {
            frames.push(postcard::from_bytes(&body).expect("a frame"));
        }
    }
    assert!(
        frames.len() >= 2,
        "the peek follows the pane: {}",
        frames.len()
    );
    assert!(
        frames.len() <= 7,
        "at most 4 frames a second, and the first one at once: {}",
        frames.len()
    );
    for frame in &frames {
        assert_eq!((frame.cols, frame.rows), (8, 3));
        assert_eq!(frame.kind, nh_proto::frame::FrameKind::Keyframe);
        assert!(
            frame
                .lines
                .iter()
                .all(|row| row.y < 3 && row.cells.len() == 8 * 6)
        );
    }
    // A peek frame shows the bottom of the screen: its rows are the last three rows of the
    // pane, cut to 8 columns.
    let mut grid = ClientGrid::new();
    grid.apply(frames.last().expect("a frame"))
        .expect("the frame applies");
    let shown: Vec<String> = grid
        .dump()
        .expect("a dump")
        .rows
        .iter()
        .map(|row| {
            row.iter()
                .map(|(text, _)| text.as_str())
                .collect::<String>()
        })
        .collect();
    assert_eq!(shown.len(), 3);
    assert!(shown.iter().all(|row| row.trim_end().is_empty()
        || row.starts_with("number-")
        || row.starts_with("number")));

    // `pane.peek_close` ends the stream.
    h.call("pane.peek_close", json!({ "pane": created["pane"] }))
        .await;
    tokio::time::sleep(Duration::from_millis(400)).await;
    while rx.try_recv().is_ok() {}
    tokio::time::sleep(Duration::from_millis(600)).await;
    assert!(rx.try_recv().is_err(), "no frame after the close");
    // A stream whose receiver is gone ends without a frame.
    sink.gone.store(true, Ordering::Relaxed);
    drop(pane);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_text_mirror_follows_the_rows_of_a_pane_only_while_someone_reads_it() {
    let h = Harness::start();
    let mut c = client(&h).await;
    let flag = h.dir.join("go");
    let script = format!(
        "echo first-line; while [ ! -e {} ]; do sleep 0.05; done; echo changed-line; exec sleep 60",
        flag.display()
    );
    let created = c
        .ok(
            "workspace.create",
            json!({ "command": { "argv": ["/bin/sh", "-c", script] } }),
        )
        .await;
    let pane = loop {
        if let Some(pane) = h.handle.pane(ulid(&created["pane"])) {
            break pane;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    wait_for(&pane, "the first line", |t| {
        t.iter().any(|row| row == "first-line")
    })
    .await;

    let name = format!("pane_text:{}", created["pane"].as_str().expect("id"));
    let reply = c.ok("projection.subscribe", json!({ "name": name })).await;
    let id = reply["subscription"].as_str().expect("an id").to_owned();
    // The mirror fills at the next look, a quarter of a second at most.
    let mut rows: Vec<String> = Vec::new();
    let deadline = Instant::now() + TIMEOUT;
    while !rows.iter().any(|row| row == "first-line") {
        assert!(Instant::now() < deadline, "the rows are {rows:?}");
        let line = c.recv().await;
        if line["method"] != "projection.message" || line["params"]["subscription"] != id.as_str() {
            continue;
        }
        let message = &line["params"]["message"];
        match message["type"].as_str() {
            Some("snapshot") => {
                let data = &message["data"];
                rows = data["order"]
                    .as_array()
                    .expect("order")
                    .iter()
                    .map(|k| {
                        data["items"][k.as_str().expect("key")]
                            .as_str()
                            .unwrap_or("")
                            .to_owned()
                    })
                    .collect();
            }
            Some("patch") => {
                for op in message["ops"].as_array().expect("ops") {
                    if op["op"] == "set" && op["key"] == "0" {
                        rows = vec![op["value"].as_str().expect("text").to_owned()];
                    }
                }
            }
            _ => {}
        }
    }
    assert_eq!(rows[0], "first-line");

    // A change of the screen reaches the follower in one patch.
    std::fs::write(&flag, b"").expect("the flag file");
    let deadline = Instant::now() + TIMEOUT;
    loop {
        assert!(Instant::now() < deadline, "no patch for the new row");
        let line = c.recv().await;
        if line["method"] != "projection.message" || line["params"]["subscription"] != id.as_str() {
            continue;
        }
        let message = &line["params"]["message"];
        if message["type"] == "patch"
            && message["ops"]
                .as_array()
                .expect("ops")
                .iter()
                .any(|op| op["op"] == "set" && op["value"] == "changed-line")
        {
            break;
        }
    }

    // Nobody follows after the unsubscribe: the hub has no subscriber, so the task skips the pane.
    c.ok("projection.unsubscribe", json!({ "subscription": id }))
        .await;
    let projection: nh_proto::api::projections::Projection = name.parse().expect("a name");
    assert_eq!(h.handle.projections().subscriber_count(projection), 0);
}
