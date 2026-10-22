//! The commands on panes and windows against a real core: the `panes`, `windows` and
//! `tree` projections, and the layout after a restart.

mod core_support;

use core_support::{
    Harness,
    client::{Client, error_of},
};
use nh_core::ErrorCode;
use serde_json::{Value, json};

async fn client(h: &Harness) -> Client {
    Client::connect(&h.api_socket()).await
}

/// A projection as a snapshot: `(order, items)`.
async fn snapshot(c: &mut Client, name: &str) -> (Vec<String>, Value) {
    let reply = c.ok("projection.subscribe", json!({"name": name})).await;
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
async fn a_split_makes_a_pane_in_the_tree_and_in_the_panes_projection() {
    let h = Harness::start_layout();
    let mut c = client(&h).await;
    let created = c.ok("workspace.create", json!({"name": "A"})).await;
    let split = c
        .ok(
            "pane.split",
            json!({"pane": "w1:p1", "direction": "right", "ratio": 0.4}),
        )
        .await;
    assert_eq!(split["handle"], "w1:p2");

    let (order, items) = snapshot(&mut c, "tree").await;
    let tab = &items[&order[0]]["tabs"][0];
    assert_eq!(tab["layout"]["type"], "split");
    assert_eq!(tab["layout"]["axis"], "row");
    assert_eq!(
        tab["layout"]["children"],
        json!([
            {"type": "pane", "pane": created["pane"]},
            {"type": "pane", "pane": split["pane"]}
        ])
    );
    let ratios: Vec<f64> = tab["layout"]["ratios"]
        .as_array()
        .expect("ratios")
        .iter()
        .map(|r| r.as_f64().expect("a number"))
        .collect();
    assert!(
        (ratios[0] - 0.6).abs() < 1e-6 && (ratios[1] - 0.4).abs() < 1e-6,
        "{ratios:?}"
    );

    let (_, panes) = snapshot(&mut c, "panes").await;
    let pane = &panes[split["pane"].as_str().expect("an id")];
    assert_eq!(pane["handle"], "w1:p2");
    assert_eq!(pane["render_id"], 2);
    assert_eq!(pane["workspace"], created["workspace"]);
    assert_eq!(pane["tab"], created["tab"]);
    assert_eq!(pane["liveness"], json!({"state": "starting"}));
    assert_eq!(
        (pane["cols"].clone(), pane["rows"].clone()),
        (json!(80), json!(24))
    );

    // The same through the CLI methods.
    let list = c.ok("pane.list", json!({"workspace": "w1"})).await;
    assert_eq!(list["panes"].as_array().map(Vec::len), Some(2));
    let shown = c.ok("pane.show", json!({"pane": "w1:p2"})).await;
    assert_eq!(shown["pane"], *pane);
    assert_eq!(
        c.ok("pane.list", json!({"tab": "w1:t1"})).await["panes"]
            .as_array()
            .map(Vec::len),
        Some(2)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn float_move_and_close_change_the_tree() {
    let h = Harness::start_layout();
    let mut c = client(&h).await;
    c.ok("workspace.create", json!({"name": "A"})).await;
    c.ok("pane.split", json!({"pane": "w1:p1", "direction": "right"}))
        .await;
    c.ok("pane.split", json!({"pane": "w1:p2", "direction": "down"}))
        .await;
    c.ok("tab.create", json!({"workspace": "w1"})).await;

    c.ok(
        "pane.float",
        json!({"pane": "w1:p3", "rect": {"x": 0.1, "y": 0.1, "w": 0.4, "h": 0.4}}),
    )
    .await;
    h.call(
        "pane.set_float_rect",
        json!({"pane": "w1:p3", "rect": {"x": 0.2, "y": 0.2, "w": 0.4, "h": 0.4}}),
    )
    .await;
    let shown = c.ok("workspace.show", json!({"workspace": "w1"})).await;
    let floating = &shown["workspace"]["tabs"][0]["floating"];
    let x = floating[0]["rect"]["x"].as_f64().expect("a number");
    assert!((x - 0.2).abs() < 1e-6, "{x}");
    c.ok("pane.unfloat", json!({"pane": "w1:p3"})).await;
    let shown = c.ok("workspace.show", json!({"workspace": "w1"})).await;
    assert!(shown["workspace"]["tabs"][0].get("floating").is_none());

    // Move p2 to the second tab, beside its pane.
    c.ok(
        "pane.move",
        json!({"pane": "w1:p2", "tab": "w1:t2", "beside": "w1:p4", "direction": "left"}),
    )
    .await;
    let panes = c.ok("pane.list", json!({"tab": "w1:t2"})).await;
    assert_eq!(panes["panes"].as_array().map(Vec::len), Some(2));

    c.ok("pane.close", json!({"pane": "w1:p3"})).await;
    let list = c.ok("pane.list", json!({})).await;
    assert_eq!(list["panes"].as_array().map(Vec::len), Some(3));

    // Errors.
    for (method, params, code) in [
        (
            "pane.split",
            json!({"pane": "w1:p9", "direction": "right"}),
            (-32000, "not_found"),
        ),
        (
            "pane.split",
            json!({"pane": "w1:p1", "direction": "right", "ratio": 2}),
            (-32602, "invalid_params"),
        ),
        (
            "pane.float",
            json!({"pane": "w1:p4", "rect": {"x": 2, "y": 0, "w": 0.5, "h": 0.5}}),
            (-32602, "invalid_params"),
        ),
        (
            "pane.unfloat",
            json!({"pane": "w1:p1"}),
            (-32602, "invalid_params"),
        ),
        (
            "pane.move",
            json!({"pane": "w1:p1", "tab": "w1:t2", "beside": "w1:p1"}),
            (-32602, "invalid_params"),
        ),
        (
            "pane.popout",
            json!({"pane": "w1:p9"}),
            (-32000, "not_found"),
        ),
        // A pane that starts, or runs, does not restart. Only an exited or lost pane does.
        (
            "pane.restart",
            json!({"pane": "w1:p1"}),
            (-32602, "invalid_params"),
        ),
    ] {
        let reply = c.call(method, params).await;
        assert_eq!(
            error_of(&reply),
            (code.0, code.1.to_owned()),
            "{method}: {reply}"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_pane_pops_out_into_a_window_and_the_windows_projection_follows() {
    let h = Harness::start_layout();
    let mut c = client(&h).await;
    let created = c.ok("workspace.create", json!({"name": "A"})).await;
    let split = c
        .ok("pane.split", json!({"pane": "w1:p1", "direction": "right"}))
        .await;
    let popped = c.ok("pane.popout", json!({"pane": "w1:p2"})).await["window"].clone();

    let (order, windows) = snapshot(&mut c, "windows").await;
    assert_eq!(order, [popped.as_str().expect("an id")]);
    let window = &windows[&order[0]];
    assert_eq!(window["visible_panes"], json!([split["pane"]]));
    assert_eq!(window["focused_pane"], split["pane"]);
    assert_eq!(
        (window["focused"].clone(), window["visible"].clone()),
        (json!(false), json!(true))
    );

    // A window for the tab, with bounds.
    let opened = h
        .call(
            "window.open",
            json!({"tab": "w1:t1", "bounds": {"x": 1, "y": 2, "width": 800, "height": 600}}),
        )
        .await["window"]
        .clone();
    let (_, windows) = snapshot(&mut c, "windows").await;
    let tab_window = &windows[opened.as_str().expect("an id")];
    assert_eq!(tab_window["tab"], created["tab"]);
    assert_eq!(tab_window["visible_panes"], json!([created["pane"]]));
    assert_eq!(tab_window["bounds"]["width"], 800);

    // Zoom shows one pane; a split ends it.
    c.ok("layout.zoom", json!({"tab": "w1:t1", "pane": "w1:p1"}))
        .await;
    h.call(
        "window.set_bounds",
        json!({"window": opened, "bounds": {"x": 0, "y": 0, "width": 640, "height": 480}}),
    )
    .await;
    let (_, windows) = snapshot(&mut c, "windows").await;
    assert_eq!(
        windows[opened.as_str().expect("an id")]["bounds"]["width"],
        640
    );

    // Closing the popout window puts the pane back.
    h.call("window.close", json!({"window": popped})).await;
    let tree = c.ok("workspace.show", json!({"workspace": "w1"})).await;
    assert_eq!(tree["workspace"]["tabs"][0]["layout"]["type"], "split");
    let (order, _) = snapshot(&mut c, "windows").await;
    assert_eq!(order, [opened.as_str().expect("an id")]);

    // The explicit list of panes.
    h.call(
        "window.set_visible_panes",
        json!({"window": opened, "panes": ["w1:p2"]}),
    )
    .await;
    let (_, windows) = snapshot(&mut c, "windows").await;
    let window = &windows[opened.as_str().expect("an id")];
    assert_eq!(window["visible_panes"], json!([split["pane"]]));
    assert!(window.get("tab").is_none());

    // A window is a UI matter: the CLI is refused, and an unknown window is not found.
    let reply = c.call("window.close", json!({"window": popped})).await;
    assert_eq!(error_of(&reply), (-32000, "permission_denied".into()));
    let error = h
        .handle
        .rpc(
            "window.close",
            json!({"window": "01ARZ3NDEKTSV4RRFFQ69G5FAV"}),
        )
        .await
        .expect_err("no such window");
    assert_eq!(error.code, ErrorCode::NotFound);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn closing_the_last_pane_closes_the_tab_and_a_closed_pane_leaves_the_panes_projection() {
    let h = Harness::start_layout();
    let mut c = client(&h).await;
    c.ok("workspace.create", json!({"name": "A"})).await;
    c.ok("tab.create", json!({"workspace": "w1"})).await;
    c.ok("pane.close", json!({"pane": "w1:p2"})).await;
    let shown = c.ok("workspace.show", json!({"workspace": "w1"})).await;
    assert_eq!(shown["workspace"]["tabs"].as_array().map(Vec::len), Some(1));
    let (_, panes) = snapshot(&mut c, "panes").await;
    assert_eq!(panes.as_object().map(serde_json::Map::len), Some(1));
    // The last pane of the last tab: the workspace stays with no tab.
    c.ok("pane.close", json!({"pane": "w1:p1"})).await;
    let shown = c.ok("workspace.show", json!({"workspace": "w1"})).await;
    assert_eq!(shown["workspace"]["tabs"], json!([]));
    let (_, panes) = snapshot(&mut c, "panes").await;
    assert_eq!(panes, json!({}));
    // Pane handles are not used again.
    let tab = c.ok("tab.create", json!({"workspace": "w1"})).await;
    assert_eq!(tab["pane_handle"], "w1:p3");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn panes_and_windows_come_back_after_a_restart_of_the_core() {
    let mut h = Harness::start_layout();
    let before = {
        let mut c = client(&h).await;
        c.ok("workspace.create", json!({"name": "A", "root": "/a"}))
            .await;
        c.ok("workspace.create", json!({"name": "B"})).await;
        c.ok(
            "pane.split",
            json!({"pane": "w1:p1", "direction": "right", "ratio": 0.3}),
        )
        .await;
        c.ok("pane.split", json!({"pane": "w1:p2", "direction": "down"}))
            .await;
        c.ok("pane.float", json!({"pane": "w1:p3"})).await;
        c.ok("pane.popout", json!({"pane": "w1:p2"})).await;
        h.call(
            "window.open",
            json!({"tab": "w2:t1", "bounds": {"x": 3, "y": 4, "width": 900, "height": 700}}),
        )
        .await;
        c.ok("layout.zoom", json!({"tab": "w2:t1", "pane": "w2:p1"}))
            .await;
        let mut all = Vec::new();
        for name in ["tree", "panes", "windows"] {
            all.push(snapshot(&mut c, name).await);
        }
        all
    };
    h.crash().await;
    h.restart();

    let mut c = client(&h).await;
    let mut after = Vec::new();
    for name in ["tree", "panes", "windows"] {
        after.push(snapshot(&mut c, name).await);
    }
    assert_eq!(after[0], before[0], "the same tree");
    assert_eq!(after[2], before[2], "the same windows");
    // The panes projection has the same rows (the order of the keys can differ).
    assert_eq!(after[1].1, before[1].1, "the same panes");

    // Counters go on: the next pane is w1:p4, and its render id is above all the others.
    let next = c
        .ok("pane.split", json!({"pane": "w1:p1", "direction": "down"}))
        .await;
    assert_eq!(next["handle"], "w1:p4");
    let (_, panes) = snapshot(&mut c, "panes").await;
    let renders: Vec<u64> = panes
        .as_object()
        .expect("panes")
        .values()
        .map(|p| p["render_id"].as_u64().expect("a render id"))
        .collect();
    assert_eq!(renders.iter().copied().max(), Some(5));
    let mut unique = renders.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(unique.len(), renders.len());
}
