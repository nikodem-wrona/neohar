//! The verbs of `nh` against a real core: a person types words, and the core does the work.

mod core_support;

use std::process::{Command, Stdio};

use core_support::Harness;
use serde_json::{Value, json};

const NH: &str = env!("CARGO_BIN_EXE_nh");

struct Ran {
    code: i32,
    out: String,
    err: String,
}

impl Ran {
    fn json(&self) -> Value {
        serde_json::from_str(&self.out).unwrap_or_else(|e| panic!("JSON: {e}: {}", self.out))
    }
}

async fn nh(h: &Harness, args: &[&str]) -> Ran {
    let socket = h.api_socket().display().to_string();
    let args: Vec<String> = ["--socket", &socket, "--json"]
        .into_iter()
        .map(str::to_owned)
        .chain(args.iter().map(|a| (*a).to_owned()))
        .collect();
    tokio::task::spawn_blocking(move || {
        let output = Command::new(NH)
            .args(&args)
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .stdin(Stdio::null())
            .output()
            .expect("nh runs");
        Ran {
            code: output.status.code().unwrap_or(-1),
            out: String::from_utf8_lossy(&output.stdout).into_owned(),
            err: String::from_utf8_lossy(&output.stderr).into_owned(),
        }
    })
    .await
    .expect("the task")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn workspaces_and_tabs_are_made_listed_and_renamed_with_words() {
    let h = Harness::start_layout();
    let made = nh(&h, &["workspace", "create", "Work"]).await;
    assert_eq!(made.code, 0, "{}", made.err);
    let handle = made.json()["handle"].as_str().expect("a handle").to_owned();
    let listed = nh(&h, &["workspace", "list"]).await.json();
    assert_eq!(listed["workspaces"][0]["name"], "Work");
    let renamed = nh(&h, &["workspace", "rename", &handle, "Play"]).await;
    assert_eq!(renamed.code, 0, "{}", renamed.err);
    let shown = nh(&h, &["workspace", "show", &handle]).await.json();
    assert_eq!(shown["workspace"]["name"], "Play");
    let tab = nh(&h, &["tab", "create", &handle, "--name", "Build"]).await;
    assert_eq!(tab.code, 0, "{}", tab.err);
    assert!(tab.json()["handle"].is_string());
    // A thing that does not exist is exit code 3.
    let missing = nh(&h, &["workspace", "show", "w99"]).await;
    assert_eq!(missing.code, 3, "{}", missing.err);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn send_keys_types_into_a_pane_and_read_prints_its_text() {
    let h = Harness::start();
    let made = h
        .call(
            "workspace.create",
            json!({"name": "Keys", "command": {"argv": ["/bin/cat"]}}),
        )
        .await;
    let pane = made["pane_handle"].as_str().expect("a handle").to_owned();
    // The program of the pane starts a moment after the layout commits.
    let id: ulid::Ulid = made["pane"]
        .as_str()
        .expect("an id")
        .parse()
        .expect("an id");
    let started = std::time::Instant::now();
    while h.handle.pane(id).is_none() {
        assert!(
            started.elapsed() < std::time::Duration::from_secs(20),
            "the pane never started"
        );
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    let sent = nh(&h, &["pane", "send-keys", &pane, "hello from nh", "Enter"]).await;
    assert_eq!(sent.code, 0, "{}", sent.err);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    loop {
        let read = nh(&h, &["pane", "read", &pane]).await;
        assert_eq!(read.code, 0, "{}", read.err);
        if read.json()["text"]
            .as_str()
            .is_some_and(|t| t.contains("hello from nh"))
        {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the pane never showed the text"
        );
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn config_set_takes_json_or_a_word_and_get_shows_it() {
    let h = Harness::start_layout();
    assert_eq!(nh(&h, &["config", "set", "tasks.ports", "2"]).await.code, 0);
    let got = nh(&h, &["config", "get", "tasks"]).await.json();
    assert_eq!(got["sections"]["tasks"]["entries"]["ports"]["value"], 2);
    assert_eq!(
        nh(&h, &["config", "set", "approvals.mode", "gate"])
            .await
            .code,
        0
    );
    let bad = nh(&h, &["config", "set", "approvals.mode", "sometimes"]).await;
    assert_ne!(bad.code, 0, "{}", bad.out);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn events_tail_prints_the_cursor_and_replays_from_a_cursor() {
    let h = Harness::start_layout();
    // With no flags the command prints the cursor of the end of the log.
    let end = nh(&h, &["events", "tail"]).await;
    assert_eq!(end.code, 0, "{}", end.err);
    let cursor = end.json()["cursor"].clone();
    assert!(cursor["seq"].is_u64());
    nh(&h, &["workspace", "create", "Work"]).await;
    // Replaying from that cursor gives the new events and then the new cursor.
    let replay = nh(&h, &["events", "tail", "--since", &cursor.to_string()]).await;
    assert_eq!(replay.code, 0, "{}", replay.err);
    let lines: Vec<Value> = replay
        .out
        .lines()
        .map(|line| serde_json::from_str(line).expect("a JSON line"))
        .collect();
    assert!(
        lines.iter().any(|l| l["kind"] == "workspace.created"),
        "{lines:?}"
    );
    assert!(lines.last().expect("a line")["cursor"]["seq"].as_u64() > cursor["seq"].as_u64());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn completions_are_made_for_the_three_shells() {
    for shell in ["zsh", "bash", "fish"] {
        let output = Command::new(NH)
            .args(["completions", shell])
            .env_clear()
            .output()
            .expect("nh runs");
        assert!(output.status.success(), "{shell}");
        let text = String::from_utf8_lossy(&output.stdout);
        assert!(
            text.contains("workspace") && text.contains("agent"),
            "{shell}: {text:.200}"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fs_list_prints_the_folders_first_and_the_files() {
    let h = Harness::start_layout();
    let dir = std::fs::canonicalize(h.dir.join("home"))
        .expect("the home exists")
        .join("listed");
    std::fs::create_dir_all(dir.join("src")).expect("a folder");
    std::fs::write(dir.join("a.txt"), "a").expect("a file");
    let ran = nh(&h, &["fs", "list", &dir.display().to_string()]).await;
    assert_eq!(ran.code, 0, "{}", ran.err);
    let names: Vec<String> = ran.json()["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .map(|e| e["name"].as_str().expect("a name").to_owned())
        .collect();
    assert_eq!(names, ["src", "a.txt"]);
}
