//! The hook gate against the real core: the shim `nh hook` holds a
//! `PermissionRequest` until a client decides, and the core delivers the decision once.
//!
//! The harness is a test here: the tests send the payload that Claude Code sends and answer
//! with `interaction.answer` (`debug.answer` when a test holds the delivery back).
//! `tools/nh-golden/tests/delivery.rs` runs the same five cases against a live Claude Code.

mod core_support;

use std::{
    io::Write,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

use core_support::Harness;
use nh_core::{ErrorCode, Tuning};
use serde_json::{Value, json};

const NH: &str = env!("CARGO_BIN_EXE_nh");

/// The payload of a `PermissionRequest` (recorded in `tests/harness-golden`).
fn payload() -> Value {
    json!({
        "hook_event_name": "PermissionRequest",
        "tool_name": "Bash",
        "tool_input": {"command": "touch note.txt", "description": "Create a note"},
        "permission_mode": "default",
    })
}

/// A socket in front of `api.sock` for the first connection of a shim.
///
/// The core records the lease of a gate before it writes its first reply, `{"wait": true,
/// "interaction": …}`. A test that stops the core when it sees the lease can stop it before the
/// shim has that reply. The shim then has no interaction to resume, and it prints nothing, as it
/// must. The front tells the test when the reply has reached the shim. When the core ends that
/// connection, the front closes and its path becomes a link to `api.sock`, so the shim connects
/// again to the core itself, as it does in the product.
struct Front {
    path: PathBuf,
    named: tokio::sync::watch::Receiver<bool>,
}

impl Front {
    fn start(api: &Path) -> Self {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
        let path = api.with_file_name("front.sock");
        let _ = std::fs::remove_file(&path);
        let listener = tokio::net::UnixListener::bind(&path).expect("the front listens");
        let (named_tx, named) = tokio::sync::watch::channel(false);
        let (api, link) = (api.to_owned(), path.clone());
        tokio::spawn(async move {
            let Ok((shim, _)) = listener.accept().await else {
                return;
            };
            let core = tokio::net::UnixStream::connect(&api)
                .await
                .expect("the core listens");
            let (mut shim_read, mut shim_write) = shim.into_split();
            let (core_read, mut core_write) = core.into_split();
            tokio::spawn(async move {
                let _ = tokio::io::copy(&mut shim_read, &mut core_write).await;
            });
            let mut lines = tokio::io::BufReader::new(core_read);
            let mut line = Vec::new();
            loop {
                line.clear();
                match lines.read_until(b'\n', &mut line).await {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {}
                }
                if shim_write.write_all(&line).await.is_err() {
                    break;
                }
                let wait = b"\"wait\":true";
                if line.windows(wait.len()).any(|part| part == wait) {
                    let _ = named_tx.send(true);
                }
            }
            // The core ended the connection. The front closes before the shim sees the end, so
            // the shim connects to the core through the link.
            drop(listener);
            let _ = std::fs::remove_file(&link);
            std::os::unix::fs::symlink(&api, &link).expect("a link to the socket of the core");
            drop(shim_write);
        });
        Self { path, named }
    }

    /// Waits until the core named the interaction to the shim.
    async fn named(&mut self) {
        tokio::time::timeout(Duration::from_secs(30), self.named.wait_for(|named| *named))
            .await
            .expect("the core names the interaction in time")
            .expect("the front runs");
    }
}

/// A shim that holds a request.
struct Shim {
    child: Child,
}

impl Shim {
    fn start(socket: &Path, env: &[(&str, &str)]) -> Self {
        let mut command = Command::new(NH);
        command
            .args(["hook", "claude", "PermissionRequest"])
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("NEOHAR", "1")
            .env("NEOHAR_SOCKET", socket)
            .env("NEOHAR_PANE_TOKEN", "pane-1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (name, value) in env {
            command.env(name, value);
        }
        let mut child = command.spawn().expect("the shim starts");
        let mut stdin = child.stdin.take().expect("stdin");
        stdin
            .write_all(payload().to_string().as_bytes())
            .expect("the payload goes in");
        drop(stdin);
        Self { child }
    }

    fn pid(&self) -> u32 {
        self.child.id()
    }

    /// Waits for the shim and returns (exit code, stdout, stderr).
    async fn finish(self) -> (Option<i32>, String, String) {
        let output = tokio::task::spawn_blocking(move || self.child.wait_with_output())
            .await
            .expect("the task")
            .expect("the shim ends");
        (
            output.status.code(),
            String::from_utf8_lossy(&output.stdout).into_owned(),
            String::from_utf8_lossy(&output.stderr).into_owned(),
        )
    }
}

/// A core with a pane in the layout, and a Claude Code session in that pane. The session is
/// there when the first hook comes, as it is for a person, so the core knows the version of the
/// harness (the test puts a `claude` that prints 2.1.292 in the `PATH`) and gates its requests.
async fn core() -> Harness {
    let core = Harness::start_layout().with_hook_pane().await;
    session(&core).await;
    core
}

async fn core_tuned(tuning: Tuning) -> Harness {
    let core = Harness::start_tuned(Tuning {
        start_holders: false,
        ..tuning
    })
    .with_hook_pane()
    .await;
    session(&core).await;
    core
}

/// Sends `SessionStart` and waits until the run may gate (`capabilities.gate` in `agents`).
async fn session(core: &Harness) {
    signal(
        core,
        "SessionStart",
        json!({"session_id": "s1", "cwd": "/work", "source": "startup"}),
    )
    .await;
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if agents(core)
            .await
            .iter()
            .any(|a| a["capabilities"]["gate"] == true)
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "the run never got the capability gate"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// The rows of the projection `agents`.
async fn agents(core: &Harness) -> Vec<Value> {
    let mut client = core_support::client::Client::connect(&core.api_socket()).await;
    let subscribed = client
        .ok("projection.subscribe", json!({"name": "agents"}))
        .await;
    let id = subscribed["subscription"]
        .as_str()
        .expect("an id")
        .to_owned();
    loop {
        let line = client.recv().await;
        if line["method"] == "projection.message"
            && line["params"]["subscription"] == id.as_str()
            && line["params"]["message"]["type"] == "snapshot"
        {
            return line["params"]["message"]["data"]["items"]
                .as_object()
                .map(|items| items.values().cloned().collect())
                .unwrap_or_default();
        }
    }
}

async fn interactions(core: &Harness) -> Vec<Value> {
    core.call("debug.interactions", json!({})).await["interactions"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

/// Waits until an interaction fits `pred`, and returns it.
async fn wait_for(core: &Harness, what: &str, pred: impl Fn(&Value) -> bool) -> Value {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Some(found) = interactions(core).await.into_iter().find(|i| pred(i)) {
            return found;
        }
        assert!(Instant::now() < deadline, "no interaction: {what}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

fn open_with_lease(i: &Value) -> bool {
    i["state"] == "open" && i["lease"] == true
}

/// What the client sends to allow or deny. The CLI and the card use this shape.
fn decision(behavior: &str) -> Value {
    json!({"kind": "approval", "behavior": behavior})
}

/// Answers like a client of the CLI.
async fn answer(
    core: &Harness,
    id: &str,
    decision: Value,
) -> Result<Value, nh_core::error::RpcError> {
    core.handle
        .rpc(
            "interaction.answer",
            json!({"interaction": id, "decision": decision, "via": "cli"}),
        )
        .await
}

const ALLOW_OUTPUT: &str = "{\"hookSpecificOutput\":{\"decision\":{\"behavior\":\"allow\"},\"hookEventName\":\"PermissionRequest\"}}\n";

/// The `PostToolUse` of the call of the tests.
fn post_tool(call: &str) -> Value {
    json!({
        "tool_use_id": call,
        "tool_name": "Bash",
        "tool_input": payload()["tool_input"],
        "tool_response": {},
    })
}

async fn signal(core: &Harness, event: &str, payload: Value) {
    // The core takes signals from the socket only. A tiny client does what the shim does.
    let socket = core.api_socket();
    let line = json!({
        "jsonrpc": "2.0", "id": 1, "method": "adapter.signal",
        "params": {"token": "pane-1", "harness": "claude", "event": event, "payload": payload},
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_held_request_gets_the_decision_and_the_tool_signal_confirms_it() {
    let core = core().await;
    signal(
        &core,
        "PreToolUse",
        json!({"tool_use_id": "toolu_1", "tool_name": "Bash", "tool_input": payload()["tool_input"]}),
    )
    .await;
    let shim = Shim::start(&core.api_socket(), &[]);
    let open = wait_for(&core, "open with a lease", open_with_lease).await;
    assert_eq!(
        open["native_ref"], "toolu_1",
        "the id comes from the PreToolUse"
    );
    assert_eq!(
        open["shim_pid"],
        json!(shim.pid()),
        "the core knows the pid of the shim"
    );

    let id = open["id"].as_str().expect("an id").to_owned();
    answer(&core, &id, decision("allow"))
        .await
        .expect("recorded");
    let (code, stdout, stderr) = shim.finish().await;
    assert_eq!((code, stderr.as_str()), (Some(0), ""));
    assert_eq!(stdout, ALLOW_OUTPUT);

    let done = wait_for(&core, "acked", |i| i["acked"] == true).await;
    assert_eq!(done["state"], "delivering");
    signal(&core, "PostToolUse", post_tool("toolu_1")).await;
    let done = wait_for(&core, "delivered", |i| i["state"] == "delivered").await;
    assert_eq!(done["attempts"], json!(["sent", "acked"]));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_core_that_is_killed_while_the_shim_waits_delivers_once_after_the_restart() {
    let mut core = core().await;
    let mut front = Front::start(&core.api_socket());
    let shim = Shim::start(&front.path, &[]);
    let open = wait_for(&core, "open with a lease", open_with_lease).await;
    let id = open["id"].as_str().expect("an id").to_owned();
    // The shim must know its interaction before the core goes, or it has nothing to resume.
    front.named().await;

    core.crash().await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    core.restart();
    // The shim connects again and resumes. The new core read the interaction from the database.
    let back = wait_for(&core, "open with a lease again", open_with_lease).await;
    assert_eq!(back["id"], json!(id), "the same interaction");

    answer(&core, &id, decision("allow"))
        .await
        .expect("recorded");
    let (code, stdout, stderr) = shim.finish().await;
    assert_eq!((code, stderr.as_str()), (Some(0), ""));
    assert_eq!(stdout, ALLOW_OUTPUT, "one decision, printed once");
    let done = wait_for(&core, "acked", |i| i["acked"] == true).await;
    assert_eq!(done["rev"], 1);
    assert_eq!(interactions(&core).await.len(), 1, "no second interaction");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_decision_given_while_the_core_was_away_is_still_delivered_once() {
    let mut core = core().await;
    let mut front = Front::start(&core.api_socket());
    let shim = Shim::start(&front.path, &[]);
    let open = wait_for(&core, "open with a lease", open_with_lease).await;
    let id = open["id"].as_str().expect("an id").to_owned();
    // The shim must know its interaction before the core goes, or it has nothing to resume.
    front.named().await;
    // The decision is recorded and kept back. Then the core ends.
    core.call(
        "debug.answer",
        json!({"interaction": id, "decision": {"kind": "approval", "behavior": "deny", "message": "no"}, "via": "cli", "hold": true}),
    )
    .await;
    core.crash().await;
    core.restart();
    let (code, stdout, stderr) = shim.finish().await;
    assert_eq!(code, Some(0));
    assert!(
        stdout.contains("\"behavior\":\"deny\"") && stdout.contains("\"message\":\"no\""),
        "stdout {stdout:?}, stderr {stderr:?}, interactions {:?}",
        interactions(&core).await
    );
    let done = wait_for(&core, "acked", |i| i["acked"] == true).await;
    assert_eq!(done["decided_by"], "debug");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_answers_at_once_give_one_winner_and_one_already_answered() {
    let core = core().await;
    let shim = Shim::start(&core.api_socket(), &[]);
    let open = wait_for(&core, "open with a lease", open_with_lease).await;
    let id = open["id"].as_str().expect("an id").to_owned();

    let call = |behavior: &'static str, by: &'static str| {
        let handle = core.handle.clone();
        let id = id.clone();
        tokio::spawn(async move {
            handle
                .rpc(
                    "debug.answer",
                    json!({"interaction": id, "decision": {"kind": "approval", "behavior": behavior}, "via": "cli", "by": by}),
                )
                .await
        })
    };
    let (a, b) = (call("allow", "alice"), call("deny", "bob"));
    let (a, b) = (a.await.expect("task"), b.await.expect("task"));
    let (winner, loser) = match (a, b) {
        (Ok(ok), Err(error)) | (Err(error), Ok(ok)) => (ok, error),
        other => panic!("one answer must win and one must fail: {other:?}"),
    };
    assert_eq!(winner["rev"], 1);
    assert_eq!(loser.code, ErrorCode::AlreadyAnswered);
    let by = loser.details["by"].as_str().expect("a name").to_owned();
    assert!(by == "alice" || by == "bob");
    let (_, stdout, _) = shim.finish().await;
    let behavior = loser.details["decision"]["behavior"]
        .as_str()
        .expect("a behavior");
    assert!(
        stdout.contains(&format!("\"behavior\":\"{behavior}\"")),
        "the winner's decision is printed: {stdout}"
    );
    assert_eq!(
        interactions(&core).await[0]["attempts"],
        json!(["sent", "acked"])
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn releasing_the_hook_prints_nothing_and_the_tool_signal_resolves_it_elsewhere() {
    let core = core().await;
    signal(
        &core,
        "PreToolUse",
        json!({"tool_use_id": "toolu_5", "tool_name": "Bash", "tool_input": payload()["tool_input"]}),
    )
    .await;
    let shim = Shim::start(&core.api_socket(), &[]);
    let open = wait_for(&core, "open with a lease", open_with_lease).await;
    let id = open["id"].as_str().expect("an id").to_owned();
    core.call(
        "debug.release",
        json!({"interaction": id, "reason": "focused"}),
    )
    .await;
    let (code, stdout, stderr) = shim.finish().await;
    assert_eq!((code, stdout.as_str(), stderr.as_str()), (Some(0), "", ""));
    // The user answers in the dialog of the harness. The tool ends.
    signal(&core, "PostToolUse", post_tool("toolu_5")).await;
    let done = wait_for(&core, "resolved elsewhere", |i| {
        i["state"] == "resolved_elsewhere"
    })
    .await;
    assert_eq!(done["mode"], "observe");
    let late = answer(&core, &id, decision("allow"))
        .await
        .expect_err("a late answer");
    assert_eq!(late.code, ErrorCode::InteractionClosed);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_pane_that_gets_the_focus_releases_the_held_hook_and_the_shim_prints_nothing() {
    let core = core_tuned(Tuning {
        focus_debounce: Duration::from_millis(100),
        ..Tuning::default()
    })
    .await;
    signal(
        &core,
        "PreToolUse",
        json!({"tool_use_id": "toolu_6", "tool_name": "Bash", "tool_input": payload()["tool_input"]}),
    )
    .await;
    let shim = Shim::start(&core.api_socket(), &[]);
    let open = wait_for(&core, "open with a lease", open_with_lease).await;
    assert_eq!(
        open["mode"], "gate",
        "the pane has no focus: the hook waits"
    );
    let id = open["id"].as_str().expect("an id").to_owned();
    // The user looks at the pane.
    let window = core.open_window().await;
    core.look_at(&window, Some(&core.hook_pane()), true).await;
    let (code, stdout, stderr) = shim.finish().await;
    assert_eq!((code, stdout.as_str(), stderr.as_str()), (Some(0), "", ""));
    let released = wait_for(&core, "observe", |i| i["mode"] == "observe").await;
    assert_eq!(released["state"], "open");
    // The harness shows its dialog, and keys can answer it.
    assert_eq!(released["answerable"], true);
    assert_eq!(released["channels"], json!(["keystroke"]));
    // The user answers in the dialog of the harness.
    signal(&core, "PostToolUse", post_tool("toolu_6")).await;
    wait_for(&core, "resolved elsewhere", |i| {
        i["state"] == "resolved_elsewhere"
    })
    .await;
    let late = answer(&core, &id, decision("allow"))
        .await
        .expect_err("a late answer");
    assert_eq!(late.code, ErrorCode::InteractionClosed);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_pane_that_has_the_focus_does_not_hold_the_hook_of_a_new_request() {
    let core = core_tuned(Tuning {
        focus_debounce: Duration::from_millis(100),
        ..Tuning::default()
    })
    .await;
    let window = core.open_window().await;
    core.look_at(&window, Some(&core.hook_pane()), true).await;
    tokio::time::sleep(Duration::from_millis(400)).await;
    signal(
        &core,
        "PreToolUse",
        json!({"tool_use_id": "toolu_7", "tool_name": "Bash", "tool_input": payload()["tool_input"]}),
    )
    .await;
    let shim = Shim::start(&core.api_socket(), &[]);
    // The shim does not wait: it prints nothing and the harness shows its dialog.
    let (code, stdout, stderr) = shim.finish().await;
    assert_eq!((code, stdout.as_str(), stderr.as_str()), (Some(0), "", ""));
    let open = wait_for(&core, "an interaction in observe", |i| {
        i["mode"] == "observe"
    })
    .await;
    assert_eq!(open["state"], "open");
    assert_eq!(open["answerable"], true);
    assert_eq!(open["channels"], json!(["keystroke"]));
    // The user steps away from the app. The next request is held.
    core.look_at(&window, Some(&core.hook_pane()), false).await;
    tokio::time::sleep(Duration::from_millis(400)).await;
    signal(&core, "PostToolUse", post_tool("toolu_7")).await;
    signal(
        &core,
        "PreToolUse",
        json!({"tool_use_id": "toolu_8", "tool_name": "Bash", "tool_input": payload()["tool_input"]}),
    )
    .await;
    let second = Shim::start(&core.api_socket(), &[]);
    let held = wait_for(&core, "a held hook", |i| {
        i["mode"] == "gate" && open_with_lease(i)
    })
    .await;
    assert_eq!(held["state"], "open");
    let id = held["id"].as_str().expect("an id").to_owned();
    answer(&core, &id, decision("allow"))
        .await
        .expect("the decision is recorded");
    let (code, stdout, _) = second.finish().await;
    assert_eq!(code, Some(0));
    assert!(
        stdout.contains("allow"),
        "the decision reaches the shim: {stdout}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_deadline_that_passes_releases_the_hook_and_fails_the_interaction() {
    let core = core_tuned(Tuning {
        gate_timeout: Duration::from_millis(400),
        ..Tuning::default()
    })
    .await;
    let shim = Shim::start(&core.api_socket(), &[]);
    let (code, stdout, stderr) = shim.finish().await;
    assert_eq!((code, stdout.as_str(), stderr.as_str()), (Some(0), "", ""));
    let failed = wait_for(&core, "failed", |i| i["state"] == "failed").await;
    assert_eq!(failed["fail_reason"], "deadline");
    let late = answer(
        &core,
        failed["id"].as_str().expect("an id"),
        decision("allow"),
    )
    .await
    .expect_err("a late answer");
    assert_eq!(late.code, ErrorCode::InteractionClosed);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_shim_that_is_killed_before_the_decision_leaves_the_decision_unused() {
    let core = core().await;
    signal(
        &core,
        "PreToolUse",
        json!({"tool_use_id": "toolu_7", "tool_name": "Bash", "tool_input": payload()["tool_input"]}),
    )
    .await;
    let mut shim = Shim::start(&core.api_socket(), &[]);
    let open = wait_for(&core, "open with a lease", open_with_lease).await;
    let id = open["id"].as_str().expect("an id").to_owned();
    core.call(
        "debug.answer",
        json!({"interaction": id, "decision": {"kind": "approval", "behavior": "allow"}, "via": "cli", "hold": true}),
    )
    .await;
    shim.child.kill().expect("the shim is killed");
    shim.child.wait().expect("the shim is gone");
    wait_for(&core, "the lease is gone", |i| i["lease"] == false).await;
    // Nobody holds the hook, so there is nothing to deliver to. The decision waits.
    core.call("debug.deliver", json!({"interaction": id})).await;
    let waiting = wait_for(&core, "the decision waits", |i| {
        i["state"] == "decision_recorded"
    })
    .await;
    assert_eq!(waiting["acked"], false);
    // The harness shows its dialog, and the user answers there. The tool ends.
    signal(&core, "PostToolUse", post_tool("toolu_7")).await;
    let done = wait_for(&core, "resolved elsewhere", |i| {
        i["state"] == "resolved_elsewhere"
    })
    .await;
    assert_eq!(
        done["decision"]["behavior"], "allow",
        "the record keeps the decision"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_shim_that_does_not_find_the_core_after_a_drop_prints_nothing() {
    let mut core = core().await;
    let shim = Shim::start(&core.api_socket(), &[("NEOHAR_GATE_RESUME_MS", "400")]);
    wait_for(&core, "open with a lease", open_with_lease).await;
    core.crash().await;
    let (code, stdout, stderr) = shim.finish().await;
    assert_eq!((code, stdout.as_str(), stderr.as_str()), (Some(0), "", ""));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_shim_stops_waiting_when_its_own_time_is_up() {
    let core = core().await;
    let started = Instant::now();
    let shim = Shim::start(&core.api_socket(), &[("NEOHAR_GATE_TIMEOUT_MS", "500")]);
    let (code, stdout, _) = shim.finish().await;
    assert_eq!((code, stdout.as_str()), (Some(0), ""));
    assert!(started.elapsed() < Duration::from_secs(10));
    let open = &interactions(&core).await[0];
    assert_eq!(
        open["state"], "open",
        "the core still holds the interaction until its own deadline"
    );
}

/// The real `nh` answers an interaction with a word, and the answer goes through the
/// delivery transaction like any other client, with `via: cli`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn nh_interaction_answer_allow_goes_through_the_delivery_transaction() {
    let core = core().await;
    let shim = Shim::start(&core.api_socket(), &[]);
    let open = wait_for(&core, "open with a lease", open_with_lease).await;
    let handle = open["handle"].as_str().expect("a handle").to_owned();
    let socket = core.api_socket().display().to_string();
    let ran = tokio::task::spawn_blocking(move || {
        Command::new(NH)
            .args([
                "--socket",
                &socket,
                "--json",
                "interaction",
                "answer",
                &handle,
                "allow",
            ])
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .output()
            .expect("nh runs")
    })
    .await
    .expect("the task");
    assert!(
        ran.status.success(),
        "{}",
        String::from_utf8_lossy(&ran.stderr)
    );
    let answered: Value = serde_json::from_slice(&ran.stdout).expect("JSON");
    assert_eq!(answered["rev"], 1);
    let (code, stdout, stderr) = shim.finish().await;
    assert_eq!((code, stderr.as_str()), (Some(0), ""));
    assert_eq!(stdout, ALLOW_OUTPUT);
    // The delivery record names the surface.
    let shown = core
        .call("interaction.show", json!({"interaction": open["id"]}))
        .await;
    assert_eq!(shown["interaction"]["delivery"]["via"], "cli");
}
