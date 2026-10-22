//! The harness manifests of the core: the files in `<config>/harnesses/`, the notes of
//! the doctor, the events, and the built-in manifests that are written for reference.

mod core_support;

use core_support::{Harness, client::Client};
use serde_json::{Value, json};

async fn client(h: &Harness) -> Client {
    Client::connect(&h.api_socket()).await
}

/// The warnings of the `health` projection.
async fn warnings(c: &mut Client) -> Vec<Value> {
    let reply = c
        .ok("projection.subscribe", json!({ "name": "health" }))
        .await;
    let id = reply["subscription"].as_str().expect("an id").to_owned();
    let health = loop {
        let line = c.recv().await;
        if line["method"] == "projection.message"
            && line["params"]["subscription"] == id.as_str()
            && line["params"]["message"]["type"] == "snapshot"
        {
            break line["params"]["message"]["data"]["items"]["_"].clone();
        }
    };
    c.ok("projection.unsubscribe", json!({ "subscription": id }))
        .await;
    health["warnings"].as_array().cloned().unwrap_or_default()
}

fn manifests_dir(h: &Harness) -> std::path::PathBuf {
    let dir = h.dir.join("config").join("harnesses");
    std::fs::create_dir_all(&dir).expect("the directory");
    dir
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_built_in_manifests_are_in_force_without_a_warning_and_are_written_for_reference() {
    let h = Harness::start();
    let mut c = client(&h).await;
    assert!(warnings(&mut c).await.is_empty());
    // The reference copy is in the data directory of the session.
    let found = walk(&h.dir).into_iter().any(|path| {
        path.ends_with("harnesses/builtin/claude.toml")
            && std::fs::read_to_string(&path).is_ok_and(|text| text.contains("id = \"claude\""))
    });
    assert!(found, "no reference copy under {}", h.dir.display());
}

fn walk(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(walk(&path));
        } else {
            out.push(path);
        }
    }
    out
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_user_manifest_makes_an_event_and_a_wrong_one_makes_a_note_that_goes_when_it_is_fixed() {
    let h = Harness::start();
    let mut watcher = client(&h).await;
    let subscription = watcher
        .ok(
            "events.subscribe",
            json!({ "filter": { "kinds": ["harness.manifest_loaded"] } }),
        )
        .await["subscription"]
        .as_str()
        .expect("an id")
        .to_owned();
    let mut c = client(&h).await;
    let dir = manifests_dir(&h);

    // A wrapper that extends claude, and a file with a regular expression that does not compile.
    std::fs::write(
        dir.join("cc.toml"),
        "schema = 1\nid = \"cc\"\nextends = \"claude\"\n[detect]\npriority = 200\n",
    )
    .expect("a file");
    std::fs::write(
        dir.join("bad.toml"),
        "schema = 1\nid = \"claude\"\n[[detect.process]]\nargv_regex = '('\n",
    )
    .expect("a file");
    c.ok("config.reload", json!({})).await;

    // The wrapper has its event, with the path of the file as the source.
    let line = loop {
        let line = watcher.recv().await;
        if line["method"] == "events.event"
            && line["params"]["subscription"] == subscription.as_str()
        {
            break line;
        }
    };
    let payload = &line["params"]["event"]["payload"];
    assert_eq!(line["params"]["event"]["kind"], "harness.manifest_loaded");
    assert_eq!(payload["id"], "cc");
    assert!(
        payload["source"]
            .as_str()
            .expect("a source")
            .ends_with("cc.toml")
    );
    assert_eq!(payload["version"].as_str().map(str::len), Some(16));
    assert!(payload["serial"].as_u64().expect("a serial") >= 1);

    // The wrong file is a note of the doctor, and claude is as it was.
    let notes = warnings(&mut c).await;
    assert_eq!(notes.len(), 1, "{notes:?}");
    let text = notes[0]["text"].as_str().expect("a text");
    assert!(
        text.contains("bad.toml") && text.contains("regular expression"),
        "{text}"
    );
    assert!(
        notes[0]["hint"]
            .as_str()
            .expect("a hint")
            .contains("stays in force")
    );

    // Fixing the file removes the note and makes an event for the new content of claude.
    std::fs::write(
        dir.join("bad.toml"),
        "schema = 1\nid = \"claude\"\n[ui]\ninterrupt_keys = [\"ctrl+c\"]\n",
    )
    .expect("a file");
    c.ok("config.reload", json!({})).await;
    let line = loop {
        let line = watcher.recv().await;
        if line["method"] == "events.event"
            && line["params"]["subscription"] == subscription.as_str()
        {
            break line;
        }
    };
    // The wrapper inherits from claude, so it changes too. Its event comes first by id.
    let mut ids = vec![line["params"]["event"]["payload"]["id"].clone()];
    while !ids.contains(&json!("claude")) {
        let line = watcher.recv().await;
        if line["method"] == "events.event"
            && line["params"]["subscription"] == subscription.as_str()
        {
            ids.push(line["params"]["event"]["payload"]["id"].clone());
        }
    }
    assert!(ids.iter().all(|id| id == "cc" || id == "claude"), "{ids:?}");
    assert!(warnings(&mut c).await.is_empty());

    // A reload that changes nothing makes no event: the next one is for the next change.
    c.ok("config.reload", json!({})).await;
    std::fs::remove_file(dir.join("cc.toml")).expect("removed");
    std::fs::write(
        dir.join("zz.toml"),
        "schema = 1\nid = \"zz\"\nextends = \"codex\"\n",
    )
    .expect("a file");
    c.ok("config.reload", json!({})).await;
    let line = loop {
        let line = watcher.recv().await;
        if line["method"] == "events.event"
            && line["params"]["subscription"] == subscription.as_str()
        {
            break line;
        }
    };
    assert_eq!(line["params"]["event"]["payload"]["id"], "zz");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_change_of_a_file_is_found_by_the_watch_without_a_reload() {
    let h = Harness::start();
    let mut watcher = client(&h).await;
    let subscription = watcher
        .ok(
            "events.subscribe",
            json!({ "filter": { "kinds": ["harness.manifest_loaded"] } }),
        )
        .await["subscription"]
        .as_str()
        .expect("an id")
        .to_owned();
    let dir = manifests_dir(&h);
    // The system needs a moment to set up a watch, and a file that comes in that moment is found
    // by the second read of the watch. So the test only waits for the event.
    std::fs::write(
        dir.join("watched.toml"),
        "schema = 1\nid = \"watched\"\nextends = \"claude\"\n",
    )
    .expect("a file");
    // Setting up a watch can take many seconds on a busy machine, so the wait is long.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(90);
    loop {
        let left = deadline.saturating_duration_since(std::time::Instant::now());
        let line = watcher
            .try_recv(left.min(std::time::Duration::from_secs(5)))
            .await;
        assert!(
            std::time::Instant::now() < deadline,
            "the watch did not find the file"
        );
        let Some(line) = line else { continue };
        if line["method"] == "events.event"
            && line["params"]["subscription"] == subscription.as_str()
            && line["params"]["event"]["payload"]["id"] == "watched"
        {
            break;
        }
    }
}
