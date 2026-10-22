//! Task workspaces against the real core: a repository in a temporary
//! directory, a draft that changes and starts, a worktree and a branch, ports, files, a tab with
//! its environment, and the end of the task.

mod core_support;

use std::{
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};

use core_support::{Harness, wait_for};
use nh_core::ErrorCode;
use serde_json::{Value, json};

fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_AUTHOR_NAME", "Ada")
        .env("GIT_AUTHOR_EMAIL", "ada@example.com")
        .env("GIT_COMMITTER_NAME", "Ada")
        .env("GIT_COMMITTER_EMAIL", "ada@example.com")
        .output()
        .expect("git runs");
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

struct Setup {
    core: Harness,
    repo: PathBuf,
    root: PathBuf,
}

/// A core, a repository with one commit on `main`, and the settings of the tasks.
async fn start(name: &str, port_low: u16) -> Setup {
    let core = Harness::start();
    let base = std::fs::canonicalize(std::env::temp_dir())
        .expect("a path")
        .join(format!("nh-task-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let repo = base.join("app");
    std::fs::create_dir_all(&repo).expect("a repository");
    git(&repo, &["init", "-q", "-b", "main"]);
    std::fs::write(repo.join("a.txt"), "one\ntwo\n").expect("a file");
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "first"]);
    let root = base.join("worktrees");
    core.call(
        "config.set",
        json!({"key": "tasks.worktree_root", "value": root.display().to_string()}),
    )
    .await;
    core.call(
        "config.set",
        json!({"key": "tasks.port_range", "value": [port_low, port_low + 20]}),
    )
    .await;
    Setup { core, repo, root }
}

/// How long a test waits for the core to reach a state. On a busy Mac, macOS checks each new
/// binary, and git can take seconds. The core sets up the two watches of a worktree one after
/// the other, and on macOS `fseventsd` sets up the watches of all processes one at a time. Under
/// `mise run test`, one setup waited more than 30 s behind the watches of the other tests (see
/// `READY` in the tests of `nh_core::config`).
const WAIT: Duration = Duration::from_secs(120);

async fn until<T>(what: &str, mut f: impl AsyncFnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + WAIT;
    loop {
        if let Some(found) = f().await {
            return found;
        }
        assert!(Instant::now() < deadline, "timeout: {what}");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// The rows of a projection, as the API socket shows them.
async fn rows(core: &Harness, name: &str) -> Vec<Value> {
    let data = snapshot(core, name).await;
    data["items"]
        .as_object()
        .map(|items| items.values().cloned().collect())
        .unwrap_or_default()
}

/// The data of the snapshot of a projection: its items and its order.
async fn snapshot(core: &Harness, name: &str) -> Value {
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
            return line["params"]["message"]["data"].clone();
        }
    }
}

async fn finished_job(core: &Harness, id: &str) -> Value {
    until("the job ends", async || {
        let job = rows(core, "jobs")
            .await
            .into_iter()
            .find(|job| job["id"] == id)?;
        (job["status"] != "running").then_some(job)
    })
    .await
}

/// A task that starts at once, and the end of its job.
async fn create(s: &Setup, title: &str, extra: Value) -> Value {
    let mut params = json!({"repo": s.repo.display().to_string(), "title": title, "start": true});
    if let (Some(into), Some(from)) = (params.as_object_mut(), extra.as_object()) {
        into.extend(from.clone());
    }
    let started = s.core.call("task.create", params).await;
    finished_job(&s.core, started["job"].as_str().expect("a job")).await
}

/// A draft: the task without its start.
async fn draft(s: &Setup, title: &str, extra: Value) -> Value {
    let mut params = json!({"repo": s.repo.display().to_string(), "title": title});
    if let (Some(into), Some(from)) = (params.as_object_mut(), extra.as_object()) {
        into.extend(from.clone());
    }
    let created = s.core.call("task.create", params).await;
    assert!(created["job"].is_null(), "a draft has no job: {created}");
    s.core
        .call("task.show", json!({"task": created["handle"]}))
        .await["task"]
        .clone()
}

async fn start_task(s: &Setup, handle: &str) -> Value {
    let started = s.core.call("task.start", json!({"task": handle})).await;
    finished_job(&s.core, started["job"].as_str().expect("a job")).await
}

async fn refused(s: &Setup, method: &str, params: Value) -> String {
    let error = s
        .core
        .handle
        .rpc(method, params)
        .await
        .expect_err("the call is refused");
    assert_eq!(error.code, ErrorCode::InvalidParams, "{}", error.message);
    error.message
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_task_gets_a_worktree_a_branch_ports_and_a_tab_with_its_environment() {
    let s = start("make", 43_200).await;
    let done = create(&s, "Fix the login bug", json!({})).await;
    assert_eq!(done["status"], "done", "{done}");
    let task = s.core.call("task.show", json!({"task": "k1"})).await["task"].clone();
    assert_eq!(task["handle"], "k1");
    assert_eq!(task["status"], "idle");
    assert_eq!(task["branch"], "neohar/fix-the-login-bug");
    assert_eq!(task["base"], "main");
    assert_eq!(task["ports"], json!([43_200]));
    let worktree = s.root.join("app").join("fix-the-login-bug");
    assert_eq!(task["worktree"], worktree.display().to_string());
    assert!(worktree.join("a.txt").exists());
    assert!(
        git(&s.repo, &["branch", "--list", "neohar/fix-the-login-bug"])
            .contains("neohar/fix-the-login-bug")
    );
    // The tab is in a workspace whose root is the repository, and its shell has the environment.
    let workspaces = s.core.call("workspace.list", json!({})).await;
    let workspace = workspaces["workspaces"]
        .as_array()
        .expect("a list")
        .iter()
        .find(|w| w["root"] == s.repo.display().to_string())
        .expect("a workspace with the root of the repository")
        .clone();
    assert_eq!(task["workspace"], workspace["id"]);
    assert!(task["tab"].is_string(), "the task has its tab: {task}");
    let pane_id: ulid::Ulid = done["result"]["pane"]
        .as_str()
        .expect("a pane")
        .parse()
        .expect("an id");
    let pane = until("the pane starts", async || s.core.handle.pane(pane_id)).await;
    until("the shell is up", async || {
        pane.text()
            .iter()
            .any(|l| !l.trim().is_empty())
            .then_some(())
    })
    .await;
    s.core
        .text(
            &pane,
            &format!(
                "test \"$PWD\" = '{}' && echo \"[cwd-ok|$NEOHAR_TASK_ID|$PORT|$NEOHAR_PORTS]\"\r",
                worktree.display()
            ),
        )
        .await;
    wait_for(&pane, "the environment", |t| {
        t.iter().any(|row| row.trim() == "[cwd-ok|k1|43200|43200]")
    })
    .await;

    // The same title again: the number goes up on the name and on the port.
    let second = create(&s, "Fix the login bug", json!({})).await;
    assert_eq!(second["status"], "done", "{second}");
    let task = s.core.call("task.show", json!({"task": "k2"})).await["task"].clone();
    assert_eq!(task["branch"], "neohar/fix-the-login-bug-2");
    assert_eq!(task["ports"], json!([43_201]));
    assert!(s.root.join("app").join("fix-the-login-bug-2").exists());
    let listed = s.core.call("task.list", json!({})).await;
    assert_eq!(listed["tasks"].as_array().expect("a list").len(), 2);
    let trees = s.core.call("worktree.list", json!({})).await;
    let tracked = trees["worktrees"]
        .as_array()
        .expect("a list")
        .iter()
        .filter(|w| w["task"].is_string())
        .count();
    assert_eq!(tracked, 2, "{trees}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_change_in_the_worktree_updates_the_diff_stats() {
    let s = start("diff", 43_300).await;
    assert_eq!(
        create(&s, "Add a feature", json!({})).await["status"],
        "done"
    );
    let worktree = s.root.join("app").join("add-a-feature");
    std::fs::write(worktree.join("a.txt"), "one\ntwo\nthree\n").expect("a change");
    std::fs::write(worktree.join("new.txt"), "x\ny\n").expect("a new file");
    // Looking at the task counts its lines again, in a moment.
    let task = until("the lines are counted", async || {
        let shown = s.core.call("task.show", json!({"task": "k1"})).await["task"].clone();
        (shown["diff_stats"]["add"] == 3).then_some(shown)
    })
    .await;
    assert_eq!(task["diff_stats"]["del"], 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn closing_a_task_gives_the_ports_back_and_a_dirty_worktree_needs_force() {
    let s = start("close", 43_400).await;
    assert_eq!(create(&s, "Refactor", json!({})).await["status"], "done");
    let worktree = s.root.join("app").join("refactor");
    std::fs::write(worktree.join("scratch.txt"), "unsaved\n").expect("a file");
    // A file that git does not track is a change that the removal loses.
    let refused = s
        .core
        .handle
        .rpc("task.close", json!({"task": "k1", "remove_worktree": true}))
        .await
        .expect_err("the worktree is dirty");
    assert_eq!(refused.code, ErrorCode::ConfirmRequired);
    assert!(worktree.exists());
    s.core
        .call(
            "task.close",
            json!({"task": "k1", "remove_worktree": true, "force": true}),
        )
        .await;
    assert!(!worktree.exists(), "the worktree is removed");
    let task = s.core.call("task.show", json!({"task": "k1"})).await["task"].clone();
    assert_eq!(task["status"], "closed");
    assert_eq!(task["ports"], json!([]));
    // The port is free for the next task.
    assert_eq!(create(&s, "Next", json!({})).await["status"], "done");
    let next = s.core.call("task.show", json!({"task": "k2"})).await["task"].clone();
    assert_eq!(next["ports"], json!([43_400]));
    // A task closed without the removal keeps its worktree.
    s.core.call("task.close", json!({"task": "k2"})).await;
    assert!(s.root.join("app").join("next").exists());
    let again = s
        .core
        .handle
        .rpc("task.close", json!({"task": "k2"}))
        .await
        .expect_err("it is closed");
    assert_eq!(again.code, ErrorCode::InvalidParams);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_setup_that_fails_stops_the_task_and_keeps_its_log() {
    let s = start("setup", 43_500).await;
    s.core
        .call(
            "config.set",
            json!({"key": "tasks.setup", "value": ["echo preparing", "exit 3"]}),
        )
        .await;
    let done = create(&s, "Broken setup", json!({"harness": "claude"})).await;
    assert_eq!(done["status"], "failed", "{done}");
    assert_eq!(done["error"]["code"], "setup_failed", "{done}");
    assert!(
        done["error"]["message"]
            .as_str()
            .expect("a message")
            .contains("exit 3")
    );
    let task = s.core.call("task.show", json!({"task": "k1"})).await["task"].clone();
    assert_eq!(task["status"], "setup_failed");
    // No tab, no agent.
    assert!(task["workspace"].is_null(), "{task}");
    assert!(task["agents"].as_array().expect("agents").is_empty());
    // The log of the setup is a blob.
    let blob = done["error"]["details"]["log"].as_str().expect("a blob");
    let bytes = s
        .core
        .db()
        .get_blob(blob)
        .expect("a read")
        .expect("the blob");
    let text = String::from_utf8_lossy(&bytes);
    assert!(
        text.contains("preparing") && text.contains("exit status"),
        "{text}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_task_with_a_harness_starts_the_agent_in_its_tab() {
    let s = start("agent", 43_600).await;
    let done = create(
        &s,
        "Write tests",
        json!({
            "harness": "claude",
            "prompt": "write the tests",
            "model": "sonnet[1m]",
            "effort": "low",
        }),
    )
    .await;
    assert_eq!(done["status"], "done", "{done}");
    let task = s.core.call("task.show", json!({"task": "k1"})).await["task"].clone();
    let agents = task["agents"].as_array().expect("agents");
    assert_eq!(agents.len(), 1, "{task}");
    let run = s.core.call("agent.show", json!({"agent": agents[0]})).await;
    assert_eq!(run["agent"]["task"], task["id"]);
    assert_eq!(run["agent"]["binding"]["state"], "bound");
    // The prompt reached the program in the pane of the task.
    let input = s.core.dir.join("home").join("claude-input");
    until("the prompt", async || {
        std::fs::read_to_string(&input)
            .ok()
            .filter(|text| text.contains("write the tests"))
    })
    .await;
    // The model and the effort level of the form are on the command line.
    let args = std::fs::read_to_string(s.core.dir.join("home").join("claude-args"))
        .expect("the arguments");
    assert!(args.contains("--model sonnet[1m] --effort low"), "{args}");
    assert_eq!(run["agent"]["model"], "sonnet[1m]");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_model_or_an_effort_level_without_an_agent_is_refused() {
    let s = start("no-agent", 43_700).await;
    for extra in [
        json!({"model": "opus"}),
        json!({"harness": "claude", "effort": "a b"}),
    ] {
        let mut params = json!({"repo": s.repo.display().to_string(), "title": "Odd"});
        for (key, value) in extra.as_object().expect("an object") {
            params[key] = value.clone();
        }
        let refused = s
            .core
            .handle
            .rpc("task.create", params)
            .await
            .expect_err("the call is refused");
        assert_eq!(refused.code, ErrorCode::InvalidParams, "{extra}");
    }
    // No task, and no worktree.
    let listed = s.core.call("task.list", json!({})).await;
    assert_eq!(listed["tasks"], json!([]));
    assert!(!s.root.exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_draft_changes_until_it_starts_and_then_only_its_title_changes() {
    let s = start("draft", 43_800).await;
    let task = draft(
        &s,
        "Fix the login bug",
        json!({"harness": "codex", "prompt": "  look at it  ", "ports": 2}),
    )
    .await;
    // A draft keeps the words of the form and makes nothing in git.
    assert_eq!(task["status"], "draft");
    assert_eq!(task["handle"], "k1");
    assert_eq!(task["harness"], "codex");
    assert_eq!(task["prompt"], "look at it");
    assert!(
        task["worktree"].is_null() && task["branch"].is_null(),
        "{task}"
    );
    assert!(task["base"].is_null() && task["tab"].is_null(), "{task}");
    assert_eq!(task["ports"], json!([]));
    assert!(!s.root.exists(), "no worktree");
    assert!(git(&s.repo, &["branch", "--list", "neohar/*"]).is_empty());

    // Everything of a draft can change. Without an agent, the model and the effort level go.
    s.core
        .call(
            "task.update",
            json!({"task": "k1", "title": "Fix the sign-in bug", "harness": "claude",
                   "model": "opus", "effort": "high", "prompt": "fix it"}),
        )
        .await;
    let task = s.core.call("task.show", json!({"task": "k1"})).await["task"].clone();
    assert_eq!(
        (
            &task["title"],
            &task["harness"],
            &task["model"],
            &task["effort"],
            &task["prompt"]
        ),
        (
            &json!("Fix the sign-in bug"),
            &json!("claude"),
            &json!("opus"),
            &json!("high"),
            &json!("fix it")
        )
    );
    s.core
        .call(
            "task.update",
            json!({"task": "k1", "harness": null, "prompt": null}),
        )
        .await;
    let task = s.core.call("task.show", json!({"task": "k1"})).await["task"].clone();
    for key in ["harness", "model", "effort", "prompt"] {
        assert!(task[key].is_null(), "{key}: {task}");
    }
    refused(&s, "task.update", json!({"task": "k1", "model": "opus"})).await;
    refused(&s, "task.update", json!({"task": "k1", "title": " "})).await;
    // A draft has nothing to close.
    assert!(
        refused(&s, "task.close", json!({"task": "k1"}))
            .await
            .contains("draft")
    );

    // The start makes the worktree with the name of the title at that time, and two ports.
    let done = start_task(&s, "k1").await;
    assert_eq!(done["status"], "done", "{done}");
    let task = s.core.call("task.show", json!({"task": "k1"})).await["task"].clone();
    assert_eq!(task["status"], "idle");
    assert_eq!(task["branch"], "neohar/fix-the-sign-in-bug");
    assert_eq!(task["base"], "main");
    assert_eq!(task["ports"], json!([43_800, 43_801]));
    assert!(s.root.join("app").join("fix-the-sign-in-bug").exists());
    assert!(task["tab"].is_string(), "{task}");

    // Now only the title changes, and a second start is refused.
    s.core
        .call("task.update", json!({"task": "k1", "title": "Sign-in bug"}))
        .await;
    let task = s.core.call("task.show", json!({"task": "k1"})).await["task"].clone();
    assert_eq!(task["title"], "Sign-in bug");
    assert_eq!(task["branch"], "neohar/fix-the-sign-in-bug");
    assert!(
        refused(&s, "task.update", json!({"task": "k1", "prompt": "more"}))
            .await
            .contains("only a draft")
    );
    assert!(
        refused(&s, "task.start", json!({"task": "k1"}))
            .await
            .contains("already")
    );
    // A task that runs is closed before it goes.
    assert!(
        refused(&s, "task.delete", json!({"task": "k1"}))
            .await
            .contains("close it")
    );
    s.core.call("task.close", json!({"task": "k1"})).await;
    s.core.call("task.delete", json!({"task": "k1"})).await;
    let listed = s.core.call("task.list", json!({})).await;
    assert_eq!(listed["tasks"], json!([]));
    // The kinds of the events of the life of the task, in order.
    let kinds: Vec<String> = s
        .core
        .db()
        .recent_events(10_000)
        .expect("events")
        .into_iter()
        .map(|event| event.kind)
        .filter(|kind| kind.starts_with("task."))
        .collect();
    assert_eq!(kinds.first().map(String::as_str), Some("task.created"));
    assert_eq!(kinds.last().map(String::as_str), Some("task.deleted"));
    assert!(kinds.contains(&"task.changed".to_owned()), "{kinds:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_files_of_a_task_reach_its_worktree_and_the_prompt_of_its_agent() {
    use base64::Engine as _;
    let s = start("files", 43_900).await;
    let notes = s.repo.parent().expect("a parent").join("plan.md");
    std::fs::write(&notes, "# The plan\n").expect("a file");
    let png = base64::engine::general_purpose::STANDARD.encode([0x89, b'P', b'N', b'G', 1, 2, 3]);
    let task = draft(
        &s,
        "Fix the layout",
        json!({
            "harness": "claude",
            "prompt": "fix the layout",
            "files": [notes.display().to_string()],
            "uploads": [
                {"name": "Screenshot.png", "data": png},
                {"name": "/a/b/Screenshot.png", "data": png},
                {"name": "spare.txt", "data": "eA=="},
            ],
        }),
    )
    .await;
    let files = task["attachments"].as_array().expect("files").clone();
    let summary: Vec<(String, String, u64)> = files
        .iter()
        .map(|f| {
            (
                f["name"].as_str().expect("a name").to_owned(),
                f["mime"].as_str().expect("a type").to_owned(),
                f["bytes"].as_u64().expect("a size"),
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            ("plan.md".into(), "text/markdown".into(), 11),
            ("Screenshot.png".into(), "image/png".into(), 7),
            ("Screenshot.png".into(), "image/png".into(), 7),
            ("spare.txt".into(), "text/plain".into(), 1),
        ]
    );
    // A file goes away by its id; an unknown id and a relative path are refused.
    s.core
        .call(
            "task.update",
            json!({"task": "k1", "remove": [files[3]["id"]]}),
        )
        .await;
    let unknown = s
        .core
        .handle
        .rpc("task.update", json!({"task": "k1", "remove": ["nope"]}))
        .await
        .expect_err("no such file");
    assert_eq!(unknown.code, ErrorCode::NotFound);
    refused(
        &s,
        "task.update",
        json!({"task": "k1", "files": ["plan.md"]}),
    )
    .await;
    // The row of the task names the blobs of its files, and the removed one is free.
    let refs = |bytes: &[u8]| {
        s.core
            .db()
            .blob_info(blake3::hash(bytes).to_hex().as_str())
            .expect("a read")
            .expect("a blob")
            .refs
    };
    assert_eq!(refs(b"x"), 0, "spare.txt was removed");
    assert_eq!(
        refs(&[0x89, b'P', b'N', b'G', 1, 2, 3]),
        2,
        "two screenshots"
    );
    // The original can go: the core keeps the bytes.
    std::fs::remove_file(&notes).expect("a removal");

    let done = start_task(&s, "k1").await;
    assert_eq!(done["status"], "done", "{done}");
    let worktree = s.root.join("app").join("fix-the-layout");
    let dir = worktree.join(".neohar").join("attachments");
    assert_eq!(
        std::fs::read_to_string(dir.join("plan.md")).expect("the notes"),
        "# The plan\n"
    );
    assert_eq!(
        std::fs::read(dir.join("Screenshot-2.png")).expect("the second shot"),
        [0x89, b'P', b'N', b'G', 1, 2, 3]
    );
    assert!(!dir.join("spare.txt").exists());
    // Git does not see them, so the worktree is clean.
    assert_eq!(git(&worktree, &["status", "--porcelain"]), "");
    let task = s.core.call("task.show", json!({"task": "k1"})).await["task"].clone();
    let paths: Vec<&str> = task["attachments"]
        .as_array()
        .expect("files")
        .iter()
        .map(|f| f["path"].as_str().expect("a path"))
        .collect();
    let wanted: Vec<String> = ["plan.md", "Screenshot.png", "Screenshot-2.png"]
        .iter()
        .map(|name| dir.join(name).display().to_string())
        .collect();
    assert_eq!(paths, wanted);
    // The prompt of the agent ends with the paths.
    let input = s.core.dir.join("home").join("claude-input");
    let typed = until("the prompt", async || {
        std::fs::read_to_string(&input)
            .ok()
            .filter(|text| text.contains("Screenshot-2.png"))
    })
    .await;
    assert!(typed.contains("fix the layout"), "{typed}");
    assert!(typed.contains("Files of this task:"), "{typed}");
    assert!(typed.contains(&format!("- {}", wanted[0])), "{typed}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_start_that_fails_before_the_worktree_leaves_the_draft() {
    let s = start("retry", 44_000).await;
    draft(&s, "Try again", json!({"base": "no-such-branch"})).await;
    let done = start_task(&s, "k1").await;
    assert_eq!(done["status"], "failed", "{done}");
    let task = s.core.call("task.show", json!({"task": "k1"})).await["task"].clone();
    assert_eq!(task["status"], "draft", "{task}");
    assert_eq!(task["base"], "no-such-branch");
    assert!(
        task["worktree"].is_null() && task["branch"].is_null(),
        "{task}"
    );
    assert_eq!(task["ports"], json!([]));
    // The person fixes the base, and the start works.
    s.core
        .call("task.update", json!({"task": "k1", "base": null}))
        .await;
    assert_eq!(start_task(&s, "k1").await["status"], "done");
    let task = s.core.call("task.show", json!({"task": "k1"})).await["task"].clone();
    assert_eq!(task["branch"], "neohar/try-again");
    assert_eq!(task["ports"], json!([44_000]));
    // A draft that goes takes nothing with it.
    draft(&s, "Never", json!({})).await;
    s.core.call("task.delete", json!({"task": "k2"})).await;
    let listed = s.core.call("task.list", json!({})).await;
    assert_eq!(listed["tasks"].as_array().expect("a list").len(), 1);
}

/// The paths of `favorite.list`, in its order, which is the order of the projection.
async fn favorites(core: &Harness) -> Vec<String> {
    let listed = core.call("favorite.list", json!({})).await;
    let paths: Vec<String> = listed["favorites"]
        .as_array()
        .expect("a list")
        .iter()
        .map(|favorite| favorite["path"].as_str().expect("a path").to_owned())
        .collect();
    let shown = snapshot(core, "favorites").await;
    assert_eq!(shown["order"], json!(paths), "{shown}");
    paths
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_favorite_folder_used_last_comes_first_and_stays_after_a_restart() {
    let mut s = start("favorites", 44_100).await;
    let repo = s.repo.display().to_string();
    // A second repository beside the first.
    let api = s.repo.with_file_name("api");
    std::fs::create_dir_all(api.join("src")).expect("a repository");
    git(&api, &["init", "-q", "-b", "main"]);
    let api_root = api.display().to_string();

    // A folder in a repository makes its root the favorite. The new one comes first.
    let added = s
        .core
        .call("favorite.add", json!({"path": format!("{repo}/")}))
        .await;
    assert_eq!(added, json!({"path": repo}));
    let shown = snapshot(&s.core, "favorites").await;
    assert_eq!(shown["items"][&repo]["name"], "app", "{shown}");
    let added = s
        .core
        .call(
            "favorite.add",
            json!({"path": api.join("src").display().to_string()}),
        )
        .await;
    assert_eq!(added["path"], api_root.as_str());
    assert_eq!(favorites(&s.core).await, [api_root.clone(), repo.clone()]);
    // A favorite that is there already stays where it is.
    s.core.call("favorite.add", json!({"path": repo})).await;
    assert_eq!(favorites(&s.core).await, [api_root.clone(), repo.clone()]);
    let error = s
        .core
        .handle
        .rpc(
            "favorite.add",
            json!({"path": s.root.display().to_string()}),
        )
        .await
        .expect_err("a folder that is not there");
    assert_eq!(error.code, ErrorCode::InvalidParams, "{}", error.message);

    // A task with a favorite makes it the one used last.
    draft(&s, "Fix the login bug", json!({})).await;
    assert_eq!(favorites(&s.core).await, [repo.clone(), api_root.clone()]);
    // A draft that moves to the other favorite makes that one first.
    s.core
        .call("task.update", json!({"task": "k1", "repo": api_root}))
        .await;
    assert_eq!(favorites(&s.core).await, [api_root.clone(), repo.clone()]);
    // A change of the title only is no use of the repository.
    s.core
        .call("task.update", json!({"task": "k1", "title": "Fix it"}))
        .await;
    s.core
        .call("task.update", json!({"task": "k1", "repo": repo}))
        .await;
    assert_eq!(favorites(&s.core).await, [repo.clone(), api_root.clone()]);

    s.core.crash().await;
    s.core.restart();
    assert_eq!(favorites(&s.core).await, [repo.clone(), api_root.clone()]);

    // The path of a favorite removes it, also when its folder is gone.
    std::fs::remove_dir_all(&api).expect("the folder goes");
    s.core
        .call("favorite.remove", json!({"path": api_root}))
        .await;
    assert_eq!(favorites(&s.core).await, [repo]);
    let error = s
        .core
        .handle
        .rpc("favorite.remove", json!({"path": api_root}))
        .await
        .expect_err("no such favorite");
    assert_eq!(error.code, ErrorCode::NotFound, "{}", error.message);
    // A folder of the repository of a favorite removes it.
    s.core
        .call(
            "favorite.remove",
            json!({"path": s.repo.join(".").display().to_string()}),
        )
        .await;
    assert!(favorites(&s.core).await.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fs_dirs_lists_the_folders_with_the_favorites_first() {
    let s = start("dirs", 44_300).await;
    // `repo` is `<base>/app`, a git repository. The rest of the tree is beside it.
    let base = s.repo.parent().expect("a parent").to_path_buf();
    for name in ["Docs", "beta", ".hidden"] {
        std::fs::create_dir_all(base.join(name)).expect("a folder");
    }
    std::fs::write(base.join("notes.txt"), "text").expect("a file");
    std::os::unix::fs::symlink(base.join("beta"), base.join("link")).expect("a symlink");
    let listed = s
        .core
        .call("fs.dirs", json!({"path": base.display().to_string()}))
        .await;
    let names = |value: &Value| -> Vec<String> {
        value["entries"]
            .as_array()
            .expect("entries")
            .iter()
            .map(|e| e["name"].as_str().expect("a name").to_owned())
            .collect()
    };
    assert_eq!(names(&listed), ["app", "beta", "Docs", "link"]);
    assert_eq!(listed["path"], base.display().to_string().as_str());
    assert_eq!(listed["truncated"], false);
    assert_eq!(listed["entries"][0]["repo"], true, "{listed}");
    assert_eq!(listed["entries"][1]["repo"], false, "{listed}");
    assert_eq!(
        listed["entries"][0]["path"],
        s.repo.display().to_string().as_str()
    );

    // A favorite comes first.
    let docs = base.join("Docs");
    git(&docs, &["init", "-q", "-b", "main"]);
    s.core
        .call("favorite.add", json!({"path": docs.display().to_string()}))
        .await;
    let listed = s
        .core
        .call("fs.dirs", json!({"path": base.display().to_string()}))
        .await;
    assert_eq!(names(&listed)[..2], ["Docs", "app"], "{listed}");

    // Without a path, the home folder of the machine, with the favorites in front.
    let home = s.core.dir.join("home");
    std::fs::create_dir_all(home.join("work")).expect("a folder");
    let listed = s.core.call("fs.dirs", json!({})).await;
    assert_eq!(listed["path"], home.display().to_string().as_str());
    assert_eq!(names(&listed)[0], "Docs", "{listed}");
    assert!(names(&listed).contains(&"work".to_owned()), "{listed}");
    // `..` is applied, and `~` is the home folder.
    let tilde = s.core.call("fs.dirs", json!({"path": "~/work/.."})).await;
    assert_eq!(tilde["path"], home.display().to_string().as_str());

    // A file, a missing path and a relative path are refused.
    for (path, code) in [
        (
            base.join("notes.txt").display().to_string(),
            ErrorCode::NotFound,
        ),
        (base.join("gone").display().to_string(), ErrorCode::NotFound),
        ("relative/dir".to_owned(), ErrorCode::InvalidParams),
    ] {
        let error = s
            .core
            .handle
            .rpc("fs.dirs", json!({"path": path}))
            .await
            .expect_err("the call is refused");
        assert_eq!(error.code, code, "{path}: {}", error.message);
    }
}

/// A window that follows a projection: its rows by key, with each patch applied.
struct Follower {
    client: core_support::client::Client,
    id: String,
    items: serde_json::Map<String, Value>,
}

impl Follower {
    async fn new(core: &Harness, name: &str) -> Self {
        let mut client = core_support::client::Client::connect(&core.api_socket()).await;
        let subscribed = client
            .ok("projection.subscribe", json!({ "name": name }))
            .await;
        let id = subscribed["subscription"]
            .as_str()
            .expect("an id")
            .to_owned();
        let mut follower = Self {
            client,
            id,
            items: serde_json::Map::new(),
        };
        let mut started = false;
        while !started {
            let line = follower.client.recv().await;
            started = follower.apply(line);
        }
        follower
    }

    /// Applies a message of the subscription. Returns whether it was the snapshot.
    fn apply(&mut self, line: Value) -> bool {
        if line["method"] != "projection.message" || line["params"]["subscription"] != *self.id {
            return false;
        }
        let message = &line["params"]["message"];
        match message["type"].as_str() {
            Some("snapshot") => {
                self.items = message["data"]["items"]
                    .as_object()
                    .cloned()
                    .unwrap_or_default();
                true
            }
            Some("patch") => {
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
                false
            }
            _ => false,
        }
    }

    /// Waits until a row is as `done` wants it, with no action of the test.
    async fn until(&mut self, what: &str, within: Duration, done: impl Fn(&Value) -> bool) {
        self.until_all(what, within, |items| items.values().any(&done))
            .await;
    }

    /// Waits until the rows are as `done` wants them, with no action of the test.
    async fn until_all(
        &mut self,
        what: &str,
        within: Duration,
        done: impl Fn(&serde_json::Map<String, Value>) -> bool,
    ) {
        let deadline = Instant::now() + within;
        loop {
            if done(&self.items) {
                return;
            }
            let left = deadline.saturating_duration_since(Instant::now());
            assert!(
                !left.is_zero(),
                "timeout: {what}: {:?}",
                self.items.values().collect::<Vec<_>>()
            );
            if let Some(line) = self.client.try_recv(left).await {
                self.apply(line);
            }
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_window_that_follows_the_changes_of_a_task_sees_files_commits_and_diffs() {
    let s = start("changes", 44_400).await;
    assert_eq!(
        create(&s, "Change things", json!({})).await["status"],
        "done"
    );
    let task = s.core.call("task.show", json!({"task": "k1"})).await["task"].clone();
    let id = task["id"].as_str().expect("an id").to_owned();
    let worktree = PathBuf::from(task["worktree"].as_str().expect("a worktree"));
    let mut window = Follower::new(&s.core, &format!("changes:{id}")).await;

    // The first window causes a count, and the watches start.
    std::fs::write(worktree.join("notes.md"), "one\ntwo\n").expect("a file");
    window
        .until("the new file is a row", WAIT, |row| {
            row["path"] == "notes.md" && row["status"] == "untracked"
        })
        .await;
    // A file that the person writes later: no hook and no call, only the watch of the worktree.
    let written = Instant::now();
    std::fs::write(worktree.join("later.txt"), "x\n").expect("a file");
    window
        .until("a file of later is a row", WAIT, |row| {
            row["path"] == "later.txt"
        })
        .await;
    eprintln!("the row came after {:?}", written.elapsed());
    std::fs::remove_file(worktree.join("later.txt")).expect("a removal");
    let row = window.items["notes.md"].clone();
    assert_eq!(
        (row["add"].clone(), row["uncommitted"].clone()),
        (json!(2), json!(true))
    );
    let shown = s.core.call("task.show", json!({"task": "k1"})).await["task"].clone();
    assert_eq!(shown["changes"]["files"], 2, "{shown}");
    assert_eq!(shown["changes"]["commits"], 0);
    assert_eq!(shown["diff_stats"], json!({"add": 3, "del": 0}));

    // The diff of a row, and no diff of a path that is not a row.
    let diff = s
        .core
        .call("task.diff", json!({"task": "k1", "path": "notes.md"}))
        .await;
    assert_eq!(diff["status"], "added");
    let lines = diff["hunks"][0]["lines"].as_array().expect("lines").clone();
    assert_eq!(lines.len(), 2, "{diff}");
    assert!(lines.iter().all(|line| line["kind"] == "add"));
    for path in ["a.txt", "../../outside.txt"] {
        let refused = s
            .core
            .handle
            .rpc("task.diff", json!({"task": "k1", "path": path}))
            .await
            .expect_err("not a row");
        assert_eq!(
            refused.code,
            ErrorCode::NotFound,
            "{path}: {}",
            refused.message
        );
    }

    // A commit in the worktree: the git directory changes, and the row is committed.
    let stamp = row["stamp"].clone();
    git(&worktree, &["add", "-A"]);
    git(&worktree, &["commit", "-q", "-m", "notes"]);
    window
        .until("the file is committed", WAIT, |row| {
            row["path"] == "notes.md" && row["uncommitted"] == false
        })
        .await;
    assert!(
        !window.items.contains_key("later.txt"),
        "a file that went is no row"
    );
    assert_eq!(window.items["notes.md"]["status"], "added");
    let shown = s.core.call("task.show", json!({"task": "k1"})).await["task"].clone();
    assert_eq!(shown["changes"]["commits"], 1, "{shown}");
    // A change of the file changes its stamp, so that an open diff loads again.
    std::fs::write(worktree.join("notes.md"), "one\ntwo\nthree\n").expect("a change");
    window
        .until("the stamp changes", WAIT, |row| {
            row["path"] == "notes.md" && row["stamp"] != stamp && row["add"] == 3
        })
        .await;

    // `task.changes` counts now, and the command line prints it (JSON, because its output is not
    // a terminal here).
    let counted = s.core.call("task.changes", json!({"task": "k1"})).await;
    assert_eq!(counted["files"][0]["path"], "notes.md", "{counted}");
    assert_eq!(counted["changes"]["commits"], 1);
    let socket = s.core.api_socket().display().to_string();
    let printed = tokio::task::spawn_blocking(move || {
        let run = |args: &[&str]| {
            let output = Command::new(env!("CARGO_BIN_EXE_nh"))
                .args(["--socket", &socket, "--json"])
                .args(args)
                .env_clear()
                .env("PATH", "/usr/bin:/bin")
                .output()
                .expect("nh runs");
            assert!(output.status.success(), "{args:?}");
            serde_json::from_slice::<Value>(&output.stdout).expect("JSON")
        };
        (
            run(&["task", "changes", "k1"]),
            run(&["task", "diff", "k1", "notes.md"]),
        )
    })
    .await
    .expect("the commands");
    assert_eq!(printed.0["files"][0]["path"], "notes.md", "{}", printed.0);
    assert_eq!(printed.0["changes"]["commits"], 1);
    let added: Vec<&Value> = printed.1["hunks"][0]["lines"]
        .as_array()
        .expect("lines")
        .iter()
        .map(|line| &line["text"]["text"])
        .collect();
    assert!(added.contains(&&json!("three")), "{}", printed.1);

    // A draft has no changes.
    let draft = draft(&s, "Later", json!({})).await;
    let message = refused(&s, "task.changes", json!({"task": draft["handle"]})).await;
    assert!(message.contains("draft"), "{message}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_window_that_follows_a_folder_sees_the_changes_of_its_worktree() {
    let s = start("folder", 44_500).await;
    let window = s.core.call("window.open", json!({})).await["window"]
        .as_str()
        .expect("a window")
        .to_owned();
    let mut head = Follower::new(&s.core, &format!("changes_head:{window}")).await;
    let mut rows = Follower::new(&s.core, &format!("changes_for:{window}")).await;
    let follow = async |path: &Path| {
        s.core
            .call(
                "changes.follow",
                json!({"window": window, "path": path.display().to_string()}),
            )
            .await;
    };

    // A folder in the main checkout, which is not the worktree of a task: the changes go from
    // HEAD, so a clean checkout has none.
    std::fs::create_dir_all(s.repo.join("src")).expect("a folder");
    follow(&s.repo.join("src")).await;
    let repo = s.repo.display().to_string();
    head.until("the head of the checkout", WAIT, |head| {
        head["root"] == repo.as_str() && head["changes"]["files"] == 0
    })
    .await;
    let shown = head.items["_"].clone();
    assert_eq!(shown["branch"], "main", "{shown}");
    assert_eq!(shown["folder"], s.repo.join("src").display().to_string());
    assert!(
        shown["base"].is_null() && shown["task"].is_null(),
        "{shown}"
    );

    // A file that the person writes: only the watch of the worktree sees it.
    std::fs::write(s.repo.join("src/new.txt"), "a\nb\n").expect("a file");
    std::fs::write(s.repo.join("a.txt"), "one\ntwo\nthree\n").expect("a change");
    rows.until("the new file is a row", WAIT, |row| {
        row["path"] == "src/new.txt" && row["status"] == "untracked"
    })
    .await;
    rows.until("the change is a row", WAIT, |row| {
        row["path"] == "a.txt" && row["status"] == "modified" && row["uncommitted"] == true
    })
    .await;
    head.until("the totals", WAIT, |head| {
        head["changes"]["files"] == 2 && head["diff_stats"] == json!({"add": 3, "del": 0})
    })
    .await;
    assert_eq!(head.items["_"]["changes"]["commits"], 0);

    // The diff of a row goes from HEAD, and a path that is not a row has none.
    let diff = s
        .core
        .call("changes.diff", json!({"window": window, "path": "a.txt"}))
        .await;
    assert_eq!(diff["status"], "modified", "{diff}");
    let added: Vec<&Value> = diff["hunks"][0]["lines"]
        .as_array()
        .expect("lines")
        .iter()
        .filter(|line| line["kind"] == "add")
        .collect();
    assert_eq!(added.len(), 1, "{diff}");
    let refused = s
        .core
        .handle
        .rpc(
            "changes.diff",
            json!({"window": window, "path": "../outside.txt"}),
        )
        .await
        .expect_err("not a row");
    assert_eq!(refused.code, ErrorCode::NotFound, "{}", refused.message);

    // A commit leaves no change behind.
    git(&s.repo, &["add", "-A"]);
    git(&s.repo, &["commit", "-q", "-m", "more"]);
    head.until("no changes after the commit", WAIT, |head| {
        head["changes"]["files"] == 0
    })
    .await;
    rows.until_all("no rows", WAIT, serde_json::Map::is_empty)
        .await;

    // The worktree of a task: the changes go from the merge base of its base, as for the task.
    assert_eq!(create(&s, "Folder task", json!({})).await["status"], "done");
    let task = s.core.call("task.show", json!({"task": "k1"})).await["task"].clone();
    let worktree = PathBuf::from(task["worktree"].as_str().expect("a worktree"));
    std::fs::write(worktree.join("task.md"), "x\n").expect("a file");
    follow(&worktree).await;
    head.until("the head of the task", WAIT, |head| {
        head["task"] == task["id"] && head["base"] == "main" && head["changes"]["files"] == 1
    })
    .await;
    assert_eq!(head.items["_"]["branch"], task["branch"]);
    rows.until("the row of the task", WAIT, |row| row["path"] == "task.md")
        .await;
    rows.until_all("only the row of the task", WAIT, |items| items.len() == 1)
        .await;

    // A folder that is not in a git worktree has a head with no root and no rows.
    let outside = s.root.parent().expect("the base").to_path_buf();
    follow(&outside).await;
    head.until("no worktree", WAIT, |head| {
        head["folder"] == outside.display().to_string().as_str() && head["root"].is_null()
    })
    .await;
    rows.until_all("no rows outside", WAIT, serde_json::Map::is_empty)
        .await;

    // A path that is not absolute, and a window that does not exist, are refused.
    let refused = s
        .core
        .handle
        .rpc("changes.follow", json!({"window": window, "path": "src"}))
        .await
        .expect_err("a relative path");
    assert_eq!(
        refused.code,
        ErrorCode::InvalidParams,
        "{}",
        refused.message
    );
    let refused = s
        .core
        .handle
        .rpc(
            "changes.follow",
            json!({"window": "01M4F1W7EM2GN3EV6NHXZJEGGA", "path": repo}),
        )
        .await
        .expect_err("no window");
    assert_eq!(refused.code, ErrorCode::NotFound, "{}", refused.message);
}
