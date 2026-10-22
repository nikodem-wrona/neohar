use std::sync::atomic::{AtomicU32, Ordering};

use nh_proto::api::projections::Projection;

use super::*;
use crate::projection::ProjectionHub;

fn supervisor() -> (Supervisor, Arc<Health>, Arc<ProjectionHub>) {
    let hub = ProjectionHub::new();
    let health = Health::new(Arc::clone(&hub));
    (
        Supervisor::new(Handle::current(), Arc::clone(&health)),
        health,
        hub,
    )
}

fn warning_ids(health: &Health) -> Vec<String> {
    health.value().warnings.into_iter().map(|w| w.id).collect()
}

#[tokio::test]
async fn a_task_that_returns_is_left_alone() {
    let (supervisor, health, _hub) = supervisor();
    let runs = Arc::new(AtomicU32::new(0));
    let counter = Arc::clone(&runs);
    supervisor
        .spawn(Task::new("quiet").restart(Restart::Times(3)), move || {
            let counter = Arc::clone(&counter);
            async move {
                counter.fetch_add(1, Ordering::SeqCst);
            }
        })
        .await
        .expect("the supervisor ends");
    assert_eq!(runs.load(Ordering::SeqCst), 1);
    assert!(warning_ids(&health).is_empty());
}

#[tokio::test]
async fn a_panic_does_not_reach_the_caller_or_the_runtime() {
    let (supervisor, health, _hub) = supervisor();
    let handle = supervisor.spawn_once("bad", async { panic!("a bug in a task") });
    // The supervisor itself ends normally.
    handle.await.expect("the supervisor does not panic");
    assert_eq!(warning_ids(&health), ["task:bad"]);
    let warning = &health.value().warnings[0];
    assert!(warning.text.contains("a bug in a task"), "{warning:?}");
    // The runtime goes on.
    assert_eq!(tokio::spawn(async { 7 }).await.expect("a task"), 7);
}

#[tokio::test]
async fn a_task_is_started_again_after_a_panic_and_a_good_run_ends_it() {
    let (supervisor, health, _hub) = supervisor();
    let runs = Arc::new(AtomicU32::new(0));
    let counter = Arc::clone(&runs);
    supervisor
        .spawn(Task::new("flaky").restart(Restart::Times(2)), move || {
            let counter = Arc::clone(&counter);
            async move {
                if counter.fetch_add(1, Ordering::SeqCst) < 2 {
                    panic!("not yet");
                }
            }
        })
        .await
        .expect("the supervisor ends");
    assert_eq!(
        runs.load(Ordering::SeqCst),
        3,
        "two restarts, and the third run is good"
    );
    assert!(
        warning_ids(&health).is_empty(),
        "a task that recovered is not a warning"
    );
}

#[tokio::test]
async fn the_supervisor_gives_up_after_the_restarts_and_runs_the_hook() {
    let (supervisor, health, hub) = supervisor();
    let runs = Arc::new(AtomicU32::new(0));
    let counter = Arc::clone(&runs);
    let (told, heard) = tokio::sync::oneshot::channel();
    supervisor
        .spawn(
            Task::new("pane-1")
                .restart(Restart::Times(1))
                .on_giveup(move |text| {
                    let _ = told.send(text.to_owned());
                }),
            move || {
                let counter = Arc::clone(&counter);
                async move {
                    counter.fetch_add(1, Ordering::SeqCst);
                    panic!("always");
                }
            },
        )
        .await
        .expect("the supervisor ends");
    assert_eq!(
        runs.load(Ordering::SeqCst),
        2,
        "the first run and one restart"
    );
    assert_eq!(heard.await.expect("the hook ran"), "always");
    assert_eq!(warning_ids(&health), ["task:pane-1"]);
    hub.flush();
    assert_eq!(
        hub.snapshot(Projection::Health).items["_"]["warnings"][0]["id"],
        "task:pane-1"
    );
}

#[tokio::test]
async fn aborting_the_supervisor_stops_the_task_under_it() {
    let (supervisor, _health, _hub) = supervisor();
    let (alive_tx, mut alive_rx) = tokio::sync::mpsc::unbounded_channel::<()>();
    let handle = supervisor.spawn(Task::new("loop"), move || {
        let alive = alive_tx.clone();
        async move {
            // The sender is dropped when the task is dropped.
            let _alive = alive;
            std::future::pending::<()>().await;
        }
    });
    tokio::task::yield_now().await;
    handle.abort();
    assert!(handle.await.expect_err("aborted").is_cancelled());
    // Every sender is gone only if the child task was dropped.
    let closed = tokio::time::timeout(Duration::from_secs(5), alive_rx.recv())
        .await
        .expect("the child task ended");
    assert!(closed.is_none());
}

#[tokio::test]
async fn the_wait_before_a_restart_grows_and_has_a_limit() {
    let (supervisor, _health, _hub) = supervisor();
    let started = std::time::Instant::now();
    let runs = Arc::new(AtomicU32::new(0));
    let counter = Arc::clone(&runs);
    supervisor
        .spawn(Task::new("slow").restart(Restart::Times(3)), move || {
            let counter = Arc::clone(&counter);
            async move {
                if counter.fetch_add(1, Ordering::SeqCst) < 3 {
                    panic!("again");
                }
            }
        })
        .await
        .expect("the supervisor ends");
    // 100 ms + 200 ms + 400 ms of waiting, with room for a busy machine.
    assert!(
        started.elapsed() >= Duration::from_millis(700),
        "{:?}",
        started.elapsed()
    );
    assert_eq!(runs.load(Ordering::SeqCst), 4);
}

#[tokio::test]
async fn a_run_that_lasted_long_enough_gives_the_task_its_restarts_back() {
    let (supervisor, health, _hub) = supervisor();
    let runs = Arc::new(AtomicU32::new(0));
    let counter = Arc::clone(&runs);
    // One restart is allowed. The task panics four times, but every run lasts 150 ms, and a run
    // of 100 ms counts as good. So it never uses up the restarts, and the fifth run ends well.
    supervisor
        .spawn(
            Task::new("long-lived")
                .restart(Restart::Times(1))
                .stable_after(Duration::from_millis(100)),
            move || {
                let counter = Arc::clone(&counter);
                async move {
                    tokio::time::sleep(Duration::from_millis(150)).await;
                    if counter.fetch_add(1, Ordering::SeqCst) < 4 {
                        panic!("after a good run");
                    }
                }
            },
        )
        .await
        .expect("the supervisor ends");
    assert_eq!(runs.load(Ordering::SeqCst), 5);
    assert!(warning_ids(&health).is_empty());
}
