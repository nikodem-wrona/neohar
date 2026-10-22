//! The transcripts against the real core: the usage, the cost and the limits that a
//! transcript gives, the place in the file after a restart, the timeline, and the approval that a
//! tool result ends. The harness is a test here: it writes the lines that Claude Code writes (the
//! golden recordings) into a file, and sends the hooks.

mod core_support;

use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

use core_support::Harness;
use nh_core::ErrorCode;
use nh_proto::api::time::Timestamp;
use serde_json::{Value, json};

const NH: &str = env!("CARGO_BIN_EXE_nh");

fn transcript(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("nh-e2e-transcripts-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("a directory");
    dir.join("session.jsonl")
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

/// The moment now, in the form that the harness writes: the usage of today is the use of the
/// lines that carry today.
fn now() -> String {
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("a clock")
        .as_millis();
    Timestamp(i64::try_from(ms).expect("a time")).to_rfc3339()
}

fn message(uuid: &str, id: &str, input: u64, output: u64, blocks: Value) -> String {
    line(json!({
        "type": "assistant", "uuid": uuid, "timestamp": now(),
        "message": {"id": id, "model": "claude-opus-5-5", "content": blocks,
            "usage": {"input_tokens": input, "output_tokens": output}}
    }))
}

async fn signal(core: &Harness, event: &str, payload: Value) {
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

/// The rows of a projection, as the API socket shows them.
async fn rows(core: &Harness, name: &str) -> Vec<Value> {
    let mut client = core_support::client::Client::connect(&core.api_socket()).await;
    let subscribed = client
        .ok("projection.subscribe", json!({ "name": name }))
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
            let data = &line["params"]["message"]["data"];
            let order: Vec<&str> = data["order"]
                .as_array()
                .map(|keys| keys.iter().filter_map(Value::as_str).collect())
                .unwrap_or_default();
            let items = data["items"].as_object();
            return match (items, order.is_empty()) {
                (Some(items), false) => order
                    .iter()
                    .filter_map(|key| items.get(*key).cloned())
                    .collect(),
                (Some(items), true) => items.values().cloned().collect(),
                _ => Vec::new(),
            };
        }
    }
}

async fn until<T>(what: &str, mut f: impl AsyncFnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(found) = f().await {
            return found;
        }
        assert!(Instant::now() < deadline, "timeout: {what}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// A core with a pane and a Claude Code session that has this transcript.
async fn session(name: &str) -> (Harness, PathBuf, String) {
    let core = Harness::start_layout().with_hook_pane().await;
    let path = transcript(name);
    signal(
        &core,
        "SessionStart",
        json!({"session_id": "s1", "cwd": "/work", "source": "startup",
            "transcript_path": path.display().to_string(), "model": "claude-opus-5-5"}),
    )
    .await;
    let run = until("the run", async || {
        rows(&core, "agents")
            .await
            .into_iter()
            .find_map(|a| a["id"].as_str().map(str::to_owned))
    })
    .await;
    (core, path, run)
}

async fn agent(core: &Harness, run: &str) -> Value {
    rows(core, "agents")
        .await
        .into_iter()
        .find(|a| a["id"] == run)
        .expect("the run")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_usage_of_a_run_follows_its_transcript_and_the_limits_show_the_day() {
    let (core, path, run) = session("usage").await;
    assert_eq!(agent(&core, &run).await["usage"]["input_tokens"], 0);
    append(&path, &prompt("u1", "make a note"));
    // One message on three lines counts once.
    for index in 0..3 {
        append(
            &path,
            &message(
                &format!("a{index}"),
                "m1",
                1000,
                20,
                json!([{"type": "text", "text": "ok"}]),
            ),
        );
    }
    let row = until("the usage", async || {
        let row = agent(&core, &run).await;
        (row["usage"]["input_tokens"] == 1000).then_some(row)
    })
    .await;
    assert_eq!(row["usage"]["output_tokens"], 20);
    // Claude Opus 5.5: 1000 tokens at 4 dollars and 20 at 20 dollars for a million.
    let cost = row["usage"]["cost_usd"].as_f64().expect("a cost");
    assert!((cost - 0.0044).abs() < 1e-9, "{cost}");
    // The table has it.
    let stored = core.db().usage_by_run().expect("usage");
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].1.input, 1000);
    // The limits of the harness show the use of today.
    let limits = until("the limits", async || {
        rows(&core, "limits")
            .await
            .into_iter()
            .find(|l| l["harness"] == "claude" && l["usage_today"]["input_tokens"] == 1000)
    })
    .await;
    assert_eq!(limits["account"], "default");
    assert_eq!(limits["usage_today"]["output_tokens"], 20);
    assert!(limits["windows"].as_array().expect("windows").is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_restart_goes_on_in_the_file_and_counts_nothing_twice() {
    let (mut core, path, run) = session("restart").await;
    append(&path, &prompt("u1", "go"));
    append(
        &path,
        &message(
            "a1",
            "m1",
            500,
            10,
            json!([{"type": "text", "text": "first"}]),
        ),
    );
    until("the first count", async || {
        (agent(&core, &run).await["usage"]["input_tokens"] == 500).then_some(())
    })
    .await;
    // Wait for the state to reach the table before the core goes.
    until("the saved place", async || {
        let state = core.db().transcript_state(run_ulid(&run)).ok().flatten()?;
        (state.offset > 0).then_some(())
    })
    .await;
    core.crash().await;
    // The harness goes on while the core is away: the next block of the same message, and a new one.
    append(
        &path,
        &message(
            "a2",
            "m1",
            500,
            25,
            json!([{"type": "text", "text": "second"}]),
        ),
    );
    append(&path, &message("a3", "m2", 300, 5, json!([])));
    core.restart();
    core.handle.wait_restored().await;
    // The run comes back, and its row has the usage that the table has.
    let first = until("the run is back", async || {
        rows(&core, "agents")
            .await
            .into_iter()
            .find(|a| a["id"] == run.as_str())
    })
    .await;
    assert!(
        first["usage"]["input_tokens"]
            .as_u64()
            .is_some_and(|n| n >= 500),
        "{first}"
    );
    let row = until("the count after the restart", async || {
        let row = agent(&core, &run).await;
        (row["usage"]["input_tokens"] == 800).then_some(row)
    })
    .await;
    // 10 and 15 more of output for m1, and 5 for m2.
    assert_eq!(row["usage"]["output_tokens"], 30);
}

fn run_ulid(run: &str) -> ulid::Ulid {
    run.parse::<nh_proto::api::ids::RunId>()
        .expect("a run id")
        .ulid()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_timeline_projection_follows_the_file_and_the_pages_give_older_items() {
    let (core, path, run) = session("timeline").await;
    let mut text = String::new();
    for index in 0..30 {
        text += &prompt(&format!("u{index:02}"), &format!("prompt {index}"));
    }
    append(&path, &text);
    // A window follows the timeline of the run.
    let name = format!("timeline:{run}");
    let mut client = core_support::client::Client::connect(&core.api_socket()).await;
    client
        .ok("projection.subscribe", json!({ "name": name }))
        .await;
    let items = until("the items", async || {
        let items = rows(&core, &name).await;
        (items.len() == 30).then_some(items)
    })
    .await;
    assert_eq!(items[0]["id"], "u00");
    assert_eq!(items[0]["body"]["type"], "prompt");
    assert_eq!(items[29]["turn"], 30);
    // A page gives the items before one.
    let page = core
        .call(
            "timeline.page",
            json!({"run": run, "before_item": "u20", "limit": 5}),
        )
        .await;
    let ids: Vec<_> = page["items"]
        .as_array()
        .expect("items")
        .iter()
        .map(|i| i["id"].as_str().expect("an id"))
        .collect();
    assert_eq!(ids, vec!["u15", "u16", "u17", "u18", "u19"]);
    assert_eq!(page["more"], true);
    let first = core
        .call(
            "timeline.page",
            json!({"run": run, "before_item": "u03", "limit": 50}),
        )
        .await;
    assert_eq!(first["items"].as_array().expect("items").len(), 3);
    assert_eq!(first["more"], false);
    // Without an item the page is the end of the timeline.
    let last = core
        .call("timeline.page", json!({"run": run, "limit": 2}))
        .await;
    assert_eq!(last["items"][1]["id"], "u29");
    // An item that does not exist is an error.
    let error = core
        .handle
        .rpc("timeline.page", json!({"run": run, "before_item": "nope"}))
        .await
        .expect_err("no such item");
    assert_eq!(error.code, ErrorCode::NotFound);
}

/// A shim of the hook `PermissionRequest` that holds a request.
fn shim(socket: &std::path::Path) -> std::process::Child {
    let mut child = Command::new(NH)
        .args(["hook", "claude", "PermissionRequest"])
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("NEOHAR", "1")
        .env("NEOHAR_SOCKET", socket)
        .env("NEOHAR_PANE_TOKEN", "pane-1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the shim starts");
    let mut stdin = child.stdin.take().expect("stdin");
    stdin
        .write_all(
            json!({"hook_event_name": "PermissionRequest", "tool_name": "Bash",
                "tool_input": {"command": "touch note.txt"}, "permission_mode": "default"})
            .to_string()
            .as_bytes(),
        )
        .expect("the payload goes in");
    child
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_tool_result_in_the_transcript_ends_the_approval_that_the_core_holds() {
    let (core, path, run) = session("reconcile").await;
    // Wait until the run may gate.
    until("the gate", async || {
        (agent(&core, &run).await["capabilities"]["gate"] == true).then_some(())
    })
    .await;
    signal(
        &core,
        "PreToolUse",
        json!({"tool_use_id": "toolu_1", "tool_name": "Bash", "tool_input": {"command": "touch note.txt"}}),
    )
    .await;
    let child = shim(&core.api_socket());
    let open = until("an open interaction", async || {
        core.call("debug.interactions", json!({})).await["interactions"]
            .as_array()?
            .iter()
            .find(|i| i["state"] == "open" && i["lease"] == true)
            .cloned()
    })
    .await;
    assert_eq!(open["native_ref"], "toolu_1");
    // The person answered in the dialog of the harness. The transcript shows the result of the call.
    append(
        &path,
        &line(
            json!({"type": "user", "uuid": "r1", "timestamp": "2026-10-08T23:05:07.000Z",
            "message": {"content": [{"type": "tool_result", "tool_use_id": "toolu_1", "content": "done"}]}}),
        ),
    );
    until("the interaction ends", async || {
        core.call("debug.interactions", json!({})).await["interactions"]
            .as_array()?
            .iter()
            .find(|i| i["handle"] == open["handle"] && i["state"] == "resolved_elsewhere")
            .cloned()
    })
    .await;
    // The held hook is released: it prints nothing.
    let output = tokio::task::spawn_blocking(move || child.wait_with_output())
        .await
        .expect("the task")
        .expect("the shim ends");
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty(), "{:?}", output.stdout);
}
