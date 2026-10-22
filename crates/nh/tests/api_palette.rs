//! The palette: the command list, the ranking of a query, and the commands that run on the
//! layout for a window.

mod core_support;

use core_support::{Harness, client::Client};
use nh_core::ErrorCode;
use serde_json::{Value, json};

async fn client(h: &Harness) -> Client {
    Client::connect(&h.api_socket()).await
}

/// The commands of a query, as `(id, enabled)`.
async fn query(h: &Harness, text: &str) -> Vec<(String, bool)> {
    let result = h.call("palette.query", json!({ "query": text })).await;
    result["items"]
        .as_array()
        .expect("items")
        .iter()
        .map(|item| {
            (
                item["command"]["id"].as_str().expect("id").to_owned(),
                item["command"]["enabled"].as_bool().expect("enabled"),
            )
        })
        .collect()
}

async fn execute(h: &Harness, command: &str, window: &Value) -> Value {
    h.call(
        "palette.execute",
        json!({ "command": command, "window": window }),
    )
    .await
}

/// The tabs of a workspace.
async fn tabs(c: &mut Client, workspace: &Value) -> Vec<Value> {
    c.ok("workspace.show", json!({ "workspace": workspace }))
        .await["workspace"]["tabs"]
        .as_array()
        .expect("tabs")
        .clone()
}

/// The row of a window in the `windows` projection.
async fn window_row(c: &mut Client, window: &Value) -> Value {
    let reply = c
        .ok("projection.subscribe", json!({ "name": "windows" }))
        .await;
    let id = reply["subscription"].as_str().expect("an id").to_owned();
    loop {
        let line = c.recv().await;
        if line["method"] == "projection.message"
            && line["params"]["subscription"] == id.as_str()
            && line["params"]["message"]["type"] == "snapshot"
        {
            let row =
                line["params"]["message"]["data"]["items"][window.as_str().expect("id")].clone();
            c.ok("projection.unsubscribe", json!({ "subscription": id }))
                .await;
            return row;
        }
    }
}

async fn error_code(h: &Harness, params: Value) -> ErrorCode {
    h.handle
        .rpc("palette.execute", params)
        .await
        .expect_err("the command fails")
        .code
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_empty_core_has_the_fixed_commands_and_the_ones_that_need_a_tab_are_disabled() {
    let h = Harness::start();
    let all = query(&h, "").await;
    let ids: Vec<&str> = all.iter().map(|(id, _)| id.as_str()).collect();
    assert_eq!(ids[0], "tab.new");
    for id in [
        "workspace.new",
        "window.new",
        "search.open",
        "settings.open",
        "phone.pair",
    ] {
        assert!(ids.contains(&id), "{id} is missing: {ids:?}");
    }
    let enabled = |id: &str| all.iter().find(|(i, _)| i == id).expect("a command").1;
    assert!(!enabled("tab.new"));
    assert!(!enabled("pane.split_right"));
    // A rename is done by the page, but it needs a tab.
    assert!(!enabled("tab.rename"));
    assert!(!enabled("workspace.rename"));
    assert!(enabled("workspace.new"));
    assert!(enabled("window.new"));

    let result = h.call("palette.query", json!({ "query": "new tab" })).await;
    let first = &result["items"][0]["command"];
    assert_eq!(first["id"], "tab.new");
    assert_eq!(first["disabled_reason"], "No tab is open.");
    assert_eq!(result["items"][0]["ranges"], json!([[0, 3], [4, 7]]));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_layout_adds_the_commands_that_go_to_its_objects_and_enables_the_others() {
    let h = Harness::start();
    let mut c = client(&h).await;
    let made = c
        .ok(
            "workspace.create",
            json!({ "name": "Alpha", "command": { "argv": ["/bin/sh", "-c", "exec sleep 60"] } }),
        )
        .await;
    let all = query(&h, "").await;
    for id in ["tab.new", "tab.rename", "workspace.rename"] {
        assert!(
            all.iter().find(|(i, _)| i == id).expect("a command").1,
            "{id} is disabled"
        );
    }
    let ids: Vec<&str> = all.iter().map(|(id, _)| id.as_str()).collect();
    for expected in [
        format!("go.workspace:{}", made["workspace"].as_str().expect("id")),
        format!("go.tab:{}", made["tab"].as_str().expect("id")),
        format!("go.pane:{}", made["pane"].as_str().expect("id")),
    ] {
        assert!(ids.contains(&expected.as_str()), "{expected} is missing");
    }
    let found = h
        .call("palette.query", json!({ "query": "workspace alpha" }))
        .await;
    assert_eq!(
        found["items"][0]["command"]["title"],
        "Go to workspace Alpha"
    );

    // The projection has the same list.
    let mut watcher = client(&h).await;
    let reply = watcher
        .ok("projection.subscribe", json!({ "name": "palette" }))
        .await;
    let id = reply["subscription"].as_str().expect("an id").to_owned();
    let snapshot = loop {
        let line = watcher.recv().await;
        if line["method"] == "projection.message"
            && line["params"]["subscription"] == id.as_str()
            && line["params"]["message"]["type"] == "snapshot"
        {
            break line["params"]["message"]["data"]["items"]["_"].clone();
        }
    };
    assert_eq!(
        snapshot["commands"].as_array().expect("commands").len(),
        all.len()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_limit_cuts_the_results_and_the_best_match_comes_first() {
    let h = Harness::start();
    let result = h
        .call("palette.query", json!({ "query": "", "limit": 3 }))
        .await;
    assert_eq!(result["items"].as_array().expect("items").len(), 3);
    let result = h.call("palette.query", json!({ "query": "split" })).await;
    let titles: Vec<&str> = result["items"]
        .as_array()
        .expect("items")
        .iter()
        .map(|item| item["command"]["title"].as_str().expect("title"))
        .collect();
    assert_eq!(titles, ["Split pane right", "Split pane down"]);
    let none = h.call("palette.query", json!({ "query": "zzzz" })).await;
    assert!(none["items"].as_array().expect("items").is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn commands_run_on_the_tab_and_the_pane_of_a_window() {
    let h = Harness::start();
    let mut c = client(&h).await;
    let made = c
        .ok(
            "workspace.create",
            json!({ "name": "Alpha", "command": { "argv": ["/bin/sh", "-c", "exec sleep 60"] } }),
        )
        .await;
    let window = h.call("window.open", json!({})).await["window"].clone();

    // A new tab, and then back to the first one.
    assert_eq!(execute(&h, "tab.new", &window).await, json!({}));
    assert_eq!(tabs(&mut c, &made["workspace"]).await.len(), 2);
    let all = tabs(&mut c, &made["workspace"]).await;
    execute(&h, "tab.next", &window).await;
    assert_eq!(window_row(&mut c, &window).await["tab"], all[1]["id"]);
    execute(&h, "tab.previous", &window).await;
    assert_eq!(window_row(&mut c, &window).await["tab"], made["tab"]);
    // The first tab is the first of the ring: the previous tab is the last.
    execute(&h, "tab.previous", &window).await;
    assert_eq!(window_row(&mut c, &window).await["tab"], all[1]["id"]);
    execute(&h, "tab.next", &window).await;
    assert_eq!(window_row(&mut c, &window).await["tab"], made["tab"]);

    // Split, zoom and float the pane in focus.
    h.call(
        "pane.focus",
        json!({ "window": window, "pane": made["pane"] }),
    )
    .await;
    execute(&h, "pane.split_right", &window).await;
    assert_eq!(
        tabs(&mut c, &made["workspace"]).await[0]["layout"]["type"],
        "split"
    );
    execute(&h, "pane.zoom", &window).await;
    assert_eq!(
        tabs(&mut c, &made["workspace"]).await[0]["zoomed"],
        made["pane"]
    );
    execute(&h, "pane.zoom", &window).await;
    assert!(tabs(&mut c, &made["workspace"]).await[0]["zoomed"].is_null());
    execute(&h, "pane.float", &window).await;
    assert_eq!(
        tabs(&mut c, &made["workspace"]).await[0]["floating"][0]["pane"],
        made["pane"]
    );
    execute(&h, "pane.float", &window).await;
    let shown = tabs(&mut c, &made["workspace"]).await;
    assert!(
        shown[0]
            .get("floating")
            .is_none_or(|f| f.as_array().is_some_and(Vec::is_empty))
    );

    // A new workspace and a new window, without a window to run them.
    let count = c.ok("workspace.list", json!({})).await["workspaces"]
        .as_array()
        .expect("workspaces")
        .len();
    h.call("palette.execute", json!({ "command": "workspace.new" }))
        .await;
    let after = c.ok("workspace.list", json!({})).await["workspaces"]
        .as_array()
        .expect("workspaces")
        .len();
    assert_eq!(after, count + 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_commands_that_go_to_an_object_select_the_tab_and_focus_the_pane() {
    let h = Harness::start();
    let mut c = client(&h).await;
    let first = c
        .ok(
            "workspace.create",
            json!({ "name": "Alpha", "command": { "argv": ["/bin/sh", "-c", "exec sleep 60"] } }),
        )
        .await;
    let second = c
        .ok(
            "tab.create",
            json!({ "workspace": first["workspace"], "name": "Two",
                "command": { "argv": ["/bin/sh", "-c", "exec sleep 60"] } }),
        )
        .await;
    let window = h.call("window.open", json!({})).await["window"].clone();
    // A new tab is the selected tab of its workspace, and a new window shows it.
    assert_eq!(window_row(&mut c, &window).await["tab"], second["tab"]);
    execute(
        &h,
        &format!("go.tab:{}", first["tab"].as_str().expect("id")),
        &window,
    )
    .await;
    assert_eq!(window_row(&mut c, &window).await["tab"], first["tab"]);

    // Go to the pane of the second tab: the window shows that tab, and the pane has the focus.
    execute(
        &h,
        &format!("go.pane:{}", second["pane"].as_str().expect("id")),
        &window,
    )
    .await;
    let row = window_row(&mut c, &window).await;
    assert_eq!(row["tab"], second["tab"]);
    assert_eq!(row["focused_pane"], second["pane"]);

    // Go to a workspace: the window shows the tab that it had selected.
    execute(
        &h,
        &format!("go.workspace:{}", first["workspace"].as_str().expect("id")),
        &window,
    )
    .await;
    assert_eq!(
        window_row(&mut c, &window).await["workspace"],
        first["workspace"]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_commands_of_the_page_name_their_action_and_a_wrong_call_has_a_code() {
    let h = Harness::start();
    let window = h.call("window.open", json!({})).await["window"].clone();
    for id in [
        "search.open",
        "inbox.open",
        "tab.rename",
        "workspace.rename",
    ] {
        let result = h
            .call(
                "palette.execute",
                json!({ "command": id, "window": window }),
            )
            .await;
        assert_eq!(result, json!({ "ui": id }));
    }
    assert_eq!(
        error_code(&h, json!({ "command": "no.such.command" })).await,
        ErrorCode::NotFound
    );
    // A command on the tab of a window needs the window, and the window needs a tab.
    assert_eq!(
        error_code(&h, json!({ "command": "tab.next" })).await,
        ErrorCode::InvalidParams
    );
    assert_eq!(
        error_code(&h, json!({ "command": "tab.next", "window": window })).await,
        ErrorCode::InvalidParams
    );
    assert_eq!(
        error_code(
            &h,
            json!({ "command": "tab.next", "window": "01JD3Q9X5K7M2N4P6R8T0V1W2Y" })
        )
        .await,
        ErrorCode::NotFound
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn closing_a_pane_with_a_program_asks_first() {
    let h = Harness::start();
    let mut c = client(&h).await;
    c.ok(
        "workspace.create",
        json!({ "name": "Alpha", "command": { "argv": ["/bin/sh", "-c", "exec sleep 60"] } }),
    )
    .await;
    let window = h.call("window.open", json!({})).await["window"].clone();
    // Wait for the program: a pane that has not started has nothing to confirm.
    let deadline = std::time::Instant::now() + core_support::TIMEOUT;
    loop {
        let shown = c.ok("pane.show", json!({ "pane": "w1:p1" })).await;
        if shown["pane"]["liveness"]["state"] == "alive" {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the pane did not start"
        );
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    // The pane runs a program, so the core wants a confirmation that the palette cannot give.
    let code = error_code(&h, json!({ "command": "pane.close", "window": window })).await;
    assert_eq!(code, ErrorCode::ConfirmRequired);
    let panes = c.ok("pane.list", json!({})).await;
    assert_eq!(panes["panes"].as_array().expect("panes").len(), 1);
    // The page asked, and it calls again with `force`.
    h.call(
        "palette.execute",
        json!({ "command": "pane.close", "window": window, "args": { "force": true } }),
    )
    .await;
    let panes = c.ok("pane.list", json!({})).await;
    assert!(panes["panes"].as_array().expect("panes").is_empty());
}

fn host() -> std::sync::Arc<nh_core::RecordingHost> {
    std::sync::Arc::new(nh_core::RecordingHost::new(
        env!("CARGO_BIN_EXE_nh").into(),
        false,
    ))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_settings_command_asks_the_host_to_open_the_settings_window() {
    let recording = host();
    let h = Harness::start_hosted(recording.clone());
    let result = h
        .call("palette.execute", json!({ "command": "settings.open" }))
        .await;
    assert_eq!(result, json!({}));
    assert!(
        recording
            .calls()
            .contains(&nh_core::host::HostCall::OpenSettings),
        "{:?}",
        recording.calls()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pair_a_phone_opens_the_settings_window_at_the_section_phone() {
    let recording = host();
    let h = Harness::start_hosted(recording.clone());
    let result = h
        .call("palette.execute", json!({ "command": "phone.pair" }))
        .await;
    assert_eq!(result, json!({}));
    assert!(
        recording
            .calls()
            .contains(&nh_core::host::HostCall::OpenSettingsAt("phone".to_owned())),
        "{:?}",
        recording.calls()
    );
}
