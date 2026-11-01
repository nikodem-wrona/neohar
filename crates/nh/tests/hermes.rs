//! The chats with Hermes Agent against the real core.
//!
//! The `hermes` of the test is a bash script that speaks enough ACP: it answers `initialize`,
//! opens session `s1`, answers a prompt with "pong", asks for an approval when the prompt says
//! "edit", ends when the prompt says "crash", and sends the history again on `session/resume`. It keeps each line that it reads in
//! `$HOME/hermes-in`.

mod core_support;

use std::time::{Duration, Instant};

use core_support::{Harness, client::Client, write_program};
use serde_json::{Value, json};

const FAKE_HERMES: &str = r#"#!/bin/bash
case "$1 $2" in
  "acp --version") echo "0.22.5"; exit 0 ;;
  "acp --check") echo "Hermes ACP check OK"; exit 0 ;;
esac
log="$HOME/hermes-in"
send() { printf '%s\n' "$1"; }
update() { send "{\"jsonrpc\":\"2.0\",\"method\":\"session/update\",\"params\":{\"sessionId\":\"s1\",\"update\":$1}}"; }
while IFS= read -r line; do
  printf '%s\n' "$line" >> "$log"
  method=""; id=""
  [[ $line =~ \"method\":\"([a-z/_]+)\" ]] && method="${BASH_REMATCH[1]}"
  [[ $line =~ \"id\":([0-9]+) ]] && id="${BASH_REMATCH[1]}"
  case "$method" in
    initialize)
      send "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{\"protocolVersion\":1,\"agentCapabilities\":{\"loadSession\":true}}}" ;;
    session/new)
      send "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{\"sessionId\":\"s1\",\"models\":{\"availableModels\":[{\"modelId\":\"openai-codex:gpt\",\"name\":\"GPT\"}],\"currentModelId\":\"openai-codex:gpt\"},\"modes\":{\"availableModes\":[{\"id\":\"default\",\"name\":\"Default\"},{\"id\":\"dont_ask\",\"name\":\"Don't Ask\"}],\"currentModeId\":\"default\"}}}" ;;
    session/resume)
      update '{"sessionUpdate":"user_message_chunk","content":{"type":"text","text":"hello"}}'
      update '{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"pong"}}'
      send "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{}}" ;;
    session/prompt)
      if [[ $line == *crash* ]]; then echo "the fake hermes crashed" >&2; exit 3; fi
      update '{"sessionUpdate":"session_info_update","title":"Say pong"}'
      if [[ $line == *edit* ]]; then
        send '{"jsonrpc":"2.0","id":0,"method":"session/request_permission","params":{"sessionId":"s1","options":[{"kind":"allow_once","name":"Allow once","optionId":"allow_once"},{"kind":"reject_once","name":"Deny","optionId":"deny"}],"toolCall":{"toolCallId":"edit-approval-1","title":"Approve edit: a.txt","kind":"edit","status":"pending","content":[{"type":"diff","path":"a.txt","newText":"x\n"}]}}}'
        IFS= read -r answer
        printf '%s\n' "$answer" >> "$log"
      fi
      update '{"sessionUpdate":"agent_message_chunk","messageId":"m1","content":{"type":"text","text":"pong"}}'
      update '{"sessionUpdate":"usage_update","used":15234,"size":500000}'
      send "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{\"stopReason\":\"end_turn\"}}" ;;
  esac
done
"#;

async fn until<T>(what: &str, mut f: impl AsyncFnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        if let Some(found) = f().await {
            return found;
        }
        assert!(Instant::now() < deadline, "timeout: {what}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// A client that follows a projection: its rows by key, with each patch applied.
struct Follower {
    client: Client,
    id: String,
    items: serde_json::Map<String, Value>,
    order: Vec<String>,
}

impl Follower {
    async fn new(core: &Harness, name: &str) -> Self {
        let mut client = Client::connect(&core.api_socket()).await;
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
        loop {
            let line = follower.client.recv().await;
            if follower.apply(&line) {
                return follower;
            }
        }
    }

    fn apply(&mut self, line: &Value) -> bool {
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
                true
            }
            _ => false,
        }
    }

    /// Waits until `f` finds something in the rows.
    async fn until<T>(&mut self, what: &str, f: impl Fn(&Self) -> Option<T>) -> T {
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            if let Some(found) = f(self) {
                return found;
            }
            let left = deadline.saturating_duration_since(Instant::now());
            assert!(!left.is_zero(), "timeout: {what}: {:?}", self.rows());
            if let Some(line) = self.client.try_recv(left.min(Duration::from_secs(1))).await {
                self.apply(&line);
            }
        }
    }

    /// The rows in their order.
    fn rows(&self) -> Vec<Value> {
        self.order
            .iter()
            .filter_map(|key| self.items.get(key).cloned())
            .collect()
    }
}

fn summary(items: &Follower) -> Vec<String> {
    items
        .rows()
        .iter()
        .map(|item| {
            let body = &item["body"];
            let kind = body["type"].as_str().unwrap_or("?");
            let text = body["markdown"]["html"]
                .as_str()
                .or_else(|| body["title"].as_str())
                .or_else(|| body["text"].as_str())
                .unwrap_or("");
            format!("{kind}:{}", text.trim())
        })
        .collect()
}

fn setup() -> (Harness, String) {
    let core = Harness::start();
    write_program(&core.dir.join("bin").join("hermes"), FAKE_HERMES);
    let folder = core.dir.join("home").join("project");
    std::fs::create_dir_all(&folder).expect("the folder is made");
    (core, folder.to_string_lossy().into_owned())
}

fn sent(core: &Harness) -> String {
    std::fs::read_to_string(core.dir.join("home").join("hermes-in")).unwrap_or_default()
}

#[tokio::test(flavor = "multi_thread")]
async fn the_check_finds_the_program_of_the_config() {
    let (core, _) = setup();
    let status = core.call("hermes.check", json!({})).await;
    assert_eq!(status["found"], json!(true), "{status}");
    assert_eq!(status["ok"], json!(true), "{status}");
    assert_eq!(status["version"], json!("0.22.5"));
    core.call(
        "config.set",
        json!({"key": "hermes.command", "value": "no-such-hermes"}),
    )
    .await;
    let status = until("the new command", async || {
        let status = core.call("hermes.check", json!({})).await;
        (status["found"] == json!(false)).then_some(status)
    })
    .await;
    assert!(
        status["message"]
            .as_str()
            .is_some_and(|m| m.contains("no-such-hermes")),
        "{status}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_chat_sends_prompts_answers_an_approval_and_comes_back_after_a_restart() {
    let (mut core, folder) = setup();
    let mut chats = Follower::new(&core, "hermes_chats").await;
    let started = core
        .call("hermes.start", json!({"cwd": folder, "prompt": "hello"}))
        .await;
    let chat = started["chat"].as_str().expect("a chat id").to_owned();
    let mut items = Follower::new(&core, &format!("hermes_chat:{chat}")).await;

    // The first prompt waits for the session, then gets its answer.
    items
        .until("the answer", |items| {
            (summary(items) == ["prompt:<p>hello</p>", "assistant:<p>pong</p>"]).then_some(())
        })
        .await;
    let row = chats
        .until("an idle chat", |chats| {
            chats
                .items
                .get(&chat)
                .filter(|row| row["state"] == "idle")
                .cloned()
        })
        .await;
    assert_eq!(row["session"], "s1");
    assert_eq!(row["title"], "Say pong");
    assert_eq!(row["model"], "openai-codex:gpt");
    assert_eq!(row["context_text"], "15k of 500k tokens");
    assert_eq!(row["modes"].as_array().map(Vec::len), Some(2));

    // The same input id twice sends one prompt.
    for _ in 0..2 {
        core.call(
            "hermes.prompt",
            json!({"chat": chat, "text": "please edit a.txt", "input_id": 1}),
        )
        .await;
    }
    let request = items
        .until("the approval", |items| {
            items.rows().iter().find_map(|item| {
                (item["body"]["type"] == "approval" && item["body"]["state"] == "open")
                    .then(|| item["body"]["request"].clone())
            })
        })
        .await;
    assert_eq!(request, "edit-approval-1");
    chats
        .until("a waiting chat", |chats| {
            chats
                .items
                .get(&chat)
                .is_some_and(|row| row["state"] == "waiting")
                .then_some(())
        })
        .await;
    core.call(
        "hermes.answer",
        json!({"chat": chat, "request": request, "option": "allow_once"}),
    )
    .await;
    chats
        .until("idle after the answer", |chats| {
            chats
                .items
                .get(&chat)
                .is_some_and(|row| row["state"] == "idle")
                .then_some(())
        })
        .await;
    let log = sent(&core);
    assert_eq!(log.matches("please edit").count(), 1, "{log}");
    assert!(
        log.contains(r#""optionId":"allow_once""#),
        "the answer reached hermes: {log}"
    );
    let reply = core
        .handle
        .rpc(
            "hermes.answer",
            json!({"chat": chat, "request": request, "option": "deny"}),
        )
        .await;
    assert!(reply.is_err(), "a second answer is refused");

    // A new core resumes the session, and the history comes back.
    drop(items);
    drop(chats);
    core.crash().await;
    core.restart();
    let mut items = Follower::new(&core, &format!("hermes_chat:{chat}")).await;
    items
        .until("the history", |items| {
            (summary(items) == ["prompt:<p>hello</p>", "assistant:<p>pong</p>"]).then_some(())
        })
        .await;
    assert!(sent(&core).contains(r#""method":"session/resume""#));

    core.call("hermes.close", json!({"chat": chat})).await;
    let mut chats = Follower::new(&core, "hermes_chats").await;
    chats
        .until("no chat", |chats| chats.items.is_empty().then_some(()))
        .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_chat_needs_an_absolute_folder() {
    let (core, _) = setup();
    let reply = core
        .handle
        .rpc("hermes.start", json!({"cwd": "relative/path"}))
        .await;
    assert!(reply.is_err());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_chat_comes_back_when_hermes_acp_ends() {
    let (core, folder) = setup();
    let started = core
        .call("hermes.start", json!({"cwd": folder, "prompt": "hello"}))
        .await;
    let chat = started["chat"].as_str().expect("a chat id").to_owned();
    let mut chats = Follower::new(&core, "hermes_chats").await;
    chats
        .until("idle", |chats| {
            chats
                .items
                .get(&chat)
                .is_some_and(|row| row["state"] == "idle")
                .then_some(())
        })
        .await;
    core.call(
        "hermes.prompt",
        json!({"chat": chat, "text": "crash now", "input_id": 2}),
    )
    .await;
    // The process ends; the core starts it again after a pause and resumes the session.
    chats
        .until("idle again after a resume", |chats| {
            sent(&core)
                .contains(r#""method":"session/resume""#)
                .then_some(())
                .and(chats.items.get(&chat))
                .filter(|row| row["state"] == "idle")
                .map(|_| ())
        })
        .await;
    assert_eq!(sent(&core).matches(r#""method":"initialize""#).count(), 2);
    let mut items = Follower::new(&core, &format!("hermes_chat:{chat}")).await;
    items
        .until("the history and a notice", |items| {
            let all = summary(items);
            (all.first().map(String::as_str) == Some("prompt:<p>hello</p>")).then_some(())
        })
        .await;
}
