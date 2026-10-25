//! Two clients on one machine ("Two clients", without a link): their own windows, the
//! state that a new client gets, the focus of a client that goes, and the first answer that wins.

use std::{path::PathBuf, sync::Arc, time::Duration};

use nh_proto::api::{
    ids::{InteractionId, WindowId},
    projections::Projection,
};
use serde_json::{Value, json};
use ulid::Ulid;

use super::*;
use crate::{
    actor::{Origin, state::TEST_DEVICE},
    agents::tests::{Fixture, Options},
    host::{HostCall, HostNotification, RecordingHost},
    interaction::tests::{allow, gate, id_of, looking_at, pre_tool, report_focus, started_with},
};

const LAPTOP: Ulid = Ulid(0x0200_0000_0000_0000_0000_0000_0000_0002);
const STUDIO: Ulid = Ulid(0x0300_0000_0000_0000_0000_0000_0000_0003);

fn info(device: Ulid, name: &str) -> ClientInfo {
    ClientInfo {
        device,
        name: name.to_owned(),
    }
}

fn recording() -> Arc<RecordingHost> {
    Arc::new(RecordingHost::new(PathBuf::from("/bin/nh"), true))
}

async fn router(f: &Fixture) -> Arc<HostRouter> {
    f.update(|parts| parts.state.host.clone())
        .await
        .expect("the fixture has a router")
}

async fn attach_client(
    f: &Fixture,
    device: Ulid,
    name: &str,
) -> (ClientAttachment, Arc<RecordingHost>) {
    let host = recording();
    let router = router(f).await;
    let attachment = attach(
        &router,
        &f.thread.handle(),
        info(device, name),
        Arc::clone(&host) as Arc<dyn Host>,
    );
    // The actor has done what the attach changes once it answers the next job.
    f.update(|_| ()).await;
    (attachment, host)
}

async fn call_as(f: &Fixture, origin: Origin, method: &str, params: Value) -> Value {
    let call = nh_proto::api::methods::Call::parse(method, params).expect("a call");
    f.thread
        .handle()
        .call(call, origin)
        .await
        .expect("the call works")
}

async fn open_window_of(f: &Fixture, device: Ulid) -> WindowId {
    let opened = call_as(
        f,
        Origin::ui().from_device(device),
        "window.open",
        json!({}),
    )
    .await;
    opened["window"]
        .as_str()
        .expect("an id")
        .parse()
        .expect("a window id")
}

fn window_row(f: &Fixture, window: WindowId) -> Value {
    f.item(Projection::Windows, &window.to_string())
        .expect("a row in windows")
}

fn notified(host: &RecordingHost) -> Vec<HostNotification> {
    host.calls()
        .into_iter()
        .filter_map(|call| match call {
            HostCall::Notify(_, n) => Some(n),
            _ => None,
        })
        .collect()
}

async fn ask(f: &Fixture, call_id: &str, command: &str) -> InteractionId {
    pre_tool(f, call_id, command).await;
    let (start, rx) = gate(f, 7, command).await;
    // The shim stays connected for the whole test.
    std::mem::forget(rx);
    id_of(&start)
}

#[tokio::test]
async fn two_devices_get_their_own_window_rows() {
    let f = Fixture::new();
    let (_laptop, _) = attach_client(&f, LAPTOP, "laptop").await;
    let (_studio, _) = attach_client(&f, STUDIO, "studio").await;
    let a = open_window_of(&f, LAPTOP).await;
    let b = open_window_of(&f, STUDIO).await;
    // A caller with no device (the CLI) gets a window of the host in the process.
    let c: WindowId = call_as(&f, Origin::cli(1), "window.open", json!({})).await["window"]
        .as_str()
        .expect("an id")
        .parse()
        .expect("a window id");
    assert_eq!(window_row(&f, a)["client"], LAPTOP.to_string());
    assert_eq!(window_row(&f, b)["client"], STUDIO.to_string());
    assert_eq!(window_row(&f, c)["client"], TEST_DEVICE.to_string());
    // The rows are in the database with their clients.
    let stored = f
        .update(|parts| parts.store.load_layout().expect("the layout").windows)
        .await;
    let client_of = |id: WindowId| {
        stored
            .iter()
            .find(|row| row.id == id.ulid())
            .and_then(|row| row.client)
    };
    assert_eq!(client_of(a), Some(LAPTOP));
    assert_eq!(client_of(b), Some(STUDIO));
    // The router sends a window request to the owner of the row.
    let (_again, laptop_host) = attach_client(&f, LAPTOP, "laptop").await;
    crate::host::Host::focus_window(router(&f).await.as_ref(), a);
    assert!(laptop_host.calls().contains(&HostCall::FocusWindow(a)));
}

#[tokio::test]
async fn the_first_client_that_attaches_adopts_the_rows_without_an_owner() {
    let f = Fixture::new();
    let window = f.open_window().await;
    // A row with no owner, as a development build from before the windows had a client left it.
    f.update(move |parts| {
        let old = parts.state.layout.clone();
        let mut next = old.clone();
        next.set_window_client(window, None);
        change_layout(parts, &old, next, Vec::new());
    })
    .await;
    assert!(window_row(&f, window).get("client").is_none());
    let (_laptop, _) = attach_client(&f, LAPTOP, "laptop").await;
    assert_eq!(window_row(&f, window)["client"], LAPTOP.to_string());
    let seen = f
        .update(|parts| parts.store.clients().expect("the clients"))
        .await;
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].name, "laptop");
}

#[tokio::test]
async fn the_windows_of_a_client_that_stayed_away_for_30_days_go() {
    let f = Fixture::new();
    let (laptop, _) = attach_client(&f, LAPTOP, "laptop").await;
    let old = open_window_of(&f, LAPTOP).await;
    let mine = open_window_of(&f, TEST_DEVICE).await;
    drop(laptop);
    // The laptop was last seen 31 days ago.
    f.update(|parts| {
        let row = ClientRow {
            device: LAPTOP,
            name: "laptop".to_owned(),
            last_seen: now_ms() - FORGET_AFTER_MS - 24 * 60 * 60 * 1000,
        };
        parts.commit(|tx| tx.touch_client(&row)).expect("stored");
    })
    .await;
    let (_studio, _) = attach_client(&f, STUDIO, "studio").await;
    assert!(f.item(Projection::Windows, &old.to_string()).is_none());
    assert!(f.item(Projection::Windows, &mine.to_string()).is_some());
    let seen = f
        .update(|parts| parts.store.clients().expect("the clients"))
        .await;
    assert!(seen.iter().all(|row| row.device != LAPTOP));
}

#[tokio::test]
async fn a_client_that_goes_leaves_no_focused_pane_and_the_gate_holds_again() {
    let f = Fixture::with_options(Options {
        notify_window: Some(Duration::from_millis(1)),
        ..Options::default()
    });
    let (f, _run) = started_with(f).await;
    let (laptop, laptop_host) = attach_client(&f, LAPTOP, "laptop").await;
    let window = open_window_of(&f, LAPTOP).await;
    // The window of the link client counts as any window.
    report_focus(&f, window, looking_at(f.pane)).await;
    let pane = f.pane;
    assert!(
        f.update(move |parts| parts.state.focus.is_focused(pane))
            .await
    );
    // On a focused pane the harness shows its own dialog: nothing is held or noticed.
    pre_tool(&f, "toolu_1", "pnpm test").await;
    let (_start, rx) = gate(&f, 6, "pnpm test").await;
    std::mem::forget(rx);
    assert!(
        notified(&laptop_host).is_empty() && notified(&f.host).is_empty(),
        "a focused pane gets no notification"
    );

    drop(laptop);
    f.update(|_| ()).await;
    let row = window_row(&f, window);
    assert_eq!(row["focused"], false);
    assert_eq!(row["visible"], false);
    assert_eq!(row["client"], LAPTOP.to_string(), "the row stays");
    assert!(
        !f.update(move |parts| parts.state.focus.is_focused(pane))
            .await
    );
    // A new approval on the pane is noticed: the host in the process shows it.
    ask(&f, "toolu_2", "rm -rf build").await;
    assert!(
        notified(&f.host)
            .iter()
            .any(|n| n.body.contains("rm -rf build")),
        "{:?}",
        f.host.calls()
    );
}

#[tokio::test]
async fn a_new_client_gets_the_notifications_of_the_open_interactions() {
    let (f, _run) = started_with(Fixture::new()).await;
    let id = ask(&f, "toolu_1", "pnpm test").await;
    assert_eq!(notified(&f.host).len(), 1);
    let (_laptop, laptop_host) = attach_client(&f, LAPTOP, "laptop").await;
    let calls = laptop_host.calls();
    assert!(calls.iter().any(|c| matches!(c, HostCall::SetBadge(1))));
    assert!(
        calls
            .iter()
            .any(|c| matches!(c, HostCall::SetTray(t) if t.entries.len() == 1))
    );
    let replayed = notified(&laptop_host);
    assert_eq!(replayed.len(), 1);
    assert_eq!(replayed[0].interaction, Some(id));
}

#[tokio::test]
async fn with_two_clients_the_first_answer_wins_and_the_late_one_sees_who_answered() {
    let (f, _run) = started_with(Fixture::with_options(Options {
        actions: true,
        ..Options::default()
    }))
    .await;
    let (_laptop, laptop_host) = attach_client(&f, LAPTOP, "laptop").await;
    // A low risk that the text shows whole, so the banner has Allow.
    let id = ask(&f, "toolu_1", "git status").await;
    // Both clients show the notification with its buttons.
    assert_eq!(notified(&f.host).len(), 1);
    assert_eq!(notified(&laptop_host).len(), 1);
    assert_eq!(notified(&laptop_host)[0].actions.len(), 2);

    let answer = |device: Ulid| {
        let call = nh_proto::api::methods::Call::parse(
            "interaction.answer",
            json!({"interaction": id.to_string(), "decision": allow(), "via": "notification"}),
        )
        .expect("a call");
        let handle = f.thread.handle();
        async move { handle.call(call, Origin::ui().from_device(device)).await }
    };
    answer(LAPTOP).await.expect("the first answer wins");
    let late = answer(TEST_DEVICE).await;
    assert!(late.is_err(), "the second answer is refused");
    // Only the client that clicked late sees who answered.
    let already = |host: &RecordingHost| {
        notified(host)
            .iter()
            .filter(|n| n.title == "Already answered")
            .count()
    };
    assert_eq!(already(&f.host), 1);
    assert_eq!(already(&laptop_host), 0);
    // The notification goes from both clients.
    for host in [&f.host, &laptop_host] {
        let withdrawn = host
            .calls()
            .iter()
            .any(|c| matches!(c, HostCall::WithdrawNotification(_)));
        assert!(withdrawn, "{:?}", host.calls());
    }
}

#[tokio::test]
async fn the_clipboard_of_an_osc_52_write_follows_the_last_input() {
    // The rule lives in the pane; here the router side: an input device that left drops the text.
    let f = Fixture::new();
    let router = router(&f).await;
    let (laptop, laptop_host) = attach_client(&f, LAPTOP, "laptop").await;
    assert!(router.clipboard_write_for(Some(LAPTOP), "one".to_owned()));
    drop(laptop);
    assert!(!router.clipboard_write_for(Some(LAPTOP), "two".to_owned()));
    assert_eq!(
        laptop_host
            .calls()
            .into_iter()
            .filter(|c| matches!(c, HostCall::ClipboardWrite(_)))
            .collect::<Vec<_>>(),
        [HostCall::ClipboardWrite("one".to_owned())]
    );
}
