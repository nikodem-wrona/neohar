//! The executor of workflows against the real core.
//!
//! The `claude` of the test (see `core_support`) draws the input box of Claude Code and keeps its
//! arguments and its input in files. The test sends the hooks of the agent itself: the start of a
//! turn, then its end with the last message, as Claude Code does.

mod core_support;

use std::time::{Duration, Instant};

use core_support::{Harness, client::Client};
use serde_json::{Value, json};

async fn until<T>(what: &str, mut f: impl AsyncFnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + Duration::from_secs(40);
    loop {
        if let Some(found) = f().await {
            return found;
        }
        assert!(Instant::now() < deadline, "timeout: {what}");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

fn read(core: &Harness, name: &str) -> String {
    std::fs::read_to_string(core.dir.join("home").join(name)).unwrap_or_default()
}

fn agent(id: &str, inputs: &[&str], extra: Value) -> Value {
    let mut node = json!({
        "id": id,
        "kind": "agent",
        "title": id,
        "harness": "claude",
        "prompt": format!("Do the {id} work in {{{{start.folder}}}}."),
        "inputs": inputs,
        "x": 300,
        "y": 0,
    });
    if let (Some(node), Some(extra)) = (node.as_object_mut(), extra.as_object()) {
        node.extend(extra.clone());
    }
    node
}

fn start() -> Value {
    json!({ "id": "start", "kind": "manual", "title": "Manual trigger", "x": 0, "y": 0 })
}

fn report(inputs: &[&str]) -> Value {
    json!({ "id": "report", "kind": "report", "title": "Report", "inputs": inputs, "x": 600, "y": 0 })
}

/// A workspace, a project folder and a workflow file with these nodes.
async fn setup(core: &Harness, nodes: Value) -> (String, String) {
    core.call("workspace.create", json!({ "name": "Work" }))
        .await;
    let folder = core.dir.join("home").join("project");
    std::fs::create_dir_all(&folder).expect("the folder is made");
    let created = core
        .call("workflow.create", json!({ "name": "Check" }))
        .await;
    let saved = core
        .call(
            "workflow.save",
            json!({
                "workflow": created["workflow"],
                "revision": created["revision"],
                "graph": { "name": "Check", "nodes": nodes },
            }),
        )
        .await;
    assert_eq!(saved["problems"], json!([]), "{saved}");
    (
        saved["workflow"].as_str().expect("a name").to_owned(),
        folder.to_string_lossy().into_owned(),
    )
}

async fn execute(core: &Harness, workflow: &str, folder: &str) -> String {
    let started = core
        .call(
            "workflow.execute",
            json!({ "workflow": workflow, "folder": folder, "notes": "Add the check." }),
        )
        .await;
    assert_eq!(started["label"], "Check 1");
    started["execution"].as_str().expect("an id").to_owned()
}

async fn execution(core: &Harness, id: &str) -> Value {
    core.call("workflow.execution", json!({ "execution": id }))
        .await["execution"]
        .clone()
}

fn node(execution: &Value, id: &str) -> Value {
    execution["nodes"]
        .as_array()
        .and_then(|nodes| nodes.iter().find(|n| n["node"] == id))
        .cloned()
        .unwrap_or(Value::Null)
}

/// Waits until the agent of a node runs, gives its pane a token that the test knows, and returns
/// the run, the pane and the session.
async fn agent_of(
    core: &Harness,
    id: &str,
    node_id: &str,
    token: &str,
) -> (String, String, String) {
    let run = until("the agent of the node runs", async || {
        let row = node(&execution(core, id).await, node_id);
        row["run"].as_str().map(str::to_owned)
    })
    .await;
    let shown = core.call("agent.show", json!({ "agent": run })).await;
    let pane = shown["agent"]["pane"].as_str().expect("a pane").to_owned();
    let session = shown["agent"]["session_id"]
        .as_str()
        .expect("a session")
        .to_owned();
    let ulid: ulid::Ulid = pane.parse().expect("a pane id");
    core.handle.pane_tokens().register(ulid, token);
    (run, pane, session)
}

async fn hook(core: &Harness, token: &str, event: &str, payload: Value) {
    let mut client = Client::connect(&core.api_socket()).await;
    client
        .ok(
            "adapter.signal",
            json!({ "token": token, "harness": "claude", "event": event, "payload": payload }),
        )
        .await;
}

/// One turn of the agent: the prompt, then the end with the last message.
async fn turn(core: &Harness, token: &str, session: &str, message: &str) {
    hook(
        core,
        token,
        "UserPromptSubmit",
        json!({ "session_id": session, "prompt": "go", "prompt_id": "p1" }),
    )
    .await;
    hook(
        core,
        token,
        "Stop",
        json!({ "session_id": session, "prompt_id": "p1", "last_assistant_message": message }),
    )
    .await;
}

async fn state_is(core: &Harness, id: &str, state: &str) -> Value {
    until(&format!("the execution is {state}"), async || {
        let found = execution(core, id).await;
        (found["state"] == state).then_some(found)
    })
    .await
}

async fn pane_gone(core: &Harness, pane: &str) {
    until("the hidden tab closes", async || {
        core.handle
            .rpc("pane.show", json!({ "pane": pane }))
            .await
            .is_err()
            .then_some(())
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_execution_runs_its_agent_in_a_hidden_tab_and_writes_the_reports() {
    let core = Harness::start();
    let (workflow, folder) = setup(
        &core,
        json!([
            start(),
            agent("check", &["start"], json!({})),
            report(&["check"])
        ]),
    )
    .await;
    let id = execute(&core, &workflow, &folder).await;
    let (_, pane, session) = agent_of(&core, &id, "check", "wf-1").await;

    // The tab of the agent is not in the tab bar, and the agent is read-only.
    let listed = core.call("workspace.list", json!({})).await;
    assert_eq!(listed["workspaces"][0]["tabs"], 1, "{listed}");
    let args = read(&core, "claude-args");
    assert!(
        args.contains("--disallowedTools Edit,Write,NotebookEdit"),
        "{args}"
    );
    assert!(args.contains("--add-dir"), "{args}");
    let input = until("the prompt is typed", async || {
        let input = read(&core, "claude-input");
        input.contains("Do not edit files").then_some(input)
    })
    .await;
    assert!(
        input.contains(&format!("Do the check work in {folder}.")),
        "{input}"
    );
    assert!(input.contains("neohar workflow \"Check\", node \"check\", execution 1."));

    turn(
        &core,
        "wf-1",
        &session,
        "# Check\n\nAll fine.\n\nVerdict: pass",
    )
    .await;
    let done = state_is(&core, &id, "done").await;
    let check = node(&done, "check");
    assert_eq!(check["verdict"], "pass", "{done}");
    let text = std::fs::read_to_string(check["report"].as_str().expect("a report"))
        .expect("the report is there");
    assert_eq!(text, "# Check\n\nAll fine.\n\nVerdict: pass\n");
    let report = std::fs::read_to_string(done["report"].as_str().expect("report.md"))
        .expect("report.md is there");
    assert!(report.starts_with("# Check 1 · project"), "{report}");
    assert!(
        report.contains("| check | Claude Code | pass |"),
        "{report}"
    );
    assert!(
        report.contains("All fine.\n\n[Full report](check.md)"),
        "{report}"
    );
    assert_eq!(node(&done, "report")["verdict"], "pass");
    // The agent stopped with its tab.
    pane_gone(&core, &pane).await;
    let shown = core
        .call("workflow.report", json!({ "execution": id }))
        .await;
    assert!(
        shown["markdown"]["html"]
            .as_str()
            .is_some_and(|html| html.contains("<table>")),
        "{shown}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_turn_without_a_verdict_gets_one_reminder() {
    let core = Harness::start();
    let (workflow, folder) = setup(
        &core,
        json!([
            start(),
            agent("check", &["start"], json!({ "can_edit": true })),
            report(&["check"])
        ]),
    )
    .await;
    let id = execute(&core, &workflow, &folder).await;
    let (_, _, session) = agent_of(&core, &id, "check", "wf-2").await;
    let args = read(&core, "claude-args");
    assert!(!args.contains("--disallowedTools"), "{args}");

    turn(&core, "wf-2", &session, "I looked at the code.").await;
    until("the reminder is typed", async || {
        read(&core, "claude-input")
            .contains("Write your full report in Markdown")
            .then_some(())
    })
    .await;
    assert_eq!(
        node(&execution(&core, &id).await, "check")["state"],
        "working"
    );

    turn(
        &core,
        "wf-2",
        &session,
        "Two small issues.\n\nVerdict: warn",
    )
    .await;
    let done = state_is(&core, &id, "done").await;
    assert_eq!(node(&done, "check")["verdict"], "warn", "{done}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_node_that_fails_pauses_the_execution_and_a_skip_goes_on() {
    let core = Harness::start();
    // An effort level that no command line carries: the start fails at once.
    let (workflow, folder) = setup(
        &core,
        json!([
            start(),
            agent("check", &["start"], json!({ "effort": "no way" })),
            report(&["check"]),
        ]),
    )
    .await;
    let id = execute(&core, &workflow, &folder).await;
    let paused = state_is(&core, &id, "paused").await;
    let check = node(&paused, "check");
    assert_eq!(check["state"], "failed");
    assert!(
        check["reason"]
            .as_str()
            .is_some_and(|r| r.starts_with("The agent did not start")),
        "{check}"
    );
    assert_eq!(node(&paused, "report")["state"], "waiting");

    core.call(
        "workflow.retry",
        json!({ "execution": id, "node": "check" }),
    )
    .await;
    let paused = state_is(&core, &id, "paused").await;
    assert_eq!(node(&paused, "check")["state"], "failed");

    core.call("workflow.skip", json!({ "execution": id, "node": "check" }))
        .await;
    let done = state_is(&core, &id, "done").await;
    assert_eq!(node(&done, "check")["state"], "skipped");
    let report = std::fs::read_to_string(done["report"].as_str().expect("report.md"))
        .expect("report.md is there");
    assert!(
        report.contains("| check | Claude Code · no way | skipped |"),
        "{report}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancel_stops_the_agents_and_their_hidden_tabs() {
    let core = Harness::start();
    let (workflow, folder) = setup(
        &core,
        json!([
            start(),
            agent("check", &["start"], json!({})),
            report(&["check"])
        ]),
    )
    .await;
    let id = execute(&core, &workflow, &folder).await;
    let (_, pane, _) = agent_of(&core, &id, "check", "wf-3").await;
    core.call("workflow.cancel", json!({ "execution": id }))
        .await;
    let cancelled = state_is(&core, &id, "cancelled").await;
    assert_eq!(node(&cancelled, "check")["state"], "cancelled");
    assert_eq!(node(&cancelled, "report")["state"], "cancelled");
    pane_gone(&core, &pane).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_workflow_with_errors_is_not_executed() {
    let core = Harness::start();
    let (workflow, folder) = setup(&core, json!([start()])).await;
    let created = core
        .call("workflow.show", json!({ "workflow": workflow }))
        .await;
    let broken = core
        .call(
            "workflow.save",
            json!({
                "workflow": workflow,
                "revision": created["revision"],
                "graph": { "name": "Check", "nodes": [start(), report(&[])] },
            }),
        )
        .await;
    assert_eq!(broken["problems"][0]["node"], "report", "{broken}");
    let refused = core
        .handle
        .rpc(
            "workflow.execute",
            json!({ "workflow": workflow, "folder": folder }),
        )
        .await
        .expect_err("a workflow with errors");
    assert!(
        refused.message.contains("has errors"),
        "{}",
        refused.message
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_turn_that_ended_while_the_core_was_away_comes_from_the_transcript() {
    let mut core = Harness::start();
    let (workflow, folder) = setup(
        &core,
        json!([
            start(),
            agent("check", &["start"], json!({})),
            report(&["check"])
        ]),
    )
    .await;
    let id = execute(&core, &workflow, &folder).await;
    let (_, pane, session) = agent_of(&core, &id, "check", "wf-4").await;
    // The transcript of a recorded turn of Claude Code, with a report as its last message.
    let golden = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/harness-golden/claude/2.1.292/marker/transcripts/transcript-1.jsonl");
    let transcript = core.dir.join("home").join("transcript.jsonl");
    std::fs::write(
        &transcript,
        std::fs::read_to_string(golden)
            .expect("the recording")
            .replace("Hello from the mock model.", "All fine.\\n\\nVerdict: pass"),
    )
    .expect("the transcript is written");
    hook(
        &core,
        "wf-4",
        "SessionStart",
        json!({ "session_id": session, "transcript_path": transcript, "source": "startup" }),
    )
    .await;
    hook(
        &core,
        "wf-4",
        "UserPromptSubmit",
        json!({ "session_id": session, "prompt": "go", "prompt_id": "p1" }),
    )
    .await;
    until("the run has its transcript", async || {
        let shown = core
            .call(
                "agent.show",
                json!({ "agent": node(&execution(&core, &id).await, "check")["run"] }),
            )
            .await;
        (shown["agent"]["facets"]["execution"]["value"]["state"] == "working").then_some(())
    })
    .await;

    // The `Stop` hook of the turn finds no core.
    core.crash().await;
    core.restart();
    let done = state_is(&core, &id, "done").await;
    assert_eq!(node(&done, "check")["verdict"], "pass", "{done}");
    pane_gone(&core, &pane).await;
}
