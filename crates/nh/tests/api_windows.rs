//! The windows of the layout and the render server: a window draws the panes that the
//! layout names, the host reports the state of a window, one window owns the size of a pane, and
//! the methods of the contract (`render.ack`, `window.focus_report`, `window.report_metrics`,
//! `pane.focus`, `pane.resize`) work with window ids. The dev WebSocket carries the calls, the
//! subscriptions and the render stream of a window.

mod core_support;

use std::time::{Duration, Instant};

use core_support::{
    Frames, Harness, TIMEOUT,
    client::{Client, error_of},
    wait_for,
};
use futures_util::{SinkExt, StreamExt};
use nh_core::{ErrorCode, Origin};
use nh_proto::{
    api::ids::WindowId,
    frame::{Frame, FrameKind},
};
use serde_json::{Value, json};
use tokio::{net::TcpStream, time::timeout};
use tokio_tungstenite::{WebSocketStream, client_async, tungstenite::Message};
use ulid::Ulid;

async fn client(h: &Harness) -> Client {
    Client::connect(&h.api_socket()).await
}

/// A program that prints a word and waits.
fn says(word: &str) -> Value {
    json!({ "argv": ["/bin/sh", "-c", format!("echo {word}; exec sleep 60")] })
}

/// The render ids of the panes whose frames come in `within`. The client acknowledges each frame.
async fn panes_seen(
    h: &Harness,
    window: &Value,
    frames: &mut Frames,
    within: Duration,
) -> Vec<u32> {
    let mut seen = Vec::new();
    let deadline = Instant::now() + within;
    while let Some(left) = deadline.checked_duration_since(Instant::now()) {
        match frames.next(left).await {
            Some(frame) => {
                if !seen.contains(&frame.pane) {
                    seen.push(frame.pane);
                }
                h.call(
                    "render.ack",
                    json!({ "window": window, "acks": [{ "pane": frame.pane, "rev": frame.rev }] }),
                )
                .await;
            }
            None => break,
        }
    }
    seen.sort_unstable();
    seen
}

fn window_id(value: &Value) -> WindowId {
    WindowId::new(
        value
            .as_str()
            .expect("an id")
            .parse::<Ulid>()
            .expect("a ULID"),
    )
}

/// The row of a window in the `windows` projection.
async fn window_row(c: &mut Client, window: &Value) -> Value {
    let reply = c
        .ok("projection.subscribe", json!({"name": "windows"}))
        .await;
    let id = reply["subscription"].as_str().expect("an id").to_owned();
    loop {
        let line = c.recv().await;
        if line["method"] == "projection.message"
            && line["params"]["subscription"] == id.as_str()
            && line["params"]["message"]["type"] == "snapshot"
        {
            let row =
                line["params"]["message"]["data"]["items"][window.as_str().expect("id")].clone();
            c.ok("projection.unsubscribe", json!({"subscription": id}))
                .await;
            return row;
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_window_draws_the_panes_that_the_layout_names() {
    let h = Harness::start();
    let mut c = client(&h).await;
    let created = c
        .ok("workspace.create", json!({ "command": says("first") }))
        .await;
    let window = h.call("window.open", json!({})).await["window"].clone();
    let mut frames = Frames::open(&h.handle, window.as_str().expect("id"), 120.0);

    // The first pane draws as a keyframe.
    let frame = frames.next(TIMEOUT).await.expect("a first frame");
    assert_eq!(frame.kind, FrameKind::Keyframe);
    let one = frame.pane;
    h.call(
        "render.ack",
        json!({ "window": window, "acks": [{ "pane": frame.pane, "rev": frame.rev }] }),
    )
    .await;

    // A split adds the second pane to the window, and a close takes it away again.
    let split = c
        .ok(
            "pane.split",
            json!({ "pane": "w1:p1", "direction": "right", "command": says("second") }),
        )
        .await;
    let seen = panes_seen(&h, &window, &mut frames, Duration::from_millis(800)).await;
    assert_eq!(seen.len(), 2, "both panes draw: {seen:?}");
    assert!(seen.contains(&one));
    let two = *seen
        .iter()
        .find(|pane| **pane != one)
        .expect("the second pane");
    let pane_two = h
        .handle
        .pane(split["pane"].as_str().expect("id").parse().expect("ulid"))
        .expect("the second pane runs");
    assert_eq!(pane_two.render_id(), two);

    c.ok("pane.close", json!({ "pane": "w1:p2", "force": true }))
        .await;
    // Whatever the second pane sends now is not for this window. The first pane has nothing new.
    tokio::time::sleep(Duration::from_millis(300)).await;
    panes_seen(&h, &window, &mut frames, Duration::from_millis(400)).await;
    h.text(
        &h.handle
            .pane(created["pane"].as_str().expect("id").parse().expect("ulid"))
            .expect("the first pane runs"),
        "x",
    )
    .await;
    let seen = panes_seen(&h, &window, &mut frames, Duration::from_millis(600)).await;
    assert_eq!(seen, [one], "only the first pane draws");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_stream_needs_a_window_of_the_layout() {
    let h = Harness::start_layout();
    let mut c = client(&h).await;
    let error = h
        .handle
        .stream(
            "render",
            json!({ "window": "01ARZ3NDEKTSV4RRFFQ69G5FAV" }),
            std::sync::Arc::new(NullSink),
        )
        .expect_err("no such window");
    assert_eq!(error.code, ErrorCode::NotFound);
    // A label is not the id of a window.
    let error = h
        .handle
        .stream(
            "render",
            json!({ "window": "main" }),
            std::sync::Arc::new(NullSink),
        )
        .expect_err("a label");
    assert_eq!(error.code, ErrorCode::InvalidParams);
    let reply = c
        .call(
            "window.focus_report",
            json!({ "window": "01ARZ3NDEKTSV4RRFFQ69G5FAV", "focused": true, "visible": true }),
        )
        .await;
    assert_eq!(
        error_of(&reply).1,
        "permission_denied",
        "a method of the UI: {reply}"
    );
    let error = h
        .handle
        .rpc(
            "window.focus_report",
            json!({ "window": "01ARZ3NDEKTSV4RRFFQ69G5FAV", "focused": true, "visible": true }),
        )
        .await
        .expect_err("no such window");
    assert_eq!(error.code, ErrorCode::NotFound);
}

struct NullSink;

impl nh_core::FrameSink for NullSink {
    fn send(&self, _: Vec<u8>) -> bool {
        true
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_report_of_a_window_reaches_the_projection_and_a_hidden_window_gets_no_frames() {
    let h = Harness::start();
    let mut c = client(&h).await;
    let flag = h.dir.join("go");
    let script = format!(
        "echo before; while [ ! -e {} ]; do sleep 0.05; done; echo after; exec sleep 60",
        flag.display()
    );
    let created = c
        .ok(
            "workspace.create",
            json!({ "command": { "argv": ["/bin/sh", "-c", script] } }),
        )
        .await;
    let pane = wait_started(&h, &created["pane"]).await;
    wait_for(&pane, "the first line", |t| {
        t.iter().any(|row| row == "before")
    })
    .await;
    let window = h.call("window.open", json!({})).await["window"].clone();
    let row = window_row(&mut c, &window).await;
    assert_eq!(row["focused"], json!(false));
    assert_eq!(row["visible"], json!(true));

    h.call(
        "window.focus_report",
        json!({ "window": window, "focused": true, "visible": true, "refresh_hz": 120.0 }),
    )
    .await;
    let deadline = Instant::now() + TIMEOUT;
    let row = loop {
        let row = window_row(&mut c, &window).await;
        if row["focused"] == json!(true) {
            break row;
        }
        assert!(Instant::now() < deadline, "{row}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    assert_eq!(row["refresh_hz"], json!(120.0));

    // A minimized window is hidden: it gets the first frame when it opens its stream, no more.
    h.call(
        "window.focus_report",
        json!({ "window": window, "focused": false, "visible": true, "minimized": true }),
    )
    .await;
    let mut frames = Frames::open(&h.handle, window.as_str().expect("id"), 120.0);
    assert!(
        frames.next(Duration::from_millis(500)).await.is_none(),
        "hidden"
    );
    std::fs::write(&flag, b"").expect("the flag");
    wait_for(&pane, "the second line", |t| {
        t.iter().any(|row| row == "after")
    })
    .await;
    assert!(
        frames.next(Duration::from_millis(300)).await.is_none(),
        "still hidden"
    );

    // It is shown again and gets a keyframe with everything.
    h.call(
        "window.focus_report",
        json!({ "window": window, "focused": true, "visible": true }),
    )
    .await;
    let frame = frames
        .next(TIMEOUT)
        .await
        .expect("a frame when it is shown");
    assert_eq!(frame.kind, FrameKind::Keyframe);
}

async fn wait_started(h: &Harness, id: &Value) -> std::sync::Arc<nh_core::pane::Pane> {
    let id: Ulid = id.as_str().expect("an id").parse().expect("a ULID");
    let deadline = Instant::now() + TIMEOUT;
    loop {
        if let Some(pane) = h.handle.pane(id) {
            return pane;
        }
        assert!(
            Instant::now() < deadline,
            "the holder of {id} did not start"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn render_ack_with_a_window_id_lets_the_next_frame_out() {
    let h = Harness::start();
    let mut c = client(&h).await;
    let counter =
        "i=0; while [ $i -lt 600 ]; do echo line$i; i=$((i+1)); sleep 0.01; done; sleep 60";
    c.ok(
        "workspace.create",
        json!({ "command": { "argv": ["/bin/sh", "-c", counter] } }),
    )
    .await;
    let window = h.call("window.open", json!({})).await["window"].clone();
    let key = window.as_str().expect("id");
    let mut frames = Frames::open(&h.handle, key, 120.0);
    let one = frames.next(TIMEOUT).await.expect("frame 1");
    let two = frames.next(TIMEOUT).await.expect("frame 2");
    assert!(
        frames.next(Duration::from_millis(400)).await.is_none(),
        "two in flight"
    );
    h.call(
        "render.ack",
        json!({ "window": window, "acks": [{ "pane": one.pane, "rev": one.rev }] }),
    )
    .await;
    let three = frames.next(TIMEOUT).await.expect("frame 3");
    assert_eq!(three.base_rev, two.rev);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pane_focus_names_a_pane_that_the_window_draws() {
    let h = Harness::start_layout();
    let mut c = client(&h).await;
    let created = c.ok("workspace.create", json!({})).await;
    let split = c
        .ok(
            "pane.split",
            json!({ "pane": "w1:p1", "direction": "right" }),
        )
        .await;
    let window = h.call("window.open", json!({})).await["window"].clone();
    h.call("pane.focus", json!({ "window": window, "pane": "w1:p2" }))
        .await;
    let row = window_row(&mut c, &window).await;
    assert_eq!(row["focused_pane"], split["pane"]);
    // The row names the workspace of the tab that the window shows.
    assert_eq!(row["workspace"], created["workspace"]);
    assert_eq!(row["tab"], created["tab"]);
    h.call(
        "pane.focus",
        json!({ "window": window, "pane": created["pane"] }),
    )
    .await;
    let row = window_row(&mut c, &window).await;
    assert_eq!(row["focused_pane"], created["pane"]);

    // A pane that another tab has is not drawn by this window.
    c.ok("tab.create", json!({ "workspace": "w1" })).await;
    let error = h
        .handle
        .rpc("pane.focus", json!({ "window": window, "pane": "w1:p3" }))
        .await
        .expect_err("not drawn");
    assert_eq!(error.code, ErrorCode::InvalidParams);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_first_window_that_sizes_a_pane_owns_its_size_until_another_one_types() {
    let h = Harness::start();
    let mut c = client(&h).await;
    let created = c
        .ok(
            "workspace.create",
            json!({ "command": { "argv": ["/bin/sh", "-c", "exec cat"] } }),
        )
        .await;
    let pane = wait_started(&h, &created["pane"]).await;
    let one = h.call("window.open", json!({})).await["window"].clone();
    let two = h.call("window.open", json!({})).await["window"].clone();

    // The window that reports a size first owns the size of the PTY.
    h.call(
        "pane.resize",
        json!({ "pane": created["pane"], "cols": 100, "rows": 30, "window": one }),
    )
    .await;
    let deadline = Instant::now() + TIMEOUT;
    while pane.size() != (100, 30) {
        assert!(Instant::now() < deadline, "the pane is {:?}", pane.size());
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    // The other window has a smaller surface. It does not resize the PTY.
    h.call(
        "pane.resize",
        json!({ "pane": created["pane"], "cols": 50, "rows": 20, "window": two }),
    )
    .await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(pane.size(), (100, 30));
    assert_eq!(pane.info().geometry_owner, Some(window_id(&one)));
    let shown = c.ok("pane.show", json!({ "pane": "w1:p1" })).await;
    assert_eq!(shown["pane"]["geometry_owner"], one);

    // Input from the other window within half a second of the change does not move the lease.
    let typing = |window: &Value| Origin::ui().from_window(window_id(window));
    h.handle
        .call_as(
            typing(&two),
            "pane.input",
            json!({ "pane": created["pane"], "input_id": 1,
                "event": { "type": "text", "text": "a" } }),
        )
        .await
        .expect("input");
    assert_eq!(pane.info().geometry_owner, Some(window_id(&one)));

    // After that time, input from the other window takes the lease. The PTY gets its size.
    tokio::time::sleep(Duration::from_millis(600)).await;
    h.handle
        .call_as(
            typing(&two),
            "pane.input",
            json!({ "pane": created["pane"], "input_id": 2,
                "event": { "type": "text", "text": "b" } }),
        )
        .await
        .expect("input");
    let deadline = Instant::now() + TIMEOUT;
    while pane.size() != (50, 20) {
        assert!(Instant::now() < deadline, "the pane is {:?}", pane.size());
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(pane.info().geometry_owner, Some(window_id(&two)));

    // The window that owns the lease closes. The lease is free, and the next window takes it.
    h.call("window.close", json!({ "window": two })).await;
    let deadline = Instant::now() + TIMEOUT;
    while pane.info().geometry_owner.is_some() {
        assert!(
            Instant::now() < deadline,
            "the lease stays with a closed window"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    h.call(
        "pane.resize",
        json!({ "pane": created["pane"], "cols": 90, "rows": 25, "window": one }),
    )
    .await;
    let deadline = Instant::now() + TIMEOUT;
    while pane.size() != (90, 25) {
        assert!(Instant::now() < deadline, "the pane is {:?}", pane.size());
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(pane.info().geometry_owner, Some(window_id(&one)));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_size_asked_for_before_the_holder_runs_is_the_size_of_the_pane() {
    let h = Harness::start();
    let mut c = client(&h).await;
    let created = c
        .ok(
            "workspace.create",
            json!({ "command": { "argv": ["/bin/sh", "-c", "exec cat"] } }),
        )
        .await;
    let window = h.call("window.open", json!({})).await["window"].clone();
    // The surface of the window is known before the holder is up.
    h.call(
        "pane.resize",
        json!({ "pane": created["pane"], "cols": 132, "rows": 43, "window": window }),
    )
    .await;
    let pane = wait_started(&h, &created["pane"]).await;
    let deadline = Instant::now() + TIMEOUT;
    while pane.size() != (132, 43) {
        assert!(Instant::now() < deadline, "the pane is {:?}", pane.size());
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_metrics_of_a_window_reach_the_health_projection() {
    let h = Harness::start_layout();
    let mut c = client(&h).await;
    c.ok("workspace.create", json!({})).await;
    let window = h.call("window.open", json!({})).await["window"].clone();
    h.call(
        "window.report_metrics",
        json!({ "window": window, "cell_width_px": 8.0, "cell_height_px": 16.0,
            "device_pixel_ratio": 2.0, "renderer": "canvas2d", "gpu": "Apple M3" }),
    )
    .await;
    let reply = c
        .ok("projection.subscribe", json!({"name": "health"}))
        .await;
    let id = reply["subscription"].as_str().expect("an id").to_owned();
    let health = loop {
        let line = c.recv().await;
        if line["method"] == "projection.message"
            && line["params"]["subscription"] == id.as_str()
            && line["params"]["message"]["type"] == "snapshot"
        {
            break line["params"]["message"]["data"]["items"]["_"].clone();
        }
    };
    assert_eq!(health["renderer"], "canvas2d");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_folder_panel_of_the_host_gives_the_folder_that_the_person_chose() {
    let host = std::sync::Arc::new(nh_core::RecordingHost::new(
        env!("CARGO_BIN_EXE_nh").into(),
        false,
    ));
    let h = Harness::start_hosted(host.clone());
    let mut c = client(&h).await;
    c.ok("workspace.create", json!({})).await;
    let window = h.call("window.open", json!({})).await["window"].clone();

    // A host without a panel: the UI shows the message, and the person types the path.
    let error = h
        .handle
        .rpc("window.pick_folder", json!({ "window": window }))
        .await
        .expect_err("no panel");
    assert_eq!(error.code, ErrorCode::Unsupported);

    // The person chooses a folder. The panel starts in the folder that the call names.
    host.answer_folder(Some("/tmp/a repo".into()));
    let picked = h
        .call(
            "window.pick_folder",
            json!({ "window": window, "start": "/tmp", "title": "Choose the repository" }),
        )
        .await;
    assert_eq!(picked, json!({ "path": "/tmp/a repo" }));

    // The person cancels. A start that is not absolute does not reach the host.
    host.answer_folder(None);
    let picked = h
        .call(
            "window.pick_folder",
            json!({ "window": window, "start": "relative/path" }),
        )
        .await;
    assert_eq!(picked, json!({ "path": null }));

    let window_id: WindowId = serde_json::from_value(window.clone()).expect("a window id");
    let picks: Vec<_> = host
        .calls()
        .into_iter()
        .filter_map(|call| match call {
            nh_core::host::HostCall::PickFolder(pick) => Some(pick),
            _ => None,
        })
        .collect();
    assert_eq!(
        picks,
        [
            nh_core::host::FolderPick {
                window: window_id,
                start: None,
                title: None,
            },
            nh_core::host::FolderPick {
                window: window_id,
                start: Some("/tmp".into()),
                title: Some("Choose the repository".into()),
            },
            nh_core::host::FolderPick {
                window: window_id,
                start: None,
                title: None,
            },
        ]
    );

    // A window that the layout does not have, and a caller that is not the UI.
    let error = h
        .handle
        .rpc(
            "window.pick_folder",
            json!({ "window": "01ARZ3NDEKTSV4RRFFQ69G5FAV" }),
        )
        .await
        .expect_err("no such window");
    assert_eq!(error.code, ErrorCode::NotFound);
    let reply = c
        .call("window.pick_folder", json!({ "window": window }))
        .await;
    assert_eq!(
        error_of(&reply).1,
        "permission_denied",
        "a method of the UI: {reply}"
    );
}

/// Opens a WebSocket to the dev server. `None` when the server refused the handshake.
async fn connect(port: u16, path_and_query: &str) -> Option<WebSocketStream<TcpStream>> {
    let stream = TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("the port is open");
    client_async(format!("ws://127.0.0.1:{port}{path_and_query}"), stream)
        .await
        .ok()
        .map(|(ws, _)| ws)
}

/// The next text message of a connection, as JSON.
async fn next_text(ws: &mut WebSocketStream<TcpStream>) -> Value {
    loop {
        match timeout(Duration::from_secs(5), ws.next())
            .await
            .expect("in time")
        {
            Some(Ok(Message::Text(text))) => return serde_json::from_str(&text).expect("JSON"),
            Some(Ok(_)) => {}
            other => panic!("the socket ended: {other:?}"),
        }
    }
}

/// The percent-encoding of a query value: every byte except the unreserved ones.
fn percent_encode(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            other => format!("%{other:02X}"),
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_dev_websocket_carries_calls_subscriptions_and_streams() {
    let h = Harness::start();
    h.call("workspace.create", json!({ "command": says("first") }))
        .await;
    let window = h.open_window().await;
    let info = h.handle.dev_ws_serve().expect("the server starts");
    assert_eq!(info.token.len(), 32);

    // A wrong token, a missing token and an unknown path are refused at the handshake.
    assert!(connect(info.port, "/control?token=wrong").await.is_none());
    assert!(connect(info.port, "/control").await.is_none());
    assert!(
        connect(info.port, &format!("/other?token={}", info.token))
            .await
            .is_none()
    );

    let mut control = connect(info.port, &format!("/control?token={}", info.token))
        .await
        .expect("the token is good");
    let send = |id: u64, body: Value| {
        let mut message = json!({ "id": id });
        message
            .as_object_mut()
            .expect("an object")
            .extend(body.as_object().expect("an object").clone());
        Message::Text(message.to_string().into())
    };
    control
        .send(send(
            1,
            json!({ "call": { "method": "session.info", "params": {} } }),
        ))
        .await
        .expect("sent");
    let answer = next_text(&mut control).await;
    assert_eq!(answer["id"], 1);
    assert_eq!(answer["result"]["name"], "test");

    control
        .send(send(
            2,
            json!({ "call": { "method": "nope", "params": {} } }),
        ))
        .await
        .expect("sent");
    let answer = next_text(&mut control).await;
    assert_eq!(
        (answer["id"].clone(), answer["error"]["code"].clone()),
        (json!(2), json!("method_not_found"))
    );

    control
        .send(send(3, json!({ "subscribe": { "name": "panes" } })))
        .await
        .expect("sent");
    let mut seen = Vec::new();
    while seen.len() < 2 {
        seen.push(next_text(&mut control).await);
    }
    assert!(
        seen.iter()
            .any(|m| m["id"] == 3 && m["result"] == json!({}))
    );
    assert!(
        seen.iter()
            .any(|m| m["sub"] == 3 && m["message"]["type"] == "snapshot")
    );

    // The render stream of the window: binary messages and nothing else, one frame in each.
    let params = json!({
        "window": window,
        "viewport_hz": 120.0,
        "proto": nh_proto::version::FRAME_PROTO,
    })
    .to_string();
    let mut stream = connect(
        info.port,
        &format!(
            "/stream?token={}&kind=render&params={}",
            info.token,
            percent_encode(&params)
        ),
    )
    .await
    .expect("the stream opens");
    let message = timeout(TIMEOUT, stream.next())
        .await
        .expect("in time")
        .expect("a message")
        .expect("no error");
    let Message::Binary(bytes) = message else {
        panic!("a binary message, not {message:?}");
    };
    let frame: Frame = postcard::from_bytes(&bytes).expect("the bytes are a frame");
    assert_eq!(frame.kind, FrameKind::Keyframe, "the first frame of a pane");
}
