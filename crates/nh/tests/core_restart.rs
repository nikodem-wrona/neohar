//! The restart flow of `nh-core` against real holders: checkpoints, the replay of the
//! journal, queued queries, the resize nudge, input ids and the reports of lost input.
//!
//! A test "crashes" the core: it stops the runtime and drops its tasks, and the holders keep
//! running. Then it starts a new core over the same directories, as a relaunch of the host does.

mod core_support;

use std::{
    path::Path,
    time::{Duration, Instant},
};

use core_support::{Harness, Messages, TIMEOUT, has_row, next_input_id, wait_for};
use nh_core::{
    Tuning,
    pane::{Liveness, Pane, Recovery, RestoreOutcome, UnconfirmedReason},
};
use nh_proto::{
    holder::{ExitStatus, HolderEvent, HolderRequest, HolderStatus},
    input::{LogicalInput, LogicalText},
};
use nh_store::PaneLiveness;
use rustix::process::{Pid, Signal, kill_process};
use serde_json::json;
use ulid::Ulid;

/// Settings with a short idle time, so that a checkpoint follows output at once.
fn quick_checkpoints() -> Tuning {
    Tuning {
        checkpoint_idle: Duration::from_millis(100),
        ..Tuning::default()
    }
}

fn flag(h: &Harness, name: &str) -> String {
    h.dir.join(name).display().to_string()
}

fn touch(path: &str) {
    std::fs::write(path, b"").expect("the flag file is written");
}

/// Waits until `path` exists. The programs of the tests make these files to say that they got
/// somewhere, because no other part of the test can see it.
async fn wait_file(path: &str) {
    let deadline = Instant::now() + TIMEOUT;
    while !Path::new(path).exists() {
        assert!(Instant::now() < deadline, "waited for {path}");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// Asks a holder for its status while no core is attached. The token is the clock, and the core
/// that starts afterwards takes a later one, so that the holder accepts it.
async fn holder_status(h: &Harness, pane: Ulid) -> HolderStatus {
    holder_status_at(h, pane, 0).await
}

/// Like [`holder_status`], with a token that is `ahead_ms` after the clock.
async fn holder_status_at(h: &Harness, pane: Ulid, ahead_ms: u64) -> HolderStatus {
    let record = h
        .db()
        .pane(pane)
        .expect("a read")
        .expect("the pane is in the database");
    let token = u64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("the clock")
            .as_millis(),
    )
    .expect("a small number")
        + ahead_ms;
    let mut link =
        nh_core::holder::connect(Path::new(&record.socket), record.secret, token, "the-test")
            .await
            .expect("the holder accepts the test");
    link.tx.send(HolderRequest::Status).expect("a request");
    let status = loop {
        match link.events.recv().await.expect("the holder answers") {
            HolderEvent::StatusReply(status) => break status,
            _ => continue,
        }
    };
    link.tx.send(HolderRequest::Detach).expect("a request");
    // The core that starts next must have a later token.
    tokio::time::sleep(Duration::from_millis(5)).await;
    status
}

/// Waits until the holder has journaled more than `offset` bytes.
async fn wait_journal_past(h: &Harness, pane: Ulid, offset: u64) -> HolderStatus {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let status = holder_status(h, pane).await;
        if status.journal.end > offset {
            return status;
        }
        assert!(Instant::now() < deadline, "waited for output past {offset}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// Waits until the pane has read the journal to `offset`, shows no restore any more, and has
/// ended the resize nudge. Before the nudge ends, an application may still redraw, and the grid
/// is one column narrow for a moment.
async fn wait_restored_screen(pane: &Pane, offset: u64) {
    let mut damage = pane.subscribe_damage();
    let deadline = Instant::now() + TIMEOUT;
    loop {
        if !pane.info().restoring && pane.settled() && pane.fed_offset() >= offset {
            return;
        }
        let left = deadline
            .checked_duration_since(Instant::now())
            .expect("the pane reads the journal in time");
        let _ = tokio::time::timeout(left.min(Duration::from_millis(50)), damage.changed()).await;
    }
}

/// What the restart report says about one pane.
struct Report {
    recovery: Recovery,
    snapshot_offset: Option<u64>,
    journal_start: u64,
    journal_end: u64,
    replay_bytes: u64,
}

fn report_of(h: &Harness, id: Ulid) -> Report {
    let report = h.handle.restore_report();
    let entry = report
        .panes
        .iter()
        .find(|p| p.pane == id)
        .unwrap_or_else(|| panic!("no entry for {id} in {report:?}"));
    match &entry.outcome {
        RestoreOutcome::Restored {
            recovery,
            snapshot_offset,
            journal_start,
            journal_end,
            replay_bytes,
            ..
        } => Report {
            recovery: *recovery,
            snapshot_offset: *snapshot_offset,
            journal_start: *journal_start,
            journal_end: *journal_end,
            replay_bytes: *replay_bytes,
        },
        other => panic!("the pane was not restored: {other:?}"),
    }
}

const COLOURS: &str = r"printf '\033[31mred\033[0m \033[1;44mbold on blue\033[0m\r\nsecond line\r\n\033[3;10Hat 3,10\033[?2004h\033[2 q'";

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_restart_gives_the_screen_back_from_a_snapshot() {
    let mut h = Harness::start_tuned(quick_checkpoints());
    let script = format!("{COLOURS}; exec sleep 600");
    let pane = h.spawn(&["/bin/sh", "-c", &script], 40, 8).await;
    wait_for(&pane, "the last line", |t| has_row(t, "second line")).await;
    wait_for(&pane, "the cursor line", |t| {
        t.iter().any(|r| r.ends_with("at 3,10"))
    })
    .await;
    let id = pane.id();
    let (before, offset) = pane.dump();
    let snapshot = h.wait_snapshot(id, offset).await;
    assert_eq!(
        snapshot.offset, offset,
        "the checkpoint is at the end of the output"
    );
    drop(pane);

    h.crash().await;
    h.restart();
    let back = h.restored(id).await;
    wait_restored_screen(&back, offset).await;

    let (after, after_offset) = back.dump();
    assert_eq!(after_offset, offset);
    let diff = after.differences(&before, 8);
    assert!(
        diff.is_empty(),
        "the screen differs after the restart: {diff:?}"
    );
    let report = report_of(&h, id);
    assert_eq!(report.recovery, Recovery::Snapshot);
    assert_eq!(report.snapshot_offset, Some(offset));
    assert_eq!(
        (report.journal_end, report.replay_bytes),
        (offset, 0),
        "there is nothing after the snapshot"
    );
    assert!(!back.info().restoring);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_pane_without_a_snapshot_reads_the_whole_journal() {
    // The default idle time is 2 s, so the crash comes before the first checkpoint.
    let mut h = Harness::start();
    let script = format!("{COLOURS}; exec sleep 600");
    let pane = h.spawn(&["/bin/sh", "-c", &script], 40, 8).await;
    wait_for(&pane, "the cursor line", |t| {
        t.iter().any(|r| r.ends_with("at 3,10"))
    })
    .await;
    let id = pane.id();
    let (before, offset) = pane.dump();
    assert!(h.db().snapshots(id).expect("a read").is_empty());
    drop(pane);

    h.crash().await;
    h.restart();
    let back = h.restored(id).await;
    wait_restored_screen(&back, offset).await;

    let diff = back.dump().0.differences(&before, 8);
    assert!(
        diff.is_empty(),
        "the screen differs after the restart: {diff:?}"
    );
    let report = report_of(&h, id);
    assert_eq!(
        (
            report.recovery,
            report.snapshot_offset,
            report.journal_start
        ),
        (Recovery::Journal, None, 0)
    );
    assert_eq!(report.replay_bytes, offset, "the whole journal is replayed");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn output_that_came_while_no_core_ran_is_replayed_after_the_snapshot() {
    let mut h = Harness::start_tuned(quick_checkpoints());
    let go = flag(&h, "go");
    let done = flag(&h, "done");
    let script = format!(
        "printf 'before\\r\\n'; while [ ! -e {go} ]; do sleep 0.02; done; \
         printf '\\033[1mafter\\033[0m\\r\\n'; : > {done}; exec sleep 600"
    );
    let pane = h.spawn(&["/bin/sh", "-c", &script], 40, 8).await;
    wait_for(&pane, "the first line", |t| has_row(t, "before")).await;
    let id = pane.id();
    let snapshot = h.wait_snapshot(id, pane.fed_offset()).await;
    drop(pane);

    h.crash().await;
    touch(&go);
    wait_file(&done).await;
    let status = wait_journal_past(&h, id, snapshot.offset).await;
    h.restart();
    let back = h.restored(id).await;
    wait_restored_screen(&back, status.journal.end).await;

    let text = back.text();
    assert!(
        has_row(&text, "before") && has_row(&text, "after"),
        "{text:?}"
    );
    let report = report_of(&h, id);
    assert_eq!(
        (report.recovery, report.snapshot_offset),
        (Recovery::Snapshot, Some(snapshot.offset))
    );
    assert_eq!(
        report.replay_bytes,
        report.journal_end - snapshot.offset,
        "only the bytes after the snapshot"
    );
    assert!(report.replay_bytes > 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_title_comes_back_and_a_new_core_has_a_higher_token() {
    let mut h = Harness::start();
    let script = "printf '\\033]0;first\\007\\033]0;second\\007x'; exec sleep 600";
    let pane = h.spawn(&["/bin/sh", "-c", script], 30, 5).await;
    wait_for(&pane, "the text", |t| has_row(t, "x")).await;
    let id = pane.id();
    let token_before = h
        .db()
        .pane(id)
        .expect("a read")
        .expect("the pane")
        .fencing_token;
    drop(pane);
    h.crash().await;
    h.restart();
    let back = h.restored(id).await;
    assert_eq!(back.info().title.as_deref(), Some("second"));
    let token_after = h
        .db()
        .pane(id)
        .expect("a read")
        .expect("the pane")
        .fencing_token;
    assert!(
        token_after > token_before,
        "{token_after} must be above {token_before}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_replay_does_not_answer_a_query_that_the_first_core_answered() {
    let mut h = Harness::start();
    // The program asks for the device attributes. The tty echoes each byte that comes back as
    // input, so the answer of the core shows on the screen as `^[[?…c`. The first core answers
    // once. If the replay answered again, the screen would show a second copy.
    let script = "printf 'ready\\033[c'; exec sleep 600";
    let pane = h.spawn(&["/bin/sh", "-c", script], 60, 5).await;
    wait_for(&pane, "the echo of the answer", |t| {
        t.join("\n").contains("^[[?")
    })
    .await;
    let id = pane.id();
    let offset = pane.fed_offset();
    drop(pane);
    h.crash().await;
    h.restart();
    let back = h.restored(id).await;
    wait_restored_screen(&back, offset).await;
    // A second answer would travel through the holder, the tty and the echo. Give it the time
    // that the nudge takes, and more.
    tokio::time::sleep(Duration::from_millis(400)).await;
    let text = back.text().join("\n");
    assert_eq!(text.matches("^[[").count(), 1, "{text:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_new_pane_does_not_answer_a_query_that_its_holder_answered_before_the_core_attached() {
    // A fast child prints a query before the core has connected. The holder answers the queries
    // that need no screen while no core is attached. The core then reads the same bytes from the
    // start of the journal, and it must not answer them a second time. The attach delay makes the
    // window wide, so that the test does not depend on the speed of the child.
    let tuning = Tuning {
        attach_delay: Duration::from_millis(400),
        ..Tuning::default()
    };
    let h = Harness::start_tuned(tuning);
    let script = "printf 'ready\\033[c'; exec sleep 600";
    let pane = h.spawn(&["/bin/sh", "-c", script], 60, 5).await;
    wait_for(&pane, "the echo of the answer", |t| {
        t.join("\n").contains("^[[?")
    })
    .await;
    // A second answer would travel through the holder, the tty and the echo.
    tokio::time::sleep(Duration::from_millis(300)).await;
    let text = pane.text().join("\n");
    assert_eq!(text.matches("^[[").count(), 1, "{text:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_new_pane_gets_the_answer_to_a_screen_query_that_came_before_the_core_attached() {
    // The holder cannot answer a cursor query: it needs the screen. It queues the query, and the
    // core answers it from the replay of the bytes before the attach.
    let tuning = Tuning {
        attach_delay: Duration::from_millis(400),
        ..Tuning::default()
    };
    let h = Harness::start_tuned(tuning);
    let out = flag(&h, "reply");
    let script = format!(
        "stty -icanon -echo; printf 'ready\\033[6n'; IFS= read -r -d R reply; \
         printf %s \"$reply\" > {out}; exec sleep 600"
    );
    let _pane = h
        .spawn(&["/usr/bin/env", "bash", "-c", &script], 40, 8)
        .await;
    wait_file(&out).await;
    // "ready" is 5 cells wide, so the cursor stands in column 6 of row 1.
    assert_eq!(
        std::fs::read_to_string(&out).expect("the reply"),
        "\x1b[1;6"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_new_pane_that_replays_its_first_bytes_takes_no_checkpoint_by_itself() {
    // The child writes before the core attaches, so the core reads the first bytes as a replay.
    // That replay is not a restart. The holder has not asked for a checkpoint (it asks after 2 s of
    // quiet), so there is none.
    let tuning = Tuning {
        attach_delay: Duration::from_millis(300),
        ..Tuning::default()
    };
    let h = Harness::start_tuned(tuning);
    let pane = h
        .spawn(&["/bin/sh", "-c", "printf early; exec sleep 600"], 40, 5)
        .await;
    wait_for(&pane, "the early text", |t| has_row(t, "early")).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(h.db().snapshots(pane.id()).expect("a read").is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_new_core_answers_the_queries_that_come_after_the_restart() {
    // The replies of a core to the program have ids of their own. The holder remembers the last
    // 4,096 ids across cores and drops a repeat. If the new core counted from zero again, its
    // first answer would have the id of the first answer of the old core, and the holder would
    // drop it.
    let mut h = Harness::start();
    let go = flag(&h, "go");
    let script = format!(
        "printf 'ready\\033[c'; while [ ! -e {go} ]; do sleep 0.02; done; printf '\\033[c'; exec sleep 600"
    );
    let pane = h.spawn(&["/bin/sh", "-c", &script], 60, 5).await;
    wait_for(&pane, "the first answer", |t| t.join("\n").contains("^[[?")).await;
    let id = pane.id();
    drop(pane);
    h.crash().await;
    h.restart();
    let back = h.restored(id).await;
    touch(&go);
    wait_for(&back, "the second answer", |t| {
        t.join("\n").matches("^[[").count() == 2
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_debug_methods_show_the_screen_the_inputs_and_the_restart() {
    let h = Harness::start();
    let pane = h
        .spawn(&["/bin/sh", "-c", "printf hello; exec cat"], 40, 5)
        .await;
    wait_for(&pane, "hello", |t| has_row(t, "hello")).await;
    let id = pane.id().to_string();

    let screen = h.call("debug.screen", json!({ "pane": id })).await;
    let (dump, offset) = pane.dump();
    assert_eq!(screen["fed_offset"], offset);
    assert_eq!(screen["restoring"], false);
    assert_eq!(screen["recovery"], "fresh");
    let shown: nh_term::ScreenDump =
        serde_json::from_value(screen["dump"].clone()).expect("the dump is a ScreenDump");
    assert_eq!(shown, dump);

    let input_id = next_input_id();
    send_text(&h, &pane, input_id, "x").await;
    wait_acked(&pane, input_id).await;
    let inputs = h.call("debug.pane_inputs", json!({ "pane": id })).await;
    assert_eq!(inputs["acked"], json!([input_id]));
    assert_eq!(inputs["pending"], 0);
    assert_eq!(inputs["unconfirmed"], json!([]));
    let next = inputs["next"].as_u64().expect("a number");
    let again = h
        .call("debug.pane_inputs", json!({ "pane": id, "since": next }))
        .await;
    assert_eq!(
        again["acked"],
        json!([]),
        "a client that passes `next` gets each ack once"
    );

    let report = h.call("debug.restore_report", json!({})).await;
    assert_eq!(report["done"], true);
    assert_eq!(report["panes"], json!([]));
}

/// The program of the query tests: it sends a cursor position query when the flag file `go`
/// appears, says so in the file `asked`, reads the reply and writes it (without the final `R`) to
/// the file `out`.
///
/// The tests do not look at the holder while the query is on its way. A client that is attached
/// to the holder, also a test, makes the holder believe that a core answers the query, and then
/// the holder queues nothing.
fn dsr_script(go: &str, asked: &str, out: &str) -> String {
    format!(
        "stty -icanon -echo; printf ready; while [ ! -e {go} ]; do sleep 0.02; done; \
         printf '\\033[6n'; : > {asked}; IFS= read -r -d R reply; printf %s \"$reply\" > {out}; \
         exec sleep 600"
    )
}

/// The time that the holder needs to read what a program wrote a moment ago. No file or event
/// tells a test that the holder has read it.
const HOLDER_READS: Duration = Duration::from_millis(250);

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_query_that_the_holder_queued_is_answered_after_the_restart() {
    let mut h = Harness::start();
    let (go, asked, out) = (flag(&h, "go"), flag(&h, "asked"), flag(&h, "reply"));
    let script = dsr_script(&go, &asked, &out);
    let pane = h
        .spawn(&["/usr/bin/env", "bash", "-c", &script], 40, 8)
        .await;
    wait_for(&pane, "ready", |t| has_row(t, "ready")).await;
    let id = pane.id();
    let before = pane.fed_offset();
    drop(pane);

    h.crash().await;
    // The program asks while no core is attached. The holder cannot answer: it does not know
    // where the cursor is. It queues the query.
    touch(&go);
    wait_file(&asked).await;
    tokio::time::sleep(HOLDER_READS).await;
    h.restart();
    let back = h.restored(id).await;
    wait_file(&out).await;
    // "ready" is 5 cells wide, so the cursor stands in column 6 of row 1.
    assert_eq!(
        std::fs::read_to_string(&out).expect("the reply"),
        "\x1b[1;6"
    );
    let report = report_of(&h, id);
    assert!(
        report.journal_end > before,
        "the query was in the journal that the new core replayed"
    );
    wait_restored_screen(&back, report.journal_end).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_query_that_is_too_old_gets_no_answer() {
    let tuning = Tuning {
        query_fresh: Duration::from_millis(200),
        ..Tuning::default()
    };
    let mut h = Harness::start_tuned(tuning.clone());
    let (go, asked, out) = (flag(&h, "go"), flag(&h, "asked"), flag(&h, "reply"));
    let script = dsr_script(&go, &asked, &out);
    let pane = h
        .spawn(&["/usr/bin/env", "bash", "-c", &script], 40, 8)
        .await;
    wait_for(&pane, "ready", |t| has_row(t, "ready")).await;
    let id = pane.id();
    drop(pane);

    h.crash().await;
    touch(&go);
    wait_file(&asked).await;
    // Let the query go stale before the new core looks at it. The holder drops a query that is
    // older than `query_fresh` when it reports its status.
    tokio::time::sleep(Duration::from_millis(600)).await;
    h.restart_tuned(tuning);
    let back = h.restored(id).await;
    wait_restored_screen(&back, report_of(&h, id).journal_end).await;
    // Give the program time to read a reply that must not come.
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(
        !Path::new(&out).exists(),
        "the program got an answer to a stale query"
    );
}

async fn send_text(h: &Harness, pane: &Pane, input_id: u64, text: &str) {
    h.call(
        "pane.input",
        json!({
            "pane": pane.id().to_string(),
            "input_id": input_id,
            "event": serde_json::to_value(LogicalInput::Text(LogicalText { text: text.into() }))
                .expect("the input serializes"),
        }),
    )
    .await;
}

async fn wait_acked(pane: &Pane, input_id: u64) {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        if pane.acked_inputs(0).0.contains(&input_id) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "waited for the ack of input {input_id}"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_input_that_is_sent_again_with_its_id_after_a_restart_is_written_once() {
    let mut h = Harness::start();
    let log = flag(&h, "log");
    let script = format!("stty -echo; : > {log}; printf ready; exec cat >> {log}");
    let pane = h.spawn(&["/bin/sh", "-c", &script], 40, 8).await;
    wait_for(&pane, "ready", |t| has_row(t, "ready")).await;
    let id = pane.id();
    let input_id = next_input_id();
    send_text(&h, &pane, input_id, "once\n").await;
    wait_acked(&pane, input_id).await;
    drop(pane);

    h.crash().await;
    h.restart();
    let back = h.restored(id).await;
    // The client does not know that the ack came. It sends the input again with the same id.
    send_text(&h, &back, input_id, "once\n").await;
    wait_acked(&back, input_id).await;
    // A different input after it shows that `cat` has had time to copy the first.
    let marker = next_input_id();
    send_text(&h, &back, marker, "marker\n").await;
    wait_acked(&back, marker).await;
    let deadline = Instant::now() + TIMEOUT;
    while !std::fs::read_to_string(&log).is_ok_and(|text| text.contains("marker")) {
        assert!(
            Instant::now() < deadline,
            "waited for the marker in the log"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(
        std::fs::read_to_string(&log).expect("the log"),
        "once\nmarker\n"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_input_without_an_ack_is_reported_as_not_written() {
    let tuning = Tuning {
        input_ack_timeout: Duration::from_millis(300),
        ..Tuning::default()
    };
    let h = Harness::start_tuned(tuning);
    let pane = h
        .spawn(&["/bin/sh", "-c", "printf up; exec cat"], 40, 8)
        .await;
    wait_for(&pane, "up", |t| has_row(t, "up")).await;
    // A stopped holder reads its socket never, so the ack cannot come.
    let holder = Pid::from_raw(pane.holder_pid()).expect("a pid");
    kill_process(holder, Signal::STOP).expect("the holder stops");
    let input_id = next_input_id();
    send_text(&h, &pane, input_id, "x").await;
    let deadline = Instant::now() + TIMEOUT;
    while pane.info().input_unconfirmed.is_empty() {
        assert!(Instant::now() < deadline, "waited for the report");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let unconfirmed = pane.info().input_unconfirmed;
    assert_eq!(unconfirmed.len(), 1);
    assert_eq!(
        (unconfirmed[0].input_id, unconfirmed[0].reason),
        (input_id, UnconfirmedReason::NotWritten)
    );
    assert_eq!(pane.pending_inputs(), 0);
    kill_process(holder, Signal::CONT).expect("the holder runs again");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn input_that_the_lost_connection_leaves_open_is_reported_as_holder_lost() {
    let h = Harness::start();
    let pane = h
        .spawn(&["/bin/sh", "-c", "printf up; exec cat"], 40, 8)
        .await;
    wait_for(&pane, "up", |t| has_row(t, "up")).await;
    let holder = Pid::from_raw(pane.holder_pid()).expect("a pid");
    kill_process(holder, Signal::STOP).expect("the holder stops");
    let input_id = next_input_id();
    send_text(&h, &pane, input_id, "x").await;
    kill_process(holder, Signal::KILL).expect("the holder dies");
    let deadline = Instant::now() + TIMEOUT;
    while pane.info().liveness != Liveness::HolderLost {
        assert!(
            Instant::now() < deadline,
            "waited for the loss of the holder"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let unconfirmed = pane.info().input_unconfirmed;
    assert_eq!(unconfirmed.len(), 1);
    assert_eq!(
        (unconfirmed[0].input_id, unconfirmed[0].reason),
        (input_id, UnconfirmedReason::HolderLost)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_quit_stores_a_checkpoint_and_the_next_core_replays_nothing() {
    // The idle time is long: only the quit makes the checkpoint.
    let mut h = Harness::start();
    let script = format!("{COLOURS}; exec sleep 600");
    let pane = h.spawn(&["/bin/sh", "-c", &script], 40, 8).await;
    wait_for(&pane, "the cursor line", |t| {
        t.iter().any(|r| r.ends_with("at 3,10"))
    })
    .await;
    let id = pane.id();
    let (before, offset) = pane.dump();
    assert!(h.db().snapshots(id).expect("a read").is_empty());

    let report = h.handle.quit().await;
    assert_eq!((report.checkpointed, report.without_checkpoint), (1, 0));
    assert_eq!(h.db().snapshots(id).expect("a read")[0].offset, offset);
    drop(pane);
    h.crash().await;
    h.restart();

    let back = h.restored(id).await;
    wait_restored_screen(&back, offset).await;
    assert!(back.dump().0.differences(&before, 8).is_empty());
    let report = report_of(&h, id);
    assert_eq!(
        (
            report.recovery,
            report.snapshot_offset,
            report.journal_end,
            report.replay_bytes
        ),
        (Recovery::Snapshot, Some(offset), offset, 0)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_pane_whose_holder_is_gone_is_marked_lost_and_not_restored() {
    let mut h = Harness::start();
    let pane = h
        .spawn(&["/bin/sh", "-c", "printf up; exec sleep 600"], 40, 8)
        .await;
    wait_for(&pane, "up", |t| has_row(t, "up")).await;
    let id = pane.id();
    let holder = Pid::from_raw(pane.holder_pid()).expect("a pid");
    drop(pane);
    h.crash().await;
    kill_process(holder, Signal::KILL).expect("the holder dies");
    // A kill takes a moment. A connection to a holder that is still dying can succeed and then
    // end, and that is no proof that the holder is gone. Wait until the process is.
    let deadline = Instant::now() + TIMEOUT;
    while rustix::process::test_kill_process(holder).is_ok() {
        assert!(Instant::now() < deadline, "the holder does not die");
        tokio::time::sleep(Duration::from_millis(5)).await;
    }

    h.restart();
    timeout_wait(&h).await;
    assert!(h.handle.pane(id).is_none());
    let report = h.handle.restore_report();
    assert!(
        matches!(&report.panes[..], [p] if p.pane == id && matches!(p.outcome, RestoreOutcome::Lost { .. })),
        "{report:?}"
    );
    assert_eq!(
        h.db().pane(id).expect("a read").expect("the pane").liveness,
        PaneLiveness::Lost
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_pane_that_another_core_holds_is_reported_and_not_marked_lost() {
    let mut h = Harness::start();
    let pane = h
        .spawn(&["/bin/sh", "-c", "printf up; exec sleep 600"], 40, 8)
        .await;
    wait_for(&pane, "up", |t| has_row(t, "up")).await;
    let id = pane.id();
    drop(pane);
    h.crash().await;
    // Another core, one hour ahead on the clock, attaches and leaves. Its token is the highest the
    // holder has seen.
    holder_status_at(&h, id, 3_600_000).await;

    h.restart();
    timeout_wait(&h).await;
    assert!(h.handle.pane(id).is_none());
    let report = h.handle.restore_report();
    assert!(
        matches!(&report.panes[..], [p] if matches!(&p.outcome, RestoreOutcome::Lost { reason } if reason.contains("StaleFence"))),
        "{report:?}"
    );
    assert_eq!(
        h.db().pane(id).expect("a read").expect("the pane").liveness,
        PaneLiveness::Running,
        "the pane belongs to the other core, so the row stays"
    );
}

async fn timeout_wait(h: &Harness) {
    tokio::time::timeout(TIMEOUT, h.handle.wait_restored())
        .await
        .expect("the restart ends in time");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_ring_that_moved_past_every_snapshot_gives_ring_only() {
    // The ring holds 8 KiB. The program prints 100 KB while no core runs.
    let tuning = Tuning {
        ring_bytes: 8 << 10,
        checkpoint_idle: Duration::from_millis(100),
        ..Tuning::default()
    };
    let mut h = Harness::start_tuned(tuning.clone());
    let go = flag(&h, "go");
    let done = flag(&h, "done");
    let script = format!(
        "printf 'start\\r\\n'; while [ ! -e {go} ]; do sleep 0.02; done; \
         i=0; while [ $i -lt 4000 ]; do printf 'line %05d of the long output\\r\\n' $i; i=$((i+1)); done; \
         printf 'end\\r\\n'; : > {done}; exec sleep 600"
    );
    let pane = h.spawn(&["/bin/sh", "-c", &script], 50, 10).await;
    wait_for(&pane, "start", |t| has_row(t, "start")).await;
    let id = pane.id();
    let snapshot = h.wait_snapshot(id, pane.fed_offset()).await;
    drop(pane);

    h.crash().await;
    touch(&go);
    wait_file(&done).await;
    let status = wait_journal_past(&h, id, snapshot.offset + 50_000).await;
    assert!(
        status.journal.start > snapshot.offset,
        "the ring moved past the snapshot"
    );
    h.restart_tuned(tuning);
    let back = h.restored(id).await;
    wait_restored_screen(&back, status.journal.end).await;

    let report = report_of(&h, id);
    assert_eq!(
        (report.recovery, report.snapshot_offset),
        (Recovery::RingOnly, None)
    );
    assert_eq!(report.journal_start, status.journal.start);
    assert_eq!(back.info().recovery, Recovery::RingOnly);
    wait_for(&back, "the end of the output", |t| has_row(t, "end")).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_child_that_exited_while_no_core_ran_is_found_exited_and_stored() {
    let mut h = Harness::start_tuned(quick_checkpoints());
    let go = flag(&h, "go");
    let done = flag(&h, "done");
    let script = format!(
        "printf 'working\\r\\n'; while [ ! -e {go} ]; do sleep 0.02; done; \
         printf 'bye\\r\\n'; : > {done}; exit 3"
    );
    let pane = h.spawn(&["/bin/sh", "-c", &script], 40, 8).await;
    wait_for(&pane, "working", |t| has_row(t, "working")).await;
    let id = pane.id();
    h.wait_snapshot(id, pane.fed_offset()).await;
    let holder = Pid::from_raw(pane.holder_pid()).expect("a pid");
    drop(pane);

    h.crash().await;
    touch(&go);
    wait_file(&done).await;
    // The holder keeps the journal and the exit status until a core acknowledges them.
    let deadline = Instant::now() + TIMEOUT;
    loop {
        if holder_status(&h, id).await.exited.is_some() {
            break;
        }
        assert!(Instant::now() < deadline, "waited for the exit");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    h.restart();
    let back = h.restored(id).await;
    let deadline = Instant::now() + TIMEOUT;
    while back.info().liveness != Liveness::Exited(ExitStatus::Code(3)) {
        assert!(
            Instant::now() < deadline,
            "waited for the exit status: {:?}",
            back.info()
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(has_row(&back.text(), "bye"));
    let stored = h.db().pane(id).expect("a read").expect("the pane");
    assert_eq!(
        (stored.liveness, stored.exit),
        (PaneLiveness::Exited, Some(ExitStatus::Code(3)))
    );
    // The core acknowledged the exit, so the holder leaves.
    let deadline = Instant::now() + TIMEOUT;
    while rustix::process::test_kill_process(holder).is_ok() {
        assert!(
            Instant::now() < deadline,
            "the holder did not leave after the ack"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_nudge_resizes_the_pty_twice_and_ends_at_the_old_size() {
    // The program sees the narrow size only if it asks while the size lasts. Each loop of the
    // program starts processes, and on a busy machine a loop takes more than the usual gap of
    // 50 ms. The gap of 1 s is shorter than the time in which the pane must settle.
    let tuning = Tuning {
        nudge_gap: Duration::from_secs(1),
        ..Tuning::default()
    };
    let mut h = Harness::start_tuned(tuning);
    // A trap would miss a size: bash runs one trap for two signals that come while `sleep` runs.
    // The program asks the terminal for its size all the time and logs each change.
    let log = flag(&h, "sizes");
    let script = format!(
        "last=; printf ready; while :; do now=$(stty size); \
         if [ \"$now\" != \"$last\" ]; then echo \"$now\" >> {log}; last=$now; fi; sleep 0.01; done"
    );
    let pane = h
        .spawn(&["/usr/bin/env", "bash", "-c", &script], 50, 9)
        .await;
    wait_for(&pane, "ready", |t| has_row(t, "ready")).await;
    let id = pane.id();
    wait_file(&log).await;
    drop(pane);
    h.crash().await;
    h.restart();
    let back = h.restored(id).await;
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let sizes = std::fs::read_to_string(&log).unwrap_or_default();
        if sizes.lines().count() >= 3 {
            assert_eq!(
                sizes, "9 50\n9 49\n9 50\n",
                "the size at the start, narrower by one column, then the old size"
            );
            break;
        }
        assert!(
            Instant::now() < deadline,
            "waited for the nudge, got {sizes:?}"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(back.size(), (50, 9));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_first_checkpoint_after_a_restart_covers_the_replay() {
    // The program prints a line every 50 ms for ever, so the pane is never idle. The holder asks
    // for a checkpoint every 300 ms. A new core must keep that request until it has caught up
    // with the journal, and then take a snapshot, or the replay grows with each restart.
    let tuning = Tuning {
        checkpoint_interval: Duration::from_millis(300),
        ..Tuning::default()
    };
    let mut h = Harness::start_tuned(tuning.clone());
    let script = "i=0; while :; do printf 'line %d\\r\\n' $i; i=$((i+1)); sleep 0.05; done";
    let pane = h.spawn(&["/bin/sh", "-c", script], 40, 8).await;
    wait_for(&pane, "some lines", |t| {
        t.iter().any(|r| r.starts_with("line 10"))
    })
    .await;
    let id = pane.id();
    let first = h.wait_snapshot(id, 1).await;
    drop(pane);
    h.crash().await;
    // The program goes on while no core runs.
    tokio::time::sleep(Duration::from_millis(600)).await;
    h.restart_tuned(tuning);
    let back = h.restored(id).await;
    wait_restored_screen(&back, 0).await;
    let end = report_of(&h, id).journal_end;
    assert!(end > first.offset, "output came while no core ran");
    // A snapshot at the end of the replay or later: the checkpoint that the holder asked for. The
    // holder asks again after 10 s when its request is lost, which this limit does not allow.
    let newer = h
        .wait_snapshot_within(id, end, Duration::from_secs(4))
        .await
        .expect("the new core takes a checkpoint soon after the replay");
    assert!(newer.offset >= end);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn restored_panes_come_back_in_the_order_of_their_render_ids() {
    // The restore connects to all holders at once, and they answer in any order. The `panes`
    // projection, and so the layout of a window, must not depend on that order.
    let mut h = Harness::start();
    let mut ids = Vec::new();
    for _ in 0..16 {
        let pane = h
            .spawn(&["/bin/sh", "-c", "printf up; exec sleep 600"], 30, 5)
            .await;
        ids.push(pane.id().to_string());
    }
    h.crash().await;
    h.restart();
    timeout_wait(&h).await;
    let mut panes = Messages::subscribe(&h.handle, "panes");
    let snapshot = panes
        .wait_for("the snapshot", |m| m["type"] == "snapshot")
        .await;
    let order: Vec<String> = snapshot["data"]["order"]
        .as_array()
        .expect("an order")
        .iter()
        .map(|v| v.as_str().expect("an id").to_owned())
        .collect();
    assert_eq!(
        order, ids,
        "the order of the render ids, which is the order of the spawns"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_checkpoint_in_the_database_restores_to_the_screen_that_the_pane_shows() {
    let h = Harness::start_tuned(quick_checkpoints());
    let script = format!("{COLOURS}; exec sleep 600");
    let pane = h.spawn(&["/bin/sh", "-c", &script], 40, 8).await;
    wait_for(&pane, "the cursor line", |t| {
        t.iter().any(|r| r.ends_with("at 3,10"))
    })
    .await;
    let (shown, offset) = pane.dump();
    let snapshot = h.wait_snapshot(pane.id(), offset).await;
    let decoded = nh_term::Snapshot::decode(&snapshot.data).expect("the snapshot decodes");
    assert_eq!(decoded.journal_offset, offset);
    let config = nh_term::EngineConfig {
        history: nh_term::MIN_HISTORY,
        theme: nh_term::xterm_palette(),
        cell_px: (0, 0),
    };
    let engine = <nh_term::AlacrittyEngine as nh_term::VtEngine>::restore(&decoded, &config)
        .expect("the snapshot restores");
    let restored = nh_term::ScreenDump::of(&engine, &nh_term::xterm_palette());
    let diff = restored.differences(&shown, 8);
    assert!(diff.is_empty(), "{diff:?}");
}

/// The guard of a test kills a holder that no core knows, and nothing makes its directory again.
///
/// The core crashes while it waits to attach to a new holder (`attach_delay`). The core never
/// stored that holder, so neither the panes of the core nor the database name it, and the holder
/// waits for a core for ever. Holders of this kind stayed after `mise run test` and kept their
/// shells: the guard must find them by the directory of the test.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_guard_kills_a_holder_whose_start_a_crash_cut() {
    let mut h = Harness::start_tuned(Tuning {
        attach_delay: Duration::from_secs(600),
        ..Tuning::default()
    });
    let made = h.call("workspace.create", json!({ "name": "Cut" })).await;
    let pane = made["pane"]
        .as_str()
        .and_then(|id| id.parse::<nh_proto::api::ids::PaneId>().ok())
        .map(nh_proto::api::ids::PaneId::ulid)
        .expect("a pane id")
        .to_string();
    // The holder accepts connections only after it wrote READY. A connection that it closes (no
    // Hello) shows that the holder now lives on when the core ends, and the core waits to attach.
    let socket = h.dir.join("run/test/holders").join(format!("{pane}.sock"));
    let deadline = Instant::now() + TIMEOUT;
    loop {
        if let Ok(mut stream) = tokio::net::UnixStream::connect(&socket).await {
            let mut rest = Vec::new();
            let closed = tokio::time::timeout(
                TIMEOUT,
                tokio::io::AsyncReadExt::read_to_end(&mut stream, &mut rest),
            )
            .await;
            assert!(closed.is_ok(), "the holder did not close the connection");
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the holder of {pane} did not start"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    h.crash().await;
    let panes = std::collections::HashSet::from([pane.clone()]);
    assert_eq!(
        core_support::reap::running(&panes).len(),
        1,
        "the holder outlives the core that forgot it"
    );
    let dir = h.dir.clone();
    drop(h);
    assert!(
        core_support::reap::running(&panes).is_empty(),
        "the holder of {pane} still runs after the guard"
    );
    assert!(!dir.exists(), "{} is still there", dir.display());
}
