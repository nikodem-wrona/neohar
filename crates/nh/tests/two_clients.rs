//! Two clients on one core: two `LinkClient`s with
//! two device ids call one core through `link.sock`. Each has its own windows, the geometry lease
//! follows the window that sized or typed last, each window has its own read state, both clients
//! get the notifications and the first answer wins, a client that goes leaves its rows without
//! focus, and the gate follows the focus of a window of the second client. A window call for the
//! window of another client is `permission_denied`.

mod core_support;

use std::{
    collections::BTreeMap,
    io::Write as _,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::Arc,
    time::{Duration, Instant},
};

use core_support::{Harness, TIMEOUT, next_input_id};
use nh_core::{
    ErrorCode, MessageSink, Origin, RecordingHost, Tuning,
    host::{HostCall, HostNotification},
};
use nh_proto::api::{ids::WindowId, projections::Projection};
use nh_remote::{ClientHello, ClientOptions, LinkClient, LinkServer, LinkTiming};
use serde_json::{Value, json};
use tokio::sync::mpsc;
use ulid::Ulid;

const NH: &str = env!("CARGO_BIN_EXE_nh");

/// The device of the first client, the laptop.
const LAPTOP: Ulid = Ulid::from_parts(1, 0xA1);
/// The device of the second client, the studio.
const STUDIO: Ulid = Ulid::from_parts(1, 0xB2);

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

/// One client: its link and the host that writes down what the core asks of it.
struct Side {
    link: LinkClient,
    host: Arc<RecordingHost>,
    device: Ulid,
}

/// A client of `device` joins the core, and the test waits until the core has attached it.
async fn join(server: &LinkServer, device: Ulid, name: &str) -> Side {
    let host = Arc::new(RecordingHost::new(PathBuf::from("/bin/nh"), true));
    let link = LinkClient::connect_unix(
        server.path(),
        ClientHello {
            device,
            name: name.to_owned(),
            app_version: "0.0.0".to_owned(),
        },
        host.clone(),
        ClientOptions {
            timing: timing(),
            ..ClientOptions::default()
        },
    )
    .await
    .expect("the link opens");
    eventually("the attach of the client", || {
        server.links().iter().any(|l| l.client.device == device)
    })
    .await;
    Side { link, host, device }
}

impl Side {
    async fn call(&self, window: Option<WindowId>, method: &str, params: Value) -> Value {
        self.link
            .call(window, method, params)
            .await
            .unwrap_or_else(|e| panic!("{method} failed: {e}"))
    }

    async fn refused(&self, window: Option<WindowId>, method: &str, params: Value) -> String {
        self.link
            .call(window, method, params)
            .await
            .expect_err(method)
            .code
            .to_string()
    }

    async fn open_window(&self, params: Value) -> WindowId {
        let opened = self.call(None, "window.open", params).await;
        window_id(&opened["window"])
    }

    /// The notifications that this client was asked to show.
    fn notified(&self) -> Vec<HostNotification> {
        self.host
            .calls()
            .into_iter()
            .filter_map(|call| match call {
                HostCall::Notify(_, note) => Some(note),
                _ => None,
            })
            .collect()
    }
}

fn window_id(value: &Value) -> WindowId {
    value.as_str().expect("an id").parse().expect("a window id")
}

async fn eventually(what: &str, done: impl FnMut() -> bool) {
    eventually_within(TIMEOUT, what, done).await;
}

async fn eventually_within(within: Duration, what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + within;
    while !done() {
        assert!(Instant::now() < deadline, "waited for {what}");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// A row of a projection of the core, as the last flush made it.
fn row(h: &Harness, projection: Projection, key: &str) -> Option<Value> {
    h.handle
        .projections()
        .snapshot(projection)
        .items
        .get(key)
        .cloned()
}

/// Waits until the row of the window fits `done`, and returns it.
async fn window_row(h: &Harness, window: WindowId, done: impl Fn(&Value) -> bool) -> Value {
    let key = window.to_string();
    let deadline = Instant::now() + TIMEOUT;
    loop {
        if let Some(found) = row(h, Projection::Windows, &key).filter(|r| done(r)) {
            return found;
        }
        assert!(
            Instant::now() < deadline,
            "the row of {key} never fit: {:?}",
            row(h, Projection::Windows, &key)
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

struct Collect(mpsc::UnboundedSender<Value>);

impl MessageSink for Collect {
    fn send(&self, message: Value) -> bool {
        self.0.send(message).is_ok()
    }
}

/// The snapshot of a projection, as a client of the link sees it.
async fn snapshot_of(side: &Side, name: &str) -> BTreeMap<String, Value> {
    let (tx, mut messages) = mpsc::unbounded_channel();
    let subscription = side
        .link
        .subscribe(name, None, Box::new(Collect(tx)))
        .await
        .unwrap_or_else(|e| panic!("subscribe {name}: {e}"));
    let message = tokio::time::timeout(TIMEOUT, async {
        loop {
            let message = messages.recv().await.expect("the subscription lives");
            if message["type"] == "snapshot" {
                return message;
            }
        }
    })
    .await
    .expect("a snapshot");
    side.link.unsubscribe(&subscription);
    message["data"]["items"]
        .as_object()
        .map(|items| items.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
        .unwrap_or_default()
}

/// The windows that a client sees as its own: the rows whose `client` is its device.
async fn own_windows(side: &Side) -> Vec<String> {
    snapshot_of(side, "windows")
        .await
        .into_iter()
        .filter(|(_, row)| row["client"] == side.device.to_string())
        .map(|(id, _)| id)
        .collect()
}

// --- the hook path of a pane, as in hook_gate.rs --------------------------------------------------

fn payload() -> Value {
    json!({
        "hook_event_name": "PermissionRequest",
        "tool_name": "Bash",
        // A low risk that the title shows whole, so a notification and the tray offer Allow.
        "tool_input": {"command": "git status", "description": "Show the status"},
        "permission_mode": "default",
    })
}

const ALLOW_OUTPUT: &str = "{\"hookSpecificOutput\":{\"decision\":{\"behavior\":\"allow\"},\"hookEventName\":\"PermissionRequest\"}}\n";

/// A shim that holds a request.
struct Shim {
    child: Child,
}

impl Shim {
    fn start(socket: &Path) -> Self {
        let mut child = Command::new(NH)
            .args(["hook", "claude", "PermissionRequest"])
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("NEOHAR", "1")
            .env("NEOHAR_SOCKET", socket)
            .env("NEOHAR_PANE_TOKEN", core_support::TEST_PANE_TOKEN)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("the shim starts");
        let mut stdin = child.stdin.take().expect("stdin");
        stdin
            .write_all(payload().to_string().as_bytes())
            .expect("the payload goes in");
        drop(stdin);
        Self { child }
    }

    /// Whether the shim still waits.
    fn waits(&mut self) -> bool {
        self.child.try_wait().expect("the shim state").is_none()
    }

    /// Waits for the shim and returns (exit code, stdout).
    async fn finish(self) -> (Option<i32>, String) {
        let output = tokio::task::spawn_blocking(move || self.child.wait_with_output())
            .await
            .expect("the task")
            .expect("the shim ends");
        (
            output.status.code(),
            String::from_utf8_lossy(&output.stdout).into_owned(),
        )
    }
}

/// Sends a hook event to the core as the shim does.
async fn signal(core: &Harness, event: &str, payload: Value) {
    let socket = core.api_socket();
    let line = json!({
        "jsonrpc": "2.0", "id": 1, "method": "adapter.signal",
        "params": {"token": core_support::TEST_PANE_TOKEN, "harness": "claude",
            "event": event, "payload": payload},
    })
    .to_string()
        + "\n";
    tokio::task::spawn_blocking(move || {
        use std::io::{BufRead, BufReader};
        let mut stream = std::os::unix::net::UnixStream::connect(socket).expect("connect");
        stream.write_all(line.as_bytes()).expect("write");
        let mut reply = String::new();
        BufReader::new(&mut stream)
            .read_line(&mut reply)
            .expect("a reply");
    })
    .await
    .expect("the task");
}

/// A core with a pane in the layout and a Claude Code session in that pane, and a focus that
/// settles fast. The pane has no holder.
///
/// The core runs the fake `claude` to learn its version, and the run may gate only after that. On
/// a Mac that is busy with other builds, the probe can fail once and the run then never gates. So
/// the function starts a new core when the gate is not there after 30 s.
async fn core_with_session() -> Harness {
    for attempt in 1..=3 {
        let core = Harness::start_tuned(Tuning {
            start_holders: false,
            focus_debounce: Duration::from_millis(100),
            notify_window: Duration::from_millis(1),
            ..Tuning::default()
        })
        .with_hook_pane()
        .await;
        signal(
            &core,
            "SessionStart",
            json!({"session_id": "s1", "cwd": "/work", "source": "startup"}),
        )
        .await;
        let deadline = Instant::now() + Duration::from_secs(30);
        while Instant::now() < deadline {
            let gates = core
                .handle
                .projections()
                .snapshot(Projection::Agents)
                .items
                .values()
                .any(|run| run["capabilities"]["gate"] == true);
            if gates {
                return core;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        eprintln!(
            "attempt {attempt}: the run has no gate after 30 s: {:?}",
            core.handle.projections().snapshot(Projection::Agents).items
        );
    }
    panic!("no core gave the run of the session the capability gate");
}

async fn interactions(core: &Harness) -> Vec<Value> {
    core.call("debug.interactions", json!({})).await["interactions"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

/// Waits until an interaction fits `pred`, and returns it.
async fn wait_for(core: &Harness, what: &str, pred: impl Fn(&Value) -> bool) -> Value {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        if let Some(found) = interactions(core).await.into_iter().find(|i| pred(i)) {
            return found;
        }
        assert!(Instant::now() < deadline, "no interaction: {what}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// The harness is about to run a tool, and its hook asks: the shim holds or passes the request.
async fn request(core: &Harness, call: &str) -> Shim {
    signal(
        core,
        "PreToolUse",
        json!({"tool_use_id": call, "tool_name": "Bash", "tool_input": payload()["tool_input"]}),
    )
    .await;
    Shim::start(&core.api_socket())
}

/// The tool ended: the request is over, wherever it was answered.
async fn tool_done(core: &Harness, call: &str) {
    signal(
        core,
        "PostToolUse",
        json!({"tool_use_id": call, "tool_name": "Bash", "tool_input": payload()["tool_input"],
            "tool_response": {}}),
    )
    .await;
}

fn held(i: &Value) -> bool {
    i["state"] == "open" && i["lease"] == true
}

fn allow() -> Value {
    json!({"kind": "approval", "behavior": "allow"})
}

fn looking(window: WindowId, pane: &str, focused: bool) -> Value {
    json!({"window": window, "focused": focused, "visible": true, "active_pane": pane})
}

// --- the tests ------------------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn each_client_has_its_own_window_rows_and_restores_them() {
    let mut h = Harness::start_layout().with_hook_pane().await;
    let server = serve(&h);
    let laptop = join(&server, LAPTOP, "laptop").await;
    let studio = join(&server, STUDIO, "studio").await;
    let on_laptop = laptop.open_window(json!({})).await;
    let on_studio = studio.open_window(json!({})).await;
    window_row(&h, on_laptop, |r| r["client"] == LAPTOP.to_string()).await;
    window_row(&h, on_studio, |r| r["client"] == STUDIO.to_string()).await;

    // Both clients see both rows, and each one takes the rows with its own device.
    assert_eq!(snapshot_of(&laptop, "windows").await.len(), 2);
    assert_eq!(own_windows(&laptop).await, [on_laptop.to_string()]);
    assert_eq!(own_windows(&studio).await, [on_studio.to_string()]);

    // Both clients go, and the core restarts. The rows are in the database.
    drop(laptop);
    drop(studio);
    eventually("the end of both links", || server.links().is_empty()).await;
    drop(server);
    h.crash().await;
    h.restart();
    let server = serve(&h);
    let laptop = join(&server, LAPTOP, "laptop").await;
    assert_eq!(own_windows(&laptop).await, [on_laptop.to_string()]);
    let studio = join(&server, STUDIO, "studio").await;
    assert_eq!(own_windows(&studio).await, [on_studio.to_string()]);
    // The rows of a client that is away have no focus: nothing reported yet after the restart.
    for window in [on_laptop, on_studio] {
        let r = window_row(&h, window, |_| true).await;
        assert_eq!(
            (r["focused"].clone(), r["visible"].clone()),
            (false.into(), false.into())
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_window_call_for_the_window_of_another_client_is_permission_denied() {
    let h = Harness::start_layout().with_hook_pane().await;
    let server = serve(&h);
    let laptop = join(&server, LAPTOP, "laptop").await;
    let studio = join(&server, STUDIO, "studio").await;
    let mine = laptop.open_window(json!({})).await;
    let theirs = studio.open_window(json!({})).await;

    let calls = |window: WindowId| {
        [
            (
                "window.focus_report",
                json!({"window": window, "focused": true, "visible": true}),
            ),
            (
                "window.report_metrics",
                json!({"window": window, "cell_width_px": 8.0, "cell_height_px": 16.0,
                    "device_pixel_ratio": 1.0, "renderer": "webgl2"}),
            ),
            ("render.ack", json!({"window": window, "acks": []})),
            ("window.close", json!({"window": window})),
        ]
    };
    // The laptop may not touch the window of the studio, and nothing changes.
    for (method, params) in calls(theirs) {
        assert_eq!(
            laptop.refused(None, method, params).await,
            "permission_denied",
            "{method}"
        );
    }
    let seen = window_row(&h, theirs, |_| true).await;
    assert_eq!(seen["focused"], false, "the refused report changed nothing");
    // The host in the process has a device too: it is another client for this window.
    for (method, params) in calls(theirs) {
        let error = h
            .handle
            .rpc(method, params)
            .await
            .expect_err("another device");
        assert_eq!(error.code, ErrorCode::PermissionDenied, "{method}");
    }
    // A call with no device (the dev socket of the app) keeps working.
    for (method, params) in [
        (
            "window.focus_report",
            json!({"window": theirs, "focused": true, "visible": true}),
        ),
        ("render.ack", json!({"window": theirs, "acks": []})),
    ] {
        h.handle
            .call_as(Origin::ui(), method, params)
            .await
            .unwrap_or_else(|e| panic!("{method} with no device: {e}"));
    }
    window_row(&h, theirs, |r| r["focused"] == true).await;

    // A client may do all of it for its own window; the close comes last.
    for (method, params) in calls(mine) {
        laptop.call(None, method, params).await;
    }
    eventually("the close of the own window", || {
        row(&h, Projection::Windows, &mine.to_string()).is_none()
    })
    .await;
    assert!(row(&h, Projection::Windows, &theirs.to_string()).is_some());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_geometry_lease_goes_to_the_window_that_sized_or_typed_last_also_across_clients() {
    let h = Harness::start();
    let server = serve(&h);
    let laptop = join(&server, LAPTOP, "laptop").await;
    let studio = join(&server, STUDIO, "studio").await;
    let created = h
        .call(
            "workspace.create",
            json!({ "command": { "argv": ["/bin/sh", "-c", "exec cat"] } }),
        )
        .await;
    let pane_id: Ulid = created["pane"]
        .as_str()
        .and_then(|id| id.parse::<nh_proto::api::ids::PaneId>().ok())
        .map(nh_proto::api::ids::PaneId::ulid)
        .expect("a pane id");
    let deadline = Instant::now() + TIMEOUT;
    let pane = loop {
        if let Some(pane) = h.handle.pane(pane_id) {
            break pane;
        }
        assert!(Instant::now() < deadline, "the holder did not start");
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    let one = laptop.open_window(json!({})).await;
    let two = studio.open_window(json!({})).await;
    let pane_ref = created["pane"].clone();
    let size_is = |want: (u16, u16)| {
        let pane = Arc::clone(&pane);
        async move {
            eventually("the size of the pane", || pane.size() == want).await;
        }
    };

    // The window that sizes first owns the lease, also when it belongs to the other client.
    resize(&laptop, &pane_ref, one, 100, 30).await;
    size_is((100, 30)).await;
    resize(&studio, &pane_ref, two, 50, 20).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(pane.size(), (100, 30));
    assert_eq!(pane.info().geometry_owner, Some(one));

    // Input from the window of the studio takes the lease after the 500 ms of the last change.
    tokio::time::sleep(Duration::from_millis(600)).await;
    type_in(&studio, &pane_ref, two, "a").await;
    size_is((50, 20)).await;
    assert_eq!(pane.info().geometry_owner, Some(two));
    // The laptop types at once: the lease stays. After the pause it goes back.
    type_in(&laptop, &pane_ref, one, "b").await;
    assert_eq!(pane.info().geometry_owner, Some(two));
    tokio::time::sleep(Duration::from_millis(600)).await;
    type_in(&laptop, &pane_ref, one, "c").await;
    size_is((100, 30)).await;
    assert_eq!(pane.info().geometry_owner, Some(one));
}

async fn resize(side: &Side, pane: &Value, window: WindowId, cols: u16, rows: u16) {
    side.call(
        Some(window),
        "pane.resize",
        json!({"pane": pane, "cols": cols, "rows": rows, "window": window}),
    )
    .await;
}

async fn type_in(side: &Side, pane: &Value, window: WindowId, text: &str) {
    side.call(
        Some(window),
        "pane.input",
        json!({"pane": pane, "input_id": next_input_id(),
            "event": {"type": "text", "text": text}}),
    )
    .await;
}

fn read_state(row: Option<Value>) -> String {
    row.expect("a row")["facets"]["read_state"]
        .as_str()
        .expect("a read state")
        .to_owned()
}

/// The read state of a run in the projection of a window, as a client of the link reads it.
async fn seen_by(side: &Side, window: WindowId, run: &str) -> String {
    let name = format!("agents_for:{window}");
    read_state(snapshot_of(side, &name).await.remove(run))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn each_window_keeps_its_own_read_state_across_clients() {
    let h = core_with_session().await;
    let server = serve(&h);
    let laptop = join(&server, LAPTOP, "laptop").await;
    let studio = join(&server, STUDIO, "studio").await;
    let on_laptop = laptop.open_window(json!({})).await;
    let on_studio = studio.open_window(json!({})).await;
    let run = h
        .handle
        .projections()
        .snapshot(Projection::Agents)
        .items
        .keys()
        .next()
        .expect("a run")
        .clone();
    let state_of = |window: WindowId| row(&h, Projection::AgentsFor(window), &run);
    let flush = || h.handle.projections().flush();

    // The windows opened after the run. The next change of the run is news for both.
    signal(
        &h,
        "UserPromptSubmit",
        json!({"session_id": "s1", "cwd": "/work", "prompt": "go"}),
    )
    .await;
    eventually("unseen in both windows", || {
        flush();
        state_of(on_laptop).is_some_and(|r| read_state(Some(r)) == "unseen")
            && state_of(on_studio).is_some_and(|r| read_state(Some(r)) == "unseen")
    })
    .await;

    // The laptop looks at the pane. It has seen the run, and the studio has not.
    let pane = h.hook_pane();
    laptop
        .call(None, "window.focus_report", looking(on_laptop, &pane, true))
        .await;
    eventually("seen on the laptop only", || {
        flush();
        read_state(state_of(on_laptop)) == "seen" && read_state(state_of(on_studio)) == "unseen"
    })
    .await;
    // The next change comes while the laptop looks: only the laptop sees it.
    signal(&h, "Stop", json!({"session_id": "s1", "cwd": "/work"})).await;
    eventually("the run is idle", || {
        h.handle
            .projections()
            .snapshot(Projection::Agents)
            .items
            .get(&run)
            .is_some_and(|r| r["facets"]["execution"]["value"]["state"] == "idle")
    })
    .await;
    flush();
    assert_eq!(read_state(state_of(on_laptop)), "seen");
    assert_eq!(read_state(state_of(on_studio)), "unseen");
    // Each client reads the projection of its own window.
    assert_eq!(seen_by(&laptop, on_laptop, &run).await, "seen");
    assert_eq!(seen_by(&studio, on_studio, &run).await, "unseen");

    // The studio looks too, and has then seen it.
    studio
        .call(None, "window.focus_report", looking(on_studio, &pane, true))
        .await;
    eventually("seen on the studio too", || {
        flush();
        read_state(state_of(on_studio)) == "seen"
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn both_clients_get_the_notification_and_the_first_answer_wins() {
    let h = core_with_session().await;
    let server = serve(&h);
    let laptop = join(&server, LAPTOP, "laptop").await;
    let studio = join(&server, STUDIO, "studio").await;
    let on_laptop = laptop.open_window(json!({})).await;
    studio.open_window(json!({})).await;

    let shim = request(&h, "toolu_1").await;
    let open = wait_for(&h, "a held approval", held).await;
    let id = open["id"].as_str().expect("an id").to_owned();
    // Both clients show the notification with its buttons.
    for side in [&laptop, &studio] {
        eventually("the notification of the approval", || {
            side.notified().iter().any(|n| {
                n.interaction.map(|i| i.to_string()).as_deref() == Some(id.as_str())
                    && n.actions.len() == 2
            })
        })
        .await;
    }

    // The laptop clicks first. The studio clicks late.
    let click = |side: &Side, window: Option<WindowId>| {
        let params = json!({"interaction": id, "decision": allow(), "via": "notification"});
        let link = side.link.clone();
        async move { link.call(window, "interaction.answer", params).await }
    };
    click(&laptop, Some(on_laptop))
        .await
        .expect("the first answer wins");
    let late = click(&studio, None).await.expect_err("the second answer");
    assert_eq!(late.code, ErrorCode::AlreadyAnswered);
    let (code, stdout) = shim.finish().await;
    assert_eq!((code, stdout.as_str()), (Some(0), ALLOW_OUTPUT));

    // Only the client that clicked late sees who answered.
    let already = |side: &Side| {
        side.notified()
            .into_iter()
            .filter(|n| n.title == "Already answered")
            .collect::<Vec<_>>()
    };
    eventually("the note on the studio", || !already(&studio).is_empty()).await;
    let note = already(&studio).remove(0);
    assert!(note.body.contains("answered by a notification"), "{note:?}");
    assert!(already(&laptop).is_empty());
    // Both clients lose the notification.
    for side in [&laptop, &studio] {
        eventually("the withdraw of the notification", || {
            side.host
                .calls()
                .iter()
                .any(|c| matches!(c, HostCall::WithdrawNotification(_)))
        })
        .await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_client_that_goes_leaves_its_rows_without_focus_and_the_gate_holds_again() {
    let h = core_with_session().await;
    let server = serve(&h);
    let laptop = join(&server, LAPTOP, "laptop").await;
    let studio = join(&server, STUDIO, "studio").await;
    let on_laptop = laptop.open_window(json!({})).await;
    let on_studio = studio.open_window(json!({})).await;
    let pane = h.hook_pane();

    // The studio looks at the pane, and the laptop has the focus in the chrome.
    studio
        .call(None, "window.focus_report", looking(on_studio, &pane, true))
        .await;
    laptop
        .call(
            None,
            "window.focus_report",
            json!({"window": on_laptop, "focused": true, "visible": true}),
        )
        .await;
    window_row(&h, on_studio, |r| {
        r["focused"] == true && r["visible"] == true
    })
    .await;
    // On a focused pane the harness shows its own dialog: the hook is not held.
    tokio::time::sleep(Duration::from_millis(400)).await;
    let (code, stdout) = request(&h, "toolu_1").await.finish().await;
    assert_eq!((code, stdout.as_str()), (Some(0), ""));
    tool_done(&h, "toolu_1").await;

    // The studio goes. Its row stays and has no focus and no visible state.
    drop(studio);
    eventually("the end of the link", || server.links().len() == 1).await;
    let gone = window_row(&h, on_studio, |r| {
        r["focused"] == false && r["visible"] == false
    })
    .await;
    assert_eq!(gone["client"], STUDIO.to_string(), "the row stays");
    assert!(gone["focused_pane"].is_null(), "no focused pane: {gone}");
    // The window of the laptop is as it was.
    let kept = window_row(&h, on_laptop, |_| true).await;
    assert_eq!(kept["focused"], true);
    // The pane has no focus after the debounce, so a new approval is held.
    tokio::time::sleep(Duration::from_millis(400)).await;
    let mut shim = request(&h, "toolu_2").await;
    let open = wait_for(&h, "a held approval", held).await;
    assert_eq!(open["mode"], "gate");
    assert!(shim.waits());
    let id = open["id"].as_str().expect("an id").to_owned();
    laptop
        .call(
            None,
            "interaction.answer",
            json!({"interaction": id, "decision": allow(), "via": "card"}),
        )
        .await;
    assert_eq!(shim.finish().await.0, Some(0));

    // The studio comes back. Its row is still its own, and it has to report its focus again.
    let studio = join(&server, STUDIO, "studio").await;
    assert_eq!(own_windows(&studio).await, [on_studio.to_string()]);
    window_row(&h, on_studio, |r| r["focused"] == false).await;
    studio
        .call(None, "window.focus_report", looking(on_studio, &pane, true))
        .await;
    window_row(&h, on_studio, |r| r["focused"] == true).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_approval_in_a_pane_that_only_the_studio_shows_follows_the_focus_of_the_studio() {
    let h = core_with_session().await;
    let server = serve(&h);
    let laptop = join(&server, LAPTOP, "laptop").await;
    let studio = join(&server, STUDIO, "studio").await;
    let hooks = h.hook_pane();
    // The laptop shows another pane. Only the window of the studio shows the pane of the hooks.
    let other = h.call("workspace.create", json!({ "name": "Other" })).await["pane"].clone();
    let on_laptop = laptop.open_window(json!({"panes": [other]})).await;
    let on_studio = studio.open_window(json!({"panes": [hooks]})).await;
    studio
        .call(
            None,
            "window.focus_report",
            looking(on_studio, &hooks, false),
        )
        .await;

    let mut shim = request(&h, "toolu_1").await;
    let open = wait_for(&h, "a held approval", held).await;
    assert_eq!(
        open["mode"], "gate",
        "the pane has no focus: the hook waits"
    );
    // The laptop has the focus, and its active element is the pane, which its window does not
    // show. That is no focus on the pane.
    laptop
        .call(
            None,
            "window.focus_report",
            looking(on_laptop, &hooks, true),
        )
        .await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(
        shim.waits(),
        "a window that does not show the pane releases nothing"
    );
    assert_eq!(wait_for(&h, "still held", held).await["mode"], "gate");

    // The studio gets the focus on the pane: the hook is released, and the harness shows its
    // own dialog.
    studio
        .call(
            None,
            "window.focus_report",
            looking(on_studio, &hooks, true),
        )
        .await;
    let (code, stdout) = shim.finish().await;
    assert_eq!((code, stdout.as_str()), (Some(0), ""));
    let released = wait_for(&h, "observe", |i| i["mode"] == "observe").await;
    assert_eq!(released["state"], "open");
    assert_eq!(released["answerable"], true);
    tool_done(&h, "toolu_1").await;

    // While the studio looks, the next request is not held.
    tokio::time::sleep(Duration::from_millis(300)).await;
    let (code, stdout) = request(&h, "toolu_2").await.finish().await;
    assert_eq!((code, stdout.as_str()), (Some(0), ""));
    tool_done(&h, "toolu_2").await;

    // The studio steps away from the app. The next request is held again.
    studio
        .call(
            None,
            "window.focus_report",
            looking(on_studio, &hooks, false),
        )
        .await;
    tokio::time::sleep(Duration::from_millis(400)).await;
    let mut shim = request(&h, "toolu_3").await;
    let again = wait_for(&h, "a held hook", |i| i["mode"] == "gate" && held(i)).await;
    assert!(shim.waits());
    let id = again["id"].as_str().expect("an id").to_owned();
    // Both clients get the notification of the held approval.
    for side in [&laptop, &studio] {
        eventually("the notification of the held approval", || {
            side.notified()
                .iter()
                .any(|n| n.interaction.map(|i| i.to_string()).as_deref() == Some(id.as_str()))
        })
        .await;
    }
    let answered = laptop
        .call(
            None,
            "interaction.answer",
            json!({"interaction": id, "decision": allow(), "via": "notification"}),
        )
        .await;
    assert!(answered.is_object());
    let (code, stdout) = shim.finish().await;
    assert_eq!((code, stdout.as_str()), (Some(0), ALLOW_OUTPUT));
}
