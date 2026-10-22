//! The notifications, the badge and the tray, with a host that writes down its calls.

use std::time::Duration;

use nh_proto::api::{
    common::Urgency,
    ids::{InteractionId, NotificationId},
    projections::Projection,
};
use serde_json::json;

use crate::{
    agents::tests::{Fixture, Options},
    host::{HostCall, HostNotification, NotificationAction},
    interaction::tests::{
        allow, answer, call, deny, focus_the_pane, gate, id_of, pre_tool, started_with,
    },
};

/// The window in which a run posts one notification, short enough for a test.
const WINDOW: Duration = Duration::from_millis(400);

async fn started(options: Options) -> (Fixture, nh_proto::api::ids::RunId) {
    started_with(Fixture::with_options(options)).await
}

async fn ask(f: &Fixture, call_id: &str, command: &str) -> InteractionId {
    pre_tool(f, call_id, command).await;
    let (start, rx) = gate(f, 7, command).await;
    // The shim stays connected for the whole test.
    std::mem::forget(rx);
    id_of(&start)
}

fn posts(f: &Fixture) -> Vec<(NotificationId, HostNotification)> {
    f.host
        .calls()
        .into_iter()
        .filter_map(|call| match call {
            HostCall::Notify(id, notification) => Some((id, notification)),
            _ => None,
        })
        .collect()
}

fn updates(f: &Fixture) -> Vec<(NotificationId, HostNotification)> {
    f.host
        .calls()
        .into_iter()
        .filter_map(|call| match call {
            HostCall::UpdateNotification(id, notification) => Some((id, notification)),
            _ => None,
        })
        .collect()
}

fn withdrawn(f: &Fixture) -> Vec<NotificationId> {
    f.host
        .calls()
        .into_iter()
        .filter_map(|call| match call {
            HostCall::WithdrawNotification(id) => Some(id),
            _ => None,
        })
        .collect()
}

fn badges(f: &Fixture) -> Vec<u32> {
    f.host
        .calls()
        .into_iter()
        .filter_map(|call| match call {
            HostCall::SetBadge(count) => Some(count),
            _ => None,
        })
        .collect()
}

fn trays(f: &Fixture) -> Vec<crate::host::TrayModel> {
    f.host
        .calls()
        .into_iter()
        .filter_map(|call| match call {
            HostCall::SetTray(tray) => Some(tray),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn an_approval_on_a_pane_without_the_focus_posts_one_notification() {
    let (f, _run) = started(Options::default()).await;
    let id = ask(&f, "toolu_1", "pnpm test").await;
    let posted = posts(&f);
    assert_eq!(posted.len(), 1, "one notification");
    let (_, notification) = &posted[0];
    assert_eq!(notification.title, "Claude Code asks for permission");
    assert_eq!(notification.body, "Run `pnpm test`");
    assert_eq!(notification.interaction, Some(id));
    assert!(
        notification.actions.is_empty(),
        "the host offers no buttons"
    );
}

#[tokio::test]
async fn a_host_with_buttons_gets_allow_and_deny_for_an_approval() {
    let (f, _run) = started(Options {
        actions: true,
        ..Options::default()
    })
    .await;
    ask(&f, "toolu_1", "pnpm test").await;
    let posted = posts(&f);
    assert_eq!(
        posted[0].1.actions,
        [NotificationAction::Allow, NotificationAction::Deny]
    );
}

#[tokio::test]
async fn the_urgency_follows_the_risk() {
    let (f, _run) = started(Options::default()).await;
    ask(&f, "toolu_1", "rm -rf build").await;
    assert_eq!(posts(&f)[0].1.urgency, Urgency::High);
}

#[tokio::test]
async fn a_pane_with_the_focus_gets_no_notification() {
    let (f, _run) = started(Options::default()).await;
    focus_the_pane(&f).await;
    // The hook is not held for a pane with the focus, so the shim gets no interaction id.
    pre_tool(&f, "toolu_1", "pnpm test").await;
    let (_start, rx) = gate(&f, 7, "pnpm test").await;
    std::mem::forget(rx);
    assert!(posts(&f).is_empty(), "the person looks at the pane");
    // The badge and the tray still follow `attention`.
    assert_eq!(badges(&f).last(), Some(&1));
}

#[tokio::test]
async fn an_answer_withdraws_the_notification() {
    let (f, _run) = started(Options::default()).await;
    let id = ask(&f, "toolu_1", "pnpm test").await;
    let (notice, _) = posts(&f)[0].clone();
    answer(&f, id, allow()).await.expect("the answer works");
    assert_eq!(withdrawn(&f), [notice]);
}

#[tokio::test]
async fn the_focus_of_the_pane_withdraws_the_notification_of_a_held_hook() {
    let (f, _run) = started(Options::default()).await;
    ask(&f, "toolu_1", "pnpm test").await;
    let (notice, _) = posts(&f)[0].clone();
    focus_the_pane(&f).await;
    for _ in 0..200 {
        if !withdrawn(&f).is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(withdrawn(&f), [notice], "the harness shows its own dialog");
}

#[tokio::test]
async fn a_second_interaction_of_the_run_changes_the_notification_in_place() {
    let (f, _run) = started(Options::default()).await;
    ask(&f, "toolu_1", "pnpm test").await;
    ask(&f, "toolu_2", "pnpm build").await;
    let posted = posts(&f);
    assert_eq!(posted.len(), 1, "a run has one notification");
    let changed = updates(&f);
    assert_eq!(changed.len(), 1);
    assert_eq!(changed[0].0, posted[0].0, "the same notification");
    assert!(
        changed[0].1.body.ends_with("(and 1 more)"),
        "{}",
        changed[0].1.body
    );
}

#[tokio::test]
async fn a_run_posts_at_most_one_notification_in_the_window() {
    let (f, _run) = started(Options {
        notify_window: Some(WINDOW),
        ..Options::default()
    })
    .await;
    let first = ask(&f, "toolu_1", "pnpm test").await;
    answer(&f, first, allow()).await.expect("the answer works");
    assert_eq!(posts(&f).len(), 1);
    // The next request comes at once. Its post waits for the end of the window.
    ask(&f, "toolu_2", "pnpm build").await;
    assert_eq!(posts(&f).len(), 1, "the window is not over");
    for _ in 0..200 {
        if posts(&f).len() == 2 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let posted = posts(&f);
    assert_eq!(posted.len(), 2, "the post goes when the window ends");
    assert_eq!(posted[1].1.body, "Run `pnpm build`");
}

#[tokio::test]
async fn a_late_click_on_a_notification_says_who_answered() {
    let (f, _run) = started(Options::default()).await;
    let id = ask(&f, "toolu_1", "pnpm test").await;
    answer(&f, id, allow()).await.expect("the card answers");
    let late = call(
        &f,
        "interaction.answer",
        json!({"interaction": id.to_string(), "decision": deny("no"), "via": "notification"}),
    )
    .await;
    assert!(late.is_err(), "the interaction has its answer");
    let last = posts(&f).pop().expect("a notification").1;
    assert_eq!(last.title, "Already answered");
    assert_eq!(last.body, "Run `pnpm test` was answered by a card.");
    assert_eq!(last.urgency, Urgency::Low);
}

#[tokio::test]
async fn the_badge_and_the_tray_follow_the_open_interactions() {
    let (f, _run) = started(Options::default()).await;
    let id = ask(&f, "toolu_1", "pnpm test").await;
    assert_eq!(badges(&f).last(), Some(&1));
    let tray = trays(&f).pop().expect("a tray");
    assert_eq!(tray.badge, 1);
    assert_eq!(tray.entries.len(), 1);
    assert_eq!(tray.entries[0].interaction, Some(id));
    assert!(tray.entries[0].approval);
    assert_eq!(tray.entries[0].label, "Claude Code: Run `pnpm test`");
    answer(&f, id, allow()).await.expect("the answer works");
    assert_eq!(badges(&f).last(), Some(&0));
    assert!(trays(&f).pop().expect("a tray").entries.is_empty());
}

#[tokio::test]
async fn a_value_that_did_not_change_is_not_sent_again() {
    let (f, _run) = started(Options::default()).await;
    ask(&f, "toolu_1", "pnpm test").await;
    let before = (badges(&f).len(), trays(&f).len());
    // The same facts again: a report of the window that changes nothing for the surfaces.
    f.update(crate::agents::runs::publish_all).await;
    assert_eq!((badges(&f).len(), trays(&f).len()), before);
}

#[tokio::test]
async fn reveal_marks_the_window_and_asks_the_host_to_show_it() {
    let (f, _run) = started(Options::default()).await;
    let window = f.open_window().await;
    let id = ask(&f, "toolu_1", "pnpm test").await;
    let shown = call(&f, "interaction.reveal", json!({}))
        .await
        .expect("reveal");
    assert_eq!(shown["interaction"], id.to_string());
    assert_eq!(shown["window"], window.to_string());
    let row = f
        .item(Projection::Windows, &window.to_string())
        .expect("a window");
    assert_eq!(row["reveal"]["interaction"], id.to_string());
    assert_eq!(row["reveal"]["seq"], 1);
    assert!(f.host.calls().contains(&HostCall::FocusWindow(window)));
    // The same card again is a new request.
    call(
        &f,
        "interaction.reveal",
        json!({"interaction": id.to_string()}),
    )
    .await
    .expect("reveal");
    let row = f
        .item(Projection::Windows, &window.to_string())
        .expect("a window");
    assert_eq!(row["reveal"]["seq"], 2);
}

#[tokio::test]
async fn reveal_with_nothing_waiting_changes_nothing() {
    let (f, _run) = started(Options::default()).await;
    let window = f.open_window().await;
    let shown = call(&f, "interaction.reveal", json!({}))
        .await
        .expect("reveal");
    assert!(shown["interaction"].is_null() || shown.get("interaction").is_none());
    let row = f
        .item(Projection::Windows, &window.to_string())
        .expect("a window");
    assert!(row.get("reveal").is_none());
}
