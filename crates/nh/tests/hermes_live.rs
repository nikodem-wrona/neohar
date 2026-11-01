//! A chat with the real Hermes Agent of this machine, through the core.
//!
//! It runs only when `NEOHAR_HERMES=1` (`mise run test:hermes`): it needs Hermes installed with
//! a model provider, the network, and it uses the quota of that provider (four short turns).
//! `NEOHAR_HERMES_BIN` names the program; without it the test uses `~/.local/bin/hermes`. Hermes
//! runs with the real `HOME`, so it finds its config and its login; it works only in a temporary
//! folder of the test. The sessions stay in the database of Hermes.

mod core_support;

use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

use core_support::{Harness, client::Client, write_program};
use serde_json::{Value, json};

fn live() -> bool {
    std::env::var("NEOHAR_HERMES").as_deref() == Ok("1")
}

/// A client that follows a projection.
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

    fn rows(&self) -> Vec<Value> {
        self.order
            .iter()
            .filter_map(|key| self.items.get(key).cloned())
            .collect()
    }

    async fn until<T>(&mut self, what: &str, f: impl Fn(&Self) -> Option<T>) -> T {
        let deadline = Instant::now() + Duration::from_secs(240);
        loop {
            if let Some(found) = f(self) {
                return found;
            }
            let left = deadline.saturating_duration_since(Instant::now());
            assert!(!left.is_zero(), "timeout: {what}: {:#?}", self.rows());
            if let Some(line) = self.client.try_recv(left.min(Duration::from_secs(1))).await {
                self.apply(&line);
            }
        }
    }
}

fn hermes_bin() -> PathBuf {
    std::env::var_os("NEOHAR_HERMES_BIN").map_or_else(
        || {
            PathBuf::from(std::env::var("HOME").expect("a HOME"))
                .join(".local")
                .join("bin")
                .join("hermes")
        },
        PathBuf::from,
    )
}

fn assistant_text(items: &Follower) -> String {
    items
        .rows()
        .iter()
        .filter(|item| item["body"]["type"] == "assistant")
        .filter_map(|item| item["body"]["markdown"]["html"].as_str().map(str::to_owned))
        .collect::<Vec<_>>()
        .join("\n")
}

#[tokio::test(flavor = "multi_thread")]
async fn a_chat_with_the_real_hermes() {
    if !live() {
        eprintln!("skipped: set NEOHAR_HERMES=1 (mise run test:hermes)");
        return;
    }
    let mut core = Harness::start();
    let home = std::env::var("HOME").expect("a HOME");
    let wrapper = core.dir.join("bin").join("hermes");
    write_program(
        &wrapper,
        &format!(
            "#!/bin/bash\nexport HOME='{home}'\nexec '{}' \"$@\"\n",
            hermes_bin().display()
        ),
    );
    let folder = core.dir.join("home").join("project");
    std::fs::create_dir_all(&folder).expect("the folder is made");
    let folder = folder.to_string_lossy().into_owned();

    let status = core.call("hermes.check", json!({})).await;
    assert_eq!(status["ok"], json!(true), "{status}");

    let mut chats = Follower::new(&core, "hermes_chats").await;
    let chat = core
        .call(
            "hermes.start",
            json!({"cwd": folder, "prompt": "Reply with exactly the word: pong"}),
        )
        .await["chat"]
        .as_str()
        .expect("a chat")
        .to_owned();
    let mut items = Follower::new(&core, &format!("hermes_chat:{chat}")).await;
    items
        .until("pong", |items| {
            assistant_text(items)
                .to_lowercase()
                .contains("pong")
                .then_some(())
        })
        .await;
    chats
        .until("idle", |chats| {
            chats
                .items
                .get(&chat)
                .is_some_and(|row| row["state"] == "idle")
                .then_some(())
        })
        .await;

    // An edit asks in the mode `default`, and the answer reaches Hermes.
    core.call(
        "hermes.prompt",
        json!({
            "chat": chat,
            "text": "Create a file hello.txt that contains the line hi. Run no shell command.",
            "input_id": 1,
        }),
    )
    .await;
    let request = items
        .until("the approval of the edit", |items| {
            items.rows().iter().find_map(|item| {
                (item["body"]["type"] == "approval" && item["body"]["state"] == "open")
                    .then(|| item["body"]["request"].as_str().map(str::to_owned))
                    .flatten()
            })
        })
        .await;
    core.call(
        "hermes.answer",
        json!({"chat": chat, "request": request, "option": "allow_once"}),
    )
    .await;
    items
        .until("the answer after the edit", |items| {
            let rows = items.rows();
            let approval = rows
                .iter()
                .position(|item| item["body"]["type"] == "approval")?;
            let last = rows.last()?;
            (rows.len() > approval + 1
                && last["body"]["type"] == "assistant"
                && last["body"]["streaming"] != true)
                .then_some(())
        })
        .await;
    chats
        .until("idle after the edit", |chats| {
            chats
                .items
                .get(&chat)
                .is_some_and(|row| row["state"] == "idle")
                .then_some(())
        })
        .await;
    let written =
        std::fs::read_to_string(PathBuf::from(&folder).join("hello.txt")).unwrap_or_default();
    assert_eq!(written.trim(), "hi", "{:#?}", items.rows());

    // A new core resumes the session: the prompts come back.
    drop(items);
    drop(chats);
    core.crash().await;
    core.restart();
    let mut items = Follower::new(&core, &format!("hermes_chat:{chat}")).await;
    items
        .until("the history", |items| {
            let prompts = items
                .rows()
                .iter()
                .filter(|item| item["body"]["type"] == "prompt")
                .count();
            (prompts >= 2).then_some(())
        })
        .await;
    core.call("hermes.close", json!({"chat": chat})).await;
}
