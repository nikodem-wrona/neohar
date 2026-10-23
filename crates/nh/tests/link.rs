//! `link.sock` and `link/1` with a real core: a client over the socket
//! calls, subscribes, opens a render stream of a real pane and gets blobs. The core refuses a
//! `Hello` of another protocol or API. The subscriptions of a link end with it. Host requests go
//! through a `LinkHost` to the host of the client, within the limits of the client.

mod core_support;

use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use core_support::{Harness, TIMEOUT, has_row, next_input_id, wait_for};
use nh_core::{
    ErrorCode, FrameSink, Host, MessageSink, RecordingHost,
    host::{HostCall, HostNotification, NotificationAction},
};
use nh_proto::{
    api::{
        common::Urgency,
        ids::{InteractionId, WindowId},
        projections::Projection,
    },
    frame::{Frame, FrameKind},
    input::{LogicalInput, LogicalText},
};
use nh_remote::{
    ClientHello, ClientOptions, Device, Hello, HostOutcome, HostRequest, LinkClient, LinkMsg,
    LinkServer, LinkTiming, OpenLink,
    codec::{Decoder, Scheduler, encode},
};
use serde_json::{Value, json};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixStream,
    sync::mpsc,
    time::timeout,
};
use ulid::Ulid;

/// Short pings, so that a test sees a round-trip time.
fn timing() -> LinkTiming {
    LinkTiming {
        ping_every: Duration::from_millis(100),
        down_after: Duration::from_secs(5),
        hello_within: Duration::from_secs(5),
    }
}

fn serve(h: &Harness) -> LinkServer {
    nh_remote::serve_with(h.handle.clone(), &h.paths, "test", timing()).expect("link.sock serves")
}

fn hello() -> ClientHello {
    ClientHello {
        device: Ulid::from_parts(1, 42),
        name: "laptop".to_owned(),
        app_version: "0.0.0".to_owned(),
    }
}

async fn connect(server: &LinkServer, host: Arc<dyn Host>) -> LinkClient {
    LinkClient::connect_unix(
        server.path(),
        hello(),
        host,
        ClientOptions {
            timing: timing(),
            ..ClientOptions::default()
        },
    )
    .await
    .expect("the link opens")
}

fn recording() -> Arc<RecordingHost> {
    Arc::new(RecordingHost::new(PathBuf::from("/bin/nh"), true))
}

struct Messages(mpsc::UnboundedSender<Value>);

impl MessageSink for Messages {
    fn send(&self, message: Value) -> bool {
        self.0.send(message).is_ok()
    }
}

struct Frames(mpsc::UnboundedSender<Vec<u8>>);

impl FrameSink for Frames {
    fn send(&self, frame: Vec<u8>) -> bool {
        self.0.send(frame).is_ok()
    }
}

async fn next<T>(rx: &mut mpsc::UnboundedReceiver<T>, what: &str) -> T {
    timeout(TIMEOUT, rx.recv())
        .await
        .unwrap_or_else(|_| panic!("waited for {what}"))
        .unwrap_or_else(|| panic!("the channel ended before {what}"))
}

/// Waits until the server has attached the client: the capabilities came.
async fn attached(server: &LinkServer) -> OpenLink {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        if let Some(link) = server.links().into_iter().next() {
            return link;
        }
        assert!(Instant::now() < deadline, "the client was never attached");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

async fn eventually(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + TIMEOUT;
    while !done() {
        assert!(Instant::now() < deadline, "waited for {what}");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn calls_projections_and_a_render_stream_go_through_the_link() {
    let h = Harness::start();
    let server = serve(&h);
    let client = connect(&server, recording()).await;

    // `Welcome` names the machine and the session of the core.
    let session = h.call("session.info", json!({})).await;
    assert_eq!(client.welcome().machine_uuid, session["machine_uuid"]);
    assert_eq!(client.welcome().session_uuid, session["session_uuid"]);

    // A subscription: the snapshot, then a patch when a pane comes.
    let panes: Projection = "panes".parse().expect("a projection");
    let before = h.handle.projections().subscriber_count(panes);
    let (tx, mut messages) = mpsc::unbounded_channel();
    let subscription = client
        .subscribe("panes", None, Box::new(Messages(tx)))
        .await
        .expect("the projection exists");
    assert!(subscription.starts_with("panes#"), "{subscription}");
    assert_eq!(
        next(&mut messages, "the snapshot").await["type"],
        "snapshot"
    );

    let created = client
        .call(
            None,
            "workspace.create",
            json!({ "command": { "argv": ["/bin/cat"] } }),
        )
        .await
        .expect("a workspace");
    loop {
        let message = next(&mut messages, "a patch").await;
        if message["type"] == "patch" {
            break;
        }
    }
    // A call that the core refuses comes back as its error.
    let error = client
        .call(None, "no.such_method", json!({}))
        .await
        .expect_err("an error");
    assert_eq!(error.code, ErrorCode::MethodNotFound);

    // A render stream of a real pane: the first frame is a keyframe.
    let window = client
        .call(None, "window.open", json!({}))
        .await
        .expect("a window")["window"]
        .clone();
    let window_id: WindowId = window.as_str().expect("id").parse().expect("a window id");
    let (tx, mut frames) = mpsc::unbounded_channel();
    client
        .stream(
            "render",
            json!({ "window": window, "viewport_hz": 120.0, "proto": nh_proto::version::FRAME_PROTO }),
            Some(window_id),
            Arc::new(Frames(tx)),
        )
        .await
        .expect("the stream opens");
    let frame: Frame = postcard::from_bytes(&next(&mut frames, "a frame").await).expect("a frame");
    assert_eq!(frame.kind, FrameKind::Keyframe);
    let ack = json!({ "window": window, "acks": [{ "pane": frame.pane, "rev": frame.rev }] });
    client
        .call(Some(window_id), "render.ack", ack)
        .await
        .expect("the ack");

    // Input of class 0 runs in the order that it came, also when the calls do not wait for each
    // other.
    let pane_id = created["pane"].as_str().expect("a pane").to_owned();
    let calls = "abcdefghijklmnopqrstuvwxyz".chars().map(|c| {
        let event = serde_json::to_value(LogicalInput::Text(LogicalText {
            text: c.to_string(),
        }))
        .expect("an input");
        let client = client.clone();
        let params = json!({ "pane": pane_id, "input_id": next_input_id(), "event": event });
        async move { client.call(Some(window_id), "pane.input", params).await }
    });
    for result in futures_util::future::join_all(calls).await {
        result.expect("the input is written");
    }
    let pane = h
        .handle
        .pane(pane_id.parse().expect("a ULID"))
        .expect("the pane runs");
    wait_for(&pane, "the letters in order", |text| {
        has_row(text, "abcdefghijklmnopqrstuvwxyz")
    })
    .await;
    // The frames of the input come through the stream.
    let more: Frame =
        postcard::from_bytes(&next(&mut frames, "a frame of the input").await).expect("a frame");
    assert_eq!(more.pane, frame.pane);

    // Pings go both ways, and the client knows the round-trip time.
    eventually("a round-trip time", || client.rtt().is_some()).await;

    // `projection.unsubscribe` through `call` ends a subscription of the client.
    client
        .call(
            None,
            "projection.unsubscribe",
            json!({ "subscription": subscription }),
        )
        .await
        .expect("the subscription ends");
    eventually("the end of the subscription in the core", || {
        h.handle.projections().subscriber_count(panes) == before
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_subscriptions_of_a_link_end_with_it() {
    let h = Harness::start();
    let server = serve(&h);
    let panes: Projection = "panes".parse().expect("a projection");
    let before = h.handle.projections().subscriber_count(panes);
    let client = connect(&server, recording()).await;
    let (tx, mut messages) = mpsc::unbounded_channel();
    let one = client
        .subscribe("panes", None, Box::new(Messages(tx.clone())))
        .await
        .expect("panes");
    let _two = client
        .subscribe("panes", None, Box::new(Messages(tx)))
        .await
        .expect("panes");
    next(&mut messages, "a snapshot").await;
    assert_eq!(h.handle.projections().subscriber_count(panes), before + 2);
    // A subscription of the link is a subscription of the core.
    assert!(client.unsubscribe(&one));
    eventually("one subscription less", || {
        h.handle.projections().subscriber_count(panes) == before + 1
    })
    .await;

    attached(&server).await;
    drop(client);
    eventually("the end of the link", || server.links().is_empty()).await;
    eventually("no subscription of the link", || {
        h.handle.projections().subscriber_count(panes) == before
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_blob_comes_whole_and_with_its_type() {
    let h = Harness::start();
    let server = serve(&h);
    let client = connect(&server, recording()).await;
    // Larger than 8 parts of 64 KiB, so that the credits count.
    let bytes: Vec<u8> = (0..1_000_000u32).map(|n| (n % 253) as u8).collect();
    let hash = h
        .db()
        .put_blob(&bytes, "image/png", 1)
        .expect("the blob is stored");
    let (got, mime) = client
        .blob(&hash)
        .await
        .expect("the blob")
        .expect("it exists");
    assert_eq!(got, bytes);
    assert_eq!(mime, "image/png");
    let missing = "0".repeat(64);
    assert_eq!(client.blob(&missing).await.expect("an answer"), None);
}

/// Sends messages on a raw socket and reads the answers, for a client that does not follow the
/// rules.
struct Raw {
    stream: UnixStream,
    decoder: Decoder,
    /// Messages that came in one read with the one that `recv` gave.
    pending: VecDeque<LinkMsg>,
}

impl Raw {
    async fn connect(server: &LinkServer) -> Self {
        Self {
            stream: UnixStream::connect(server.path()).await.expect("connects"),
            decoder: Decoder::default(),
            pending: VecDeque::new(),
        }
    }

    async fn send(&mut self, msg: &LinkMsg) {
        let mut scheduler = Scheduler::default();
        scheduler.push(encode(msg).expect("encodes"));
        let mut wire = Vec::new();
        while scheduler.next_part(&mut wire) {}
        self.stream.write_all(&wire).await.expect("writes");
    }

    /// The next message that is not a ping, or `None` when the core ends the link.
    async fn recv(&mut self) -> Option<LinkMsg> {
        if let Some(msg) = self.pending.pop_front() {
            return Some(msg);
        }
        let mut buf = vec![0u8; 65_536];
        loop {
            let n = timeout(TIMEOUT, self.stream.read(&mut buf))
                .await
                .expect("an answer in time")
                .expect("reads");
            if n == 0 {
                return None;
            }
            let messages = self.decoder.feed(&buf[..n]).expect("link/1");
            self.pending.extend(
                messages
                    .into_iter()
                    .filter(|m| !matches!(m, LinkMsg::Ping { .. } | LinkMsg::Pong { .. })),
            );
            if let Some(msg) = self.pending.pop_front() {
                return Some(msg);
            }
        }
    }
}

fn raw_hello(proto: &str, api: &str) -> LinkMsg {
    LinkMsg::Hello(Hello {
        proto: proto.to_owned(),
        api: api.to_owned(),
        app_version: "9.9.9".to_owned(),
        device: Device {
            id: Ulid::from_parts(2, 2).to_string(),
            name: "old".to_owned(),
        },
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_hello_of_another_protocol_or_api_is_refused() {
    let h = Harness::start();
    let server = serve(&h);
    for (hello, reason) in [
        (raw_hello("link/2", "neohar/1"), "proto"),
        (raw_hello("link/1", "neohar/2"), "api"),
    ] {
        let mut raw = Raw::connect(&server).await;
        raw.send(&hello).await;
        assert_eq!(
            raw.recv().await,
            Some(LinkMsg::Refused {
                reason: reason.to_owned()
            })
        );
    }
    // A minor version of the same major is welcome.
    let mut raw = Raw::connect(&server).await;
    raw.send(&raw_hello("link/1", "neohar/1.7")).await;
    assert!(matches!(raw.recv().await, Some(LinkMsg::Welcome(_))));
    // A message that is not `Hello` first ends the link.
    let mut raw = Raw::connect(&server).await;
    raw.send(&LinkMsg::Subscribe {
        sub: 1,
        name: "panes".to_owned(),
        since: None,
    })
    .await;
    assert!(matches!(raw.recv().await, Some(LinkMsg::Bye { .. })));
    assert_eq!(raw.recv().await, None);
    // A client of the same versions is welcome.
    LinkClient::connect_unix(
        server.path(),
        hello(),
        recording(),
        ClientOptions::default(),
    )
    .await
    .expect("the same versions are welcome");
}

fn notification(title: &str) -> HostNotification {
    HostNotification {
        title: title.to_owned(),
        body: "Allow `rm -rf build`?".to_owned(),
        urgency: Urgency::High,
        interaction: None,
        pane: None,
        actions: vec![NotificationAction::Allow, NotificationAction::Deny],
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn host_requests_reach_the_client_within_the_limits() {
    let h = Harness::start();
    let server = serve(&h);
    let host = recording();
    let client = connect(&server, host.clone()).await;
    let link = attached(&server).await;
    assert_eq!(link.client.device, hello().device);
    assert_eq!(link.client.name, "laptop");
    let core_host: Arc<dyn Host> = link.host.clone();
    // The answer of the client after `Welcome`.
    assert!(core_host.notification_capabilities().actions);

    let mine = WindowId::new(Ulid::from_parts(3, 1));
    let other = WindowId::new(Ulid::from_parts(3, 2));
    client.own_window(mine, true);
    // At attach, the router gives the new client the badge and the tray. A request of the test
    // goes after them, so the calls of the test start after it.
    core_host.set_badge(100);
    eventually("the calls of the attach", || {
        host.calls().last() == Some(&HostCall::SetBadge(100))
    })
    .await;
    let start = host.calls().len();

    core_host.notify(notification("Claude needs you"));
    core_host.set_badge(3);
    core_host.clipboard_write("copied".to_owned());
    core_host.focus_window(mine);
    core_host.open_settings();
    // Refused: a window of another client, text over the limits.
    core_host.focus_window(other);
    core_host.notify(notification(&"t".repeat(201)));
    core_host.clipboard_write("x".repeat((1 << 20) + 1));
    // Never sent: a URL.
    core_host.open_url("https://example.com".to_owned());
    // The next request shows that the ones before it were handled.
    core_host.set_badge(4);
    eventually("the last badge", || {
        host.calls().last() == Some(&HostCall::SetBadge(4))
    })
    .await;
    let calls = host.calls()[start..].to_vec();
    assert!(matches!(&calls[0], HostCall::Notify(_, n) if n.title == "Claude needs you"));
    assert_eq!(
        calls[1..],
        [
            HostCall::SetBadge(3),
            HostCall::ClipboardWrite("copied".to_owned()),
            HostCall::FocusWindow(mine),
            HostCall::OpenSettings,
            HostCall::SetBadge(4),
        ]
    );

    // A machine posts at most 10 notifications in 10 s: one is shown, 9 more fit.
    for n in 0..12 {
        core_host.notify(notification(&format!("n{n}")));
    }
    core_host.set_badge(5);
    eventually("the badge after the notifications", || {
        host.calls().last() == Some(&HostCall::SetBadge(5))
    })
    .await;
    let shown = host
        .calls()
        .iter()
        .filter(|c| matches!(c, HostCall::Notify(..)))
        .count();
    assert_eq!(shown, 10);

    // The client of a link that ended is detached, and its host sends nothing more.
    drop(client);
    eventually("the end of the link", || link.host.is_down()).await;
    core_host.set_badge(6);
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_ne!(host.calls().last(), Some(&HostCall::SetBadge(6)));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_new_client_shows_every_open_approval_and_the_next_one() {
    let h = Harness::start();
    let server = serve(&h);
    // More open approvals than the limit of 10 notifications in 10 s.
    let approval = |n: u128| HostNotification {
        interaction: Some(InteractionId::new(Ulid::from_parts(7, n))),
        ..notification(&format!("approval {n}"))
    };
    for n in 0..12 {
        h.handle.router().notify(approval(n));
    }
    let host = recording();
    let _client = connect(&server, host.clone()).await;
    attached(&server).await;
    let notified = || {
        host.calls()
            .iter()
            .filter(|c| matches!(c, HostCall::Notify(..)))
            .count()
    };
    eventually("the 12 open approvals", || notified() == 12).await;
    // The attach took one notification of the limit, so a new approval shows too.
    h.handle.router().notify(approval(12));
    eventually("the new approval", || notified() == 13).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_call_before_the_capabilities_waits_for_the_host_of_its_client() {
    let h = Harness::start();
    let server = serve(&h);
    let mut raw = Raw::connect(&server).await;
    raw.send(&raw_hello("link/1", nh_proto::version::API_VERSION))
        .await;
    assert!(matches!(raw.recv().await, Some(LinkMsg::Welcome(_))));
    let Some(LinkMsg::HostRequest { id: asked, request }) = raw.recv().await else {
        panic!("expected the request of the capabilities");
    };
    assert_eq!(
        postcard::from_bytes::<HostRequest>(&request).expect("a request"),
        HostRequest::NotificationCapabilities
    );
    // The settings window opens on the client that asked, so the call needs the host of this
    // client, which is not attached before the answer.
    raw.send(&LinkMsg::Call {
        id: 1,
        window: None,
        device: None,
        method: "palette.execute".to_owned(),
        params: serde_json::to_vec(&json!({ "command": "settings.open" })).expect("JSON"),
    })
    .await;
    assert!(
        timeout(Duration::from_millis(300), raw.recv())
            .await
            .is_err(),
        "the call waits for the attach"
    );
    raw.send(&LinkMsg::HostReply {
        id: asked,
        result: HostOutcome::Capabilities { actions: true },
    })
    .await;
    let (mut replied, mut opened) = (false, false);
    while !(replied && opened) {
        match raw.recv().await.expect("the link stays") {
            LinkMsg::Reply { id: 1, .. } => replied = true,
            LinkMsg::HostRequest { request, .. } => {
                opened |= postcard::from_bytes::<HostRequest>(&request)
                    .is_ok_and(|r| r == HostRequest::OpenSettings);
            }
            _ => {}
        }
    }
    assert_eq!(server.links().len(), 1);
}
