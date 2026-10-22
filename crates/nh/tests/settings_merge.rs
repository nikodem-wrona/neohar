//! The `settings` of a window of a remote machine: the sections of the
//! client come from the core of this Mac, the others from the machine. Two real cores stand for
//! the two sides, and each is reached over a link.

mod core_support;

use std::{path::PathBuf, sync::Arc, time::Duration};

use core_support::{Harness, TIMEOUT};
use nh_core::{MessageSink, RecordingHost};
use nh_remote::{
    ClientHello, ClientOptions, Link, LinkClient, LinkServer, LinkTiming, SettingsFeed, Side,
    settings::route_config,
};
use serde_json::{Value, json};
use tokio::{sync::mpsc, time::timeout};
use ulid::Ulid;

fn timing() -> LinkTiming {
    LinkTiming {
        ping_every: Duration::from_millis(100),
        down_after: Duration::from_secs(5),
        hello_within: Duration::from_secs(5),
    }
}

fn serve(h: &Harness) -> LinkServer {
    nh_remote::serve_with(h.handle.clone(), &h.paths, "test", timing()).expect("link.sock serves")
}

async fn connect(server: &LinkServer, device: u64) -> LinkClient {
    LinkClient::connect_unix(
        server.path(),
        ClientHello {
            device: Ulid::from_parts(1, u128::from(device)),
            name: "laptop".to_owned(),
            app_version: "0.0.0".to_owned(),
        },
        Arc::new(RecordingHost::new(PathBuf::from("/bin/nh"), true)),
        ClientOptions {
            timing: timing(),
            ..ClientOptions::default()
        },
    )
    .await
    .expect("the link opens")
}

struct Tx(mpsc::UnboundedSender<Value>);

impl MessageSink for Tx {
    fn send(&self, message: Value) -> bool {
        self.0.send(message).is_ok()
    }
}

/// The two sides: the core of this Mac and the core of the machine, each with a link.
struct Pair {
    local: Harness,
    machine: Harness,
    _servers: (LinkServer, LinkServer),
    to_local: LinkClient,
    to_machine: LinkClient,
}

async fn pair() -> Pair {
    let (local, machine) = (Harness::start(), Harness::start());
    let servers = (serve(&local), serve(&machine));
    let to_local = connect(&servers.0, 71).await;
    let to_machine = connect(&servers.1, 72).await;
    Pair {
        local,
        machine,
        _servers: servers,
        to_local,
        to_machine,
    }
}

async fn section(h: &Harness, name: &str) -> Value {
    h.call("config.get", json!({ "section": name })).await
}

fn value_of(reply: &Value, section: &str, key: &str) -> Value {
    reply["sections"][section]["entries"][key]["value"].clone()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_key_of_the_client_changes_the_local_file_and_any_other_key_the_machine() {
    let p = pair().await;
    let (local, machine): (
        &dyn nh_remote::settings::Calls,
        &dyn nh_remote::settings::Calls,
    ) = (&p.to_local, &p.to_machine);

    let set = |key: &str, value: Value| {
        let params = json!({ "key": key, "value": value });
        async move { route_config(local, machine, None, "config.set", &params).await }
    };
    set("ui.font_size", json!(19))
        .await
        .expect("routed")
        .expect("set");
    set("approvals.mode", json!("observe"))
        .await
        .expect("routed")
        .expect("set");

    // The local core has the font size, and the machine has not.
    assert_eq!(
        value_of(&section(&p.local, "ui").await, "ui", "font_size"),
        json!(19)
    );
    assert_eq!(
        value_of(&section(&p.machine, "ui").await, "ui", "font_size"),
        json!(13)
    );
    // The machine has the mode, and the local core has not.
    assert_eq!(
        value_of(&section(&p.machine, "approvals").await, "approvals", "mode"),
        json!("observe")
    );
    assert_eq!(
        value_of(&section(&p.local, "approvals").await, "approvals", "mode"),
        json!("auto")
    );
    let file = |h: &Harness| {
        let dir = h.paths.config_dir().expect("a config dir");
        std::fs::read_to_string(dir.join(nh_core::config::FILE_NAME)).unwrap_or_default()
    };
    let (local_file, machine_file) = (file(&p.local), file(&p.machine));
    assert!(local_file.contains("font_size = 19"), "{local_file}");
    assert!(!local_file.contains("observe"), "{local_file}");
    assert!(
        machine_file.contains("mode = \"observe\""),
        "{machine_file}"
    );
    assert!(!machine_file.contains("font_size"), "{machine_file}");

    // `config.get` with no section joins the two: `ui` from here, `approvals` from the machine.
    let all = route_config(local, machine, None, "config.get", &json!({}))
        .await
        .expect("routed")
        .expect("get");
    assert_eq!(value_of(&all, "ui", "font_size"), json!(19));
    assert!(all["sections"]["approvals"].is_object());

    // `config.reload` goes to both, and a method that is not config.* is not routed.
    let reloaded = route_config(local, machine, None, "config.reload", &json!({}))
        .await
        .expect("routed")
        .expect("reload");
    assert_eq!(reloaded["errors"], json!([]));
    assert!(
        route_config(local, machine, None, "pane.list", &json!({}))
            .await
            .is_none()
    );
}

/// Feeds the merge from the two links, as the app does.
async fn follow(feed: &Arc<SettingsFeed>, side: Side, link: &LinkClient) {
    let (tx, mut rx) = mpsc::unbounded_channel();
    Link::subscribe(link, "settings", None, Box::new(Tx(tx)))
        .await
        .expect("settings subscribes");
    let feed = Arc::clone(feed);
    tokio::spawn(async move {
        while let Some(message) = rx.recv().await {
            feed.apply(side, &message);
        }
    });
}

async fn next(rx: &mut mpsc::UnboundedReceiver<Value>) -> Value {
    timeout(TIMEOUT, rx.recv())
        .await
        .expect("a message in time")
        .expect("the feed is open")
}

#[tokio::test(flavor = "multi_thread")]
async fn the_merged_projection_has_the_client_sections_from_here_and_the_rest_from_the_machine() {
    let p = pair().await;
    // The machine has its own value in `ui`: the merge must not show it.
    p.machine
        .call("config.set", json!({"key": "ui.font_size", "value": 31}))
        .await;
    p.machine
        .call(
            "config.set",
            json!({"key": "approvals.mode", "value": "gate"}),
        )
        .await;
    let feed = Arc::new(SettingsFeed::new("studio"));
    let (tx, mut rx) = mpsc::unbounded_channel();
    feed.subscribe(None, Box::new(Tx(tx)));
    follow(&feed, Side::Local, &p.to_local).await;
    follow(&feed, Side::Machine, &p.to_machine).await;

    let snapshot = next(&mut rx).await;
    assert_eq!(snapshot["type"], "snapshot");
    let items = &snapshot["data"]["items"];
    assert_eq!(items["ui"]["entries"]["font_size"]["value"], json!(13));
    assert_eq!(
        items["approvals"]["entries"]["mode"]["value"],
        json!("gate")
    );
    let order = snapshot["data"]["order"].as_array().expect("an order");
    for section in ["ui", "theme", "keys", "approvals"] {
        assert!(order.contains(&json!(section)), "{section} is missing");
    }
    let first = snapshot["version"].as_u64().expect("a version");

    // A change on this Mac comes as a patch of the merge.
    p.local
        .call("config.set", json!({"key": "ui.font_size", "value": 17}))
        .await;
    let patch = loop {
        let message = next(&mut rx).await;
        if message["type"] == "patch" {
            break message;
        }
    };
    assert!(patch["version"].as_u64().expect("a version") > first);
    assert_eq!(patch["ops"][0]["key"], "ui");
    assert_eq!(
        patch["ops"][0]["value"]["entries"]["font_size"]["value"],
        json!(17)
    );

    // A change on the machine too.
    p.machine
        .call(
            "config.set",
            json!({"key": "approvals.mode", "value": "observe"}),
        )
        .await;
    let patch = loop {
        let message = next(&mut rx).await;
        if message["type"] == "patch" {
            break message;
        }
    };
    assert_eq!(patch["ops"][0]["key"], "approvals");
}
