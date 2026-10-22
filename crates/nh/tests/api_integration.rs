//! The installer of the hooks of Claude Code through the API: `integration.install`,
//! `status`, `list`, `uninstall`, and what the files look like afterwards.

mod core_support;

use core_support::{Harness, client::Client};
use serde_json::{Value, json};

async fn client(h: &Harness) -> Client {
    Client::connect(&h.api_socket()).await
}

fn settings(h: &Harness) -> std::path::PathBuf {
    h.dir.join("home").join(".claude").join("settings.json")
}

const USER: &str = "{\n  \"model\": \"opus\",\n  \"env\": { \"A\":   \"b\" }\n}\n";

fn write_user_file(h: &Harness) {
    let path = settings(h);
    std::fs::create_dir_all(path.parent().expect("a parent")).expect("a directory");
    std::fs::write(&path, USER).expect("a file");
}

fn hook_count(text: &str) -> usize {
    text.matches(" hook claude ").count()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn install_shows_a_diff_first_and_then_changes_the_file_once() {
    let h = Harness::start_layout();
    write_user_file(&h);
    let mut c = client(&h).await;

    // A dry run changes nothing and returns the diff.
    let dry = c
        .ok(
            "integration.install",
            json!({"target": "claude", "dry_run": true}),
        )
        .await;
    assert_eq!(dry["changes"][0]["action"], "modified");
    assert!(dry["changes"][0]["backup"].is_null());
    let diff = dry["notes"][0].as_str().expect("a diff");
    assert!(
        diff.starts_with("--- ") && diff.contains("+  \"hooks\": {"),
        "{diff}"
    );
    assert_eq!(
        std::fs::read_to_string(settings(&h)).expect("the file"),
        USER
    );

    // The install writes the hooks, makes a backup, and points the hooks at the sidecar.
    let done = c
        .ok("integration.install", json!({"target": "claude"}))
        .await;
    assert_eq!(done["changes"][0]["action"], "modified");
    let backup = done["changes"][0]["backup"].as_str().expect("a backup");
    assert_eq!(std::fs::read_to_string(backup).expect("the backup"), USER);
    let text = std::fs::read_to_string(settings(&h)).expect("the file");
    assert_eq!(hook_count(&text), 17);
    assert!(
        text.contains("\"env\": { \"A\":   \"b\" }"),
        "the user's spacing stays"
    );
    let value: Value = serde_json::from_str(&text).expect("JSON");
    let command = value["hooks"]["PermissionRequest"][0]["hooks"][0]["command"]
        .as_str()
        .expect("a command");
    assert!(
        command.ends_with(" hook claude PermissionRequest"),
        "{command}"
    );
    assert!(command.starts_with(env!("CARGO_BIN_EXE_nh")), "{command}");
    assert!(
        done["notes"][0]
            .as_str()
            .is_some_and(|n| n.contains("new session"))
    );

    // A second install changes nothing, and makes no second backup.
    let again = c
        .ok("integration.install", json!({"target": "claude"}))
        .await;
    assert_eq!(again["changes"][0]["action"], "unchanged");
    assert_eq!(
        std::fs::read_to_string(settings(&h)).expect("the file"),
        text
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn status_and_list_follow_the_file_and_uninstall_restores_it() {
    let h = Harness::start_layout();
    write_user_file(&h);
    let mut c = client(&h).await;

    let status = c
        .ok("integration.status", json!({"target": "claude"}))
        .await;
    assert_eq!(status["status"][0]["harness"], "claude");
    assert_eq!(status["status"][0]["installed"], false);
    assert_eq!(
        status["status"][0]["files"][0],
        settings(&h).display().to_string()
    );
    let list = c.ok("integration.list", json!({})).await;
    assert_eq!(list["integrations"][0]["installed"], false);

    c.ok("integration.install", json!({"target": "claude"}))
        .await;
    let status = c.ok("integration.status", json!({})).await;
    assert_eq!(status["status"][0]["installed"], true);
    assert_eq!(status["status"][1]["harness"], "codex");
    assert_eq!(status["status"][1]["installed"], false);
    let list = c.ok("integration.list", json!({})).await;
    assert_eq!(list["integrations"][0]["installed"], true);
    assert_eq!(list["integrations"][0]["harness"], "claude");

    let removed = c
        .ok("integration.uninstall", json!({"target": "claude"}))
        .await;
    assert_eq!(removed["changes"][0]["action"], "modified");
    assert_eq!(
        std::fs::read_to_string(settings(&h)).expect("the file"),
        USER
    );
    let status = c
        .ok("integration.status", json!({"target": "claude"}))
        .await;
    assert_eq!(status["status"][0]["installed"], false);
    // Nothing is left to remove.
    let nothing = c
        .ok("integration.uninstall", json!({"target": "claude"}))
        .await;
    assert_eq!(nothing["changes"][0]["action"], "unchanged");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_status_names_the_time_of_the_last_hook() {
    let h = Harness::start_layout();
    let mut c = client(&h).await;
    let made = c.ok("workspace.create", json!({"name": "A"})).await;
    h.handle.pane_tokens().register(
        made["pane"]
            .as_str()
            .expect("a pane")
            .parse()
            .expect("an id"),
        "t-int",
    );
    let before = c
        .ok("integration.status", json!({"target": "claude"}))
        .await;
    assert!(before["status"][0]["last_signal_at"].is_null());
    c.ok(
        "adapter.signal",
        json!({"token": "t-int", "harness": "claude", "event": "SessionStart",
               "payload": {"session_id": "s"}}),
    )
    .await;
    let after = c
        .ok("integration.status", json!({"target": "claude"}))
        .await;
    assert!(after["status"][0]["last_signal_at"].is_string(), "{after}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_project_scope_writes_the_local_settings_of_the_repository() {
    let h = Harness::start_layout();
    let mut c = client(&h).await;
    let repo = h.dir.join("repo");
    std::fs::create_dir_all(&repo).expect("a directory");
    // A project install needs to know the repository.
    let refused = c
        .call(
            "integration.install",
            json!({"target": "claude", "scope": "project"}),
        )
        .await;
    assert!(refused.get("error").is_some(), "{refused}");
    let done = c
        .ok(
            "integration.install",
            json!({"target": "claude", "scope": "project", "project_root": repo.display().to_string()}),
        )
        .await;
    assert_eq!(done["changes"][0]["action"], "created");
    let file = repo.join(".claude").join("settings.local.json");
    assert_eq!(
        hook_count(&std::fs::read_to_string(&file).expect("the file")),
        17
    );
    assert!(!settings(&h).exists(), "the user scope is untouched");
    c.ok(
        "integration.uninstall",
        json!({"target": "claude", "scope": "project", "project_root": repo.display().to_string()}),
    )
    .await;
    let text = std::fs::read_to_string(&file).expect("the file");
    assert_eq!(hook_count(&text), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_file_that_is_not_json_is_refused_and_stays() {
    let h = Harness::start_layout();
    let path = settings(&h);
    std::fs::create_dir_all(path.parent().expect("a parent")).expect("a directory");
    std::fs::write(&path, "// my settings\n{}\n").expect("a file");
    let mut c = client(&h).await;
    let refused = c
        .call("integration.install", json!({"target": "claude"}))
        .await;
    assert!(refused.get("error").is_some(), "{refused}");
    assert_eq!(
        std::fs::read_to_string(&path).expect("the file"),
        "// my settings\n{}\n"
    );
    // `all` does both harnesses when the file is right.
    std::fs::write(&path, USER).expect("a file");
    let all = c.ok("integration.install", json!({"target": "all"})).await;
    assert_eq!(all["changes"].as_array().map(Vec::len), Some(2));
    assert!(codex_file(&h).exists());
    let notes = all["notes"].as_array().expect("notes");
    assert!(notes.iter().any(|n| {
        n.as_str()
            .is_some_and(|n| n.contains("Trust all and continue"))
    }));
}

fn codex_file(h: &Harness) -> std::path::PathBuf {
    h.dir.join("home").join(".codex").join("hooks.json")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_hooks_of_codex_are_written_to_hooks_json_and_the_trust_is_only_read() {
    let h = Harness::start_layout();
    let mut c = client(&h).await;
    let done = c
        .ok("integration.install", json!({"target": "codex"}))
        .await;
    assert_eq!(done["changes"][0]["action"], "created");
    let text = std::fs::read_to_string(codex_file(&h)).expect("the file");
    assert!(!text.contains("_neohar"), "Codex gets no extra key");
    let value: Value = serde_json::from_str(&text).expect("JSON");
    assert_eq!(
        value["hooks"].as_object().map(serde_json::Map::len),
        Some(12)
    );
    let command = value["hooks"]["PermissionRequest"][0]["hooks"][0]["command"]
        .as_str()
        .expect("a command");
    assert!(
        command.ends_with(" hook codex PermissionRequest"),
        "{command}"
    );
    assert_eq!(value["hooks"]["Interrupt"][0]["hooks"][0]["timeout"], 3);
    assert!(
        done["notes"][0]
            .as_str()
            .is_some_and(|n| n.contains("/hooks"))
    );

    // Nothing is trusted until Codex says so in its own config.
    let status = c.ok("integration.status", json!({"target": "codex"})).await;
    assert_eq!(status["status"][0]["harness"], "codex");
    assert_eq!(status["status"][0]["installed"], true);
    assert_eq!(status["status"][0]["hooks_trusted"], false);
    assert_eq!(
        status["status"][0]["files"][0],
        codex_file(&h).display().to_string()
    );
    let config = h.dir.join("home").join(".codex").join("config.toml");
    let path = codex_file(&h).display().to_string();
    let events = value["hooks"].as_object().expect("events");
    let mut toml = String::new();
    for event in events.keys() {
        // Codex writes the event in snake case: `PreToolUse` is `pre_tool_use`.
        let mut key = String::new();
        for (i, c) in event.chars().enumerate() {
            if c.is_ascii_uppercase() && i > 0 {
                key.push('_');
            }
            key.push(c.to_ascii_lowercase());
        }
        toml.push_str(&format!(
            "[hooks.state.\"{path}:{key}:0:0\"]\ntrusted_hash = \"sha256:x\"\n\n"
        ));
    }
    std::fs::write(&config, &toml).expect("a config");
    let status = c.ok("integration.status", json!({"target": "codex"})).await;
    assert_eq!(status["status"][0]["hooks_trusted"], true);
    // The core did not touch the config of Codex.
    assert_eq!(std::fs::read_to_string(&config).expect("the config"), toml);

    // Uninstall takes the hooks out and leaves the config alone.
    let removed = c
        .ok("integration.uninstall", json!({"target": "codex"}))
        .await;
    assert_eq!(removed["changes"][0]["action"], "modified");
    let after = std::fs::read_to_string(codex_file(&h)).expect("the file");
    assert_eq!(
        serde_json::from_str::<Value>(&after).expect("JSON"),
        json!({})
    );
    assert_eq!(std::fs::read_to_string(&config).expect("the config"), toml);
    let list = c.ok("integration.list", json!({})).await;
    assert_eq!(list["integrations"][1]["harness"], "codex");
    assert_eq!(list["integrations"][1]["installed"], false);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_project_scope_of_codex_is_the_hooks_file_of_the_repository() {
    let h = Harness::start_layout();
    let mut c = client(&h).await;
    let repo = h.dir.join("repo");
    std::fs::create_dir_all(&repo).expect("a directory");
    c.ok(
        "integration.install",
        json!({"target": "codex", "scope": "project", "project_root": repo.display().to_string()}),
    )
    .await;
    let file = repo.join(".codex").join("hooks.json");
    assert_eq!(
        hook_count_codex(&std::fs::read_to_string(&file).expect("the file")),
        12
    );
    assert!(!codex_file(&h).exists(), "the user scope is untouched");
}

fn hook_count_codex(text: &str) -> usize {
    text.matches(" hook codex ").count()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_matrix_is_empty_without_an_installed_harness_and_the_doctor_waits() {
    let h = Harness::start_layout();
    let mut c = client(&h).await;
    let matrix = c.ok("integration.capabilities", json!({})).await;
    assert!(matrix["rows"].is_array());
    let doctor = c.call("integration.doctor", json!({})).await;
    assert!(doctor.get("error").is_some());
}
