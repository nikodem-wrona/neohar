//! The Git view against the real core: a window that follows a folder sees the head, the
//! worktrees, the branches and the changes of the git worktree that holds it, and the reads give
//! the history. The CLI reads take a path instead of a window.

mod core_support;

use std::{
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};

use core_support::Harness;
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
    base: PathBuf,
    /// The main worktree, on `main`, one commit ahead of `origin/main`.
    repo: PathBuf,
    /// A linked worktree on `topic`.
    topic: PathBuf,
}

/// A core, a bare `origin`, and a clone with a linked worktree.
fn start(name: &str) -> Setup {
    let core = Harness::start();
    let base = std::fs::canonicalize(std::env::temp_dir())
        .expect("a path")
        .join(format!("nh-git-view-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let origin = base.join("origin.git");
    let repo = base.join("app");
    let topic = base.join("app-topic");
    std::fs::create_dir_all(&repo).expect("a repository");
    git(&base, &["init", "-q", "--bare", "-b", "main", "origin.git"]);
    git(&repo, &["init", "-q", "-b", "main"]);
    std::fs::write(repo.join("a.txt"), "one\ntwo\n").expect("a file");
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "first"]);
    git(
        &repo,
        &["remote", "add", "origin", &origin.display().to_string()],
    );
    git(&repo, &["push", "-q", "-u", "origin", "main"]);
    std::fs::write(repo.join("b.txt"), "bee\n").expect("a file");
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "second"]);
    git(&repo, &["branch", "topic"]);
    git(
        &repo,
        &[
            "worktree",
            "add",
            "-q",
            &topic.display().to_string(),
            "topic",
        ],
    );
    Setup {
        core,
        base,
        repo,
        topic,
    }
}

/// How long a test waits for the core to reach a state. See `WAIT` in `changes.rs`.
const WAIT: Duration = Duration::from_secs(120);

/// A window that follows a projection: its rows by key, with each patch applied.
struct Follower {
    client: core_support::client::Client,
    id: String,
    items: serde_json::Map<String, Value>,
    order: Vec<String>,
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
            order: Vec::new(),
        };
        let mut started = false;
        while !started {
            let line = follower.client.recv().await;
            started = follower.apply(line);
        }
        follower
    }

    fn apply(&mut self, line: Value) -> bool {
        if line["method"] != "projection.message" || line["params"]["subscription"] != *self.id {
            return false;
        }
        let message = &line["params"]["message"];
        let strings = |value: &Value| -> Vec<String> {
            value
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|key| key.as_str().map(str::to_owned))
                .collect()
        };
        match message["type"].as_str() {
            Some("snapshot") => {
                self.items = message["data"]["items"]
                    .as_object()
                    .cloned()
                    .unwrap_or_default();
                self.order = strings(&message["data"]["order"]);
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
                        Some("order") => self.order = strings(&op["keys"]),
                        _ => {}
                    }
                }
                false
            }
            _ => false,
        }
    }

    /// The rows in their order.
    fn rows(&self) -> Vec<&Value> {
        self.order
            .iter()
            .filter_map(|key| self.items.get(key))
            .collect()
    }

    async fn until(
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

fn text(path: &Path) -> String {
    path.display().to_string()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_window_that_follows_a_folder_sees_its_repository_worktrees_and_branches() {
    let s = start("follow");
    let window = s.core.call("window.open", json!({})).await["window"]
        .as_str()
        .expect("a window")
        .to_owned();
    let mut head = Follower::new(&s.core, &format!("git_head:{window}")).await;
    let mut worktrees = Follower::new(&s.core, &format!("git_worktrees:{window}")).await;
    let mut branches = Follower::new(&s.core, &format!("git_branches:{window}")).await;
    let mut changes = Follower::new(&s.core, &format!("git_changes:{window}")).await;
    std::fs::create_dir_all(s.repo.join("src")).expect("a folder");
    s.core
        .call(
            "git.follow",
            json!({"window": window, "path": text(&s.repo.join("src"))}),
        )
        .await;

    let root = text(&s.repo);
    head.until("the head of the main worktree", WAIT, |items| {
        items
            .get("_")
            .is_some_and(|head| head["worktree"] == root.as_str() && head["branch"] == "main")
    })
    .await;
    let shown = head.items["_"].clone();
    assert_eq!(shown["repo_name"], "app", "{shown}");
    assert_eq!(shown["upstream"], "origin/main", "{shown}");
    assert_eq!(shown["ahead"], 1, "{shown}");
    assert_eq!(shown["remote"], "origin", "{shown}");
    assert_eq!(shown["fetched_text"], "Never fetched", "{shown}");

    let topic = text(&s.topic);
    worktrees
        .until("both worktrees", WAIT, |items| items.len() == 2)
        .await;
    let rows = worktrees.rows();
    assert_eq!(rows[0]["path"], root.as_str());
    assert!(
        rows[0]["main"] == true && rows[0]["current"] == true,
        "{rows:?}"
    );
    assert_eq!(rows[1]["path"], topic.as_str());
    assert_eq!(rows[1]["branch"], "topic");
    assert_eq!(rows[1]["dirty"], 0, "{rows:?}");

    branches
        .until("the branches", WAIT, |items| items.len() == 3)
        .await;
    let names: Vec<&str> = branches
        .rows()
        .iter()
        .filter_map(|row| row["name"].as_str())
        .collect();
    assert_eq!(names, ["main", "topic", "origin/main"]);
    let rows = branches.rows();
    assert_eq!(rows[0]["current"], true);
    assert_eq!(
        rows[1]["worktree"],
        topic.as_str(),
        "out in the other worktree"
    );
    assert_eq!(rows[2]["kind"], "remote");

    // A change of the worktree is a row, and its diff goes from HEAD.
    std::fs::write(s.repo.join("a.txt"), "one\ntwo\nthree\n").expect("a change");
    changes
        .until("the change is a row", WAIT, |items| {
            items
                .get("a.txt")
                .is_some_and(|row| row["status"] == "modified")
        })
        .await;
    let diff = s
        .core
        .call("git.diff", json!({"window": window, "file": "a.txt"}))
        .await;
    assert_eq!(diff["status"], "modified", "{diff}");
    let refused = s
        .core
        .handle
        .rpc("git.diff", json!({"window": window, "file": "../x.txt"}))
        .await
        .expect_err("not a row");
    assert_eq!(refused.code, ErrorCode::NotFound, "{}", refused.message);

    // The history: the newest first, with the mark of the commit that a push would send.
    let log = s
        .core
        .call("git.log", json!({"window": window, "limit": 1}))
        .await;
    let commits = log["commits"].as_array().expect("commits");
    assert_eq!(commits.len(), 1, "{log}");
    assert_eq!(log["more"], true, "{log}");
    assert_eq!(commits[0]["subject"], "second");
    assert_eq!(commits[0]["head"], true);
    assert_eq!(commits[0]["unpushed"], true);
    assert_eq!(commits[0]["time_text"], "just now");
    let page = s
        .core
        .call("git.log", json!({"window": window, "skip": 1}))
        .await;
    assert_eq!(page["commits"][0]["subject"], "first", "{page}");
    assert_eq!(page["commits"][0]["unpushed"], false, "{page}");
    assert_eq!(
        page["commits"][0]["branches"],
        json!(["origin/main"]),
        "{page}"
    );
    assert_eq!(page["more"], false);

    let hash = commits[0]["short"].as_str().expect("a hash").to_owned();
    let shown = s
        .core
        .call("git.show", json!({"window": window, "commit": hash}))
        .await;
    assert_eq!(shown["files"][0]["path"], "b.txt", "{shown}");
    assert_eq!(shown["files"][0]["status"], "added", "{shown}");
    assert_eq!(shown["parents"].as_array().map(Vec::len), Some(1));
    let diff = s
        .core
        .call(
            "git.diff",
            json!({"window": window, "file": "b.txt", "commit": hash}),
        )
        .await;
    assert_eq!(diff["status"], "added", "{diff}");
    for bad in ["--output=/tmp/x", "HEAD~1", "ffffffff"] {
        let refused = s
            .core
            .handle
            .rpc("git.show", json!({"window": window, "commit": bad}))
            .await
            .expect_err("not a commit");
        assert_eq!(
            refused.code,
            ErrorCode::NotFound,
            "{bad}: {}",
            refused.message
        );
    }

    // The other worktree: its head and its branch.
    s.core
        .call("git.follow", json!({"window": window, "path": topic}))
        .await;
    head.until("the head of the linked worktree", WAIT, |items| {
        items
            .get("_")
            .is_some_and(|head| head["worktree"] == topic.as_str() && head["branch"] == "topic")
    })
    .await;
    changes
        .until(
            "no changes in the linked worktree",
            WAIT,
            serde_json::Map::is_empty,
        )
        .await;
    worktrees
        .until("the linked worktree is the current one", WAIT, |items| {
            items.get(&topic).is_some_and(|row| row["current"] == true)
        })
        .await;
    let main = &worktrees.items[&root];
    assert_eq!(main["dirty"], 1, "{main}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_reads_of_the_cli_take_a_path_and_the_repositories_keep_the_added_ones() {
    let s = start("cli");
    let status = s
        .core
        .call("git.status", json!({"path": text(&s.topic)}))
        .await;
    assert_eq!(status["head"]["branch"], "topic", "{status}");
    assert_eq!(status["head"]["repo_name"], "app", "{status}");
    assert_eq!(status["changes"], json!([]));
    let list = s
        .core
        .call("git.worktrees", json!({"path": text(&s.repo)}))
        .await;
    assert_eq!(
        list["worktrees"].as_array().map(Vec::len),
        Some(2),
        "{list}"
    );
    let list = s
        .core
        .call("git.branches", json!({"path": text(&s.repo)}))
        .await;
    assert_eq!(list["branches"][0]["name"], "main", "{list}");

    // The repositories of the workspaces and the panes, and the ones that the person added.
    let mut repos = Follower::new(&s.core, "git_repos").await;
    let other = s.base.join("other");
    std::fs::create_dir_all(&other).expect("a repository");
    git(&other, &["init", "-q", "-b", "trunk"]);
    s.core
        .call("git.repo_add", json!({"path": text(&other)}))
        .await;
    let common = text(&other.join(".git"));
    repos
        .until("the added repository", WAIT, |items| {
            items
                .get(&common)
                .is_some_and(|row| row["name"] == "other" && row["source"] == "added")
        })
        .await;
    let listed = s.core.call("git.repos", json!({})).await;
    assert!(
        listed["repos"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|row| row["common_dir"] == common.as_str()),
        "{listed}"
    );

    // Each refusal has its code.
    let refusals = [
        (
            "git.status",
            json!({"path": "app"}),
            ErrorCode::InvalidParams,
        ),
        ("git.status", json!({}), ErrorCode::InvalidParams),
        (
            "git.status",
            json!({"path": text(&s.base)}),
            ErrorCode::NotFound,
        ),
        (
            "git.repo_add",
            json!({"path": text(&s.base)}),
            ErrorCode::NotFound,
        ),
        (
            "git.follow",
            json!({"window": "01M4F1W7EM2GN3EV6NHXZJEGGA", "path": text(&s.repo)}),
            ErrorCode::NotFound,
        ),
    ];
    for (method, params, code) in refusals {
        let refused = s
            .core
            .handle
            .rpc(method, params.clone())
            .await
            .expect_err("refused");
        assert_eq!(refused.code, code, "{method} {params}: {}", refused.message);
    }
}

/// Reads `git.status` of a path until `done` holds.
async fn until_status(core: &Harness, path: &Path, what: &str, done: impl Fn(&Value) -> bool) {
    let deadline = Instant::now() + WAIT;
    loop {
        let status = core.call("git.status", json!({"path": text(path)})).await;
        if done(&status) {
            return;
        }
        assert!(Instant::now() < deadline, "timeout: {what}: {status}");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_switch_takes_or_leaves_the_changes_and_refuses_a_branch_of_another_worktree() {
    let s = start("switch");
    let at = |branch: &str| json!({"path": text(&s.repo), "branch": branch});
    git(&s.repo, &["branch", "other"]);

    // The branch of the linked worktree goes to that worktree.
    let refused = s
        .core
        .handle
        .rpc("git.switch", at("topic"))
        .await
        .expect_err("out in another worktree");
    assert_eq!(refused.code, ErrorCode::Conflict, "{}", refused.message);
    assert_eq!(refused.details["kind"], "worktree");
    assert_eq!(refused.details["worktree"], text(&s.topic).as_str());

    // A clean worktree switches at once, by name or by reference.
    s.core.call("git.switch", at("other")).await;
    until_status(&s.core, &s.repo, "on other", |st| {
        st["head"]["branch"] == "other"
    })
    .await;
    s.core.call("git.switch", at("refs/heads/main")).await;
    until_status(&s.core, &s.repo, "on main", |st| {
        st["head"]["branch"] == "main"
    })
    .await;

    // A worktree with changes asks, then leaves them in a stash of the branch.
    std::fs::write(s.repo.join("a.txt"), "changed\n").expect("a change");
    let refused = s
        .core
        .handle
        .rpc("git.switch", at("other"))
        .await
        .expect_err("changes");
    assert_eq!(
        refused.code,
        ErrorCode::ConfirmRequired,
        "{}",
        refused.message
    );
    assert_eq!(refused.details["kind"], "changes");
    assert_eq!(refused.details["files"], 1);
    let mut leave = at("other");
    leave["changes"] = json!("leave");
    s.core.call("git.switch", leave).await;
    until_status(&s.core, &s.repo, "on other and clean", |st| {
        st["head"]["branch"] == "other" && st["changes"] == json!([])
    })
    .await;

    // Back on main, the stash of main shows, and it comes back.
    s.core.call("git.switch", at("main")).await;
    let status = s
        .core
        .call("git.status", json!({"path": text(&s.repo)}))
        .await;
    let stash = status["head"]["stash"].clone();
    assert_eq!(stash["branch"], "main", "{status}");
    s.core
        .call(
            "git.unstash",
            json!({"path": text(&s.repo), "stash": stash["commit"]}),
        )
        .await;
    until_status(&s.core, &s.repo, "the change is back", |st| {
        st["changes"][0]["path"] == "a.txt" && st["head"]["stash"].is_null()
    })
    .await;

    // "Bring" takes the change to the other branch.
    let mut bring = at("other");
    bring["changes"] = json!("bring");
    s.core.call("git.switch", bring).await;
    until_status(&s.core, &s.repo, "the change is on other", |st| {
        st["head"]["branch"] == "other" && st["changes"][0]["path"] == "a.txt"
    })
    .await;

    // A remote branch with no local one gets a local branch that tracks it.
    git(&s.repo, &["stash", "-q"]);
    git(&s.repo, &["push", "-q", "origin", "other:feature"]);
    git(&s.repo, &["fetch", "-q", "origin"]);
    s.core.call("git.switch", at("origin/feature")).await;
    until_status(&s.core, &s.repo, "on the new feature", |st| {
        st["head"]["branch"] == "feature" && st["head"]["upstream"] == "origin/feature"
    })
    .await;
    let refused = s
        .core
        .handle
        .rpc("git.switch", at("nothing"))
        .await
        .expect_err("no branch");
    assert_eq!(refused.code, ErrorCode::NotFound, "{}", refused.message);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_fetch_and_a_pull_run_as_jobs_and_a_diverged_branch_is_refused() {
    let s = start("pull");
    git(&s.repo, &["push", "-q", "origin", "main"]);
    // Another clone pushes a commit, so the main worktree is behind after a fetch.
    let other = s.base.join("other");
    git(
        &s.base,
        &[
            "clone",
            "-q",
            &text(&s.base.join("origin.git")),
            &text(&other),
        ],
    );
    std::fs::write(other.join("c.txt"), "sea\n").expect("a file");
    git(&other, &["add", "."]);
    git(&other, &["commit", "-q", "-m", "third"]);
    git(&other, &["push", "-q", "origin", "main"]);

    let started = s
        .core
        .call("git.fetch", json!({"path": text(&s.repo)}))
        .await;
    assert!(started["job"].is_string(), "{started}");
    until_status(&s.core, &s.repo, "behind after the fetch", |st| {
        st["head"]["behind"] == 1 && st["head"]["fetched_text"] != "Never fetched"
    })
    .await;
    s.core
        .call("git.pull", json!({"path": text(&s.repo)}))
        .await;
    until_status(&s.core, &s.repo, "even after the pull", |st| {
        st["head"]["behind"] == 0 && st["head"]["ahead"] == 0
    })
    .await;
    assert!(s.repo.join("c.txt").exists());

    // A commit here and one there: the branch diverged, and a pull is refused.
    std::fs::write(other.join("d.txt"), "dee\n").expect("a file");
    git(&other, &["add", "."]);
    git(&other, &["commit", "-q", "-m", "fourth"]);
    git(&other, &["push", "-q", "origin", "main"]);
    std::fs::write(s.repo.join("e.txt"), "ee\n").expect("a file");
    git(&s.repo, &["add", "."]);
    git(&s.repo, &["commit", "-q", "-m", "fifth"]);
    git(&s.repo, &["fetch", "-q", "origin"]);
    let refused = s
        .core
        .handle
        .rpc("git.pull", json!({"path": text(&s.repo)}))
        .await
        .expect_err("diverged");
    assert_eq!(refused.code, ErrorCode::Conflict, "{}", refused.message);
    assert_eq!(refused.details["kind"], "diverged");

    // A repository with no remote has nothing to fetch.
    let lone = s.base.join("lone");
    std::fs::create_dir_all(&lone).expect("a folder");
    git(&lone, &["init", "-q", "-b", "main"]);
    let refused = s
        .core
        .handle
        .rpc("git.fetch", json!({"path": text(&lone)}))
        .await
        .expect_err("no remote");
    assert_eq!(refused.code, ErrorCode::NotFound, "{}", refused.message);
}

/// A program for a pane: it writes the token of its pane to a file and waits.
const WAITER: &str = r#"
import os, sys, time
open(sys.argv[1], "w").write(os.environ.get("NEOHAR_PANE_TOKEN", ""))
while True:
    time.sleep(60)
"#;

/// Sends a signal of a hook of Claude Code with the token of a pane.
async fn signal(core: &Harness, token: &str, event: &str, payload: Value) {
    let socket = core.api_socket();
    let line = json!({
        "jsonrpc": "2.0", "id": 1, "method": "adapter.signal",
        "params": {"token": token, "harness": "claude", "event": event, "payload": payload},
    })
    .to_string()
        + "\n";
    tokio::task::spawn_blocking(move || {
        use std::io::{BufRead, BufReader, Write};
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_write_under_a_working_agent_asks_first_and_force_writes_anyway() {
    let s = start("guard");
    git(&s.repo, &["branch", "other"]);
    let script = s.base.join("waiter.py");
    let token_file = s.base.join("token");
    std::fs::write(&script, WAITER).expect("the program");
    // A pane whose folder is in the main worktree, with a run of Claude Code in it.
    s.core
        .call(
            "workspace.create",
            json!({"name": "agent", "command": {
                "argv": ["/usr/bin/python3", text(&script), text(&token_file)],
                "cwd": text(&s.repo),
            }}),
        )
        .await;
    let deadline = Instant::now() + WAIT;
    let token = loop {
        if let Some(token) = std::fs::read_to_string(&token_file)
            .ok()
            .filter(|token| !token.trim().is_empty())
        {
            break token.trim().to_owned();
        }
        assert!(Instant::now() < deadline, "no token");
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    let session = json!({"session_id": "s1", "cwd": text(&s.repo), "source": "startup"});
    signal(&s.core, &token, "SessionStart", session).await;
    let prompt = json!({"session_id": "s1", "prompt": "go"});
    signal(&s.core, &token, "UserPromptSubmit", prompt.clone()).await;
    until_status(&s.core, &s.repo, "the agent works in the worktree", |st| {
        st["head"]["agents"][0]["state"] == "working"
    })
    .await;
    let worktrees = s
        .core
        .call("git.worktrees", json!({"path": text(&s.repo)}))
        .await;
    assert_eq!(worktrees["worktrees"][0]["agents"][0]["harness"], "claude");
    assert!(
        worktrees["worktrees"][1]["agents"].is_null(),
        "the linked worktree has no agent: {worktrees}"
    );

    // A switch and a pull ask first, and name the agent.
    let switch = json!({"path": text(&s.repo), "branch": "other"});
    let refused = s
        .core
        .handle
        .rpc("git.switch", switch.clone())
        .await
        .expect_err("an agent works");
    assert_eq!(
        refused.code,
        ErrorCode::ConfirmRequired,
        "{}",
        refused.message
    );
    assert_eq!(refused.details["kind"], "agents");
    assert_eq!(refused.details["agents"][0]["state"], "working");
    assert_eq!(
        refused.message,
        "Claude Code is working in app. A switch changes its files."
    );
    let refused = s
        .core
        .handle
        .rpc("git.pull", json!({"path": text(&s.repo)}))
        .await
        .expect_err("an agent works");
    assert_eq!(
        refused.code,
        ErrorCode::ConfirmRequired,
        "{}",
        refused.message
    );
    // The linked worktree has no agent, so its switch does not ask.
    s.core
        .call(
            "git.switch",
            json!({"path": text(&s.topic), "branch": "refs/heads/other"}),
        )
        .await;
    s.core
        .call(
            "git.switch",
            json!({"path": text(&s.topic), "branch": "topic"}),
        )
        .await;

    // `force` writes anyway.
    let mut forced = switch.clone();
    forced["force"] = json!(true);
    s.core.call("git.switch", forced).await;
    until_status(&s.core, &s.repo, "on other", |st| {
        st["head"]["branch"] == "other"
    })
    .await;

    // An agent at the end of its turn is idle, and a write does not ask.
    signal(&s.core, &token, "Stop", json!({"session_id": "s1"})).await;
    until_status(&s.core, &s.repo, "the agent is idle", |st| {
        st["head"]["agents"][0]["state"] == "idle"
    })
    .await;
    s.core
        .call(
            "git.switch",
            json!({"path": text(&s.repo), "branch": "main"}),
        )
        .await;
    until_status(&s.core, &s.repo, "on main", |st| {
        st["head"]["branch"] == "main"
    })
    .await;
}
