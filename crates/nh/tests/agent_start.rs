//! Launching and resuming an agent against the real core.
//!
//! The `claude` of the test (see `core_support`) draws the input box of Claude Code and keeps its
//! arguments and its input in files. The core opens a pane, types the command line into its
//! shell, waits for the run and for the input box, and sends the prompt.

mod core_support;

use std::time::{Duration, Instant};

use core_support::Harness;
use nh_core::ErrorCode;
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

fn home(core: &Harness) -> std::path::PathBuf {
    core.dir.join("home")
}

fn read(core: &Harness, name: &str) -> String {
    std::fs::read_to_string(home(core).join(name)).unwrap_or_default()
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
            return line["params"]["message"]["data"]["items"]
                .as_object()
                .map(|items| items.values().cloned().collect())
                .unwrap_or_default();
        }
    }
}

async fn job(core: &Harness, id: &str) -> Value {
    rows(core, "jobs")
        .await
        .into_iter()
        .find(|job| job["id"] == id)
        .expect("the job is in the projection")
}

async fn finished(core: &Harness, id: &str) -> Value {
    until("the job ends", async || {
        let row = job(core, id).await;
        (row["status"] != "running").then_some(row)
    })
    .await
}

async fn first_pane(core: &Harness) -> String {
    let made = core
        .call("workspace.create", json!({ "name": "Work" }))
        .await;
    made["pane"].as_str().expect("a pane").to_owned()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_agent_starts_in_a_new_pane_with_its_name_session_and_prompt() {
    let core = Harness::start();
    let pane = first_pane(&core).await;
    let started = core
        .call(
            "agent.start",
            json!({
                "harness": "claude",
                "placement": {"type": "split", "pane": pane, "direction": "right"},
                "name": "worker",
                "model": "opus",
                "effort": "high",
                "prompt": "write a test",
            }),
        )
        .await;
    let id = started["job"].as_str().expect("a job").to_owned();
    let done = finished(&core, &id).await;
    assert_eq!(done["status"], "done", "{done}");
    // The command line has the session id that the core chose, the model and the effort level.
    until("the arguments", async || {
        read(&core, "claude-args")
            .contains("--session-id")
            .then_some(())
    })
    .await;
    let args = read(&core, "claude-args");
    assert!(args.contains("--model opus --effort high"), "{args}");
    let session = args
        .split_whitespace()
        .skip_while(|word| *word != "--session-id")
        .nth(1)
        .expect("a session id")
        .to_owned();
    // The prompt is typed after the input box shows, and Enter sends it.
    until("the prompt", async || {
        read(&core, "claude-input")
            .contains("write a test")
            .then_some(())
    })
    .await;
    // The run is bound at once, because the core launched it, and it has its name and session.
    let listed = core.call("agent.list", json!({})).await;
    let run = &listed["agents"][0];
    assert_eq!(run["name"], "worker");
    assert_eq!(run["binding"]["state"], "bound");
    assert_eq!(run["session_id"], session.as_str());
    // A name belongs to one live agent.
    let again = core
        .handle
        .rpc(
            "agent.start",
            json!({
                "harness": "claude",
                "placement": {"type": "split", "pane": pane, "direction": "down"},
                "name": "worker",
            }),
        )
        .await
        .expect_err("the name is taken");
    assert_eq!(again.code, ErrorCode::NameTaken);
    // The agent can be called by its name.
    let shown = core.call("agent.show", json!({"agent": "worker"})).await;
    assert_eq!(shown["agent"]["id"], run["id"]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_choices_come_from_the_manifests_and_the_catalog_of_codex() {
    let core = Harness::start();
    let menus = core.call("agent.choices", json!({})).await;
    let harness = |name: &str| {
        menus["harnesses"]
            .as_array()
            .expect("a list")
            .iter()
            .find(|h| h["harness"] == name)
            .cloned()
            .expect("the harness")
    };
    let claude = harness("claude");
    assert_eq!(
        claude["models"][0],
        json!({"id": "fable", "label": "Fable"})
    );
    assert!(
        claude["models"]
            .as_array()
            .expect("models")
            .iter()
            .any(|m| m["id"] == "opus[1m]"),
        "{claude}"
    );
    assert_eq!(
        claude["efforts"],
        json!(["low", "medium", "high", "xhigh", "max"])
    );
    // Without the catalog of Codex, the manifest has no list of models.
    let codex = harness("codex");
    assert_eq!(codex["models"], json!([]));
    assert_eq!(codex["efforts"], json!(["low", "medium", "high", "xhigh"]));

    // The catalog in `~/.codex` gives the models that the picker of Codex lists.
    let dir = home(&core).join(".codex");
    std::fs::create_dir_all(&dir).expect("a directory");
    std::fs::write(
        dir.join("models_cache.json"),
        json!({"models": [
            {"slug": "gpt-hidden", "visibility": "hide", "priority": 1},
            {"slug": "gpt-b", "display_name": "GPT-B", "visibility": "list", "priority": 3,
             "supported_reasoning_levels": [{"effort": "low"}, {"effort": "max"}]},
            {"slug": "gpt-a", "display_name": "GPT-A", "visibility": "list", "priority": 2},
        ]})
        .to_string(),
    )
    .expect("the catalog");
    let menus = core
        .call("agent.choices", json!({"harness": "codex"}))
        .await;
    assert_eq!(
        menus["harnesses"],
        json!([{
            "harness": "codex",
            "models": [
                {"id": "gpt-a", "label": "GPT-A"},
                {"id": "gpt-b", "label": "GPT-B", "efforts": ["low", "max"]},
            ],
            "efforts": ["low", "medium", "high", "xhigh"],
            // The fake `codex` of the test is logged in.
            "installed": true,
            "login": "logged_in",
            "available": true,
        }])
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_effort_level_that_a_command_line_cannot_carry_is_refused() {
    let core = Harness::start();
    let pane = first_pane(&core).await;
    for (effort, model) in [("very high", "opus"), ("high", " ")] {
        let refused = core
            .handle
            .rpc(
                "agent.start",
                json!({
                    "harness": "claude",
                    "placement": {"type": "split", "pane": pane, "direction": "right"},
                    "model": model,
                    "effort": effort,
                }),
            )
            .await
            .expect_err("the call is refused");
        assert_eq!(
            refused.code,
            ErrorCode::InvalidParams,
            "{effort:?} {model:?}"
        );
    }
    // Nothing started.
    let panes = core.call("pane.list", json!({})).await;
    assert_eq!(panes["panes"].as_array().map(Vec::len), Some(1), "{panes}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_agent_that_ended_resumes_in_its_pane_with_its_session() {
    let core = Harness::start();
    let pane = first_pane(&core).await;
    let started = core
        .call(
            "agent.start",
            json!({
                "harness": "claude",
                "placement": {"type": "split", "pane": pane, "direction": "right"},
                "name": "worker",
            }),
        )
        .await;
    let id = started["job"].as_str().expect("a job").to_owned();
    assert_eq!(finished(&core, &id).await["status"], "done");
    let listed = core.call("agent.list", json!({})).await;
    let run = listed["agents"][0]["id"]
        .as_str()
        .expect("a run")
        .to_owned();
    let session = listed["agents"][0]["session_id"]
        .as_str()
        .expect("a session")
        .to_owned();
    // A run that lives cannot be resumed.
    let live = core
        .handle
        .rpc("agent.resume", json!({"agent": run}))
        .await
        .expect_err("the agent lives");
    assert_eq!(live.code, ErrorCode::AgentLive);
    core.call("agent.stop", json!({"agent": run, "force": true}))
        .await;
    until("the run ends", async || {
        let listed = core.call("agent.list", json!({})).await;
        listed["agents"]
            .as_array()
            .is_some_and(Vec::is_empty)
            .then_some(())
    })
    .await;
    let resumed = core.call("agent.resume", json!({"agent": run})).await;
    let job_id = resumed["job"].as_str().expect("a job").to_owned();
    let done = finished(&core, &job_id).await;
    assert_eq!(done["status"], "done", "{done}");
    until("the resume command line", async || {
        read(&core, "claude-args")
            .contains(&format!("--resume {session}"))
            .then_some(())
    })
    .await;
    let listed = core.call("agent.list", json!({})).await;
    assert_eq!(listed["agents"][0]["session_id"], session.as_str());
    assert_eq!(listed["agents"][0]["name"], "worker");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn agents_that_ended_with_the_machine_are_offered_and_resumed_from_the_palette() {
    let mut core = Harness::start();
    let pane = first_pane(&core).await;
    let started = core
        .call(
            "agent.start",
            json!({
                "harness": "claude",
                "placement": {"type": "split", "pane": pane, "direction": "right"},
                "name": "worker",
            }),
        )
        .await;
    let id = started["job"].as_str().expect("a job").to_owned();
    assert_eq!(finished(&core, &id).await["status"], "done");
    let listed = core.call("agent.list", json!({})).await;
    let session = listed["agents"][0]["session_id"]
        .as_str()
        .expect("a session")
        .to_owned();
    // The machine restarts: the core, the holders and their programs are gone.
    let holders: Vec<i32> = core.handle.panes().iter().map(|p| p.holder_pid()).collect();
    core.crash().await;
    for holder in &holders {
        let pid = rustix::process::Pid::from_raw(*holder).expect("a pid");
        rustix::process::kill_process(pid, rustix::process::Signal::KILL).expect("a kill");
    }
    until("the holders die", async || {
        holders
            .iter()
            .all(|holder| {
                rustix::process::Pid::from_raw(*holder)
                    .is_none_or(|pid| rustix::process::test_kill_process(pid).is_err())
            })
            .then_some(())
    })
    .await;
    let _ = std::fs::remove_file(home(&core).join("claude-args"));
    core.restart();
    core.handle.wait_restored().await;
    // The run ended, and the core offers to resume it (`agents.resume_on_launch = "ask"`).
    let notice = until("the notice", async || {
        let listed = core.call("interaction.list", json!({})).await;
        listed["interactions"]
            .as_array()
            .and_then(|list| list.iter().find(|i| i["kind"] == "notice").cloned())
    })
    .await;
    assert_eq!(notice["title"], "Resume 1 agent");
    assert_eq!(notice["answerable"], false);
    let live = core.call("agent.list", json!({})).await;
    assert!(live["agents"].as_array().expect("a list").is_empty());
    core.call("palette.execute", json!({"command": "agent.resume_all"}))
        .await;
    until("the resume command line", async || {
        read(&core, "claude-args")
            .contains(&format!("--resume {session}"))
            .then_some(())
    })
    .await;
    let closed = core
        .call("interaction.list", json!({"open_only": false}))
        .await;
    let notice = closed["interactions"]
        .as_array()
        .and_then(|list| list.iter().find(|i| i["kind"] == "notice"))
        .cloned()
        .expect("the notice");
    assert_eq!(notice["state"], "dismissed");
    // Nothing waits any more.
    let again = core
        .handle
        .rpc("palette.execute", json!({"command": "agent.resume_all"}))
        .await;
    assert!(again.is_ok(), "the command runs, and its job fails");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn with_always_the_agents_that_ended_with_the_machine_resume_at_once() {
    let mut core = Harness::start();
    let pane = first_pane(&core).await;
    let started = core
        .call(
            "agent.start",
            json!({
                "harness": "claude",
                "placement": {"type": "split", "pane": pane, "direction": "right"},
            }),
        )
        .await;
    let id = started["job"].as_str().expect("a job").to_owned();
    assert_eq!(finished(&core, &id).await["status"], "done");
    let session = core.call("agent.list", json!({})).await["agents"][0]["session_id"]
        .as_str()
        .expect("a session")
        .to_owned();
    let holders: Vec<i32> = core.handle.panes().iter().map(|p| p.holder_pid()).collect();
    core.crash().await;
    for holder in &holders {
        let pid = rustix::process::Pid::from_raw(*holder).expect("a pid");
        rustix::process::kill_process(pid, rustix::process::Signal::KILL).expect("a kill");
    }
    until("the holders die", async || {
        holders
            .iter()
            .all(|holder| {
                rustix::process::Pid::from_raw(*holder)
                    .is_none_or(|pid| rustix::process::test_kill_process(pid).is_err())
            })
            .then_some(())
    })
    .await;
    let _ = std::fs::remove_file(home(&core).join("claude-args"));
    std::fs::create_dir_all(core.dir.join("config")).expect("the config directory");
    std::fs::write(
        core.dir.join("config").join("config.toml"),
        "[agents]\nresume_on_launch = \"always\"\n",
    )
    .expect("the config file");
    core.restart();
    core.handle.wait_restored().await;
    until("the resume command line", async || {
        read(&core, "claude-args")
            .contains(&format!("--resume {session}"))
            .then_some(())
    })
    .await;
    let listed = core
        .call("interaction.list", json!({"open_only": false}))
        .await;
    assert!(
        listed["interactions"]
            .as_array()
            .expect("a list")
            .iter()
            .all(|i| i["kind"] != "notice"),
        "no notice with always"
    );
}
