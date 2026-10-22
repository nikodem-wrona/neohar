//! A phone reaches another machine through the host of its core. The test
//! host gives the listener one machine, `studio`: a second core behind its `link.sock`, with a
//! `LinkClient` as the app has. The window of the phone on the machine belongs to the phone, a key
//! of the phone reaches a pane there, a stream of the phone attaches it to the machine until the
//! stream closes, a file goes into the blob store of the machine, and the host of the app opens no
//! native window for the phone.

mod core_support;
mod phone_support;

use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use async_trait::async_trait;
use core_support::{Harness, Messages, TIMEOUT, wait_for};
use futures_util::StreamExt;
use nh_core::{
    ClientInfo, ErrorCode, FrameSink, MessageSink, RecordingHost, RpcError, host::HostCall,
};
use nh_proto::api::ids::WindowId;
use nh_remote::{ClientHello, ClientOptions, Device, Link, LinkClient, LinkServer, LinkTiming};
use nh_web::{PhoneMachine, PhoneMachines};
use phone_support::{Rig, Ws, call, connect, error_code, http, percent, refused_with};
use serde_json::{Value, json};
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::Message;
use ulid::Ulid;

/// The device of the app that hosts the core of the listener.
const APP: Ulid = Ulid::from_parts(1, 0xA1);

fn timing() -> LinkTiming {
    LinkTiming {
        ping_every: Duration::from_millis(100),
        down_after: Duration::from_secs(5),
        hello_within: Duration::from_secs(5),
    }
}

/// The machines of the test host: `studio` is the link, and `down` is a machine whose link is
/// down.
struct TestMachines {
    link: Arc<LinkClient>,
}

impl PhoneMachines for TestMachines {
    fn machine(&self, label: &str) -> Result<Arc<dyn PhoneMachine>, RpcError> {
        match label {
            "studio" => Ok(Arc::new(TestMachine {
                link: Arc::clone(&self.link),
            })),
            "down" => Err(RpcError::new(
                ErrorCode::LinkDown,
                "the link to down is not up",
            )),
            _ => Err(RpcError::new(
                ErrorCode::NotFound,
                format!("no machine {label}"),
            )),
        }
    }
}

/// One machine of the test host: the calls and the streams of the phone go over the link with
/// the device of the phone, as the app sends them.
struct TestMachine {
    link: Arc<LinkClient>,
}

fn device_of(phone: &ClientInfo) -> Device {
    Device {
        id: phone.device.to_string(),
        name: phone.name.clone(),
    }
}

#[async_trait]
impl PhoneMachine for TestMachine {
    async fn call(
        &self,
        phone: &ClientInfo,
        window: Option<WindowId>,
        method: &str,
        params: Value,
    ) -> Result<Value, RpcError> {
        self.link
            .relay_call(&device_of(phone), window, method, params)
            .await
    }

    async fn subscribe(
        &self,
        name: &str,
        since: Option<u64>,
        sink: Box<dyn MessageSink>,
    ) -> Result<String, RpcError> {
        Link::subscribe(self.link.as_ref(), name, since, sink).await
    }

    async fn unsubscribe(&self, subscription: &str) -> bool {
        Link::unsubscribe(self.link.as_ref(), subscription)
    }

    async fn stream(
        &self,
        phone: &ClientInfo,
        kind: &str,
        params: Value,
        window: Option<WindowId>,
        sink: Arc<dyn FrameSink>,
    ) -> Result<(), RpcError> {
        self.link
            .relay_stream(&device_of(phone), kind, params, window, sink)
            .await
    }

    async fn put_blob(&self, name: &str, bytes: Vec<u8>) -> Result<String, RpcError> {
        Link::put_blob(self.link.as_ref(), name, bytes).await
    }

    async fn blob(&self, hash: &str) -> Result<Option<(Vec<u8>, String)>, RpcError> {
        Link::blob(self.link.as_ref(), hash).await
    }
}

/// The core of the listener with its phone listener, and the machine `studio` behind its link.
struct Setup {
    rig: Rig,
    studio: Harness,
    /// The host of the app on the link: what the core of the machine asks of it.
    host: Arc<RecordingHost>,
    _server: LinkServer,
    _link: Arc<LinkClient>,
}

async fn setup() -> Setup {
    let rig = Rig::with(Harness::start());
    let studio = Harness::start();
    let server = nh_remote::serve_with(studio.handle.clone(), &studio.paths, "test", timing())
        .expect("link.sock serves");
    let host = Arc::new(RecordingHost::new(PathBuf::from("/bin/nh"), true));
    let link = Arc::new(
        LinkClient::connect_unix(
            server.path(),
            ClientHello {
                device: APP,
                name: "this-mac".to_owned(),
                app_version: "0.0.0".to_owned(),
            },
            host.clone(),
            ClientOptions {
                timing: timing(),
                ..ClientOptions::default()
            },
        )
        .await
        .expect("the link opens"),
    );
    rig.phone.set_machines(Arc::new(TestMachines {
        link: Arc::clone(&link),
    }));
    Setup {
        rig,
        studio,
        host,
        _server: server,
        _link: link,
    }
}

impl Setup {
    /// A socket of a phone with a path of the listener.
    async fn socket(
        &self,
        path: &str,
        token: &str,
    ) -> Result<Ws, tokio_tungstenite::tungstenite::Error> {
        connect(
            self.rig.port,
            &self.rig.host(),
            &self.rig.origin(),
            path,
            Some(token),
        )
        .await
    }

    /// A pane on the machine that echoes its input.
    async fn pane(&self) -> (Value, Arc<nh_core::pane::Pane>) {
        let created = self
            .studio
            .call(
                "workspace.create",
                json!({ "command": { "argv": ["/bin/sh", "-c", "echo ready; exec cat"] } }),
            )
            .await;
        let id = created["pane"]
            .as_str()
            .and_then(|text| text.parse::<Ulid>().ok())
            .expect("a pane id");
        let deadline = Instant::now() + TIMEOUT;
        let pane = loop {
            if let Some(pane) = self.studio.handle.pane(id) {
                break pane;
            }
            assert!(Instant::now() < deadline, "no pane");
            tokio::time::sleep(Duration::from_millis(20)).await;
        };
        wait_for(&pane, "the shell", |rows| rows.iter().any(|r| r == "ready")).await;
        (created["pane"].clone(), pane)
    }
}

fn device(text: &str) -> Ulid {
    text.parse().expect("a device id")
}

/// The row of a window in a message of `windows`.
fn window_row(message: &Value, window: &str) -> Option<Value> {
    if message["type"] == "snapshot" {
        return Some(message["data"]["items"][window].clone()).filter(|v| !v.is_null());
    }
    message["ops"]
        .as_array()?
        .iter()
        .rev()
        .find(|op| op["op"] == "set" && op["key"] == window)
        .map(|op| op["value"].clone())
}

/// Waits until `test` holds.
async fn eventually(what: &str, test: impl Fn() -> bool) {
    let deadline = Instant::now() + TIMEOUT;
    while !test() {
        assert!(Instant::now() < deadline, "waited for {what}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_phone_owns_its_window_on_a_machine_and_types_into_a_pane_there() {
    let s = setup().await;
    let (pane_id, pane) = s.pane().await;
    let (token, phone) = s.rig.paired().await;
    let mut windows = Messages::subscribe(&s.studio.handle, "windows");
    let mut ws = s
        .socket("/rpc?machine=studio", &token)
        .await
        .expect("the socket of the machine");

    // The machine makes the window of the phone, and the row belongs to the phone.
    let reply = call(&mut ws, 1, "phone.window", json!({})).await;
    let window = reply["result"]["window"]
        .as_str()
        .expect("a window")
        .to_owned();
    windows
        .wait_for("the row of the phone", |m| {
            window_row(m, &window).is_some_and(|row| row["client"] == json!(phone))
        })
        .await;
    let again = call(&mut ws, 2, "phone.window", json!({})).await;
    assert_eq!(again["result"]["window"], json!(window), "{again}");

    // A key of the phone reaches the pane of the machine.
    let typed = call(
        &mut ws,
        3,
        "pane.input",
        json!({
            "pane": pane_id,
            "input_id": 1,
            "event": { "type": "text", "text": "from the phone" }
        }),
    )
    .await;
    assert!(typed["result"].is_object(), "{typed}");
    wait_for(&pane, "the key", |rows| {
        rows.iter().any(|r| r.contains("from the phone"))
    })
    .await;

    // On the machine the call is one of a phone: what acts on the screen of a Mac is refused.
    let opened = call(&mut ws, 4, "fs.open", json!({ "path": "/tmp" })).await;
    assert_eq!(error_code(&opened), Some("unsupported"), "{opened}");
    // `phone.visible` goes to the core where the phone is paired, not to the machine.
    let visible = call(&mut ws, 5, "phone.visible", json!({ "visible": true })).await;
    assert!(visible["result"].is_object(), "{visible}");
    // A phone follows the projections of its own window only.
    let other = call(
        &mut ws,
        6,
        "projection.subscribe",
        json!({ "name": format!("agents_for:{}", WindowId::new(Ulid::from_parts(1, 9))) }),
    )
    .await;
    assert_eq!(error_code(&other), Some("permission_denied"), "{other}");

    // The host of the app opened no native window for the phone.
    assert!(
        !s.host
            .calls()
            .iter()
            .any(|c| matches!(c, HostCall::OpenWindow(spec) if spec.id.to_string() == window)),
        "{:?}",
        s.host.calls()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_stream_of_the_phone_attaches_it_to_the_machine_until_it_closes() {
    let s = setup().await;
    let _pane = s.pane().await;
    let (token, phone) = s.rig.paired().await;
    let phone = device(&phone);
    let mut ws = s
        .socket("/rpc?machine=studio", &token)
        .await
        .expect("the socket of the machine");
    call(&mut ws, 1, "phone.window", json!({})).await;
    let router = Arc::clone(s.studio.handle.router());
    assert!(!router.is_phone(Some(phone)));

    // The render stream of the phone window: frames come, and the machine has the phone as a
    // client while it is open.
    let path = format!(
        "/stream?machine=studio&kind=render&params={}",
        percent("{}")
    );
    let mut stream = s.socket(&path, &token).await.expect("the stream");
    let frame = timeout(TIMEOUT, async {
        loop {
            match stream.next().await {
                Some(Ok(Message::Binary(bytes))) => return bytes,
                Some(Ok(Message::Text(text))) => panic!("the stream sent text: {text}"),
                Some(Ok(_)) => {}
                other => panic!("the stream ended: {other:?}"),
            }
        }
    })
    .await
    .expect("a frame in time");
    assert!(!frame.is_empty());
    eventually("the attach of the phone", || router.is_phone(Some(phone))).await;

    // The stream closes: the client sends `StreamClose`, and the machine detaches the phone.
    drop(stream);
    eventually("the detach of the phone", || !router.is_phone(Some(phone))).await;

    // A render stream of a window that is not the window of the phone is refused.
    let other = WindowId::new(Ulid::from_parts(1, 9)).to_string();
    let path = format!(
        "/stream?machine=studio&kind=render&params={}",
        percent(&json!({ "window": other }).to_string())
    );
    assert_eq!(refused_with(s.socket(&path, &token).await), 403);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_file_of_the_phone_goes_into_the_blob_store_of_the_machine() {
    let s = setup().await;
    let (pane_id, pane) = s.pane().await;
    let (token, _) = s.rig.paired().await;
    let headers = s.rig.headers(&token);
    let headers: Vec<(&str, &str)> = headers
        .iter()
        .map(|(n, v)| (n.as_str(), v.as_str()))
        .collect();
    let png = b"\x89PNG\r\n\x1a\nfake image";
    let put = http(
        s.rig.port,
        "POST",
        "/blob?machine=studio&name=shot.png",
        &headers,
        Some(png),
    )
    .await;
    assert_eq!(put.status, 200, "{}", put.body);
    let hash = put.json()["hash"].as_str().expect("a hash").to_owned();
    // The blob is on the machine, and not on the core of the listener.
    assert!(s.rig.h.handle.blob(&hash).await.expect("a read").is_none());
    let got = http(
        s.rig.port,
        "GET",
        &format!("/blob/{hash}?machine=studio"),
        &headers,
        None,
    )
    .await;
    assert_eq!(got.status, 200, "{}", got.body);
    assert_eq!(got.bytes, png);

    // The path of the blob goes into the pane of the machine.
    let mut ws = s
        .socket("/rpc?machine=studio", &token)
        .await
        .expect("the socket of the machine");
    call(&mut ws, 1, "phone.window", json!({})).await;
    let dropped = call(
        &mut ws,
        2,
        "pane.drop_blobs",
        json!({
            "pane": pane_id,
            "input_id": 1,
            "blobs": [{ "hash": hash, "name": "shot.png" }]
        }),
    )
    .await;
    assert!(dropped["result"].is_object(), "{dropped}");
    wait_for(&pane, "the path of the file", |rows| {
        rows.iter().any(|r| r.contains("shot.png"))
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_machine_that_is_unknown_or_down_is_refused() {
    let s = setup().await;
    let (token, _) = s.rig.paired().await;
    assert_eq!(
        refused_with(s.socket("/rpc?machine=nope", &token).await),
        404
    );
    assert_eq!(
        refused_with(s.socket("/rpc?machine=down", &token).await),
        503
    );
    // `local` is the core of the listener.
    let mut ws = s
        .socket("/rpc?machine=local", &token)
        .await
        .expect("the socket of the listener");
    let reply = call(&mut ws, 1, "phone.window", json!({})).await;
    assert!(reply["result"]["window"].is_string(), "{reply}");
    // A listener without the machines of a host (`nh daemon`) knows no other machine.
    let alone = Rig::start();
    let (token, _) = alone.paired().await;
    let refused = connect(
        alone.port,
        &alone.host(),
        &alone.origin(),
        "/rpc?machine=studio",
        Some(&token),
    )
    .await;
    assert_eq!(refused_with(refused), 404);
}
