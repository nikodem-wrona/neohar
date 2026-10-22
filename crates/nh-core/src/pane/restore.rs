//! The restart flow.
//!
//! A new core opens `state.db` and finds the panes that were running. For each pane it connects to
//! the holder with a fencing token above the old one, asks for `Status`, and loads the newest
//! snapshot that the ring of the holder still covers. It restores the engine from the snapshot and
//! attaches from the offset of the snapshot. The pane task replays the journal up to the end that
//! `Status` reported, answers the queries that the holder queued, and nudges the size of the PTY so
//! that applications redraw ([`super::task`]).
//!
//! A pane whose holder is gone cannot come back (a reboot, or a holder that was killed). The store
//! marks it `lost`. The layout keeps such a pane, and `pane.restart` starts it again.

use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use nh_proto::holder::{HolderEvent, HolderRequest, HolderStatus};
use nh_store::{PaneLiveness, PaneRecord, SnapshotRecord};
use nh_term::{AlacrittyEngine, EngineConfig, MIN_HISTORY, Snapshot, VtEngine, xterm_palette};
use tokio::{
    sync::{mpsc, watch},
    time::timeout,
};
use ulid::Ulid;

use super::{Liveness, Pane, PaneParts, Recovery, replay::Replay, task};
use crate::{holder, persist::now_ms, runtime::Inner};

/// How long a holder may take to answer `Status`.
const STATUS_TIMEOUT: Duration = Duration::from_secs(5);

/// How a pane came back, or why it did not.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RestoreOutcome {
    Restored {
        recovery: Recovery,
        /// The offset of the snapshot that the pane started from.
        snapshot_offset: Option<u64>,
        /// The range of the journal at the time of the restart.
        journal_start: u64,
        journal_end: u64,
        /// How many bytes the replay fed to the engine.
        replay_bytes: u64,
        /// The time from the start of the core to the first revision after the replay.
        restored_ms: u64,
    },
    Lost {
        reason: String,
    },
}

/// One pane in a [`RestoreReport`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaneRestore {
    pub pane: Ulid,
    pub outcome: RestoreOutcome,
}

/// What the restart of a core did, pane by pane.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RestoreReport {
    pub panes: Vec<PaneRestore>,
    /// The time from the start of the core until the last pane was done. `None` while panes are
    /// still restoring.
    pub finished_ms: Option<u64>,
}

impl RestoreReport {
    /// The report as `debug.restore_report` returns it.
    pub fn to_json(&self) -> serde_json::Value {
        use serde_json::json;
        let panes: Vec<_> = self
            .panes
            .iter()
            .map(|entry| match &entry.outcome {
                RestoreOutcome::Restored {
                    recovery,
                    snapshot_offset,
                    journal_start,
                    journal_end,
                    replay_bytes,
                    restored_ms,
                } => json!({
                    "pane": entry.pane.to_string(),
                    "outcome": "restored",
                    "recovery": recovery.as_str(),
                    "snapshot_offset": snapshot_offset,
                    "journal_start": journal_start,
                    "journal_end": journal_end,
                    "replay_bytes": replay_bytes,
                    "restored_ms": restored_ms,
                }),
                RestoreOutcome::Lost { reason } => json!({
                    "pane": entry.pane.to_string(),
                    "outcome": "lost",
                    "reason": reason,
                }),
            })
            .collect();
        json!({ "done": self.finished_ms.is_some(), "finished_ms": self.finished_ms, "panes": panes })
    }
}

/// The progress of the restart of one core. The runtime makes it once.
pub(crate) struct RestoreState {
    started: Instant,
    report: Mutex<RestoreReport>,
    pending: AtomicUsize,
    done: watch::Sender<bool>,
}

impl RestoreState {
    pub fn new(started: Instant) -> Arc<Self> {
        Arc::new(Self {
            started,
            report: Mutex::new(RestoreReport::default()),
            pending: AtomicUsize::new(0),
            done: watch::channel(false).0,
        })
    }

    pub fn report(&self) -> RestoreReport {
        super::lock(&self.report).clone()
    }

    /// A receiver that holds `true` when every pane is done.
    pub fn subscribe(&self) -> watch::Receiver<bool> {
        self.done.subscribe()
    }

    fn elapsed_ms(&self) -> u64 {
        u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    fn push(&self, pane: Ulid, outcome: RestoreOutcome) {
        let mut report = super::lock(&self.report);
        report.panes.push(PaneRestore { pane, outcome });
        if self.pending.fetch_sub(1, Ordering::AcqRel) == 1 {
            report.finished_ms = Some(self.elapsed_ms());
            drop(report);
            self.done.send_replace(true);
        }
    }

    /// Says how many panes the restart brings back. Call it once, before the first pane is done.
    fn expect(&self, panes: usize) {
        self.pending.store(panes, Ordering::Release);
        if panes == 0 {
            super::lock(&self.report).finished_ms = Some(self.elapsed_ms());
            self.done.send_replace(true);
        }
    }
}

/// What the task of a restored pane reports to when its replay ends.
///
/// The restart is done when every pane has reported once. A task that ends in another way, for
/// example by a panic, reports through [`Drop`], so that nobody waits for ever.
pub(crate) struct Progress {
    state: Arc<RestoreState>,
    pane: Ulid,
    snapshot_offset: Option<u64>,
    journal: (u64, u64),
    reported: AtomicBool,
}

impl Progress {
    /// The replay ended and the pane shows its screen.
    pub fn done(&self, pane: &Pane, replay_bytes: u64) {
        if self.reported.swap(true, Ordering::AcqRel) {
            return;
        }
        self.state.push(
            pane.id,
            RestoreOutcome::Restored {
                recovery: pane.info().recovery,
                snapshot_offset: self.snapshot_offset,
                journal_start: self.journal.0,
                journal_end: self.journal.1,
                replay_bytes,
                restored_ms: self.state.elapsed_ms(),
            },
        );
    }

    /// The connection to the holder ended before the replay did.
    pub fn lost(&self, pane: &Pane) {
        self.fail(
            pane.id,
            "the connection to the holder ended during the replay",
        );
    }

    fn fail(&self, pane: Ulid, reason: &str) {
        if self.reported.swap(true, Ordering::AcqRel) {
            return;
        }
        self.state.push(
            pane,
            RestoreOutcome::Lost {
                reason: reason.to_owned(),
            },
        );
    }
}

impl Drop for Progress {
    fn drop(&mut self) {
        self.fail(self.pane, "the task of the pane ended during the replay");
    }
}

/// Starts the restore of every pane in `records` on the runtime of the core. The records are the
/// panes that were running, with their snapshots.
pub(crate) fn start(
    inner: &Arc<Inner>,
    state: &Arc<RestoreState>,
    records: Vec<(PaneRecord, Vec<SnapshotRecord>)>,
) {
    state.expect(records.len());
    let counter = Arc::new(watch::channel(0usize).0);
    for (index, (record, snapshots)) in records.into_iter().enumerate() {
        let inner = Arc::clone(inner);
        let state = Arc::clone(state);
        let turn = Turn {
            index,
            counter: Arc::clone(&counter),
        };
        inner.runtime.clone().spawn(async move {
            let id = record.id;
            if let Err(failure) = bring_back(&inner, &state, record, snapshots, &turn).await {
                // A pane that does not come back still takes its turn, or the panes after it
                // would wait for ever.
                turn.wait().await;
                turn.pass();
                tracing::warn!(pane = %id, reason = %failure.reason, "the pane does not come back");
                let pane = nh_proto::api::ids::PaneId::new(id);
                // A pane of the layout whose holder is gone starts again with the login shell
                // ("Reboot").
                let mut recreated = false;
                if failure.holder_gone {
                    recreated = matches!(
                        inner
                            .actor
                            .update(move |parts| crate::actor::layout::recreate(parts, pane))
                            .await,
                        Ok(true)
                    );
                    if !recreated {
                        let lost = inner
                            .factory
                            .persist
                            .run(move |store| {
                                store.set_liveness(id, PaneLiveness::Lost, None, now_ms())
                            })
                            .await;
                        if let Err(error) = lost {
                            tracing::error!(pane = %id, %error, "the loss of the pane is not stored");
                        }
                    }
                }
                if !recreated {
                    // A pane of the layout shows that it has no process, and `pane.restart` can
                    // start it again. Another pane is not in the layout, and this changes nothing.
                    inner
                        .actor
                        .update_detached(move |parts| crate::actor::layout::apply_lost(parts, pane));
                }
                state.push(
                    id,
                    RestoreOutcome::Lost {
                        reason: failure.reason,
                    },
                );
            }
        });
    }
}

/// The turn of a pane to appear in the `panes` projection.
///
/// The restart connects to all holders at once, and they answer in any order. The projection lists
/// panes in the order in which they appear, and a window lays them out in that order. So each pane
/// waits for the panes before it (the records come in the order of their render ids) before it
/// publishes its row. Only this short step waits: the connections, the `Status` calls and the
/// restore of the engines run in parallel.
struct Turn {
    index: usize,
    counter: Arc<watch::Sender<usize>>,
}

impl Turn {
    /// Waits until every pane before this one has published its row, or has given up.
    async fn wait(&self) {
        let mut turn = self.counter.subscribe();
        let _ = turn.wait_for(|next| *next >= self.index).await;
    }

    /// This pane has published its row, or has given up. The next pane may go.
    fn pass(&self) {
        self.counter
            .send_modify(|next| *next = (*next).max(self.index + 1));
    }
}

impl Drop for Turn {
    /// A task that ends in another way, for example by a panic, must not stop the others.
    fn drop(&mut self) {
        self.pass();
    }
}

/// Why a pane did not come back.
struct Failure {
    reason: String,
    /// The holder is gone for good, so the store marks the pane `lost`. A holder that is there but
    /// does not answer, or that another core owns, is left alone: the next restart may succeed.
    holder_gone: bool,
}

impl Failure {
    fn kept(reason: impl ToString) -> Self {
        Self {
            reason: reason.to_string(),
            holder_gone: false,
        }
    }
}

impl From<holder::HolderError> for Failure {
    fn from(error: holder::HolderError) -> Self {
        Self {
            // No socket, or nobody listens on it.
            holder_gone: matches!(error, holder::HolderError::Connect { .. }),
            reason: error.to_string(),
        }
    }
}

/// The replay of what the holder of a new pane journaled before the core attached.
///
/// A child can print a query in the first milliseconds, before the core has connected. The holder
/// answers the queries that need no screen then, and it queues the others. The core
/// attaches from offset 0 and reads the same bytes. Read live, they would get a second answer. So
/// the core reads them as a replay: with the effects off, except for the answers to the queued
/// queries. Returns `None` when the journal was empty at `Hello`, with the events that came
/// before the answer to `Status`.
pub(super) async fn pre_attach_replay(
    link: &mut holder::HolderLink,
    fresh_ms: i64,
) -> Result<(Option<Replay>, Vec<HolderEvent>), holder::HolderError> {
    let end = link.hello.journal.end;
    if end == 0 {
        return Ok((None, Vec::new()));
    }
    link.tx.send(HolderRequest::Status)?;
    let mut backlog = Vec::new();
    loop {
        match timeout(STATUS_TIMEOUT, link.events.recv()).await {
            Ok(Some(HolderEvent::StatusReply(status))) => {
                // Nothing is queued after `Hello`, because a core is attached then.
                let queued: Vec<_> = status
                    .queued_queries
                    .into_iter()
                    .filter(|query| query.offset <= end)
                    .collect();
                return Ok((Some(Replay::new_pane(end, &queued, fresh_ms)), backlog));
            }
            Ok(Some(other)) => backlog.push(other),
            Ok(None) => return Err(holder::HolderError::Closed),
            Err(_) => return Err(holder::HolderError::Timeout),
        }
    }
}

/// Reads events until the holder answers `Status`. The events before it wait for the task.
async fn read_status(
    events: &mut mpsc::Receiver<HolderEvent>,
) -> Result<(HolderStatus, Vec<HolderEvent>), Failure> {
    let mut backlog = Vec::new();
    loop {
        match timeout(STATUS_TIMEOUT, events.recv()).await {
            Ok(Some(HolderEvent::StatusReply(status))) => return Ok((status, backlog)),
            Ok(Some(other)) => backlog.push(other),
            Ok(None) => return Err(Failure::kept("the holder closed the connection")),
            Err(_) => return Err(Failure::kept("the holder did not answer Status")),
        }
    }
}

/// The engine that a restored pane starts with, and where it reads the journal from.
pub(super) struct Plan {
    pub engine: AlacrittyEngine,
    /// The journal offset to attach from.
    pub from: u64,
    pub recovery: Recovery,
    pub snapshot_offset: Option<u64>,
}

/// Chooses the starting point of a restored pane. `snapshots` come newest first.
///
/// The newest snapshot that the ring still covers and that decodes wins. Without one, the pane
/// reads the whole journal when the ring still starts at 0, which is exact. Otherwise it reads the
/// ring only (`ring_only`).
pub(super) fn plan(
    record: &PaneRecord,
    snapshots: &[SnapshotRecord],
    status: &HolderStatus,
) -> Plan {
    let journal = &status.journal;
    let config = EngineConfig {
        history: MIN_HISTORY,
        theme: xterm_palette(),
        cell_px: (0, 0),
    };
    for snapshot in snapshots {
        if snapshot.offset < journal.start || snapshot.offset > journal.end {
            continue;
        }
        let engine = Snapshot::decode(&snapshot.data)
            .map_err(|error| error.to_string())
            .and_then(|decoded| {
                AlacrittyEngine::restore(&decoded, &config).map_err(|error| error.to_string())
            });
        match engine {
            Ok(engine) => {
                return Plan {
                    engine,
                    from: snapshot.offset,
                    recovery: Recovery::Snapshot,
                    snapshot_offset: Some(snapshot.offset),
                };
            }
            Err(error) => {
                tracing::warn!(pane = %record.id, offset = snapshot.offset, %error, "a snapshot does not restore");
            }
        }
    }
    if snapshots.is_empty() && journal.start == 0 {
        Plan {
            engine: AlacrittyEngine::new(record.cols, record.rows, MIN_HISTORY, xterm_palette()),
            from: 0,
            recovery: Recovery::Journal,
            snapshot_offset: None,
        }
    } else {
        Plan {
            engine: AlacrittyEngine::new(status.cols, status.rows, MIN_HISTORY, xterm_palette()),
            from: journal.start,
            recovery: Recovery::RingOnly,
            snapshot_offset: None,
        }
    }
}

/// Connects to the holder of one pane and starts its task in replay mode.
async fn bring_back(
    inner: &Arc<Inner>,
    state: &Arc<RestoreState>,
    record: PaneRecord,
    snapshots: Vec<SnapshotRecord>,
    turn: &Turn,
) -> Result<(), Failure> {
    let factory = &inner.factory;
    let socket = PathBuf::from(&record.socket);
    let mut link =
        holder::connect(&socket, record.secret, factory.fencing_token, "nh-core").await?;
    factory.health.holder_protocol(link.hello.proto);
    link.tx.send(HolderRequest::Status)?;
    let (status, backlog) = read_status(&mut link.events).await?;
    let plan = plan(&record, &snapshots, &status);
    let journal = status.journal;
    link.tx.send(HolderRequest::Attach {
        from_offset: plan.from,
    })?;
    let (id, token) = (record.id, factory.fencing_token);
    factory
        .persist
        .run(move |store| store.set_attached(id, token, now_ms()))
        .await
        .map_err(Failure::kept)?;

    let fresh_ms = i64::try_from(factory.tuning.query_fresh.as_millis()).unwrap_or(i64::MAX);
    let replay = (plan.from < journal.end)
        .then(|| Replay::new(plan.from, journal.end, &status.queued_queries, fresh_ms));
    let exited = status.exited;
    turn.wait().await;
    let (pane, control) = factory.assemble(PaneParts {
        id: record.id,
        render_id: record.render_id,
        holder_pid: record.holder_pid,
        child_pid: link.hello.child_pid,
        tx: link.tx,
        engine: plan.engine,
        fed_offset: plan.from,
        asked_size: (status.cols, status.rows),
        restoring: true,
        recovery: plan.recovery,
        liveness: exited.map_or(Liveness::Running, Liveness::Exited),
    });
    inner.insert_pane(&pane);
    turn.pass();
    let progress = Progress {
        state: Arc::clone(state),
        pane: record.id,
        snapshot_offset: plan.snapshot_offset,
        journal: (journal.start, journal.end),
        reported: AtomicBool::new(false),
    };
    // A snapshot that is not at a cut point cannot be taken, so a pane that starts from the ring
    // alone takes its first checkpoint at the first cut point of the stream.
    let at_cut = plan.recovery != Recovery::RingOnly;
    factory.start_task(
        &pane,
        link.events,
        control,
        task::Init {
            backlog,
            replay,
            last_checkpoint: plan.snapshot_offset.unwrap_or(0),
            restore: Some(progress),
            at_cut,
            nudge: exited.is_none(),
        },
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use nh_proto::holder::{JournalInfo, TrackedModes};
    use nh_term::VtEngine;

    use super::*;

    fn record() -> PaneRecord {
        PaneRecord {
            id: Ulid::from(1u128),
            render_id: 1,
            socket: "/tmp/x.sock".to_owned(),
            holder_pid: 1,
            secret: [0; 32],
            fencing_token: 1,
            argv: vec![],
            cwd: "/".to_owned(),
            cols: 30,
            rows: 6,
            liveness: PaneLiveness::Running,
            exit: None,
        }
    }

    fn status(start: u64, end: u64) -> HolderStatus {
        HolderStatus {
            child_pid: 1,
            fg_pgid: 1,
            exited: None,
            cols: 40,
            rows: 8,
            journal: JournalInfo {
                start,
                end,
                capacity: 1 << 20,
                last_checkpoint: 0,
                last_cut_point: end,
            },
            tracked: TrackedModes::default(),
            queued_queries: vec![],
            holder_version: "test".to_owned(),
            started_at_ms: 0,
        }
    }

    fn snapshot_at(offset: u64) -> SnapshotRecord {
        let mut engine = AlacrittyEngine::new(30, 6, MIN_HISTORY, xterm_palette());
        engine.advance(b"hi", &mut nh_term::Replaying::default());
        let snapshot = engine.checkpoint(offset).expect("a snapshot");
        SnapshotRecord {
            pane: Ulid::from(1u128),
            offset,
            created_at_ms: 0,
            data: snapshot.encode(),
        }
    }

    #[test]
    fn the_newest_snapshot_that_the_ring_covers_wins() {
        let snapshots = [snapshot_at(900), snapshot_at(500)];
        let plan = plan(&record(), &snapshots, &status(0, 1000));
        assert_eq!(
            (plan.from, plan.recovery, plan.snapshot_offset),
            (900, Recovery::Snapshot, Some(900))
        );
        assert_eq!(plan.engine.size(), (30, 6));
    }

    #[test]
    fn a_snapshot_before_the_start_of_the_ring_is_skipped_for_an_older_one_that_fits() {
        // The ring starts at 600. The newest snapshot (500) is out of it, and so is the older.
        let snapshots = [snapshot_at(500), snapshot_at(400)];
        let plan = plan(&record(), &snapshots, &status(600, 1000));
        assert_eq!((plan.from, plan.recovery), (600, Recovery::RingOnly));
        assert_eq!(plan.engine.size(), (40, 8), "the size of the PTY now");
    }

    #[test]
    fn a_snapshot_that_does_not_decode_gives_way_to_the_older_one() {
        let mut broken = snapshot_at(900);
        broken.data = vec![0xff; 16];
        let snapshots = [broken, snapshot_at(500)];
        let plan = plan(&record(), &snapshots, &status(0, 1000));
        assert_eq!((plan.from, plan.recovery), (500, Recovery::Snapshot));
    }

    #[test]
    fn no_snapshot_and_a_whole_ring_replay_the_journal_from_zero_at_the_first_size() {
        let plan = plan(&record(), &[], &status(0, 1000));
        assert_eq!((plan.from, plan.recovery), (0, Recovery::Journal));
        assert_eq!(
            plan.engine.size(),
            (30, 6),
            "the size the pane started with"
        );
    }

    #[test]
    fn no_snapshot_and_a_wrapped_ring_read_the_ring_only() {
        let plan = plan(&record(), &[], &status(4096, 9000));
        assert_eq!((plan.from, plan.recovery), (4096, Recovery::RingOnly));
    }

    #[test]
    fn a_snapshot_after_the_end_of_the_journal_is_not_trusted() {
        // Only another holder with the same id could do this. The pane must not start from it.
        let plan = plan(&record(), &[snapshot_at(5000)], &status(0, 1000));
        assert_eq!(plan.recovery, Recovery::RingOnly);
    }
}
