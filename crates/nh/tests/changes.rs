//! The Changes tab against the real core: a window that follows a folder sees the changes of the
//! git worktree that holds it, from `HEAD`.

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
    /// The temporary folder of the test. It is not in a git repository.
    base: PathBuf,
    repo: PathBuf,
}

/// A core, and a repository with one commit on `main`.
fn start(name: &str) -> Setup {
    let core = Harness::start();
    let base = std::fs::canonicalize(std::env::temp_dir())
        .expect("a path")
        .join(format!("nh-changes-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let repo = base.join("app");
    std::fs::create_dir_all(&repo).expect("a repository");
    git(&repo, &["init", "-q", "-b", "main"]);
    std::fs::write(repo.join("a.txt"), "one\ntwo\n").expect("a file");
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "first"]);
    Setup { core, base, repo }
}

/// How long a test waits for the core to reach a state. On a busy Mac, macOS checks each new
/// binary, and git can take seconds. The core sets up the two watches of a worktree one after
/// the other, and on macOS `fseventsd` sets up the watches of all processes one at a time. Under
/// `mise run test`, one setup waited more than 30 s behind the watches of the other tests (see
/// `READY` in the tests of `nh_core::config`).
const WAIT: Duration = Duration::from_secs(120);

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
async fn a_window_that_follows_a_folder_sees_the_changes_of_its_worktree() {
    let s = start("folder");
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

    // A folder in a checkout: the changes go from HEAD, so a clean checkout has none.
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

    // A file that the person writes: only the watch of the worktree sees it.
    std::fs::write(s.repo.join("src/new.txt"), "a\nb\n").expect("a file");
    std::fs::write(s.repo.join("a.txt"), "one\ntwo\nthree\n").expect("a change");
    rows.until("the new file is a row", WAIT, |row| {
        row["path"] == "src/new.txt" && row["status"] == "untracked"
    })
    .await;
    rows.until("the change is a row", WAIT, |row| {
        row["path"] == "a.txt" && row["status"] == "modified"
    })
    .await;
    head.until("the totals", WAIT, |head| {
        head["changes"]["files"] == 2 && head["diff_stats"] == json!({"add": 3, "del": 0})
    })
    .await;

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

    // Another repository: its rows replace the rows of the first one.
    let other = s.base.join("other");
    std::fs::create_dir_all(&other).expect("a repository");
    git(&other, &["init", "-q", "-b", "trunk"]);
    std::fs::write(other.join("notes.md"), "x\n").expect("a file");
    follow(&other).await;
    let other_root = other.display().to_string();
    head.until("the head of the other repository", WAIT, |head| {
        head["root"] == other_root.as_str() && head["changes"]["files"] == 1
    })
    .await;
    assert_eq!(head.items["_"]["branch"], "trunk");
    rows.until("the row of the other repository", WAIT, |row| {
        row["path"] == "notes.md"
    })
    .await;
    rows.until_all("only the row of the other repository", WAIT, |items| {
        items.len() == 1
    })
    .await;

    // A folder that is not in a git worktree has a head with no root and no rows.
    let outside = s.base.clone();
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
