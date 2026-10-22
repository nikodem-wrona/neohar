//! The commands on workspaces, groups, tabs and layouts against a real core: results,
//! the `tree` projection, events, errors, and the layout after a restart of the core.

mod core_support;

use core_support::{
    Harness,
    client::{Client, error_of},
};
use serde_json::{Value, json};

async fn client(h: &Harness) -> Client {
    Client::connect(&h.api_socket()).await
}

/// The `tree` projection as a snapshot: `(order, items)`.
async fn tree(c: &mut Client) -> (Vec<String>, Value) {
    let reply = c.ok("projection.subscribe", json!({"name": "tree"})).await;
    let id = reply["subscription"].as_str().expect("an id").to_owned();
    loop {
        let line = c.recv().await;
        if line["method"] == "projection.message"
            && line["params"]["subscription"] == id.as_str()
            && line["params"]["message"]["type"] == "snapshot"
        {
            let data = &line["params"]["message"]["data"];
            let order = data["order"]
                .as_array()
                .expect("an order")
                .iter()
                .map(|k| k.as_str().expect("a key").to_owned())
                .collect();
            c.ok("projection.unsubscribe", json!({"subscription": id}))
                .await;
            return (order, data["items"].clone());
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_workspace_starts_with_a_tab_and_a_pane_and_the_tree_shows_it() {
    let h = Harness::start_layout();
    let mut c = client(&h).await;
    let created = c
        .ok("workspace.create", json!({"name": "Main", "root": "/work"}))
        .await;
    assert_eq!(created["handle"], "w1");
    assert_eq!(created["tab_handle"], "w1:t1");
    assert_eq!(created["pane_handle"], "w1:p1");

    let (order, items) = tree(&mut c).await;
    assert_eq!(order, [created["workspace"].as_str().expect("an id")]);
    let workspace = &items[&order[0]];
    assert_eq!(workspace["name"], "Main");
    assert_eq!(workspace["root"], "/work");
    assert_eq!(workspace["machine"], "local");
    assert_eq!(workspace["selected_tab"], created["tab"]);
    let tabs = workspace["tabs"].as_array().expect("tabs");
    assert_eq!(tabs.len(), 1);
    assert_eq!(tabs[0]["handle"], "w1:t1");
    assert_eq!(
        tabs[0]["layout"],
        json!({"type": "pane", "pane": created["pane"]})
    );

    // The same data through the CLI methods.
    let list = c.ok("workspace.list", json!({})).await;
    assert_eq!(list["workspaces"][0]["handle"], "w1");
    assert_eq!(
        (
            list["workspaces"][0]["tabs"].clone(),
            list["workspaces"][0]["panes"].clone()
        ),
        (json!(1), json!(1))
    );
    let shown = c.ok("workspace.show", json!({"workspace": "w1"})).await;
    assert_eq!(shown["workspace"], *workspace);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_change_is_a_patch_of_the_tree_and_an_event() {
    let h = Harness::start_layout();
    let mut watcher = client(&h).await;
    let mut c = client(&h).await;
    let reply = watcher
        .ok("projection.subscribe", json!({"name": "tree"}))
        .await;
    let subscription = reply["subscription"].as_str().expect("an id").to_owned();
    let events = watcher
        .ok(
            "events.subscribe",
            json!({"filter": {"kinds": ["workspace.*", "tab.*", "pane.*"]}}),
        )
        .await;
    let events_id = events["subscription"].as_str().expect("an id").to_owned();

    let created = c.ok("workspace.create", json!({"name": "One"})).await;
    // The events come at the commit, and the patch up to 16 ms later: read until both are here.
    let (mut patch, mut seen) = (None, Vec::new());
    while patch.is_none() || seen.len() < 3 {
        let line = watcher.recv().await;
        if line["method"] == "projection.message"
            && line["params"]["subscription"] == subscription.as_str()
            && line["params"]["message"]["type"] == "patch"
        {
            patch = Some(line);
        } else if line["method"] == "events.event"
            && line["params"]["subscription"] == events_id.as_str()
        {
            seen.push((
                line["params"]["event"]["kind"]
                    .as_str()
                    .expect("a kind")
                    .to_owned(),
                line["params"]["event"]["subject"].clone(),
            ));
        }
    }
    // The patch: the workspace and the new order.
    let patch = patch.expect("a patch");
    let ops = patch["params"]["message"]["ops"].as_array().expect("ops");
    assert_eq!(ops[0]["op"], "set");
    assert_eq!(ops[0]["key"], created["workspace"]);
    assert_eq!(ops[0]["value"]["name"], "One");
    assert_eq!(
        ops[1],
        json!({"op": "order", "keys": [created["workspace"]]})
    );
    // Three events, in order, with the ids as subjects.
    assert_eq!(
        seen,
        [
            ("workspace.created".to_owned(), created["workspace"].clone()),
            ("tab.created".to_owned(), created["tab"].clone()),
            ("pane.created".to_owned(), created["pane"].clone()),
        ]
    );

    // A rename changes only that workspace.
    c.ok(
        "workspace.rename",
        json!({"workspace": "w1", "name": "Renamed"}),
    )
    .await;
    let patch = loop {
        let line = watcher.recv().await;
        if line["method"] == "projection.message" && line["params"]["message"]["type"] == "patch" {
            break line;
        }
    };
    let ops = patch["params"]["message"]["ops"].as_array().expect("ops");
    assert_eq!(ops.len(), 1, "only the changed workspace: {ops:?}");
    assert_eq!(ops[0]["value"]["name"], "Renamed");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn groups_and_the_order_of_workspaces() {
    let h = Harness::start_layout();
    let mut c = client(&h).await;
    let group = c.ok("group.create", json!({"name": "Work"})).await["group"]
        .as_str()
        .expect("an id")
        .to_owned();
    for name in ["A", "B", "C"] {
        c.ok("workspace.create", json!({"name": name})).await;
    }
    c.ok("workspace.move", json!({"workspace": "w3", "before": "w1"}))
        .await;
    c.ok("workspace.move", json!({"workspace": "w2", "group": group}))
        .await;
    let list = c.ok("workspace.list", json!({})).await;
    let handles: Vec<_> = list["workspaces"]
        .as_array()
        .expect("a list")
        .iter()
        .map(|w| w["handle"].as_str().expect("a handle").to_owned())
        .collect();
    assert_eq!(handles, ["w3", "w1", "w2"]);
    let in_group = c.ok("workspace.list", json!({"group": group})).await;
    assert_eq!(in_group["workspaces"].as_array().map(Vec::len), Some(1));
    assert_eq!(in_group["workspaces"][0]["handle"], "w2");

    // The tree shows the group of the workspace, with its name.
    let (order, items) = tree(&mut c).await;
    assert_eq!(items[&order[2]]["group"]["name"], "Work");
    // A rename of the group changes the row of its workspace.
    c.ok("group.rename", json!({"group": group, "name": "Job"}))
        .await;
    let (order, items) = tree(&mut c).await;
    assert_eq!(items[&order[2]]["group"]["name"], "Job");

    let groups = c.ok("group.list", json!({})).await;
    assert_eq!(groups["groups"][0]["name"], "Job");
    assert_eq!(
        groups["groups"][0]["workspaces"].as_array().map(Vec::len),
        Some(1)
    );
    let shown = c.ok("group.show", json!({"group": group})).await;
    assert_eq!(shown["id"], group.as_str());

    // Closing the group keeps the workspace.
    c.ok("group.close", json!({"group": group})).await;
    let (order, items) = tree(&mut c).await;
    assert_eq!(order.len(), 3);
    assert!(items[&order[2]].get("group").is_none());
    assert_eq!(c.ok("group.list", json!({})).await["groups"], json!([]));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tabs_are_made_selected_moved_renamed_and_closed() {
    let h = Harness::start_layout();
    let mut c = client(&h).await;
    c.ok("workspace.create", json!({"name": "A"})).await;
    c.ok("workspace.create", json!({"name": "B"})).await;
    let made = c
        .ok("tab.create", json!({"workspace": "w1", "name": "Logs"}))
        .await;
    assert_eq!(
        (made["handle"].as_str(), made["pane_handle"].as_str()),
        (Some("w1:t2"), Some("w1:p2"))
    );

    c.ok("tab.rename", json!({"tab": "w1:t2", "name": "Output"}))
        .await;
    c.ok("tab.select", json!({"tab": "w1:t1"})).await;
    let shown = c.ok("workspace.show", json!({"workspace": "w1"})).await;
    assert_eq!(
        shown["workspace"]["selected_tab"],
        shown["workspace"]["tabs"][0]["id"]
    );
    assert_eq!(shown["workspace"]["tabs"][1]["name"], "Output");

    // A tab goes to another workspace, and its handle stays.
    c.ok(
        "tab.move",
        json!({"tab": "w1:t2", "workspace": "w2", "index": 0}),
    )
    .await;
    let moved = c.ok("workspace.show", json!({"workspace": "w2"})).await;
    assert_eq!(moved["workspace"]["tabs"][0]["handle"], "w1:t2");
    let panes = c.ok("workspace.list", json!({})).await;
    assert_eq!(panes["workspaces"][1]["panes"], 2);

    // Closing tabs: the pane goes with its tab. The last tab can close, too.
    c.ok("tab.close", json!({"tab": "w1:t2"})).await;
    c.ok("tab.close", json!({"tab": "w1:t1"})).await;
    let empty = c.ok("workspace.show", json!({"workspace": "w1"})).await;
    assert_eq!(empty["workspace"]["tabs"], json!([]));
    assert!(empty["workspace"].get("selected_tab").is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_workspace_closes_with_its_tabs_and_its_handle_is_not_used_again() {
    let h = Harness::start_layout();
    let mut c = client(&h).await;
    c.ok("workspace.create", json!({"name": "A"})).await;
    c.ok("workspace.create", json!({"name": "B"})).await;
    c.ok("workspace.close", json!({"workspace": "w1"})).await;
    let list = c.ok("workspace.list", json!({})).await;
    assert_eq!(list["workspaces"].as_array().map(Vec::len), Some(1));
    let again = c.ok("workspace.create", json!({"name": "C"})).await;
    assert_eq!(again["handle"], "w3");
    let (order, _) = tree(&mut c).await;
    assert_eq!(order.len(), 2);
    // The closed workspace is gone from every method.
    let reply = c.call("workspace.show", json!({"workspace": "w1"})).await;
    assert_eq!(error_of(&reply), (-32000, "not_found".into()));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn errors_have_the_codes_of_the_contract() {
    let h = Harness::start_layout();
    let mut c = client(&h).await;
    c.ok("workspace.create", json!({"name": "A"})).await;
    for (method, params, code) in [
        (
            "workspace.show",
            json!({"workspace": "w9"}),
            (-32000, "not_found"),
        ),
        (
            "workspace.rename",
            json!({"workspace": "w1", "name": "  "}),
            (-32602, "invalid_params"),
        ),
        (
            "workspace.create",
            json!({"machine": "devbox"}),
            (-32000, "unsupported"),
        ),
        (
            "workspace.create",
            json!({"group": "01ARZ3NDEKTSV4RRFFQ69G5FAV"}),
            (-32000, "not_found"),
        ),
        (
            "workspace.move",
            json!({"workspace": "w1", "before": "w1"}),
            (-32602, "invalid_params"),
        ),
        ("tab.select", json!({"tab": "w1:t7"}), (-32000, "not_found")),
        (
            "tab.create",
            json!({"workspace": "w1", "name": "x\ny"}),
            (-32602, "invalid_params"),
        ),
        (
            "layout.set_ratios",
            json!({"tab": "w1:t1", "path": [], "ratios": [0.5, 0.5]}),
            (-32602, "invalid_params"),
        ),
        (
            "layout.zoom",
            json!({"tab": "w1:t1", "pane": "w1:p9"}),
            (-32000, "not_found"),
        ),
        // A handle of the wrong kind is refused by the parser of the params.
        (
            "tab.select",
            json!({"tab": "w1"}),
            (-32602, "invalid_params"),
        ),
        (
            "group.show",
            json!({"group": "01ARZ3NDEKTSV4RRFFQ69G5FAV"}),
            (-32000, "not_found"),
        ),
    ] {
        let reply = c.call(method, params).await;
        assert_eq!(
            error_of(&reply),
            (code.0, code.1.to_owned()),
            "{method}: {reply}"
        );
    }
    // Nothing of the failed commands is left.
    let list = c.ok("workspace.list", json!({})).await;
    assert_eq!(list["workspaces"].as_array().map(Vec::len), Some(1));
    assert_eq!(list["workspaces"][0]["tabs"], 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn zoom_the_only_pane_and_end_the_zoom() {
    let h = Harness::start_layout();
    let mut c = client(&h).await;
    let created = c.ok("workspace.create", json!({"name": "A"})).await;
    c.ok(
        "layout.zoom",
        json!({"tab": "w1:t1", "pane": created["pane"]}),
    )
    .await;
    let shown = c.ok("workspace.show", json!({"workspace": "w1"})).await;
    assert_eq!(shown["workspace"]["tabs"][0]["zoomed"], created["pane"]);
    c.ok("layout.zoom", json!({"tab": "w1:t1"})).await;
    let shown = c.ok("workspace.show", json!({"workspace": "w1"})).await;
    assert!(shown["workspace"]["tabs"][0].get("zoomed").is_none());
    c.ok("layout.equalize", json!({"tab": "w1:t1"})).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_layout_comes_back_after_a_restart_of_the_core() {
    let mut h = Harness::start_layout();
    let (before_tree, before_list, start) = {
        let mut c = client(&h).await;
        // Where the log is before anything happens.
        let start = c.ok("events.subscribe", json!({})).await["cursor"].clone();
        let group = c.ok("group.create", json!({"name": "Work"})).await["group"].clone();
        c.ok(
            "workspace.create",
            json!({"name": "A", "root": "/a", "group": group}),
        )
        .await;
        c.ok("workspace.create", json!({"name": "B"})).await;
        c.ok("workspace.create", json!({"name": "C"})).await;
        c.ok("tab.create", json!({"workspace": "w1", "name": "Logs"}))
            .await;
        c.ok("tab.select", json!({"tab": "w1:t1"})).await;
        c.ok("workspace.move", json!({"workspace": "w3", "before": "w1"}))
            .await;
        c.ok("workspace.close", json!({"workspace": "w2"})).await;
        (
            tree(&mut c).await,
            c.ok("workspace.list", json!({})).await,
            start,
        )
    };
    h.crash().await;
    h.restart();

    let mut c = client(&h).await;
    let (after_tree, after_list) = (tree(&mut c).await, c.ok("workspace.list", json!({})).await);
    assert_eq!(after_tree, before_tree, "the same tree");
    assert_eq!(after_list, before_list);
    let handles: Vec<_> = after_list["workspaces"]
        .as_array()
        .expect("a list")
        .iter()
        .map(|w| w["handle"].as_str().expect("a handle"))
        .collect();
    assert_eq!(handles, ["w3", "w1"]);

    // The handles never come back, also across the restart: w2 is closed, so the next is w4,
    // and the next tab of w1 is t3.
    let next = c.ok("workspace.create", json!({"name": "D"})).await;
    assert_eq!(next["handle"], "w4");
    let tab = c.ok("tab.create", json!({"workspace": "w1"})).await;
    assert_eq!(tab["handle"], "w1:t3");
    // The pane numbers go on too.
    assert_eq!(tab["pane_handle"], "w1:p3");
    // The event log is the same log: a cursor from before the restart still works, and the
    // events of the first run come first.
    let mut events = client(&h).await;
    let reply = events
        .ok("events.subscribe", json!({"cursor": start}))
        .await;
    assert!(reply["subscription"].is_string());
    let first = loop {
        let line = events.recv().await;
        if line["method"] == "events.event" {
            break line;
        }
    };
    assert_eq!(first["params"]["event"]["kind"], "group.created");
    assert_eq!(
        first["params"]["event"]["seq"],
        start["seq"].as_u64().unwrap_or(0) + 1
    );
}
