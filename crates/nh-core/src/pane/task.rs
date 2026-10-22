//! The task of a pane: the events of the holder in, the screen out.
//!
//! The holder sends output and markers in the order of its journal, and it cuts an `Output` at
//! the offset of each marker. So the engine is exactly at a marker's offset when the
//! marker arrives. The task applies a resize there, and a `CutPoint` is where a checkpoint can be
//! taken: the parser of the engine is between sequences, so a snapshot there is exact.
//!
//! The task has three more jobs besides the screen:
//!
//! - **Checkpoints.** When the holder asks (`CheckpointWanted`), the task takes a snapshot at the
//!   next cut point, stores it, and tells the holder (`CheckpointTaken`). A quit asks for the same.
//! - **Replay.** A pane that a restart brought back reads the journal after its snapshot with the
//!   side effects off ([`Replay`]). When the replay ends, the task answers the queued queries,
//!   nudges the size of the PTY so that applications redraw, and clears `restoring`.
//! - **Inputs.** The task follows the ids of the inputs that the holder did not acknowledge, and
//!   reports the ones that time out or that the connection loses.

use std::{
    sync::{Arc, atomic::Ordering},
    time::Instant as StdInstant,
};

use nh_proto::holder::{ExitStatus, HolderEvent, HolderRequest, Marker, StreamId};
use nh_store::{ArchivedLine, ArchivedRun, FLUSH_INTERVAL_MS, PaneLiveness, ScrollbackWriter};
use nh_term::{EffectSink, HistoryLine, SnapshotError, VtEngine};
use tokio::{
    sync::{mpsc, oneshot},
    time::{Instant, sleep_until},
};

use super::{
    Control, INTERNAL_INPUT, Liveness, Pane, Recovery, Screen, UnconfirmedReason, lock,
    replay::Replay, restore,
};
use crate::{
    holder::HolderTx,
    persist::{Persist, now_ms},
    runtime::Tuning,
};

/// The most events that one wake-up handles before it publishes the screen. A flood of output
/// then cannot hold a frame back for long.
const BATCH: usize = 64;

/// How long after the replay the pane waits for the markers of the nudge before it calls itself
/// settled anyway. A holder without a terminal (the child exited) does not resize.
const SETTLE_LIMIT: std::time::Duration = std::time::Duration::from_secs(2);

/// How long lines wait in the writer of the archive before they are written.
const FLUSH_INTERVAL: std::time::Duration =
    std::time::Duration::from_millis(FLUSH_INTERVAL_MS as u64);

/// The shortest time between two titles in the `panes` projection.
const TITLE_INTERVAL: std::time::Duration = std::time::Duration::from_millis(250);

/// The shortest time between two bell events of a pane.
const BELL_INTERVAL: std::time::Duration = std::time::Duration::from_secs(2);

/// A pane may raise this many notifications (OSC 9 and 777) in the window below.
const NOTIFY_LIMIT: usize = 3;
const NOTIFY_WINDOW: std::time::Duration = std::time::Duration::from_secs(10);

/// How long the pane waits after a failed write to the archive.
const FLUSH_RETRY: std::time::Duration = std::time::Duration::from_secs(5);

/// A line of the engine as the archive keeps it.
fn archived(line: HistoryLine) -> ArchivedLine {
    ArchivedLine {
        line_no: line.line_no,
        wrapped: line.wrapped,
        text: line.text,
        runs: line
            .runs
            .into_iter()
            .map(|run| ArchivedRun {
                start: run.start,
                len: run.len,
                style: run.style,
            })
            .collect(),
    }
}

/// What the task starts with.
pub(super) struct Init {
    /// Events that the restart flow read before it asked for the journal. The task handles them
    /// first.
    pub backlog: Vec<HolderEvent>,
    /// The replay of a pane that a restart brought back. `None` for a new pane, and for a
    /// restored pane that has nothing to replay.
    pub replay: Option<Replay>,
    /// The journal offset of the newest snapshot in the database.
    pub last_checkpoint: u64,
    /// Set for a restored pane. The task reports to it when the replay ends.
    pub restore: Option<restore::Progress>,
    /// Whether the engine starts at a cut point. A pane that starts from the ring alone does not
    /// know.
    pub at_cut: bool,
    /// Whether to nudge the size of the PTY when the restore ends. A pane whose child exited has
    /// no terminal to resize.
    pub nudge: bool,
}

/// A snapshot that the batch took and that is not stored yet.
struct Taken {
    offset: u64,
    bytes: Vec<u8>,
    size: (u16, u16),
}

/// What a batch of events asks for after the screen is updated.
#[derive(Default)]
struct Outcome {
    fed: bool,
    resized: bool,
    /// The input ids that the holder acknowledged.
    acks: Vec<u64>,
    exited: Option<ExitStatus>,
    /// Bell, notifications and clipboard writes that the batch caused.
    effects: Vec<PaneEffect>,
    checkpoint: Option<Taken>,
    /// The replay reached its end in this batch.
    replay_done: Option<Replay>,
    recovery: Option<Recovery>,
    /// The engine applied the last resize of the nudge.
    settled: bool,
}

/// The state of the checkpoints of a pane.
struct Checkpoints {
    /// The holder asked, or a quit needs one. The next cut point takes it.
    wanted: bool,
    /// The engine is at a cut point: its parser is between sequences.
    at_cut: bool,
    /// The journal offset of the newest stored snapshot.
    last_taken: u64,
}

/// A quit that waits for its checkpoint.
struct Quit {
    /// A snapshot at this offset or later satisfies the quit.
    target: u64,
    deadline: Instant,
    done: Vec<oneshot::Sender<bool>>,
}

struct Task {
    pane: Arc<Pane>,
    persist: Arc<Persist>,
    tuning: Tuning,
    /// The lines that left the screen and that the archive did not take yet.
    history: Vec<HistoryLine>,
    /// The archive of the pane. `None` when the database refused to make a writer, and the
    /// lines are lost then.
    archive: Option<ScrollbackWriter>,
    /// When the archive must flush the lines that wait.
    flush_by: Option<Instant>,
    /// When the pane published a title last.
    title_at: Option<Instant>,
    /// When a title that came too soon is published.
    title_by: Option<Instant>,
    /// When the pane sent the last bell event.
    bell_at: Option<Instant>,
    /// When the pane raised its last notifications, for the limit.
    notified: std::collections::VecDeque<Instant>,
    replay: Option<Replay>,
    restore: Option<restore::Progress>,
    cp: Checkpoints,
    /// Whether the end of a restore nudges the size of the PTY.
    nudge_on_restore: bool,
    /// When to send the second resize of the nudge.
    nudge: Option<Instant>,
    /// The size that the second resize asked for. The pane is settled when the engine has applied
    /// a resize marker with this size.
    nudge_final: Option<(u16, u16)>,
    /// When the pane calls itself settled without that marker.
    settle_by: Option<Instant>,
    quit: Option<Quit>,
    /// The lease is ended. The connection closes soon.
    detached: bool,
    /// Whether the quit that ended the lease got its checkpoint, and who waits for the answer.
    detach_answer: Option<(bool, Vec<oneshot::Sender<bool>>)>,
}

/// Runs until the connection to the holder ends.
pub(super) async fn run(
    pane: Arc<Pane>,
    mut events: mpsc::Receiver<HolderEvent>,
    mut control: mpsc::UnboundedReceiver<Control>,
    init: Init,
    persist: Arc<Persist>,
    tuning: Tuning,
) {
    let archive = {
        let id = pane.id;
        match persist.run(move |store| store.scrollback_writer(id)).await {
            Ok(writer) => Some(writer),
            Err(error) => {
                tracing::warn!(pane = %id, %error, "the scrollback is not archived");
                None
            }
        }
    };
    // The answer that the user asked the pane to remember for OSC 52 writes.
    {
        let id = pane.id;
        match persist.run(move |store| store.clipboard_policy(id)).await {
            Ok(policy) => lock(&pane.clipboard).policy = policy,
            Err(error) => tracing::warn!(pane = %id, %error, "the clipboard answer is not read"),
        }
    }
    let mut task = Task {
        pane,
        persist,
        tuning,
        history: Vec::new(),
        archive,
        flush_by: None,
        title_at: None,
        title_by: None,
        bell_at: None,
        notified: std::collections::VecDeque::new(),
        replay: init.replay,
        restore: init.restore,
        cp: Checkpoints {
            wanted: false,
            at_cut: init.at_cut,
            last_taken: init.last_checkpoint,
        },
        nudge_on_restore: init.nudge,
        nudge: None,
        nudge_final: None,
        settle_by: None,
        quit: None,
        detached: false,
        detach_answer: None,
    };
    if task.restore.is_some() && task.replay.is_none() {
        // The snapshot is at the end of the journal: there is nothing to replay.
        task.complete_restore(None).await;
    }
    let mut batch = init.backlog;
    if !batch.is_empty() {
        task.step(&mut batch).await;
    }
    loop {
        let wake = task.next_wake();
        let first = tokio::select! {
            event = events.recv() => match event {
                Some(event) => event,
                None => break,
            },
            request = control.recv() => {
                if let Some(request) = request {
                    task.control(request).await;
                }
                continue;
            }
            () = sleep_opt(wake) => {
                task.on_timer().await;
                continue;
            }
        };
        batch.push(first);
        while batch.len() < BATCH
            && let Ok(next) = events.try_recv()
        {
            batch.push(next);
        }
        task.step(&mut batch).await;
    }
    task.finish();
}

/// Sleeps until `at`, or for ever when there is no time.
async fn sleep_opt(at: Option<Instant>) {
    match at {
        Some(at) => sleep_until(at).await,
        None => std::future::pending().await,
    }
}

impl Task {
    /// The time of the next thing that needs no event: the end of a synchronized update, an input
    /// that waits too long, the second step of the nudge, or the end of the wait of a quit.
    fn next_wake(&self) -> Option<Instant> {
        let sync = lock(&self.pane.screen)
            .engine
            .sync_deadline()
            .map(Instant::from_std);
        let input = lock(&self.pane.inputs)
            .deadline(self.tuning.input_ack_timeout)
            .map(Instant::from_std);
        [
            sync,
            input,
            self.nudge,
            self.settle_by,
            self.flush_by,
            self.title_by,
            self.quit.as_ref().map(|quit| quit.deadline),
        ]
        .into_iter()
        .flatten()
        .min()
    }

    async fn on_timer(&mut self) {
        let now = Instant::now();
        if lock(&self.pane.screen)
            .engine
            .sync_deadline()
            .is_some_and(|at| Instant::from_std(at) <= now)
        {
            self.flush_sync();
        }
        let expired =
            lock(&self.pane.inputs).expired(now.into_std(), self.tuning.input_ack_timeout);
        self.pane
            .report_unconfirmed(&expired, UnconfirmedReason::NotWritten);
        if self.nudge.is_some_and(|at| at <= now) {
            self.nudge = None;
            self.nudge_final = self.send_resize(0);
        }
        if self.settle_by.is_some_and(|at| at <= now) {
            self.settle();
        }
        if self.flush_by.is_some_and(|at| at <= now) {
            self.flush_archive().await;
        }
        if self.title_by.is_some_and(|at| at <= now) {
            self.publish_title();
        }
        if self.quit.as_ref().is_some_and(|quit| quit.deadline <= now) {
            tracing::warn!(pane = %self.pane.id, "the quit gets no checkpoint in time");
            self.flush_archive().await;
            self.end_lease(false);
        }
    }

    async fn control(&mut self, request: Control) {
        match request {
            Control::Wake => {}
            Control::Flush(done) => {
                self.flush_archive().await;
                let _ = done.send(());
            }
            Control::Quit(done) => {
                if self.detached {
                    let _ = done.send(false);
                    return;
                }
                if let Some(quit) = &mut self.quit {
                    quit.done.push(done);
                    return;
                }
                let target = lock(&self.pane.screen).fed_offset;
                self.quit = Some(Quit {
                    target,
                    deadline: Instant::now() + self.tuning.quit_wait,
                    done: vec![done],
                });
                if self.cp.last_taken >= target {
                    // The newest snapshot is the screen.
                    self.flush_archive().await;
                    self.end_lease(true);
                    return;
                }
                self.cp.wanted = true;
                if self.cp.at_cut {
                    // The engine stands at a cut point already: the last marker was one, and no
                    // output came since.
                    let mut out = Outcome::default();
                    {
                        let pane = Arc::clone(&self.pane);
                        let mut screen = lock(&pane.screen);
                        self.try_checkpoint(&mut screen, &mut out);
                    }
                    self.after(out).await;
                }
            }
        }
    }

    /// Handles a batch of events, then the work that needs to wait.
    async fn step(&mut self, batch: &mut Vec<HolderEvent>) {
        let out = self.handle(batch);
        self.after(out).await;
    }

    /// Feeds a batch of events to the engine and publishes the screen.
    fn handle(&mut self, batch: &mut Vec<HolderEvent>) -> Outcome {
        let pane = Arc::clone(&self.pane);
        let mut out = Outcome::default();
        let mut effects = Vec::new();
        let mut live = Effects {
            tx: &pane.tx,
            ids: &pane.internal_ids,
            later: &mut effects,
        };
        let mut screen = lock(&pane.screen);
        for event in batch.drain(..) {
            match event {
                HolderEvent::Output {
                    offset,
                    stream: StreamId::Pty,
                    bytes,
                } => {
                    match &mut self.replay {
                        Some(replay) => {
                            // Bytes after the end of the replay are new output. Their replies
                            // are wanted, so they are not part of the replay.
                            let split = usize::try_from(replay.end.saturating_sub(offset))
                                .map_or(bytes.len(), |n| n.min(bytes.len()));
                            replay.feed(&mut screen.engine, offset, &bytes[..split], &mut live);
                            screen.engine.advance(&bytes[split..], &mut live);
                        }
                        None => screen.engine.advance(&bytes, &mut live),
                    }
                    screen.fed_offset = offset + bytes.len() as u64;
                    self.cp.at_cut = false;
                    out.fed = true;
                    if self
                        .replay
                        .as_ref()
                        .is_some_and(|replay| screen.fed_offset >= replay.end)
                    {
                        out.replay_done = self.replay.take();
                        // A restored pane has caught up. The next cut point is a good place for
                        // a snapshot: the holder may have asked for one long ago. A new pane
                        // replays only the bytes before its attach, and it asks for nothing.
                        if self.restore.is_some() {
                            self.cp.wanted = true;
                        }
                    }
                }
                HolderEvent::Marker {
                    marker: Marker::Resize { cols, rows },
                    ..
                } => {
                    screen.engine.resize(cols, rows);
                    screen.resized_at = StdInstant::now();
                    out.resized = true;
                    if self.nudge_final == Some((cols, rows)) {
                        out.settled = true;
                    }
                }
                HolderEvent::Marker {
                    offset,
                    marker: Marker::CutPoint,
                } => {
                    // The holder cuts `Output` at the offset of each marker, so the engine is at
                    // `offset` now. A marker of another offset would be a fault of the holder.
                    if offset == screen.fed_offset {
                        self.cp.at_cut = true;
                        // A snapshot in the middle of a replay is stale at once. The pane asks
                        // for one when the replay ends.
                        if self.cp.wanted && self.replay.is_none() {
                            self.try_checkpoint(&mut screen, &mut out);
                        }
                    }
                }
                HolderEvent::CheckpointWanted { .. } => {
                    self.cp.wanted = true;
                    if self.cp.at_cut && self.replay.is_none() {
                        self.try_checkpoint(&mut screen, &mut out);
                    }
                }
                HolderEvent::InputAck { input_id, .. } => out.acks.push(input_id),
                HolderEvent::ChildExited { status } => out.exited = Some(status),
                HolderEvent::ForegroundChanged { pgid } => {
                    pane.foreground.send_replace(Some(pgid));
                }
                HolderEvent::Truncated { requested, start } => {
                    tracing::warn!(pane = %pane.id, requested, start, "the journal no longer holds the offset");
                    if self.replay.is_some() && start > screen.fed_offset {
                        // The ring moved on while the pane restored. The bytes between the
                        // snapshot and `start` are gone, so the screen may show artifacts.
                        screen.fed_offset = start;
                        out.recovery = Some(Recovery::RingOnly);
                    }
                }
                HolderEvent::LeaseLost { by_fencing_token } => {
                    tracing::warn!(pane = %pane.id, by_fencing_token, "another core took the holder");
                }
                // The other events do not change the screen.
                _ => {}
            }
        }
        if out.fed || out.resized {
            self.archive_history(&mut screen.engine);
            publish(&pane, &mut screen, out.resized);
        }
        out.effects = effects;
        out
    }

    /// Carries out the effects of the output. A program can ring the bell or ask for a
    /// notification as fast as it can write, so both have a limit.
    fn carry_out(&mut self, effects: Vec<PaneEffect>) {
        let now = Instant::now();
        for effect in effects {
            match effect {
                PaneEffect::Bell => {
                    if self
                        .bell_at
                        .is_none_or(|at| now.duration_since(at) >= BELL_INTERVAL)
                    {
                        self.bell_at = Some(now);
                        self.pane.bell();
                    }
                }
                PaneEffect::Notify { title, body } => {
                    while self
                        .notified
                        .front()
                        .is_some_and(|at| now.duration_since(*at) >= NOTIFY_WINDOW)
                    {
                        self.notified.pop_front();
                    }
                    if self.notified.len() < NOTIFY_LIMIT {
                        self.notified.push_back(now);
                        self.pane.notify(title, body);
                    }
                }
                PaneEffect::Clipboard { text } => self.pane.clipboard_store(text),
            }
        }
    }

    /// Shows the title of the engine in the `panes` projection, but at most four times a second.
    /// A program that sets its title for each line of output would load every client.
    /// A title that comes too soon waits, and a newer one replaces it.
    fn publish_title(&mut self) {
        let title = lock(&self.pane.screen).engine.title().map(str::to_owned);
        if lock(&self.pane.info).title == title {
            self.title_by = None;
            return;
        }
        let now = Instant::now();
        if let Some(at) = self.title_at
            && now < at + TITLE_INTERVAL
        {
            self.title_by = Some(at + TITLE_INTERVAL);
            return;
        }
        self.title_at = Some(now);
        self.title_by = None;
        self.pane.update_info(|info| {
            info.title = title;
            true
        });
    }

    /// Takes the lines that left the screen and gives them to the archive. The writer drops a
    /// line that it holds already, which is the replay rule of a pane that came back.
    fn archive_history(&mut self, engine: &mut dyn VtEngine) {
        engine.take_new_history(&mut self.history);
        let Some(writer) = &mut self.archive else {
            self.history.clear();
            return;
        };
        if self.history.is_empty() {
            return;
        }
        writer.push(self.history.drain(..).map(archived), now_ms());
        if self.flush_by.is_none() && writer.buffered() > 0 {
            self.flush_by = Some(Instant::now() + FLUSH_INTERVAL);
        }
    }

    /// Writes the lines that wait to the archive. If the write fails, the lines stay and the next
    /// flush tries again after a few seconds.
    async fn flush_archive(&mut self) {
        self.flush_by = None;
        let Some(mut writer) = self.archive.take() else {
            return;
        };
        if writer.buffered() == 0 {
            self.archive = Some(writer);
            return;
        }
        let id = self.pane.id;
        let result = self
            .persist
            .run(move |store| {
                let flushed = writer.flush(store, now_ms());
                Ok((writer, flushed))
            })
            .await;
        match result {
            Ok((writer, flushed)) => {
                if let Err(error) = flushed {
                    tracing::warn!(pane = %id, %error, "the scrollback is not written");
                    self.flush_by = Some(Instant::now() + FLUSH_RETRY);
                }
                self.archive = Some(writer);
            }
            // The state actor ended with the core, and the writer went with the closure.
            Err(error) => tracing::warn!(pane = %id, %error, "the scrollback is not written"),
        }
    }

    /// Takes a checkpoint if the engine is at a cut point that a snapshot does not cover yet.
    fn try_checkpoint(&mut self, screen: &mut Screen, out: &mut Outcome) {
        let offset = screen.fed_offset;
        if offset <= self.cp.last_taken
            || out.checkpoint.as_ref().is_some_and(|t| t.offset >= offset)
        {
            // A snapshot of this screen exists, or this batch took one and waits to store it.
            // The request stays: the holder asked for a snapshot of what comes next, and it asks
            // again only after 10 s. A new core finds this request at its start, before the replay.
            return;
        }
        // The snapshot holds the lines that the pane did not take. Take them first.
        self.archive_history(&mut screen.engine);
        match screen.engine.checkpoint(offset) {
            Ok(snapshot) => {
                self.cp.wanted = false;
                out.checkpoint = Some(Taken {
                    offset,
                    bytes: snapshot.encode(),
                    size: screen.engine.size(),
                });
            }
            // A synchronized update holds output back that is on no screen yet. The next cut
            // point is a later try.
            Err(SnapshotError::SyncOpen) => {}
            Err(error) => {
                self.cp.wanted = false;
                tracing::warn!(pane = %self.pane.id, %error, "no checkpoint");
            }
        }
    }

    /// The work after a batch that cannot hold the lock of the screen: the disk, the holder, and
    /// the facts of the pane.
    async fn after(&mut self, out: Outcome) {
        let pane = Arc::clone(&self.pane);
        self.carry_out(out.effects);
        if self
            .archive
            .as_ref()
            .is_some_and(|writer| writer.flush_due(now_ms()))
        {
            self.flush_archive().await;
        }
        for input_id in out.acks {
            lock(&pane.inputs).acked(input_id);
        }
        self.publish_title();
        let size = lock(&pane.screen).engine.size();
        pane.update_info(|info| {
            let mut changed = false;
            if (info.cols, info.rows) != size {
                (info.cols, info.rows) = size;
                changed = true;
            }
            if let Some(status) = out.exited
                && info.liveness != Liveness::Exited(status)
            {
                info.liveness = Liveness::Exited(status);
                changed = true;
            }
            if let Some(recovery) = out.recovery
                && info.recovery != recovery
            {
                info.recovery = recovery;
                changed = true;
            }
            changed
        });

        if let Some(taken) = out.checkpoint {
            self.store_checkpoint(taken).await;
        }
        if let Some(status) = out.exited {
            // The last lines of the program are in the archive before the holder may go.
            self.flush_archive().await;
            // Persist the exit before the holder may go ("Exit").
            let id = pane.id;
            let stored = self
                .persist
                .run(move |store| {
                    store.set_liveness(id, PaneLiveness::Exited, Some(status), now_ms())
                })
                .await;
            if let Err(error) = stored {
                tracing::error!(pane = %id, %error, "the exit is not stored");
            }
            // All the output of the child came before `ChildExited`, so the holder can go.
            let _ = pane.tx.send(HolderRequest::ExitAck);
        }
        if out.settled {
            self.settle();
        }
        if let Some(replay) = out.replay_done {
            self.complete_restore(Some(replay)).await;
        }
        if let Some(quit) = &self.quit
            && self.cp.last_taken >= quit.target
        {
            self.flush_archive().await;
            self.end_lease(true);
        }
    }

    /// Stores a snapshot and tells the holder that the journal before it is covered.
    async fn store_checkpoint(&mut self, taken: Taken) {
        let id = self.pane.id;
        let Taken {
            offset,
            bytes,
            size,
        } = taken;
        let started = StdInstant::now();
        // The snapshot says that the lines up to its offset were taken. The archive must hold
        // them before the snapshot is stored, or a crash would lose lines that no replay brings
        // back.
        self.flush_archive().await;
        let stored = self
            .persist
            .run(move |store| store.save_snapshot(id, offset, &bytes, size, now_ms()))
            .await;
        match stored {
            Ok(()) => {
                self.cp.last_taken = offset;
                let _ = self.pane.tx.send(HolderRequest::CheckpointTaken { offset });
                tracing::debug!(pane = %id, offset, store_us = started.elapsed().as_micros() as u64, "checkpoint");
            }
            // The holder asks again after 10 s.
            Err(error) => {
                tracing::error!(pane = %id, offset, %error, "the checkpoint is not stored")
            }
        }
    }

    /// The replay ended, or there was none: answer the queued queries, nudge the size, show the
    /// pane ("Restart flow").
    async fn complete_restore(&mut self, replay: Option<Replay>) {
        let pane = Arc::clone(&self.pane);
        let mut replayed = 0;
        if let Some(replay) = replay {
            replayed = replay.bytes;
            for bytes in replay.replies(now_ms()) {
                let input_id = INTERNAL_INPUT | pane.internal_ids.fetch_add(1, Ordering::Relaxed);
                let _ = pane.tx.send(HolderRequest::Input { input_id, bytes });
            }
            if let Some(up_to_seq) = replay.up_to_seq() {
                let _ = pane.tx.send(HolderRequest::QueriesAnswered { up_to_seq });
            }
        }
        #[cfg(feature = "debug")]
        if self.restore.is_some() {
            let dump = pane.dump();
            *lock(&pane.restored_dump) = Some((dump.1, dump.0));
        }
        let exited = matches!(pane.info().liveness, Liveness::Exited(_));
        if self.nudge_on_restore && !exited && self.send_resize(1).is_some() {
            self.nudge = Some(Instant::now() + self.tuning.nudge_gap);
            self.settle_by = Some(Instant::now() + SETTLE_LIMIT);
        } else {
            self.settle();
        }
        pane.update_info(|info| {
            info.restoring = false;
            true
        });
        if let Some(progress) = self.restore.take() {
            progress.done(&pane, replayed);
        }
    }

    /// Asks the holder for the size that the pane has, narrowed by `narrow` columns. The resize
    /// nudge is two of these: the second one restores the size.
    ///
    /// It bypasses [`Pane::resize`], which stops requests for a size that was asked already. The
    /// size comes from `asked_size`, so a window that resized in between wins.
    ///
    /// Returns the size that it asked for, or `None` when the pane is too narrow to nudge.
    fn send_resize(&self, narrow: u16) -> Option<(u16, u16)> {
        let (cols, rows) = *lock(&self.pane.asked_size);
        if cols < 3 {
            return None;
        }
        let _ = self.pane.tx.send(HolderRequest::Resize {
            cols: cols - narrow,
            rows,
            px_width: 0,
            px_height: 0,
        });
        Some((cols - narrow, rows))
    }

    /// The nudge is over, or there was none.
    fn settle(&mut self) {
        self.nudge = None;
        self.nudge_final = None;
        self.settle_by = None;
        self.pane.settled.store(true, Ordering::Release);
    }

    /// Ends the lease on the holder after a quit. The connection closes, and then the task ends.
    fn end_lease(&mut self, checkpointed: bool) {
        let done = self.quit.take().map(|quit| quit.done).unwrap_or_default();
        let _ = self.pane.tx.send(HolderRequest::Detach);
        self.detached = true;
        self.detach_answer = Some((checkpointed, done));
    }

    /// Ends a synchronized update that timed out, and shows what it held back.
    fn flush_sync(&mut self) {
        let pane = Arc::clone(&self.pane);
        let mut effects = Vec::new();
        {
            let mut fx = Effects {
                tx: &pane.tx,
                ids: &pane.internal_ids,
                later: &mut effects,
            };
            let mut screen = lock(&pane.screen);
            screen.engine.flush_sync(&mut fx);
            self.archive_history(&mut screen.engine);
            publish(&pane, &mut screen, false);
        }
        self.carry_out(effects);
    }

    /// The connection ended. Report what it leaves open.
    fn finish(&mut self) {
        self.settle();
        let pane = &self.pane;
        let pending = lock(&pane.inputs).take_pending();
        if !self.detached {
            pane.report_unconfirmed(&pending, UnconfirmedReason::HolderLost);
        }
        pane.update_info(|info| {
            let lost = info.liveness == Liveness::Running && !self.detached;
            if lost {
                info.liveness = Liveness::HolderLost;
            }
            let was_restoring = info.restoring;
            info.restoring = false;
            lost || was_restoring
        });
        let mut waiting = self.quit.take().map(|quit| quit.done).unwrap_or_default();
        let checkpointed = match self.detach_answer.take() {
            Some((checkpointed, done)) => {
                waiting.extend(done);
                checkpointed
            }
            None => false,
        };
        for done in waiting {
            let _ = done.send(checkpointed);
        }
        if let Some(progress) = self.restore.take() {
            progress.lost(pane);
        }
    }
}

/// Starts a new revision and wakes the render servers when a client can see a change.
pub(super) fn publish(pane: &Pane, screen: &mut Screen, resized: bool) {
    let Screen { engine, frames, .. } = screen;
    let absorbed = frames.absorb(engine);
    if !absorbed.changed {
        return;
    }
    let rev = frames.rev();
    pane.rev.store(rev, Ordering::Release);
    // A change of size is never minor, even when the engine reports no damaged row.
    if !absorbed.damage.minor || resized {
        pane.non_minor_rev.store(rev, Ordering::Release);
    }
    pane.damage.send_replace(rev);
}

/// An effect of the output that the task carries out after the batch, outside the lock of the
/// screen: a notification or the clipboard may take time.
enum PaneEffect {
    Bell,
    Notify { title: String, body: String },
    Clipboard { text: String },
}

/// What the engine does to the world outside the screen. A reply to a query goes to
/// the holder at once. The other effects wait in `later`.
struct Effects<'a> {
    tx: &'a HolderTx,
    ids: &'a std::sync::atomic::AtomicU64,
    later: &'a mut Vec<PaneEffect>,
}

impl EffectSink for Effects<'_> {
    fn pty_write(&mut self, bytes: &[u8]) {
        let input_id = INTERNAL_INPUT | self.ids.fetch_add(1, Ordering::Relaxed);
        let _ = self.tx.send(HolderRequest::Input {
            input_id,
            bytes: bytes.to_vec(),
        });
    }

    fn bell(&mut self) {
        self.later.push(PaneEffect::Bell);
    }

    // The title is read from the engine after the batch.
    fn title_changed(&mut self) {}

    fn clipboard_store(&mut self, _clipboard: u8, text: &str) {
        self.later.push(PaneEffect::Clipboard {
            text: text.to_owned(),
        });
    }

    // The core never answers an OSC 52 read.
    fn clipboard_load_requested(&mut self) {}

    fn notify(&mut self, title: &str, body: &str) {
        self.later.push(PaneEffect::Notify {
            title: title.to_owned(),
            body: body.to_owned(),
        });
    }
}
