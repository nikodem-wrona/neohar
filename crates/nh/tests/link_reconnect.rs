//! The reconnect of a link against real cores.
//!
//! A `PersistentLink` reaches `link.sock` through a proxy that the test controls: it can cut every
//! connection (the link goes down with no `Bye`), and it can hold back the replies of the core, so
//! that a call is in flight when the cut comes. The pauses of the tries are in the unit tests of
//! `nh-remote` with a paused clock; the tests here use the real clock.

mod core_support;

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use core_support::{Harness, TIMEOUT, next_input_id};
use nh_core::{ErrorCode, FrameSink, Host as _, MessageSink, RecordingHost};
use nh_proto::{
    api::ids::WindowId,
    frame::{Frame, FrameKind},
    input::{LogicalInput, LogicalText},
};
use nh_remote::{
    ClientHello, ClientOptions, Link, LinkServer, LinkStatus, LinkTiming, PersistentLink,
    UnixConnector,
};
use serde_json::{Value, json};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{UnixListener, UnixStream},
    sync::mpsc,
    task::JoinHandle,
    time::timeout,
};
use ulid::Ulid;

static COUNTER: AtomicUsize = AtomicUsize::new(1);

/// A proxy between a client and `link.sock` that the test can cut.
struct Proxy {
    path: PathBuf,
    target: Arc<Mutex<PathBuf>>,
    connections: Arc<Mutex<Vec<JoinHandle<()>>>>,
    made: Arc<AtomicUsize>,
    hold: Arc<AtomicBool>,
    accept: JoinHandle<()>,
}

impl Proxy {
    fn start(target: &Path) -> Self {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = PathBuf::from(format!("/tmp/nhp-{}-{n}.sock", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let listener = UnixListener::bind(&path).expect("the proxy listens");
        let target = Arc::new(Mutex::new(target.to_owned()));
        let connections: Arc<Mutex<Vec<JoinHandle<()>>>> = Arc::default();
        let made = Arc::new(AtomicUsize::new(0));
        let hold = Arc::new(AtomicBool::new(false));
        let accept = {
            let (target, connections, made, hold) = (
                Arc::clone(&target),
                Arc::clone(&connections),
                Arc::clone(&made),
                Arc::clone(&hold),
            );
            tokio::spawn(async move {
                loop {
                    let Ok((client, _)) = listener.accept().await else {
                        return;
                    };
                    made.fetch_add(1, Ordering::SeqCst);
                    let to = target.lock().expect("lock").clone();
                    let hold = Arc::clone(&hold);
                    let task = tokio::spawn(async move {
                        let Ok(core) = UnixStream::connect(&to).await else {
                            return;
                        };
                        let (mut client_read, mut client_write) = client.into_split();
                        let (mut core_read, mut core_write) = core.into_split();
                        let up = async move {
                            let mut buf = [0u8; 16 << 10];
                            while let Ok(n) = client_read.read(&mut buf).await {
                                if n == 0 || core_write.write_all(&buf[..n]).await.is_err() {
                                    break;
                                }
                            }
                        };
                        let down = async move {
                            let mut buf = [0u8; 16 << 10];
                            while let Ok(n) = core_read.read(&mut buf).await {
                                if n == 0 {
                                    break;
                                }
                                // A held reply is lost: the test cuts the link right after.
                                if hold.load(Ordering::SeqCst) {
                                    continue;
                                }
                                if client_write.write_all(&buf[..n]).await.is_err() {
                                    break;
                                }
                            }
                        };
                        tokio::select! { () = up => {}, () = down => {} }
                    });
                    connections.lock().expect("lock").push(task);
                }
            })
        };
        Self {
            path,
            target,
            connections,
            made,
            hold,
            accept,
        }
    }

    /// Ends every connection at once, with no `Bye`.
    fn cut(&self) {
        for task in self.connections.lock().expect("lock").drain(..) {
            task.abort();
        }
    }

    /// While held, the replies of the core do not reach the client.
    fn hold_replies(&self, hold: bool) {
        self.hold.store(hold, Ordering::SeqCst);
    }

    fn retarget(&self, target: &Path) {
        *self.target.lock().expect("lock") = target.to_owned();
    }

    fn connections_made(&self) -> usize {
        self.made.load(Ordering::SeqCst)
    }
}

impl Drop for Proxy {
    fn drop(&mut self) {
        self.accept.abort();
        self.cut();
        let _ = std::fs::remove_file(&self.path);
    }
}

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

fn host() -> Arc<RecordingHost> {
    Arc::new(RecordingHost::new(PathBuf::from("/bin/nh"), true))
}

async fn link_via(proxy: &Proxy, host: Arc<RecordingHost>) -> PersistentLink {
    let connector = Arc::new(UnixConnector {
        path: proxy.path.clone(),
        hello: ClientHello {
            device: Ulid::from_parts(1, 31),
            name: "laptop".to_owned(),
            app_version: "0.0.0".to_owned(),
        },
        host,
        options: ClientOptions {
            timing: timing(),
            ..ClientOptions::default()
        },
    });
    PersistentLink::connect(connector)
        .await
        .expect("the first link opens")
}

/// Waits until the status of the link fits `pred`.
async fn status_is(link: &PersistentLink, what: &str, pred: impl Fn(&LinkStatus) -> bool) {
    let mut status = link.status();
    let waited = timeout(TIMEOUT, async {
        loop {
            if pred(&status.borrow_and_update()) {
                return;
            }
            status.changed().await.expect("the status lives");
        }
    })
    .await;
    assert!(
        waited.is_ok(),
        "waited for {what}: {:?}",
        link.status().borrow()
    );
}

fn is_up(status: &LinkStatus) -> bool {
    matches!(status, LinkStatus::Up)
}

fn is_down(status: &LinkStatus) -> bool {
    !is_up(status)
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

/// The items of a projection that a sink has seen, built from its snapshots and patches.
#[derive(Default)]
struct Store {
    items: HashMap<String, Value>,
    snapshots: usize,
    patches: usize,
    resyncs: usize,
    version: u64,
}

impl Store {
    fn apply(&mut self, message: &Value) {
        match message["type"].as_str() {
            Some("snapshot") => {
                self.snapshots += 1;
                self.items.clear();
                for (key, value) in message["data"]["items"].as_object().into_iter().flatten() {
                    self.items.insert(key.clone(), value.clone());
                }
            }
            Some("patch") => {
                self.patches += 1;
                for op in message["ops"].as_array().into_iter().flatten() {
                    let key = op["key"].as_str().unwrap_or_default().to_owned();
                    match op["op"].as_str() {
                        Some("set") => {
                            self.items.insert(key, op["value"].clone());
                        }
                        Some("remove") => {
                            self.items.remove(&key);
                        }
                        _ => {}
                    }
                }
            }
            Some("resync") => self.resyncs += 1,
            _ => {}
        }
        if let Some(version) = message["version"].as_u64() {
            self.version = version;
        }
    }
}

/// Drains `rx` into `store` until `done` holds, or fails after the timeout.
async fn fill(
    rx: &mut mpsc::UnboundedReceiver<Value>,
    store: &mut Store,
    what: &str,
    done: impl Fn(&Store) -> bool,
) {
    let deadline = Instant::now() + TIMEOUT;
    while !done(store) {
        let left = deadline
            .checked_duration_since(Instant::now())
            .unwrap_or_else(|| panic!("waited for {what}"));
        let message = timeout(left, rx.recv())
            .await
            .unwrap_or_else(|_| panic!("waited for {what}"))
            .expect("the subscription lives");
        store.apply(&message);
    }
}

fn text_input(pane: &str, text: &str) -> (u64, Value) {
    let id = next_input_id();
    let event = serde_json::to_value(LogicalInput::Text(LogicalText {
        text: text.to_owned(),
    }))
    .expect("an input");
    (id, json!({ "pane": pane, "input_id": id, "event": event }))
}

/// A pane that writes what it reads to a file, line by line, with no echo.
async fn pane_to_file(link: &PersistentLink, file: &Path) -> String {
    let script = format!("stty -echo; cat >> {}", file.display());
    let created = link
        .call(
            None,
            "workspace.create",
            json!({ "command": { "argv": ["/bin/sh", "-c", script] } }),
        )
        .await
        .expect("a workspace");
    created["pane"].as_str().expect("a pane").to_owned()
}

/// Types a line into the pane once its holder runs: the first input can come before the process.
async fn type_ready(link: &PersistentLink, pane: &str) {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let (_, ready) = text_input(pane, "ready\r");
        match link.call(None, "pane.input", ready).await {
            Ok(_) => return,
            Err(error) if error.code == ErrorCode::PaneGone && Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            Err(error) => panic!("ready: {error:?}"),
        }
    }
}

async fn file_is(file: &Path, what: &str, expected: &str) {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let text = std::fs::read_to_string(file).unwrap_or_default();
        if text == expected {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "waited for {what}: the file has {text:?}, not {expected:?}"
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_subscription_gets_the_patches_that_it_missed_with_no_gap() {
    let h = Harness::start_layout();
    let server = serve(&h);
    let proxy = Proxy::start(server.path());
    let link = link_via(&proxy, host()).await;

    let (tx, mut rx) = mpsc::unbounded_channel();
    link.subscribe("tree", None, Box::new(Messages(tx)))
        .await
        .expect("the projection exists");
    let mut store = Store::default();
    fill(&mut rx, &mut store, "the snapshot", |s| s.snapshots == 1).await;
    h.call("workspace.create", json!({ "name": "one" })).await;
    fill(&mut rx, &mut store, "the first workspace", |s| {
        s.items.len() == 1
    })
    .await;
    let version = store.version;

    proxy.cut();
    status_is(&link, "the loss", is_down).await;
    // Two changes while the link is down: the host misses them.
    h.call("workspace.create", json!({ "name": "two" })).await;
    h.call("workspace.create", json!({ "name": "three" })).await;
    status_is(&link, "the new link", is_up).await;

    // After the new link the store is whole: the missed workspaces come with the patches that the
    // core kept, or with a new snapshot if it kept none. The host did nothing for it.
    fill(&mut rx, &mut store, "the missed workspaces", |s| {
        s.items.len() == 3
    })
    .await;
    assert!(store.version > version, "the versions only grow");
    assert_eq!(store.resyncs, 0, "the core could catch up");
    assert_eq!(store.snapshots, 1, "the catch-up came as patches");
    assert_eq!(proxy.connections_made(), 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_render_stream_comes_back_with_a_keyframe() {
    let h = Harness::start();
    let server = serve(&h);
    let proxy = Proxy::start(server.path());
    let link = link_via(&proxy, host()).await;
    link.call(
        None,
        "workspace.create",
        json!({ "command": { "argv": ["/bin/cat"] } }),
    )
    .await
    .expect("a workspace");
    let window = link
        .call(None, "window.open", json!({}))
        .await
        .expect("a window")["window"]
        .clone();
    let window_id: WindowId = window.as_str().expect("id").parse().expect("a window id");
    let (tx, mut frames) = mpsc::unbounded_channel();
    link.stream(
        "render",
        json!({ "window": window, "viewport_hz": 120.0, "proto": nh_proto::version::FRAME_PROTO }),
        Some(window_id),
        Arc::new(Frames(tx)),
    )
    .await
    .expect("the stream opens");
    let first: Frame = postcard::from_bytes(
        &timeout(TIMEOUT, frames.recv())
            .await
            .expect("a frame")
            .expect("open"),
    )
    .expect("a frame");
    assert_eq!(first.kind, FrameKind::Keyframe);

    proxy.cut();
    status_is(&link, "the loss", is_down).await;
    status_is(&link, "the new link", is_up).await;
    // A core that lost its client forgets that the window is visible. The page reports again when
    // its link comes back (`shell.tsx`), and so does this test.
    link.call(
        Some(window_id),
        "window.focus_report",
        json!({ "window": window, "focused": true, "visible": true, "active_pane": null }),
    )
    .await
    .expect("the report");
    // The new stream starts with keyframes.
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let bytes = timeout(deadline - Instant::now(), frames.recv())
            .await
            .expect("a keyframe after the cut")
            .expect("open");
        let frame: Frame = postcard::from_bytes(&bytes).expect("a frame");
        if frame.kind == FrameKind::Keyframe && proxy.connections_made() == 2 {
            break;
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_input_in_flight_at_the_cut_is_applied_once() {
    let h = Harness::start();
    let server = serve(&h);
    let proxy = Proxy::start(server.path());
    let link = link_via(&proxy, host()).await;
    let file = h.dir.join("typed.txt");
    let pane = pane_to_file(&link, &file).await;

    type_ready(&link, &pane).await;
    file_is(&file, "the first line", "ready\n").await;

    // The core gets the input, and its reply does not come back. Then the link is cut.
    proxy.hold_replies(true);
    let (input_id, once) = text_input(&pane, "once\r");
    let sent = {
        let link = link.clone();
        tokio::spawn(async move { link.call(None, "pane.input", once).await })
    };
    file_is(&file, "the input in flight", "ready\nonce\n").await;
    proxy.cut();
    proxy.hold_replies(false);

    // The host sends the same input again with the same id. The holder drops it, and the call
    // gets its answer.
    let answer = timeout(TIMEOUT, sent)
        .await
        .expect("an answer")
        .expect("task");
    assert!(answer.is_ok(), "input {input_id}: {answer:?}");
    status_is(&link, "the new link", is_up).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    file_is(&file, "one line each", "ready\nonce\n").await;
    assert_eq!(proxy.connections_made(), 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_call_in_flight_at_the_cut_gets_link_down_and_a_new_call_fails_at_once() {
    let h = Harness::start_layout();
    let server = serve(&h);
    let proxy = Proxy::start(server.path());
    let link = link_via(&proxy, host()).await;

    proxy.hold_replies(true);
    let pending = {
        let link = link.clone();
        tokio::spawn(async move { link.call(None, "workspace.list", json!({})).await })
    };
    tokio::time::sleep(Duration::from_millis(200)).await;
    proxy.cut();
    let error = timeout(TIMEOUT, pending)
        .await
        .expect("an answer")
        .expect("task")
        .expect_err("the outcome is unknown");
    assert_eq!(error.code, ErrorCode::LinkDown);
    // While the link is down a call fails at once, with the reason.
    proxy.hold_replies(false);
    let started = Instant::now();
    if let Err(error) = link.call(None, "workspace.list", json!({})).await {
        assert_eq!(error.code, ErrorCode::LinkDown);
        assert!(started.elapsed() < Duration::from_millis(100), "at once");
    }
    status_is(&link, "the new link", is_up).await;
    link.call(None, "workspace.list", json!({}))
        .await
        .expect("a call works on the new link");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_input_is_not_sent_again_to_a_core_that_restarted() {
    let mut h = Harness::start();
    let mut server = serve(&h);
    let proxy = Proxy::start(server.path());
    let link = link_via(&proxy, host()).await;
    let file = h.dir.join("typed.txt");
    let pane = pane_to_file(&link, &file).await;
    type_ready(&link, &pane).await;
    file_is(&file, "the first line", "ready\n").await;
    let epoch = link.welcome().core_epoch;

    proxy.hold_replies(true);
    let (input_id, lost) = text_input(&pane, "lost\r");
    let sent = {
        let link = link.clone();
        tokio::spawn(async move { link.call(None, "pane.input", lost).await })
    };
    file_is(&file, "the input in flight", "ready\nlost\n").await;
    proxy.cut();
    proxy.hold_replies(false);

    // The core restarts while the link is down. The holder and the pane keep running.
    drop(server);
    h.crash().await;
    h.restart();
    server = serve(&h);
    let answer = timeout(TIMEOUT, sent)
        .await
        .expect("an answer")
        .expect("task");
    let error = answer.expect_err("the input is not sent again");
    assert_eq!(error.code, ErrorCode::InputUnconfirmed);
    assert_eq!(error.details["reason"], "core_restarted");
    assert_eq!(error.details["input_id"], json!(input_id));

    status_is(&link, "the new link", is_up).await;
    assert_ne!(link.welcome().core_epoch, epoch, "another run of the core");
    tokio::time::sleep(Duration::from_millis(300)).await;
    file_is(&file, "nothing applied twice", "ready\nlost\n").await;
    drop(server);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_different_machine_ends_the_link_and_nothing_tries_again() {
    let first = Harness::start_layout();
    let second = Harness::start_layout();
    let (server_one, server_two) = (serve(&first), serve(&second));
    let proxy = Proxy::start(server_one.path());
    let link = link_via(&proxy, host()).await;

    proxy.retarget(server_two.path());
    proxy.cut();
    status_is(&link, "the stop", |s| matches!(s, LinkStatus::Stopped(_))).await;
    let LinkStatus::Stopped(reason) = link.status().borrow().clone() else {
        unreachable!()
    };
    assert_eq!(reason, "the target is now a different machine");
    let made = proxy.connections_made();
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert_eq!(proxy.connections_made(), made, "no try after the stop");
    let error = link
        .call(None, "workspace.list", json!({}))
        .await
        .expect_err("the link is gone");
    assert_eq!(error.code, ErrorCode::LinkDown);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_nudge_closes_the_link_and_opens_a_new_one_at_once() {
    let h = Harness::start_layout();
    let server = serve(&h);
    let proxy = Proxy::start(server.path());
    let link = link_via(&proxy, host()).await;
    let started = Instant::now();
    link.nudge();
    status_is(&link, "the new link", |s| {
        // The status changes on the way: down, then up. Wait for the second connection.
        is_up(s)
    })
    .await;
    let deadline = Instant::now() + TIMEOUT;
    while proxy.connections_made() < 2 {
        assert!(Instant::now() < deadline, "no second connection");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    status_is(&link, "the link up again", is_up).await;
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "a nudge does not wait for the first pause of 0.5 s and a failed try: {:?}",
        started.elapsed()
    );
    link.call(None, "workspace.list", json!({}))
        .await
        .expect("a call works on the new link");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_open_approvals_come_again_to_the_host_of_a_new_link() {
    let h = Harness::start_layout();
    let server = serve(&h);
    let proxy = Proxy::start(server.path());
    let approval = |n: u128| nh_core::host::HostNotification {
        title: format!("approval {n}"),
        body: String::new(),
        urgency: nh_proto::api::common::Urgency::Normal,
        interaction: Some(nh_proto::api::ids::InteractionId::new(Ulid::from_parts(
            7, n,
        ))),
        pane: None,
        actions: Vec::new(),
    };
    for n in 0..3 {
        h.handle.router().notify(approval(n));
    }
    let host = host();
    let link = link_via(&proxy, Arc::clone(&host)).await;
    let notified = |host: &RecordingHost| {
        host.calls()
            .iter()
            .filter(|c| matches!(c, nh_core::host::HostCall::Notify(..)))
            .count()
    };
    let deadline = Instant::now() + TIMEOUT;
    while notified(&host) < 3 {
        assert!(
            Instant::now() < deadline,
            "the open approvals at the attach"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    proxy.cut();
    status_is(&link, "the loss", is_down).await;
    status_is(&link, "the new link", is_up).await;
    // The core sends the notifications of the open interactions to the client again.
    let deadline = Instant::now() + TIMEOUT;
    while notified(&host) < 6 {
        assert!(
            Instant::now() < deadline,
            "the open approvals after the cut"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}
