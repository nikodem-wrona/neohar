//! A program inside a pane may not weaken the approvals with the powers of the CLI.
//!
//! The pane runs a small shell program that calls `nh` with the socket of its environment, as a
//! tool of an agent could. The core reads the parents of the client, finds the holder of the
//! pane, and refuses the calls that answer interactions or weaken the approvals. A harmless call
//! goes on. The same calls from outside the pane (this test process) go on. Input from inside a
//! pane to a pane that waits for an answer is refused too, and input to it is fine again once it
//! waits for nothing.

mod core_support;

use std::{
    io::Write,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

use core_support::Harness;
use serde_json::{Value, json};

/// The program in the pane. Each call writes its output and its exit code to a file of its own.
const PROGRAM: &str = r#"#!/bin/sh
dir="$1"
"$NEOHAR_BIN" config set approvals.mode observe > "$dir/weaken.out" 2>&1
echo "exit $?" >> "$dir/weaken.out"
"$NEOHAR_BIN" interaction answer i1 allow > "$dir/answer.out" 2>&1
echo "exit $?" >> "$dir/answer.out"
"$NEOHAR_BIN" api call git.trust '{"path": "."}' > "$dir/trust.out" 2>&1
echo "exit $?" >> "$dir/trust.out"
"$NEOHAR_BIN" config set ui.font_size 15 > "$dir/harmless.out" 2>&1
echo "exit $?" >> "$dir/harmless.out"
touch "$dir/done"
sleep 600
"#;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("nh-guard-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a directory");
    dir
}

async fn wait_for(path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !path.exists() {
        assert!(Instant::now() < deadline, "timeout: {}", path.display());
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

fn read(dir: &Path, name: &str) -> String {
    std::fs::read_to_string(dir.join(name)).unwrap_or_default()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_program_in_a_pane_may_not_weaken_the_approvals() {
    let dir = scratch("weaken");
    let script = dir.join("program.sh");
    std::fs::write(&script, PROGRAM).expect("the program");
    let core = Harness::start()
        .with_command_pane(&[
            "/bin/sh",
            script.to_str().expect("a path"),
            dir.to_str().expect("a path"),
        ])
        .await;
    wait_for(&dir.join("done")).await;

    let weaken = read(&dir, "weaken.out");
    assert!(!weaken.contains("exit 0"), "{weaken}");
    assert!(weaken.contains("outside a neohar pane"), "{weaken}");
    let answer = read(&dir, "answer.out");
    assert!(!answer.contains("exit 0"), "{answer}");
    assert!(answer.contains("outside a neohar pane"), "{answer}");
    let trust = read(&dir, "trust.out");
    assert!(!trust.contains("exit 0"), "{trust}");
    assert!(trust.contains("outside a neohar pane"), "{trust}");
    let harmless = read(&dir, "harmless.out");
    assert!(harmless.contains("exit 0"), "{harmless}");

    // The mode did not change, and the harmless key did.
    let settings = core.call("config.get", json!({})).await;
    let sections = &settings["sections"];
    assert_ne!(
        sections["approvals"]["entries"]["mode"]["value"], "observe",
        "{sections}"
    );
    assert_eq!(
        sections["ui"]["entries"]["font_size"]["value"], 15,
        "{sections}"
    );

    // The same call from outside the pane goes on.
    let socket = core.api_socket().display().to_string();
    let ran = tokio::task::spawn_blocking(move || {
        std::process::Command::new(env!("CARGO_BIN_EXE_nh"))
            .args([
                "--socket",
                &socket,
                "config",
                "set",
                "approvals.mode",
                "observe",
            ])
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
    let settings = core.call("config.get", json!({})).await;
    assert_eq!(
        settings["sections"]["approvals"]["entries"]["mode"]["value"],
        "observe"
    );
}

/// The pane that waits: it writes its token and its id, and logs every byte that it gets.
const WAITING: &str = r#"#!/bin/sh
dir="$1"
echo "$NEOHAR_PANE_TOKEN" > "$dir/b.token"
echo "$NEOHAR_PANE_ULID" > "$dir/b.pane"
stty raw -echo
exec cat > "$dir/b.keys"
"#;

/// The pane that types into the other one, once before and once after its request ends.
const TYPIST: &str = r#"#!/bin/sh
dir="$1"
while [ ! -f "$dir/go1" ]; do sleep 0.1; done
pane=$(cat "$dir/b.pane")
"$NEOHAR_BIN" api call pane.paste "{\"pane\":\"$pane\",\"input_id\":1,\"text\":\"1\"}" > "$dir/one.out" 2>&1
echo "exit $?" >> "$dir/one.out"
touch "$dir/one.done"
while [ ! -f "$dir/go2" ]; do sleep 0.1; done
"$NEOHAR_BIN" api call pane.paste "{\"pane\":\"$pane\",\"input_id\":2,\"text\":\"2\"}" > "$dir/two.out" 2>&1
echo "exit $?" >> "$dir/two.out"
touch "$dir/two.done"
sleep 600
"#;

/// A shim of a hook that holds a request of the harness. It is killed at the end of the test.
struct Shim(Child);

impl Drop for Shim {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn signal(socket: &Path, token: &str, event: &str, payload: Value) {
    use std::io::{BufRead, BufReader};
    let line = json!({
        "jsonrpc": "2.0", "id": 1, "method": "adapter.signal",
        "params": {"token": token, "harness": "claude", "event": event, "payload": payload},
    })
    .to_string()
        + "\n";
    let mut stream = std::os::unix::net::UnixStream::connect(socket).expect("connect");
    stream.write_all(line.as_bytes()).expect("write");
    let mut reply = String::new();
    BufReader::new(&mut stream)
        .read_line(&mut reply)
        .expect("a reply");
    assert!(reply.contains("\"result\""), "the signal failed: {reply}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_program_in_a_pane_may_not_type_into_a_pane_that_waits_for_an_answer() {
    let dir = scratch("input");
    let (waiting, typist) = (dir.join("waiting.sh"), dir.join("typist.sh"));
    std::fs::write(&waiting, WAITING).expect("a program");
    std::fs::write(&typist, TYPIST).expect("a program");
    let arg = dir.to_str().expect("a path");
    let core = Harness::start()
        .with_command_pane(&["/bin/sh", waiting.to_str().expect("a path"), arg])
        .await
        .with_command_pane(&["/bin/sh", typist.to_str().expect("a path"), arg])
        .await;
    wait_for(&dir.join("b.pane")).await;
    let token = read(&dir, "b.token").trim().to_owned();
    let socket = core.api_socket();

    // A Claude Code session in the waiting pane asks for permission, and its hook is held.
    let (at, with) = (socket.clone(), token.clone());
    tokio::task::spawn_blocking(move || {
        signal(
            &at,
            &with,
            "SessionStart",
            json!({"session_id": "s1", "cwd": "/work", "source": "startup"}),
        );
        signal(
            &at,
            &with,
            "PreToolUse",
            json!({"session_id": "s1", "cwd": "/work", "tool_name": "Bash",
                   "tool_use_id": "toolu_1", "tool_input": {"command": "ls"}}),
        );
    })
    .await
    .expect("the task");
    let mut child = Command::new(env!("CARGO_BIN_EXE_nh"))
        .args(["hook", "claude", "PermissionRequest"])
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("NEOHAR", "1")
        .env("NEOHAR_SOCKET", &socket)
        .env("NEOHAR_PANE_TOKEN", &token)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("the shim starts");
    let mut stdin = child.stdin.take().expect("stdin");
    stdin
        .write_all(
            json!({"session_id": "s1", "cwd": "/work", "hook_event_name": "PermissionRequest",
                   "tool_name": "Bash", "tool_input": {"command": "ls"},
                   "permission_mode": "default"})
            .to_string()
            .as_bytes(),
        )
        .expect("the payload goes in");
    drop(stdin);
    let _shim = Shim(child);
    let deadline = Instant::now() + Duration::from_secs(30);
    let open = loop {
        let listed = core
            .call("interaction.list", json!({"open_only": true}))
            .await;
        let found = listed["interactions"]
            .as_array()
            .and_then(|list| list.iter().find(|i| i["kind"] == "approval").cloned());
        if let Some(found) = found {
            break found;
        }
        assert!(Instant::now() < deadline, "no open approval: {listed}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    };

    // The other pane types "1" into the pane that waits: refused, and nothing goes in.
    std::fs::write(dir.join("go1"), "").expect("a file");
    wait_for(&dir.join("one.done")).await;
    let one = read(&dir, "one.out");
    assert!(!one.contains("exit 0"), "{one}");
    assert!(one.contains("waits for an answer"), "{one}");
    assert!(!read(&dir, "b.keys").contains('1'), "the key went in");

    // The request ends. The pane waits for nothing, and input to it goes in again.
    core.call("interaction.dismiss", json!({"interaction": open["id"]}))
        .await;
    std::fs::write(dir.join("go2"), "").expect("a file");
    wait_for(&dir.join("two.done")).await;
    let two = read(&dir, "two.out");
    assert!(two.contains("exit 0"), "{two}");
    let deadline = Instant::now() + Duration::from_secs(10);
    while !read(&dir, "b.keys").contains('2') {
        assert!(Instant::now() < deadline, "the text never reached the pane");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}
