//! Answers by verified keystrokes against the real core.
//!
//! A pane runs a small shell program that draws the approval dialog of Claude Code and reads keys.
//! The core reads the dialog from the screen (the built-in manifest `claude`), opens a provisional
//! interaction, and types the answer of a client. The program logs every key that it gets.

mod core_support;

use std::{
    io::Write,
    path::PathBuf,
    time::{Duration, Instant},
};

use core_support::Harness;
use serde_json::{Value, json};

/// The program in the pane. Escape sequences of the arrow keys are `ESC [ A` and `ESC [ B`.
const DIALOG: &str = r#"#!/bin/bash
log="$1"
# The core gives the pane a token, as it does for every pane. A hook of the pane carries it.
echo "$NEOHAR_PANE_TOKEN" > "$log.token"
labels=("Yes" "Yes, and always allow access to this folder" "No")
ptr=0
draw() {
  printf '\033[2J\033[H'
  echo "──────────────────────────────────────────"
  echo " Bash command"
  echo "╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌"
  echo " touch note.txt"
  echo "╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌"
  echo " Do you want to proceed?"
  for i in 0 1 2; do
    mark=" "
    [ "$i" = "$ptr" ] && mark="❯"
    echo " $mark $((i + 1)). ${labels[$i]}"
  done
  echo
  echo " Esc to cancel"
}
# Keys one at a time, with no echo. The output keeps its processing, so a line ends with a carriage
# return and the dialog is drawn as the harness draws it. Enter reads as an empty key.
stty -echo -icanon min 1 time 0
draw
while true; do
  IFS= read -rsn1 key || exit 1
  if [ "$key" = $'\033' ]; then
    IFS= read -rsn2 -t 0.05 rest
    case "$rest" in
      '[A') echo "Up" >> "$log"; [ "$ptr" -gt 0 ] && ptr=$((ptr - 1)); draw ;;
      '[B') echo "Down" >> "$log"; [ "$ptr" -lt 2 ] && ptr=$((ptr + 1)); draw ;;
      *) echo "Esc" >> "$log" ;;
    esac
  elif [ -z "$key" ]; then
    echo "Enter" >> "$log"
    echo "selected $((ptr + 1))" >> "$log"
    printf '\033[2J\033[H'
    echo "Answered: ${labels[$ptr]}"
    sleep 600
    exit 0
  elif [ "$key" = "1" ] || [ "$key" = "2" ] || [ "$key" = "3" ]; then
    # A digit picks the option and answers at once, as Claude Code does.
    ptr=$((key - 1))
    echo "Digit $key" >> "$log"
    echo "selected $key" >> "$log"
    printf '\033[2J\033[H'
    echo "Answered: ${labels[$ptr]}"
    sleep 600
    exit 0
  else
    echo "Key $key" >> "$log"
  fi
done
"#;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("nh-keys-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a directory");
    dir
}

struct Setup {
    core: Harness,
    log: PathBuf,
}

async fn start(name: &str) -> Setup {
    let dir = scratch(name);
    let script = dir.join("dialog.sh");
    std::fs::write(&script, DIALOG).expect("the program");
    let log = dir.join("keys.log");
    std::fs::File::create(&log).expect("the log");
    let core = Harness::start()
        .with_command_pane(&[
            "/bin/bash",
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
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(found) = f().await {
            return found;
        }
        assert!(Instant::now() < deadline, "timeout: {what}");
        tokio::time::sleep(Duration::from_millis(50)).await;
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

/// What the core knows, for a failure.
async fn dump(core: &Harness) -> String {
    let listed = core
        .call("interaction.list", json!({"open_only": false}))
        .await;
    let runs = agents(core).await;
    let mut screen = String::new();
    if let Some(pane) = runs.first().and_then(|r| r["pane"].as_str()) {
        let mut client = core_support::client::Client::connect(&core.api_socket()).await;
        let read = client
            .ok("pane.read", json!({"pane": pane, "visible": true}))
            .await;
        screen = read["text"].as_str().unwrap_or("").to_owned();
    }
    format!("{listed}\n{runs:?}\nSCREEN:\n{screen}")
}

/// Starts the session, so that the run is bound and its version is known, and waits for the
/// provisional interaction of the dialog on the screen.
async fn dialog_interaction(s: &Setup) -> Value {
    let token_file = PathBuf::from(format!("{}.token", s.log.display()));
    let token = until("the token of the pane", async || {
        std::fs::read_to_string(&token_file)
            .ok()
            .map(|t| t.trim().to_owned())
            .filter(|t| !t.is_empty())
    })
    .await;
    signal(
        &s.core,
        &token,
        "SessionStart",
        json!({"session_id": "s1", "cwd": "/work", "source": "startup"}),
    )
    .await;
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let listed = s.core.call("interaction.list", json!({})).await;
        let found = listed["interactions"].as_array().and_then(|list| {
            list.iter()
                .find(|i| i["state"] == "open" && i["answerable"] == true)
                .cloned()
        });
        if let Some(found) = found {
            return found;
        }
        assert!(
            Instant::now() < deadline,
            "no answerable interaction: {}",
            dump(&s.core).await
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

fn keys(s: &Setup) -> Vec<String> {
    std::fs::read_to_string(&s.log)
        .unwrap_or_default()
        .lines()
        .map(str::to_owned)
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_dialog_on_the_screen_is_answered_with_the_digit_of_the_option() {
    let s = start("deny").await;
    let open = dialog_interaction(&s).await;
    assert_eq!(open["kind"], "approval");
    assert_eq!(open["mode"], "observe");
    assert_eq!(open["channels"], json!(["keystroke"]));
    let id = open["id"].as_str().expect("an id").to_owned();
    s.core
        .call(
            "interaction.answer",
            json!({"interaction": id, "decision": {"kind": "approval", "behavior": "deny"}, "via": "cli"}),
        )
        .await;
    until("the program got the answer", async || {
        keys(&s).contains(&"selected 3".to_owned()).then_some(())
    })
    .await;
    // The accelerator is one key, and no Enter follows it.
    assert_eq!(keys(&s), ["Digit 3", "selected 3"]);
    // The dialog is gone from the screen: that is the resolution, and the answer is delivered.
    let deadline = Instant::now() + Duration::from_secs(20);
    let done = loop {
        let listed = s
            .core
            .call("interaction.list", json!({"open_only": false}))
            .await;
        let found = listed["interactions"].as_array().and_then(|list| {
            list.iter()
                .find(|i| i["id"] == id.as_str() && i["state"] == "delivered")
                .cloned()
        });
        if let Some(found) = found {
            break found;
        }
        assert!(
            Instant::now() < deadline,
            "not delivered: {}",
            dump(&s.core).await
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    assert_eq!(
        done["delivery"]["decided_by"], "ui",
        "the in-process handle is the UI"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn allow_is_one_enter_when_the_pointer_is_already_on_it() {
    let s = start("allow").await;
    let open = dialog_interaction(&s).await;
    let id = open["id"].as_str().expect("an id").to_owned();
    s.core
        .call(
            "interaction.answer",
            json!({"interaction": id, "decision": {"kind": "approval", "behavior": "allow"}, "via": "cli"}),
        )
        .await;
    until("the program got the answer", async || {
        keys(&s).contains(&"selected 1".to_owned()).then_some(())
    })
    .await;
    assert_eq!(keys(&s), ["Enter", "selected 1"]);
}
