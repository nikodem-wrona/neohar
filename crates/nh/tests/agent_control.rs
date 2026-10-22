//! Control of a run against the real core.
//!
//! A pane runs a small Python program in raw mode that writes every read of its input to a log.
//! A hook that carries the token of the pane binds a run of Claude Code to it. Then the calls
//! `agent.prompt`, `agent.reply`, `agent.interrupt`, `agent.stop` and `agent.wait` type into the
//! pane, and the log shows what the program got. A turn that ends while nobody looks at the pane
//! puts the run in `attention`.

mod core_support;

use std::{
    io::Write,
    path::PathBuf,
    time::{Duration, Instant},
};

use core_support::Harness;
use nh_core::{ErrorCode, Tuning};
use nh_proto::api::projections::Projection;
use serde_json::{Value, json};

/// The program in the pane. It logs the bytes of each read. Two Ctrl+C end it.
const PROGRAM: &str = r#"
import os, sys, tty
log = sys.argv[1]
open(log + ".token", "w").write(os.environ.get("NEOHAR_PANE_TOKEN", ""))
tty.setraw(0)
sys.stdout.write("> ")
sys.stdout.flush()
interrupts = 0
while True:
    data = os.read(0, 4096)
    if not data:
        break
    with open(log, "a") as f:
        f.write(repr(data) + "\n")
    interrupts += data.count(b"\x03")
    if interrupts >= 2:
        break
"#;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("nh-agent-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a directory");
    dir
}

struct Setup {
    core: Harness,
    log: PathBuf,
}

async fn start(name: &str) -> Setup {
    start_tuned(name, Tuning::default()).await
}

async fn start_tuned(name: &str, tuning: Tuning) -> Setup {
    let dir = scratch(name);
    let script = dir.join("program.py");
    std::fs::write(&script, PROGRAM).expect("the program");
    let log = dir.join("input.log");
    std::fs::File::create(&log).expect("the log");
    let core = Harness::start_tuned(tuning)
        .with_command_pane(&[
            "/usr/bin/python3",
            script.to_str().expect("a path"),
            log.to_str().expect("a path"),
        ])
        .await;
    Setup { core, log }
}

async fn signal(core: &Harness, token: &str, event: &str, payload: Value) {
    let socket = core.api_socket();
    let line = json!({
        "jsonrpc": "2.0", "id": 1, "method": "adapter.signal",
        "params": {"token": token, "harness": "claude", "event": event, "payload": payload},
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
        assert!(reply.contains("\"result\""), "the signal failed: {reply}");
    })
    .await
    .expect("the task");
}

async fn until<T>(what: &str, mut f: impl AsyncFnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(found) = f().await {
            return found;
        }
        assert!(Instant::now() < deadline, "timeout: {what}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// The token of the pane, which the program wrote to a file.
async fn token(s: &Setup) -> String {
    let token_file = PathBuf::from(format!("{}.token", s.log.display()));
    until("the token of the pane", async || {
        std::fs::read_to_string(&token_file)
            .ok()
            .map(|t| t.trim().to_owned())
            .filter(|t| !t.is_empty())
    })
    .await
}

/// Binds a run of Claude Code to the pane by a hook with its token. Returns the id of the run.
async fn bound_run(s: &Setup) -> String {
    let token = token(s).await;
    signal(
        &s.core,
        &token,
        "SessionStart",
        json!({"session_id": "s1", "cwd": "/work", "source": "startup"}),
    )
    .await;
    until("a run in agent.list", async || {
        let listed = s.core.call("agent.list", json!({})).await;
        listed["agents"]
            .as_array()
            .and_then(|list| list.first())
            .and_then(|run| run["id"].as_str().map(str::to_owned))
    })
    .await
}

fn input(s: &Setup) -> String {
    std::fs::read_to_string(&s.log).unwrap_or_default()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_bound_run_is_listed_shown_and_typed_into() {
    let s = start("prompt").await;
    let id = bound_run(&s).await;
    let shown = s.core.call("agent.show", json!({"agent": id})).await;
    assert_eq!(shown["agent"]["harness"], "claude");
    assert_eq!(shown["agent"]["binding"]["state"], "bound");
    // A move on the board answers. An agent that does not exist is not found.
    s.core
        .call("agent.move", json!({"agent": id, "index": 0}))
        .await;
    let missing = s
        .handle_error("agent.move", json!({"agent": "nobody", "index": 0}))
        .await;
    assert_eq!(missing.code, ErrorCode::NotFound);
    // The prompt is pasted, and Enter sends it.
    s.core
        .call("agent.prompt", json!({"agent": id, "text": "write a test"}))
        .await;
    until("the program got the prompt and the Enter", async || {
        let log = input(&s);
        (log.contains("write a test") && log.contains("\\r")).then_some(())
    })
    .await;
    // With `submit: false` nothing follows the text.
    let before = input(&s).lines().count();
    s.core
        .call(
            "agent.prompt",
            json!({"agent": id, "text": "more", "submit": false}),
        )
        .await;
    until("the second paste", async || {
        (input(&s).lines().count() > before).then_some(())
    })
    .await;
    let log = input(&s);
    let last = log.lines().last().expect("a line");
    assert!(last.contains("more") && !last.contains("\\r"), "{last}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn interrupt_presses_the_keys_of_the_manifest() {
    let s = start("interrupt").await;
    let id = bound_run(&s).await;
    s.core.call("agent.interrupt", json!({"agent": id})).await;
    until("the program got Esc", async || {
        input(&s).contains("\\x1b").then_some(())
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stop_sends_the_quit_keys_and_wait_sees_the_end() {
    let s = start("stop").await;
    let id = bound_run(&s).await;
    // The run is idle after the session start, and has not ended.
    let idle = s
        .cli(
            "agent.wait",
            json!({"agent": id, "until": "idle", "timeout_ms": 5000}),
        )
        .await;
    assert_eq!(idle["reached"], true);
    let early = s
        .cli(
            "agent.wait",
            json!({"agent": id, "until": "exited", "timeout_ms": 300}),
        )
        .await;
    assert_eq!(early["reached"], false, "the program runs");
    s.core.call("agent.stop", json!({"agent": id})).await;
    let ended = s
        .cli(
            "agent.wait",
            json!({"agent": id, "until": "exited", "timeout_ms": 20000}),
        )
        .await;
    assert_eq!(
        ended["reached"],
        true,
        "two Ctrl+C end the program: {} {}",
        input(&s),
        ended["agent"]
    );
    assert_eq!(input(&s).matches("\\x03").count(), 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_run_that_is_not_bound_gets_no_keys_until_the_user_links_it() {
    let s = start("link").await;
    let id = bound_run(&s).await;
    s.core.call("agent.unlink", json!({"agent": id})).await;
    let refused = s
        .handle_error("agent.prompt", json!({"agent": id, "text": "hello"}))
        .await;
    assert_eq!(refused.code, ErrorCode::NotBound);
    let pane = s.core.call("agent.show", json!({"agent": id})).await["agent"]["pane"]
        .as_str()
        .expect("a pane")
        .to_owned();
    s.core
        .call("agent.link", json!({"pane": pane, "harness": "claude"}))
        .await;
    s.core
        .call("agent.prompt", json!({"agent": id, "text": "hello"}))
        .await;
    until("the program got the prompt", async || {
        input(&s).contains("hello").then_some(())
    })
    .await;
}

impl Setup {
    /// A call as a client of the CLI, which `agent.wait` is for.
    async fn cli(&self, method: &str, params: Value) -> Value {
        self.core
            .handle
            .call_as(nh_core::Origin::cli(1), method, params)
            .await
            .unwrap_or_else(|e| panic!("{method} failed: {e}"))
    }

    async fn handle_error(&self, method: &str, params: Value) -> nh_core::RpcError {
        self.core
            .handle
            .rpc(method, params)
            .await
            .expect_err("the call fails")
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stopping_everything_gives_a_bound_agent_the_quit_keys_first() {
    let s = start("everything").await;
    bound_run(&s).await;
    let report = s.core.handle.stop_everything(Duration::from_secs(10)).await;
    assert_eq!(report.left, 0, "{report:?}");
    // The program quit by itself after two Ctrl+C, before any signal reached it.
    assert_eq!(input(&s).matches("\\x03").count(), 2, "{}", input(&s));
}

// --- agent.reply and the end of a turn ------------------------------------------

/// A core whose turn ends count after 200 ms.
fn quick_turns() -> Tuning {
    Tuning {
        turn_settle: Duration::from_millis(200),
        ..Tuning::default()
    }
}

/// The reasons of the run in `attention`.
fn reasons(s: &Setup, run: &str) -> Vec<String> {
    let hub = s.core.handle.projections();
    hub.flush();
    hub.snapshot(Projection::Attention)
        .items
        .get(run)
        .and_then(|row| row["reasons"].as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|reason| reason.as_str().map(str::to_owned))
        .collect()
}

async fn geometry_owner(s: &Setup, run: &str) -> Value {
    let pane = s.core.call("agent.show", json!({"agent": run})).await["agent"]["pane"].clone();
    s.core.call("pane.show", json!({"pane": pane})).await["pane"]["geometry_owner"].clone()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_turn_that_ends_unseen_waits_and_a_reply_goes_once() {
    let s = start_tuned("reply", quick_turns()).await;
    let id = bound_run(&s).await;
    let token = token(&s).await;
    // A turn starts and ends, and no window looks at the pane.
    signal(
        &s.core,
        &token,
        "UserPromptSubmit",
        json!({"session_id": "s1", "prompt": "go"}),
    )
    .await;
    signal(&s.core, &token, "Stop", json!({"session_id": "s1"})).await;
    until("the run waits after its turn", async || {
        (reasons(&s, &id) == ["turn_ended"]).then_some(())
    })
    .await;
    let owner = geometry_owner(&s, &id).await;
    // The reply is pasted, and Enter sends it.
    let reply = json!({"run": id, "input_id": 77, "text": "next step"});
    s.core.call("agent.reply", reply.clone()).await;
    until("the program got the reply and the Enter", async || {
        let log = input(&s);
        (log.contains("next step") && log.contains("\\r")).then_some(())
    })
    .await;
    until("the run does not wait any more", async || {
        reasons(&s, &id).is_empty().then_some(())
    })
    .await;
    assert_eq!(geometry_owner(&s, &id).await, owner, "no lease changes");
    // The same input id again writes nothing.
    let lines = input(&s).lines().count();
    s.core.call("agent.reply", reply).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(input(&s).lines().count(), lines, "{}", input(&s));
    assert_eq!(input(&s).matches("next step").count(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_reply_to_a_run_that_works_is_refused_with_not_idle() {
    let s = start("reply-busy").await;
    let id = bound_run(&s).await;
    let token = token(&s).await;
    signal(
        &s.core,
        &token,
        "UserPromptSubmit",
        json!({"session_id": "s1", "prompt": "go"}),
    )
    .await;
    until("the run works", async || {
        let shown = s.core.call("agent.show", json!({"agent": id})).await;
        (shown["agent"]["facets"]["execution"]["value"]["state"] == "working").then_some(())
    })
    .await;
    let refused = s
        .handle_error(
            "agent.reply",
            json!({"run": id, "input_id": 5, "text": "hello"}),
        )
        .await;
    assert_eq!(refused.code, ErrorCode::NotIdle);
    assert_eq!(refused.details["execution"]["state"], "working");
    assert_eq!(refused.details["pending"], json!([]));
    assert!(!input(&s).contains("hello"));
    // An empty text is not a reply.
    let empty = s
        .handle_error("agent.reply", json!({"run": id, "input_id": 6, "text": ""}))
        .await;
    assert_eq!(empty.code, ErrorCode::InvalidParams);
}
