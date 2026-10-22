//! The desk on the phone against a real core: the
//! phone window, its render stream, the geometry lease, the checks of a window of another device,
//! the projections of a window, an attached file, the fonts of the host and the removal.

mod core_support;
mod phone_support;

use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use core_support::{Harness, Messages, TIMEOUT, wait_for};
use futures_util::StreamExt;
use nh_core::Origin;
use nh_proto::{api::ids::WindowId, frame::Frame};
use nh_web::{FontFace, FontSource};
use phone_support::{Rig, Ws, call, error_code, http, notification, refused_with};
use serde_json::{Value, json};
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::Message;
use ulid::Ulid;

fn ulid(value: &Value) -> Ulid {
    value.as_str().expect("an id").parse().expect("a ULID")
}

fn window_id(value: &Value) -> WindowId {
    WindowId::new(ulid(value))
}

/// A core whose first workspace runs `cat`, so a key or a paste shows in the pane.
async fn rig_with_pane() -> (Rig, Value, Arc<nh_core::pane::Pane>) {
    rig_running("echo ready; exec cat").await
}

/// A rig with a pane that runs a script of `/bin/sh`. The script prints `ready` first.
async fn rig_running(script: &str) -> (Rig, Value, Arc<nh_core::pane::Pane>) {
    let rig = Rig::with(Harness::start());
    let created = rig
        .h
        .call(
            "workspace.create",
            json!({ "command": { "argv": ["/bin/sh", "-c", script] } }),
        )
        .await;
    let id = ulid(&created["pane"]);
    let deadline = Instant::now() + TIMEOUT;
    let pane = loop {
        if let Some(pane) = rig.h.handle.pane(id) {
            break pane;
        }
        assert!(Instant::now() < deadline, "no pane");
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    wait_for(&pane, "the shell", |rows| rows.iter().any(|r| r == "ready")).await;
    (rig, created, pane)
}

/// The window of the phone: the result of `phone.window`.
async fn phone_window(ws: &mut Ws, id: u64) -> Value {
    let reply = call(ws, id, "phone.window", json!({})).await;
    reply["result"]["window"].clone()
}

/// The next binary message of a stream, as a frame.
async fn next_frame(ws: &mut Ws) -> Frame {
    let found = timeout(TIMEOUT, async {
        loop {
            match ws.next().await {
                Some(Ok(Message::Binary(bytes))) => {
                    return postcard::from_bytes::<Frame>(&bytes).expect("the bytes are a frame");
                }
                Some(Ok(Message::Text(text))) => panic!("the stream sent text: {text}"),
                Some(Ok(_)) => {}
                other => panic!("the stream ended: {other:?}"),
            }
        }
    })
    .await;
    found.expect("a frame in time")
}

async fn size_becomes(pane: &nh_core::pane::Pane, wanted: (u16, u16)) {
    let deadline = Instant::now() + TIMEOUT;
    while pane.size() != wanted {
        assert!(Instant::now() < deadline, "the pane is {:?}", pane.size());
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// The row of a window in a message of `windows`.
fn window_row(message: &Value, window: &Value) -> Option<Value> {
    let key = window.as_str()?;
    if message["type"] == "snapshot" {
        return Some(message["data"]["items"][key].clone()).filter(|v| !v.is_null());
    }
    message["ops"]
        .as_array()?
        .iter()
        .rev()
        .find(|op| op["op"] == "set" && op["key"] == key)
        .map(|op| op["value"].clone())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_phone_gets_one_window_and_its_render_stream_draws_the_pane() {
    let (rig, _, pane) = rig_with_pane().await;
    let (token, device) = rig.paired().await;
    let mut ws = rig.rpc(Some(&token)).await.expect("the socket");
    let window = phone_window(&mut ws, 1).await;
    assert!(window.is_string(), "{window}");
    assert_eq!(phone_window(&mut ws, 2).await, window, "one window");
    // A second socket of the same phone has the same window.
    let mut again = rig.rpc(Some(&token)).await.expect("a second socket");
    assert_eq!(phone_window(&mut again, 1).await, window);

    // The row belongs to the phone, so no desktop opens a native window for it.
    let mut windows = Messages::subscribe(&rig.h.handle, "windows");
    let row = windows
        .wait_for("the row of the phone", |m| window_row(m, &window).is_some())
        .await;
    let row = window_row(&row, &window).expect("a row");
    assert_eq!(row["client"], json!(device));
    assert_eq!(row["single"], false);

    // The render stream of the window gives a keyframe of the pane that the tab shows.
    let mut stream = rig
        .stream(
            Some(&token),
            "render",
            &json!({ "viewport_hz": 60, "proto": nh_proto::version::FRAME_PROTO }),
        )
        .await
        .expect("the stream");
    let frame = next_frame(&mut stream).await;
    assert_eq!(frame.pane, pane.render_id());
    assert_eq!(frame.kind, nh_proto::frame::FrameKind::Keyframe);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_key_and_a_claim_of_the_phone_take_the_lease_for_the_phone_window() {
    let (rig, created, pane) = rig_with_pane().await;
    let (token, _) = rig.paired().await;
    let mut ws = rig.rpc(Some(&token)).await.expect("the socket");
    let phone = phone_window(&mut ws, 1).await;
    let mac = rig.h.call("window.open", json!({})).await["window"].clone();
    let id = &created["pane"];

    // The Mac sizes the pane first and owns the lease. The phone reports a smaller box.
    rig.h
        .call(
            "pane.resize",
            json!({ "pane": id, "cols": 100, "rows": 30, "window": mac }),
        )
        .await;
    size_becomes(&pane, (100, 30)).await;
    let resized = call(
        &mut ws,
        2,
        "pane.resize",
        json!({ "pane": id, "cols": 56, "rows": 20, "window": phone }),
    )
    .await;
    assert!(resized["result"].is_object(), "{resized}");
    tokio::time::sleep(Duration::from_millis(600)).await;
    assert_eq!(pane.size(), (100, 30));

    // A key from the phone takes the lease: the PTY gets the size of the phone.
    let typed = call(
        &mut ws,
        3,
        "pane.input",
        json!({ "pane": id, "input_id": 1, "event": { "type": "text", "text": "a" } }),
    )
    .await;
    assert!(typed["result"].is_object(), "{typed}");
    size_becomes(&pane, (56, 20)).await;
    assert_eq!(pane.info().geometry_owner, Some(window_id(&phone)));
    wait_for(&pane, "the key", |rows| {
        rows.iter().any(|r| r.contains('a'))
    })
    .await;

    // A key on the Mac takes it back after the debounce.
    tokio::time::sleep(Duration::from_millis(600)).await;
    rig.h
        .handle
        .call_as(
            Origin::ui().from_window(window_id(&mac)),
            "pane.input",
            json!({ "pane": id, "input_id": 2, "event": { "type": "text", "text": "b" } }),
        )
        .await
        .expect("a key on the Mac");
    size_becomes(&pane, (100, 30)).await;

    // The keyboard of the phone opens: `pane.claim` takes the lease and sends no byte.
    tokio::time::sleep(Duration::from_millis(600)).await;
    let claimed = call(
        &mut ws,
        4,
        "pane.claim",
        json!({ "pane": id, "window": phone }),
    )
    .await;
    assert!(claimed["result"].is_object(), "{claimed}");
    size_becomes(&pane, (56, 20)).await;
    // A second claim within the debounce does not move the lease.
    rig.h
        .handle
        .call_as(
            Origin::ui(),
            "pane.claim",
            json!({ "pane": id, "window": mac }),
        )
        .await
        .expect("a claim of the Mac");
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(pane.info().geometry_owner, Some(window_id(&phone)));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_phone_cannot_use_the_window_of_another_device() {
    let rig = Rig::start();
    let (first, _) = rig.paired().await;
    let (second, _) = rig.paired().await;
    let mut one = rig.rpc(Some(&first)).await.expect("a socket");
    let mut two = rig.rpc(Some(&second)).await.expect("a socket");
    let mine = phone_window(&mut one, 1).await;
    let other = phone_window(&mut two, 1).await;
    assert_ne!(mine, other, "each phone has its own window");

    let report = call(
        &mut one,
        2,
        "window.focus_report",
        json!({ "window": other, "focused": true, "visible": true }),
    )
    .await;
    assert_eq!(error_code(&report), Some("permission_denied"), "{report}");
    let single = call(
        &mut one,
        3,
        "window.set_single",
        json!({ "window": other, "single": true }),
    )
    .await;
    assert_eq!(error_code(&single), Some("permission_denied"), "{single}");
    let agents = call(
        &mut one,
        4,
        "projection.subscribe",
        json!({ "name": format!("agents_for:{}", other.as_str().expect("an id")) }),
    )
    .await;
    assert_eq!(error_code(&agents), Some("permission_denied"), "{agents}");
    let stream = rig
        .stream(Some(&first), "render", &json!({ "window": other }))
        .await;
    assert_eq!(refused_with(stream), 403);

    // Its own window passes.
    let own = call(
        &mut one,
        5,
        "window.set_single",
        json!({ "window": mine, "single": true }),
    )
    .await;
    assert!(own["result"].is_object(), "{own}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_stream_needs_the_cookie_and_the_origin() {
    let rig = Rig::start();
    let (token, _) = rig.paired().await;
    assert_eq!(
        refused_with(rig.stream(None, "render", &json!({})).await),
        401
    );
    let foreign = phone_support::connect(
        rig.port,
        &rig.host(),
        "https://evil.example",
        "/stream?kind=render&params=%7B%7D",
        Some(&token),
    )
    .await;
    assert_eq!(refused_with(foreign), 403);
    // A kind that a phone has no use for.
    assert_eq!(
        refused_with(rig.stream(Some(&token), "bench", &json!({})).await),
        400
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_phone_follows_the_projections_of_its_window_and_the_link() {
    let rig = Rig::start();
    let (token, _) = rig.paired().await;
    let mut ws = rig.rpc(Some(&token)).await.expect("the socket");
    let window = phone_window(&mut ws, 1).await;
    let mut id = 10;
    for name in [
        "windows".to_owned(),
        "settings".to_owned(),
        "jobs".to_owned(),
        "health".to_owned(),
        format!("agents_for:{}", window.as_str().expect("an id")),
    ] {
        id += 1;
        let reply = call(&mut ws, id, "projection.subscribe", json!({ "name": name })).await;
        assert!(
            reply["result"]["subscription"].is_string(),
            "{name}: {reply}"
        );
    }
    let link = call(
        &mut ws,
        2,
        "projection.subscribe",
        json!({ "name": "link" }),
    )
    .await;
    assert!(link["result"]["subscription"].is_string(), "{link}");
    let snapshot = notification(&mut ws, "the link", |m| {
        m["type"] == "snapshot" && m["data"]["items"]["_"]["machine"] == "local"
    })
    .await;
    assert_eq!(snapshot["data"]["items"]["_"]["state"], "local");
    let versions = call(
        &mut ws,
        3,
        "projection.versions",
        json!({ "names": ["link", "windows"] }),
    )
    .await;
    assert!(
        versions["result"]["versions"]["link"].is_u64(),
        "{versions}"
    );
    assert!(
        versions["result"]["versions"]["windows"].is_u64(),
        "{versions}"
    );
    let unsubscribed = call(
        &mut ws,
        4,
        "projection.unsubscribe",
        json!({ "subscription": link["result"]["subscription"] }),
    )
    .await;
    assert!(unsubscribed["result"].is_object(), "{unsubscribed}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_phone_window_has_the_focus_only_while_the_phone_is_connected() {
    let rig = Rig::start();
    let (token, _) = rig.paired().await;
    let mut ws = rig.rpc(Some(&token)).await.expect("the socket");
    let window = phone_window(&mut ws, 1).await;
    let mut windows = Messages::subscribe(&rig.h.handle, "windows");
    let report = call(
        &mut ws,
        2,
        "window.focus_report",
        json!({ "window": window, "focused": true, "visible": true }),
    )
    .await;
    assert!(report["result"].is_object(), "{report}");
    windows
        .wait_for("the focus of the phone", |m| {
            window_row(m, &window).is_some_and(|row| row["focused"] == true)
        })
        .await;
    // The socket closes: the phone is no longer a client, and its window has no focus.
    drop(ws);
    windows
        .wait_for("the window without the phone", |m| {
            window_row(m, &window)
                .is_some_and(|row| row["focused"] == false && row["visible"] == false)
        })
        .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_removed_phone_loses_its_streams_and_its_window() {
    let (rig, _, _) = rig_with_pane().await;
    let (token, device) = rig.paired().await;
    let mut ws = rig.rpc(Some(&token)).await.expect("the socket");
    let window = phone_window(&mut ws, 1).await;
    let mut stream = rig
        .stream(Some(&token), "render", &json!({ "viewport_hz": 60 }))
        .await
        .expect("the stream");
    next_frame(&mut stream).await;
    let mut windows = Messages::subscribe(&rig.h.handle, "windows");

    rig.h
        .call("phone.remove", json!({ "device": device }))
        .await;
    let closed = timeout(Duration::from_secs(1), async {
        loop {
            match stream.next().await {
                None | Some(Err(_)) | Some(Ok(Message::Close(_))) => return,
                Some(Ok(_)) => {}
            }
        }
    })
    .await;
    assert!(closed.is_ok(), "the stream is still open after 1 s");
    windows
        .wait_for("the window of the phone goes", |m| {
            m["type"] == "patch"
                && m["ops"].as_array().is_some_and(|ops| {
                    ops.iter()
                        .any(|op| op["op"] == "remove" && op["key"] == window)
                })
        })
        .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_attached_file_reaches_the_pane_as_a_quoted_path() {
    let (rig, created, pane) = rig_with_pane().await;
    let (token, _) = rig.paired().await;
    let mut ws = rig.rpc(Some(&token)).await.expect("the socket");
    phone_window(&mut ws, 1).await;
    let headers = rig.headers(&token);
    let headers: Vec<(&str, &str)> = headers
        .iter()
        .map(|(n, v)| (n.as_str(), v.as_str()))
        .collect();
    let put = http(
        rig.port,
        "POST",
        "/blob?name=shot.png",
        &headers,
        Some(b"\x89PNG\r\n\x1a\nfake"),
    )
    .await;
    assert_eq!(put.status, 200, "{}", put.body);
    let hash = put.json()["hash"].as_str().expect("a hash").to_owned();
    // Without the cookie, nothing is kept.
    let anonymous = http(
        rig.port,
        "POST",
        "/blob?name=x.png",
        &[("Host", &rig.host()), ("Origin", &rig.origin())],
        Some(b"x"),
    )
    .await;
    assert_eq!(anonymous.status, 401);

    let dropped = call(
        &mut ws,
        2,
        "pane.drop_blobs",
        json!({ "pane": created["pane"], "input_id": 1,
            "blobs": [{ "hash": hash, "name": "shot.png" }] }),
    )
    .await;
    assert!(dropped["result"].is_object(), "{dropped}");
    wait_for(&pane, "the path of the file", |rows| {
        rows.iter().any(|row| row.contains("shot.png"))
    })
    .await;
}

/// A font source with one family whose regular face is a file of the test.
struct OneFamily(PathBuf);

impl FontSource for OneFamily {
    fn faces(&self, family: &str) -> Vec<FontFace> {
        if family != "Test Mono" {
            return Vec::new();
        }
        vec![FontFace {
            family: family.to_owned(),
            bold: false,
            italic: false,
            path: self.0.clone(),
        }]
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_fonts_come_from_the_source_of_the_host() {
    let rig = Rig::start();
    let (token, _) = rig.paired().await;
    let cookie = format!("nh_device={token}");
    let headers = [("Host", rig.host()), ("Cookie", cookie)];
    let headers: Vec<(&str, &str)> = headers.iter().map(|(n, v)| (*n, v.as_str())).collect();
    let sheet = "/font/faces.css?families=%22Test%20Mono%22%2C%20Menlo";

    // `nh daemon` gives no source: the sheet is empty.
    let empty = http(rig.port, "GET", sheet, &headers, None).await;
    assert_eq!(empty.status, 200);
    assert_eq!(empty.body, "");

    let file = rig.h.dir.join("test-mono.ttf");
    std::fs::write(&file, b"font bytes").expect("a font file");
    rig.phone.set_font_source(Arc::new(OneFamily(file)));
    let css = http(rig.port, "GET", sheet, &headers, None).await;
    assert_eq!(css.status, 200);
    assert_eq!(css.header("content-type"), Some("text/css; charset=utf-8"));
    assert!(
        css.body
            .contains("url(\"/font/file?family=Test+Mono&weight=400&style=normal\")"),
        "{}",
        css.body
    );
    let face = http(
        rig.port,
        "GET",
        "/font/file?family=Test+Mono&weight=400&style=normal",
        &headers,
        None,
    )
    .await;
    assert_eq!(face.status, 200);
    assert_eq!(face.bytes, b"font bytes");
    assert_eq!(face.header("content-type"), Some("font/ttf"));
    // Only a face that the source names.
    let bold = http(
        rig.port,
        "GET",
        "/font/file?family=Test+Mono&weight=700&style=normal",
        &headers,
        None,
    )
    .await;
    assert_eq!(bold.status, 404);
    let anonymous = http(rig.port, "GET", sheet, &[("Host", &rig.host())], None).await;
    assert_eq!(anonymous.status, 401);

    // The page may compile the decoder of the frames, and the fonts come from the listener.
    let page = http(rig.port, "GET", "/", &[("Host", &rig.host())], None).await;
    let csp = page.header("content-security-policy").expect("a policy");
    assert!(
        csp.contains("script-src 'self' 'wasm-unsafe-eval'"),
        "{csp}"
    );
    assert!(csp.contains("font-src 'self'"), "{csp}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_phone_copies_a_path_into_its_own_clipboard_and_gets_no_new_window() {
    let rig = Rig::start();
    let (token, _) = rig.paired().await;
    let mut ws = rig.rpc(Some(&token)).await.expect("the socket");
    phone_window(&mut ws, 1).await;
    let path = rig.h.dir.display().to_string();
    let copied = call(
        &mut ws,
        2,
        "fs.copy_path",
        json!({ "path": path, "return_text": true }),
    )
    .await;
    assert_eq!(copied["result"]["text"], json!(path), "{copied}");
    // Without `return_text`, the core has no clipboard of a phone to write.
    let refused = call(&mut ws, 3, "fs.copy_path", json!({ "path": path })).await;
    assert_eq!(error_code(&refused), Some("unsupported"), "{refused}");

    // The settings of a phone are a view of the page, and a phone has one window.
    let settings = call(
        &mut ws,
        4,
        "palette.execute",
        json!({ "command": "settings.open" }),
    )
    .await;
    assert_eq!(settings["result"]["ui"], "settings.open", "{settings}");
    let new_window = call(
        &mut ws,
        5,
        "palette.execute",
        json!({ "command": "window.new" }),
    )
    .await;
    assert_eq!(error_code(&new_window), Some("unsupported"), "{new_window}");
    let found = call(
        &mut ws,
        6,
        "palette.query",
        json!({ "query": "new window" }),
    )
    .await;
    let ids: Vec<&str> = found["result"]["items"]
        .as_array()
        .expect("items")
        .iter()
        .filter_map(|item| item["command"]["id"].as_str())
        .collect();
    assert!(!ids.contains(&"window.new"), "{ids:?}");
}

/// An Enter from the phone, as the key bar sends it.
fn enter() -> Value {
    json!({ "type": "key", "code": "Enter", "key": "Enter", "mods": [], "kind": "press", "location": 0 })
}

/// The OSC 52 write that waits in a pane.
async fn clipboard_request(pane: &nh_core::pane::Pane) -> nh_proto::api::model::ClipboardRequest {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        if let Some(request) = pane.info().clipboard_request {
            return request;
        }
        assert!(Instant::now() < deadline, "no clipboard request");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_remembered_clipboard_write_waits_for_the_copy_of_the_phone() {
    let (rig, created, pane) = rig_running(
        r#"echo ready; read a; printf '\033]52;c;%s\007' "$(printf 'first text' | base64)";
read b; printf '\033]52;c;%s\007' "$(printf 'second text' | base64)"; exec sleep 60"#,
    )
    .await;
    let (token, _) = rig.paired().await;
    let mut ws = rig.rpc(Some(&token)).await.expect("the socket");
    phone_window(&mut ws, 1).await;
    let id = &created["pane"];

    // The first write after a key of the phone asks, and Allow gives the text to the page.
    let typed = call(
        &mut ws,
        2,
        "pane.input",
        json!({ "pane": id, "input_id": 1, "event": enter() }),
    )
    .await;
    assert!(typed["result"].is_object(), "{typed}");
    let first = clipboard_request(&pane).await;
    assert_eq!(first.preview, "first text");
    assert!(!first.allowed);
    let allowed = call(
        &mut ws,
        3,
        "pane.clipboard_allow",
        json!({ "pane": id, "allow": true, "remember": true, "return_text": true }),
    )
    .await;
    assert_eq!(allowed["result"]["text"], "first text", "{allowed}");
    assert!(pane.info().clipboard_request.is_none());

    // With the remembered Allow, a later write still waits for a tap on the phone: no host can
    // write the clipboard of a phone.
    call(
        &mut ws,
        4,
        "pane.input",
        json!({ "pane": id, "input_id": 2, "event": enter() }),
    )
    .await;
    let second = clipboard_request(&pane).await;
    assert_eq!(second.preview, "second text");
    assert!(second.allowed);
    assert!(pane.info().clipboard_dropped.is_none());
    let copied = call(
        &mut ws,
        5,
        "pane.clipboard_allow",
        json!({ "pane": id, "allow": true, "return_text": true }),
    )
    .await;
    assert_eq!(copied["result"]["text"], "second text", "{copied}");
    assert!(pane.info().clipboard_request.is_none());
}
