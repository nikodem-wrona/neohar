//! The end of a turn that nobody saw, with a host that writes down its calls.

use std::time::{Duration, Instant};

use nh_agents::arbiter::{Input, Observation};
use nh_proto::api::{
    common::Source, ids::RunId, model::Execution, projections::Projection, time::Timestamp,
};
use serde_json::Value;

use crate::{
    agents::{
        runs,
        tests::{Fixture, Options},
    },
    host::{HostCall, HostNotification},
    interaction::tests::{focus_the_pane, started_with},
    persist::now_ms,
};

/// The settle time of a test: short, and long enough to see a flap inside it.
const SETTLE: Duration = Duration::from_millis(300);

async fn started(options: Options) -> (Fixture, RunId) {
    started_with(Fixture::with_options(Options {
        turn_settle: Some(SETTLE),
        ..options
    }))
    .await
}

async fn set(f: &Fixture, run: RunId, execution: Execution) {
    f.update(move |parts| {
        runs::input(
            parts,
            run,
            Input::new(
                Source::Structured,
                Timestamp(now_ms()),
                Observation::Execution(execution),
            ),
        );
    })
    .await;
}

/// A turn that starts and ends.
async fn turn(f: &Fixture, run: RunId) {
    set(f, run, Execution::Working).await;
    set(f, run, Execution::Idle).await;
}

fn attention(f: &Fixture, run: RunId) -> Option<Value> {
    f.item(Projection::Attention, &run.to_string())
}

fn reasons(f: &Fixture, run: RunId) -> Vec<String> {
    attention(f, run)
        .and_then(|row| row["reasons"].as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|reason| reason.as_str().map(str::to_owned))
        .collect()
}

async fn eventually(what: &str, mut ok: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !ok() {
        assert!(Instant::now() < deadline, "timeout: {what}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

fn posts(f: &Fixture) -> Vec<HostNotification> {
    f.host
        .calls()
        .into_iter()
        .filter_map(|call| match call {
            HostCall::Notify(_, notification) => Some(notification),
            _ => None,
        })
        .collect()
}

fn withdrawn(f: &Fixture) -> usize {
    f.host
        .calls()
        .into_iter()
        .filter(|call| matches!(call, HostCall::WithdrawNotification(_)))
        .count()
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

async fn waits(f: &Fixture) -> bool {
    let pane = f.pane;
    f.update(move |parts| parts.state.turn_waits.contains(pane))
        .await
}

#[tokio::test]
async fn a_turn_end_on_a_pane_without_the_focus_counts_after_the_settle_time() {
    let (f, run) = started(Options::default()).await;
    turn(&f, run).await;
    assert!(reasons(&f, run).is_empty(), "not before the settle time");
    eventually("the reason", || reasons(&f, run) == ["turn_ended"]).await;
    let row = attention(&f, run).expect("a row");
    assert_eq!(row["urgency"], "low");
    assert!(row["interactions"].as_array().is_some_and(Vec::is_empty));
    // A call through the actor waits for the step that published the row.
    assert!(waits(&f).await, "the input path knows the pane");
    assert_eq!(badges(&f).last(), Some(&1));
    let posted = posts(&f);
    assert_eq!(posted.len(), 1, "one notification: {posted:?}");
    let notification = &posted[0];
    assert_eq!(notification.title, "Claude Code finished");
    assert_eq!(notification.interaction, None);
    assert_eq!(notification.pane, Some(f.pane));
    assert!(notification.actions.is_empty(), "Open only");
}

#[tokio::test]
async fn a_flap_shorter_than_the_settle_time_counts_for_nothing() {
    let (f, run) = started(Options::default()).await;
    turn(&f, run).await;
    tokio::time::sleep(SETTLE / 3).await;
    set(&f, run, Execution::Working).await;
    tokio::time::sleep(SETTLE * 2).await;
    assert!(reasons(&f, run).is_empty(), "the run works again");
    assert!(posts(&f).is_empty());
    // The end that stays counts, with a settle time of its own.
    set(&f, run, Execution::Idle).await;
    eventually("the reason", || reasons(&f, run) == ["turn_ended"]).await;
}

#[tokio::test]
async fn a_turn_end_on_a_pane_with_the_focus_counts_for_nothing() {
    let (f, run) = started(Options::default()).await;
    focus_the_pane(&f).await;
    turn(&f, run).await;
    tokio::time::sleep(SETTLE * 2).await;
    assert!(reasons(&f, run).is_empty(), "the person looks at the pane");
    assert!(posts(&f).is_empty());
}

#[tokio::test]
async fn the_focus_of_the_pane_ends_the_reason_and_the_notification() {
    let (f, run) = started(Options::default()).await;
    turn(&f, run).await;
    eventually("the reason", || !reasons(&f, run).is_empty()).await;
    focus_the_pane(&f).await;
    eventually("no reason", || attention(&f, run).is_none()).await;
    // The actor publishes the projection first, then it tells the host.
    eventually("the notification goes", || withdrawn(&f) == 1).await;
    eventually("the badge", || badges(&f).last() == Some(&0)).await;
    assert!(!waits(&f).await);
}

#[tokio::test]
async fn a_new_turn_ends_the_reason() {
    let (f, run) = started(Options::default()).await;
    turn(&f, run).await;
    eventually("the reason", || !reasons(&f, run).is_empty()).await;
    set(&f, run, Execution::Working).await;
    eventually("no reason", || attention(&f, run).is_none()).await;
    eventually("the notification goes", || withdrawn(&f) == 1).await;
    assert!(!waits(&f).await);
}

#[tokio::test]
async fn an_input_to_the_pane_ends_the_reason() {
    let (f, run) = started(Options::default()).await;
    turn(&f, run).await;
    eventually("the reason", || !reasons(&f, run).is_empty()).await;
    // What the input path of the panes does for a pane in the set.
    let pane = f.pane;
    f.update(move |parts| crate::agents::turn::seen(parts, pane))
        .await;
    eventually("no reason", || attention(&f, run).is_none()).await;
    assert!(!waits(&f).await);
    // Another input to the same pane changes nothing.
    f.update(move |parts| crate::agents::turn::seen(parts, pane))
        .await;
    assert_eq!(withdrawn(&f), 1);
}

#[tokio::test]
async fn the_setting_turns_off_the_notification_and_keeps_the_row() {
    let (f, run) = started(Options {
        config: Some("[agents]\nturn_end_notice = false\n"),
        ..Options::default()
    })
    .await;
    turn(&f, run).await;
    eventually("the reason", || reasons(&f, run) == ["turn_ended"]).await;
    assert!(waits(&f).await);
    assert!(posts(&f).is_empty(), "no notification");
    assert_eq!(badges(&f).last(), Some(&1), "the badge counts the run");
}

#[tokio::test]
async fn a_run_that_ends_does_not_wait() {
    let (f, run) = started(Options::default()).await;
    turn(&f, run).await;
    eventually("the reason", || !reasons(&f, run).is_empty()).await;
    f.update(move |parts| runs::end(parts, run, Source::Process, Some(0)))
        .await;
    eventually("no reason", || attention(&f, run).is_none()).await;
    assert!(!waits(&f).await);
}
