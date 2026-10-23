//! The runs of agents in the state of the actor.
//!
//! A [`Run`] holds the arbiter, the capabilities, the binding and the ids of one run. The functions
//! here change a run for one input (a process that appeared, a version that was found, a pane that
//! ended), and each does the same four things: it changes the run in memory, writes the row and the
//! events in one transaction, and publishes the projections. They run on the thread of the actor,
//! like every other change of the state.

use std::{collections::HashMap, sync::Arc};

use nh_agents::{
    adapter::{
        AdapterSignal,
        effects::{Effect, effects},
    },
    arbiter::{Arbiter, CancelReason, Change, Input, Observation},
    capabilities::{self, Context, Coverage, Transports, Unvalidated},
    harness::name as harness_name,
    manifest::{Loaded, Manifest},
    matcher::Detection,
    run::{Identified, ReadStates, Sessions, binding_for_detection, promote},
};
use nh_proto::api::{
    common::{Harness, Source},
    events::{
        AdapterDisagreement, AgentDetected, AgentHarnessVersionUnvalidated, AgentIdentified,
        AgentStateChanged, EventPayload,
    },
    ids::{InteractionId, PaneId, RunId, WindowId},
    model::{
        Binding, Capabilities, Execution, Facets, RateLimitWindow, ReadState, UnvalidatedHarness,
        Usage,
    },
    time::Timestamp,
};
use nh_store::{RunRow, Store, StoreError};
use semver::Version;

use super::project;
use crate::{ErrorCode, actor::Parts, interaction::gate::GateReply, persist::now_ms};

/// How long a run stays `starting` when nobody says anything, before the process source says
/// `unknown`.
pub(crate) const SETTLE_AFTER_MS: i64 = 5_000;

/// One run.
#[derive(Clone)]
pub(crate) struct Run {
    pub id: RunId,
    /// A name that is unique among the live runs. A call can use it instead of the id.
    pub name: Option<String>,
    pub pane: Option<PaneId>,
    pub harness: Harness,
    /// The id of the manifest that matched. A wrapper of the user has its own.
    pub manifest: String,
    pub binding: Binding,
    pub arbiter: Arbiter,
    pub sessions: Sessions,
    pub version: Option<Version>,
    pub unvalidated: Option<Unvalidated>,
    pub capabilities: Capabilities,
    pub yolo: bool,
    /// The permission mode that the hooks report has no approval dialogs. The run is
    /// `yolo` when the arguments or the mode say so.
    pub yolo_mode: bool,
    /// A hook of the run has arrived with the token of its pane, so the hooks are a transport.
    pub hooks: bool,
    /// Codex: no argument of the process turns the app-server daemon off, so the hooks can run in
    /// the shared daemon instead of the pane.
    pub shared_daemon: bool,
    /// When the screen saw the current turn start, if it started after the run settled.
    pub screen_turn: Option<Timestamp>,
    /// The screen saw a turn end, and no hook had come: the hooks do not reach neohar.
    pub silent_turn: bool,
    /// The rules of the screen manifest matched the live screen of the run.
    pub screen_validates: bool,
    pub model: Option<String>,
    pub transcript_path: Option<String>,
    /// The tokens and the cost that the transcript showed.
    pub usage: Usage,
    /// The window of a rate limit that is the most used now.
    pub rate_limit: Option<RateLimitWindow>,
    /// The process that the run is: its pid and its start time.
    pub process: Option<(i32, u64)>,
    pub started_at: Timestamp,
    pub ended_at: Option<Timestamp>,
    /// Which windows saw the last change of the execution state.
    pub read: ReadStates<WindowId>,
    /// The end of a turn that nobody saw. Memory only.
    pub turn: super::turn::Turn,
}

impl Run {
    pub fn live(&self) -> bool {
        self.ended_at.is_none()
    }

    /// The read state for the projections. A run is `unseen` when no window saw its last change.
    pub fn read_state(&self, windows: &[WindowId]) -> ReadState {
        if windows.is_empty() || windows.iter().any(|w| self.read.get(w) == ReadState::Seen) {
            ReadState::Seen
        } else {
            ReadState::Unseen
        }
    }

    fn row(&self) -> RunRow {
        let (binding, confidence) = match &self.binding {
            Binding::Bound => ("bound", 1.0),
            Binding::Suggested { confidence } => ("suggested", f64::from(*confidence)),
            Binding::Unlinked => ("unlinked", 0.0),
        };
        RunRow {
            id: self.id.ulid(),
            pane: self.pane.map(PaneId::ulid),
            harness: harness_name(self.harness).to_owned(),
            manifest: self.manifest.clone(),
            name: self.name.clone(),
            session_id: self.sessions.current().map(str::to_owned),
            previous_session_ids: self.sessions.previous().to_vec(),
            transcript_path: self.transcript_path.clone(),
            binding: binding.to_owned(),
            binding_confidence: confidence,
            launch_mode: capabilities::TUI.to_owned(),
            version: self.version.as_ref().map(Version::to_string),
            capabilities: serde_json::to_value(&self.capabilities).unwrap_or_default(),
            facets: serde_json::to_value(self.arbiter.facets(ReadState::Seen)).unwrap_or_default(),
            yolo: self.yolo,
            resume: None,
            model: self.model.clone(),
            process: self.process,
            started_at: self.started_at.0,
            ended_at: self.ended_at.map(|at| at.0),
        }
    }

    /// A run from its row. A row that this version cannot read gives `None`.
    fn from_row(row: &RunRow) -> Option<Self> {
        let harness = match row.harness.as_str() {
            "claude" => Harness::Claude,
            "codex" => Harness::Codex,
            _ => return None,
        };
        let facets: Facets = serde_json::from_value(row.facets.clone()).ok()?;
        let capabilities: Capabilities = serde_json::from_value(row.capabilities.clone()).ok()?;
        let binding = match row.binding.as_str() {
            "bound" => Binding::Bound,
            "suggested" => Binding::Suggested {
                confidence: row.binding_confidence as f32,
            },
            _ => Binding::Unlinked,
        };
        let started_at = Timestamp(row.started_at);
        Some(Self {
            id: RunId::new(row.id),
            name: row.name.clone(),
            pane: row.pane.map(PaneId::new),
            harness,
            manifest: if row.manifest.is_empty() {
                harness_name(harness).to_owned()
            } else {
                row.manifest.clone()
            },
            binding,
            arbiter: Arbiter::restore(&facets),
            sessions: Sessions::from_parts(
                row.session_id.clone(),
                row.previous_session_ids.clone(),
            ),
            version: row.version.as_deref().and_then(|v| Version::parse(v).ok()),
            unvalidated: None,
            capabilities,
            yolo: row.yolo,
            // The next hook sets them again.
            yolo_mode: false,
            hooks: false,
            shared_daemon: false,
            screen_turn: None,
            silent_turn: false,
            screen_validates: false,
            model: row.model.clone(),
            transcript_path: row.transcript_path.clone(),
            // `transcripts::started` fills the usage from the table `usage`.
            usage: Usage::default(),
            rate_limit: None,
            process: row.process,
            started_at,
            ended_at: row.ended_at.map(Timestamp),
            read: ReadStates::new(),
            turn: super::turn::Turn::default(),
        })
    }
}

/// What the core knows about a run it started in a pane, before the run is seen (`agent.start`,
/// `agent.resume`). The run that appears in the pane takes it: the run is bound, because neohar
/// launched the harness there, and it gets the name and the session id.
#[derive(Clone, Debug)]
pub(crate) struct Expected {
    pub harness: Harness,
    pub name: Option<String>,
    pub session: Option<String>,
    pub model: Option<String>,
}

/// All the runs of the core.
#[derive(Clone, Default)]
pub(crate) struct Runs {
    pub by_id: HashMap<RunId, Run>,
    /// The launches that wait for their run, by pane.
    pub expected: HashMap<PaneId, Expected>,
    /// The runs that ended because the machine restarted and that can resume.
    pub rebooted: Vec<RunId>,
    /// The runs that the notice of the restart offers to resume.
    pub offered: Vec<RunId>,
    /// The notice that offers them.
    pub offer_notice: Option<InteractionId>,
    /// The live run of each pane. A pane has at most one.
    live: HashMap<PaneId, RunId>,
    /// The order of the board: every run of `by_id`, as the person put them. A new run comes
    /// last, or takes the place of the run that ended in its pane.
    board: Vec<RunId>,
    /// The order of the board changed, and the store does not have it yet. The next write of a run
    /// writes it.
    board_changed: bool,
    coverage: Arc<Coverage>,
}

impl Runs {
    pub fn new() -> Self {
        Self {
            coverage: Arc::new(Coverage::builtin()),
            ..Self::default()
        }
    }

    /// The runs that the last core left open, in the order of the board. A run whose pane is gone
    /// ends now, and the store learns it in `ended`.
    pub fn load(store: &mut Store, panes: &dyn Fn(PaneId) -> bool) -> Result<Self, StoreError> {
        let mut runs = Self::new();
        let mut ended = Vec::new();
        for row in store.open_runs()? {
            let Some(mut run) = Run::from_row(&row) else {
                // A row that this version cannot read is history. It must not stay open.
                tracing::warn!(run = %row.id, "a run of the table is not readable and ends now");
                let mut unreadable = row.clone();
                unreadable.ended_at = Some(now_ms());
                ended.push(unreadable);
                continue;
            };
            match run.pane {
                Some(pane) if panes(pane) => {
                    runs.live.insert(pane, run.id);
                }
                _ => {
                    run.ended_at = Some(Timestamp(now_ms()));
                    ended.push(run.row());
                }
            }
            runs.board.push(run.id);
            runs.by_id.insert(run.id, run);
        }
        if !ended.is_empty() {
            store.tx(now_ms(), |tx| {
                ended.iter().try_for_each(|row| tx.put_run(row))
            })?;
        }
        Ok(runs)
    }

    /// Forgets the runs of a pane that ended, and gives their ids. A run that ended stays in the
    /// projection until another one starts in its pane.
    pub fn prune_ended(&mut self, pane: PaneId) -> Vec<RunId> {
        let old: Vec<RunId> = self
            .by_id
            .values()
            .filter(|run| run.pane == Some(pane) && !run.live())
            .map(|run| run.id)
            .collect();
        for id in &old {
            self.by_id.remove(id);
        }
        self.board.retain(|id| !old.contains(id));
        old
    }

    /// The runs in the order of the board.
    pub fn board(&self) -> &[RunId] {
        &self.board
    }

    /// The place on the board of a run of the pane that ended. The next run of the pane takes it.
    pub fn ended_place(&self, pane: PaneId) -> Option<usize> {
        self.board.iter().position(|id| {
            self.by_id
                .get(id)
                .is_some_and(|run| run.pane == Some(pane) && !run.live())
        })
    }

    /// Puts a new run on the board: at `place`, or at the end.
    fn join_board(&mut self, id: RunId, place: Option<usize>) {
        match place {
            Some(at) if at < self.board.len() => {
                self.board.insert(at, id);
                self.board_changed = true;
            }
            _ => self.board.push(id),
        }
    }

    /// Moves a run to `index` on the board, or to the end when `index` is past it. Returns whether
    /// the order changed.
    pub fn move_on_board(&mut self, id: RunId, index: usize) -> bool {
        let Some(from) = self.board.iter().position(|run| *run == id) else {
            return false;
        };
        let to = index.min(self.board.len() - 1);
        if from == to {
            return false;
        }
        self.board.remove(from);
        self.board.insert(to, id);
        self.board_changed = true;
        true
    }

    /// The order of the board, once after it changed.
    fn take_board(&mut self) -> Option<Vec<RunId>> {
        std::mem::take(&mut self.board_changed).then(|| self.board.clone())
    }

    /// A window appeared. Every run counts as seen by it until the run changes.
    pub fn join_window(&mut self, window: WindowId) {
        for run in self.by_id.values_mut() {
            run.read.joined(window);
        }
    }

    /// A window is gone.
    pub fn leave_window(&mut self, window: &WindowId) {
        for run in self.by_id.values_mut() {
            run.read.left(window);
        }
    }

    /// The live run of a pane.
    pub fn of_pane(&self, pane: PaneId) -> Option<&Run> {
        self.live.get(&pane).and_then(|id| self.by_id.get(id))
    }

    /// The run that a reference names: its id, or its name. A name belongs to a live run.
    pub fn resolve(&self, reference: &nh_proto::api::ids::Ref<RunId>) -> Option<RunId> {
        use nh_proto::api::ids::RefValue;
        match &reference.0 {
            RefValue::Id(id) => self.by_id.contains_key(id).then_some(*id),
            RefValue::Name(name) => self
                .by_id
                .values()
                .find(|run| run.live() && run.name.as_deref() == Some(name.as_str()))
                .map(|run| run.id),
            RefValue::Handle(_) => None,
        }
    }
}

/// What a run can do before anything is known about it: be seen.
pub(crate) fn observe_only() -> Capabilities {
    Capabilities {
        observe: true,
        gate: false,
        answer_native: Vec::new(),
        answer_keystroke: false,
        reconcile: false,
        resume: false,
        steer: false,
        survive_disconnect: false,
    }
}

fn manifest<'a>(loaded: &'a Loaded, run: &Run) -> Option<&'a Manifest> {
    loaded
        .get(&run.manifest)
        .or_else(|| loaded.get(harness_name(run.harness)))
}

/// The capabilities of a run now: the manifest, the version, the transports and the binding.
fn resolve(coverage: &Coverage, loaded: &Loaded, run: &Run) -> capabilities::Resolved {
    match manifest(loaded, run) {
        Some(manifest) => capabilities::resolve(
            &Context {
                manifest,
                harness: run.harness,
                version: run.version.as_ref(),
                mode: capabilities::TUI,
                transports: Transports { hooks: run.hooks },
                bound: matches!(run.binding, Binding::Bound),
                screen_validates: run.screen_validates,
            },
            coverage,
        ),
        None => capabilities::Resolved {
            capabilities: observe_only(),
            unvalidated: Some(Unvalidated::NoVersion),
        },
    }
}

/// The events of the changes that the arbiter gave.
fn payloads(run: &Run, changes: &[Change]) -> Vec<EventPayload> {
    changes
        .iter()
        .filter_map(|change| match change {
            Change::State {
                facet,
                from,
                to,
                source,
                confidence,
            } => Some(EventPayload::AgentStateChanged(AgentStateChanged {
                run: run.id,
                facet: *facet,
                from: from.clone(),
                to: to.clone(),
                source: *source,
                confidence: *confidence,
            })),
            Change::Disagreement {
                facet,
                structured,
                other,
            } => Some(EventPayload::AdapterDisagreement(AdapterDisagreement {
                run: run.id,
                facet: *facet,
                structured: structured.clone(),
                other: other.clone(),
            })),
            // [`cancels_of`] reads the cancels, and the reconcile needs a capability that no TUI
            // run has.
            Change::Cancel { .. } | Change::Reconcile => None,
        })
        .collect()
}

/// The interactions that the arbiter closes without an answer, and why.
type Cancelled = Vec<(Vec<InteractionId>, &'static str)>;

fn cancels_of(changes: &[Change]) -> Cancelled {
    changes
        .iter()
        .filter_map(|change| match change {
            Change::Cancel { ids, reason } => Some((
                ids.clone(),
                match reason {
                    CancelReason::ProcessExited => "process_exited",
                    CancelReason::TurnEnded => "turn_ended",
                    CancelReason::SessionEnded => "session_ended",
                    CancelReason::Superseded => "superseded",
                    CancelReason::DialogGone => "dialog_gone",
                },
            )),
            _ => None,
        })
        .collect()
}

/// Closes the interactions of the table that the arbiter cancelled.
fn cancel_interactions(parts: &mut Parts<'_>, cancelled: Cancelled) {
    for (ids, reason) in cancelled {
        crate::interaction::apply::cancel(parts, &ids, reason);
    }
}

/// Writes the row of a run and the events in one transaction, and publishes. A store that fails
/// leaves the run in memory as it is: the processes do not wait for the disk.
fn record(parts: &mut Parts<'_>, id: RunId, events: Vec<EventPayload>) {
    let Some(run) = parts.state.runs.by_id.get(&id) else {
        return;
    };
    let mut row = run.row();
    // The row of a run points to a pane only while the pane exists. A pane that was closed left
    // the table, and the history of the run stays without it.
    if row
        .pane
        .is_some_and(|pane| !parts.state.layout.panes.contains_key(&PaneId::new(pane)))
    {
        row.pane = None;
    }
    let subject = id.to_string();
    let board = parts.state.runs.take_board();
    let stored = parts.commit(|tx| {
        tx.put_run(&row)?;
        if let Some(board) = &board {
            tx.put_board_order(&board.iter().map(|run| run.ulid()).collect::<Vec<_>>())?;
        }
        for payload in &events {
            tx.append(Some(&subject), payload)?;
        }
        Ok(())
    });
    if let Err(error) = stored {
        tracing::warn!(run = %subject, %error, "a run is not stored");
    }
    // A lower source that disagreed with a structured one is a bad signal of this version.
    let disagreements = events
        .iter()
        .filter(|payload| matches!(payload, EventPayload::AdapterDisagreement(_)))
        .count();
    if disagreements > 0
        && let Some(run) = parts.state.runs.by_id.get(&id)
    {
        let (harness, version) = (run.harness, run.version.as_ref().map(ToString::to_string));
        let n = u32::try_from(disagreements).unwrap_or(u32::MAX);
        super::drift::record(parts, harness, version.as_deref(), n, n);
    }
    publish(parts, id);
    if board.is_some() {
        let windows: Vec<WindowId> = parts.state.layout.windows.iter().map(|w| w.id).collect();
        project::publish_board(parts.projections, &parts.state.runs, &windows);
    }
}

/// Publishes the run in `agents` and `attention`, and its link in `panes`. The badge and the tray
/// follow `attention`.
pub(crate) fn publish(parts: &mut Parts<'_>, id: RunId) {
    publish_rows(parts, id);
    // A run that is rate limited shows in the limits of its harness.
    crate::transcripts::publish_limits(parts);
    if let Some(host) = parts.state.host.clone() {
        crate::native::summary(parts.state, host.as_ref());
    }
}

fn publish_rows(parts: &mut Parts<'_>, id: RunId) {
    let windows: Vec<WindowId> = parts.state.layout.windows.iter().map(|w| w.id).collect();
    let runs = &parts.state.runs;
    project::publish_run(
        parts.projections,
        runs,
        &parts.state.interactions,
        id,
        &windows,
    );
    // The pane shows its live run.
    if let Some(run) = runs.by_id.get(&id)
        && let Some(pane) = run.pane
    {
        let live = runs.live.get(&pane).copied();
        if let Some(entry) = parts.state.layout.panes.get_mut(&pane)
            && entry.run != live
        {
            entry.run = live;
            crate::layout::project::publish_pane(parts.projections, &parts.state.layout, pane);
        }
    }
}

/// Publishes `attention` for a run again: its interactions changed.
pub(crate) fn publish_attention(
    state: &crate::actor::State,
    hub: &crate::projection::ProjectionHub,
    id: RunId,
) {
    let windows: Vec<WindowId> = state.layout.windows.iter().map(|w| w.id).collect();
    project::publish_run(hub, &state.runs, &state.interactions, id, &windows);
}

/// Publishes every run, after a start of the core.
pub(crate) fn publish_all(parts: &mut Parts<'_>) {
    let ids: Vec<RunId> = parts.state.runs.by_id.keys().copied().collect();
    for id in ids {
        publish(parts, id);
    }
}

pub(crate) fn loaded_manifests(parts: &Parts<'_>) -> Arc<Loaded> {
    loaded(parts)
}

/// The manifest of a run.
pub(crate) fn manifest_of<'a>(
    loaded: &'a Loaded,
    run: &Run,
) -> Option<&'a nh_agents::manifest::Manifest> {
    manifest(loaded, run)
}

fn loaded(parts: &Parts<'_>) -> Arc<Loaded> {
    parts
        .state
        .harnesses
        .as_ref()
        .map(|harnesses| harnesses.current())
        .unwrap_or_default()
}

/// How a run was found, and what is known about it at once.
struct Found {
    harness: Harness,
    manifest: String,
    /// `process` or `hook`. The event `agent.detected` carries it.
    via: &'static str,
    binding: Binding,
    process: Option<(i32, u64)>,
    yolo: bool,
    hooks: bool,
}

/// Makes a run in a pane, and records it.
fn create(parts: &mut Parts<'_>, pane: PaneId, found: Found) -> Option<RunId> {
    if !parts.state.layout.panes.contains_key(&pane) {
        return None;
    }
    let now = Timestamp(now_ms());
    let id = RunId::new(crate::ids::new_ulid().ok()?);
    let mut run = Run {
        id,
        name: None,
        pane: Some(pane),
        harness: found.harness,
        manifest: found.manifest,
        binding: found.binding,
        arbiter: Arbiter::new(now, Source::Process),
        sessions: Sessions::default(),
        version: None,
        unvalidated: None,
        capabilities: observe_only(),
        yolo: found.yolo,
        yolo_mode: false,
        hooks: found.hooks,
        shared_daemon: false,
        screen_turn: None,
        silent_turn: false,
        screen_validates: false,
        model: None,
        transcript_path: None,
        usage: Usage::default(),
        rate_limit: None,
        process: found.process,
        started_at: now,
        ended_at: None,
        read: ReadStates::new(),
        turn: super::turn::Turn::default(),
    };
    let manifests = loaded(parts);
    adopt(&mut run, parts.state.runs.expected.remove(&pane));
    run.capabilities = resolve(&parts.state.runs.coverage, &manifests, &run).capabilities;
    // The run that ended before in this pane leaves the projections. It stays in the table. The
    // new run takes its place on the board.
    let windows: Vec<WindowId> = parts.state.layout.windows.iter().map(|w| w.id).collect();
    let place = parts.state.runs.ended_place(pane);
    for old in parts.state.runs.prune_ended(pane) {
        project::remove_run(parts.projections, old, &windows);
    }
    for window in &windows {
        run.read.joined(*window);
    }
    parts.state.runs.live.insert(pane, id);
    parts.state.runs.by_id.insert(id, run);
    parts.state.runs.join_board(id, place);
    record(
        parts,
        id,
        vec![EventPayload::AgentDetected(AgentDetected {
            run: id,
            harness: found.harness,
            via: found.via.to_owned(),
        })],
    );
    // A run that a hook made has no process walk that asks for the version.
    if found.via == "hook"
        && let Some(hosts) = parts.state.adapters.clone()
        && let Some(rule) = parts
            .state
            .runs
            .by_id
            .get(&id)
            .and_then(|run| manifest(&manifests, run))
            .and_then(|manifest| manifest.version.clone())
    {
        hosts.probe_version(id, rule);
    }
    Some(id)
}

/// A run takes what the core expected of it, if the harness is the one that it launched. A name
/// that a live run has already is not given twice.
fn adopt(run: &mut Run, expected: Option<Expected>) {
    let Some(expected) = expected.filter(|e| e.harness == run.harness) else {
        return;
    };
    run.binding = promote();
    run.name = expected.name;
    if run.model.is_none() {
        run.model = expected.model;
    }
    if let Some(session) = expected.session {
        run.sessions.identify(&session);
    }
}

/// Like [`adopt`], for a run that exists already.
fn adopt_existing(parts: &mut Parts<'_>, pane: PaneId, id: RunId) {
    let Some(expected) = parts.state.runs.expected.remove(&pane) else {
        return;
    };
    let taken = expected.name.as_ref().is_some_and(|name| {
        parts
            .state
            .runs
            .by_id
            .values()
            .any(|other| other.id != id && other.live() && other.name.as_ref() == Some(name))
    });
    let Some(run) = parts.state.runs.by_id.get_mut(&id) else {
        return;
    };
    adopt(
        run,
        Some(Expected {
            name: if taken { None } else { expected.name },
            ..expected
        }),
    );
    reresolve(parts, id);
}

/// A pane whose holder was gone starts again with a new shell: what ran in it ended with the
/// machine. A run with a session can resume.
pub(crate) fn rebooted(parts: &mut Parts<'_>, pane: PaneId) {
    let Some(id) = parts.state.runs.live.get(&pane).copied() else {
        return;
    };
    let resumable = parts
        .state
        .runs
        .by_id
        .get(&id)
        .is_some_and(|run| run.sessions.current().is_some());
    end(parts, id, Source::Holder, None);
    if resumable {
        parts.state.runs.rebooted.push(id);
    }
}

/// The core launched a harness in a pane and waits for its run (`agent.start`).
pub(crate) fn expect(parts: &mut Parts<'_>, pane: PaneId, expected: Expected) {
    parts.state.runs.expected.insert(pane, expected);
    // The run may be there already: a hook of the pane came before the core noted the launch.
    if let Some(id) = parts.state.runs.live.get(&pane).copied() {
        adopt_existing(parts, pane, id);
        record(parts, id, Vec::new());
    }
}

/// A harness process appeared in the foreground tree of a pane. Returns the run. A hook may have
/// made the run first: the pane has a live run of the same harness with no process yet, and the
/// process becomes its process.
pub(crate) fn start(
    parts: &mut Parts<'_>,
    pane: PaneId,
    detection: &Detection,
    yolo: bool,
) -> Option<RunId> {
    if let Some(id) = parts.state.runs.live.get(&pane).copied()
        && let Some(run) = parts.state.runs.by_id.get_mut(&id)
        && run.harness == detection.harness
        && run.process.is_none()
    {
        run.process = Some(detection.key());
        run.manifest.clone_from(&detection.manifest);
        run.yolo = yolo;
        adopt_existing(parts, pane, id);
        reresolve(parts, id);
        record(parts, id, Vec::new());
        return Some(id);
    }
    create(
        parts,
        pane,
        Found {
            harness: detection.harness,
            manifest: detection.manifest.clone(),
            via: "process",
            binding: binding_for_detection(false),
            process: Some(detection.key()),
            yolo,
            hooks: false,
        },
    )
}

/// A hook arrived with the token of a pane. The run of that harness in the pane is bound by it,
/// and the hooks are a transport of the run now. A pane without such a run gets one,
/// found by the hook. Returns the run.
pub(crate) fn bind_by_signal(
    parts: &mut Parts<'_>,
    pane: PaneId,
    harness: Harness,
) -> Option<RunId> {
    if let Some(id) = parts.state.runs.live.get(&pane).copied() {
        let same = parts.state.runs.by_id.get(&id).map(|run| run.harness) == Some(harness);
        if same {
            let run = parts.state.runs.by_id.get_mut(&id)?;
            let changed = !matches!(run.binding, Binding::Bound) || !run.hooks;
            adopt_existing(parts, pane, id);
            let run = parts.state.runs.by_id.get_mut(&id)?;
            if changed {
                run.binding = promote();
                run.hooks = true;
                reresolve(parts, id);
                record(parts, id, Vec::new());
                update_hints(parts);
            }
            return Some(id);
        }
        // Another harness speaks in this pane: the old run is over.
        end_in_pane(parts, pane, Source::Process, None);
    }
    create(
        parts,
        pane,
        Found {
            harness,
            manifest: harness_name(harness).to_owned(),
            via: "hook",
            binding: Binding::Bound,
            process: None,
            yolo: false,
            hooks: true,
        },
    )
}

/// The user links a pane to a harness (`agent.link`). The live run of the pane becomes
/// bound. A pane without a run gets one, and then the harness must be named. The link is a
/// decision of the user, so it is deterministic evidence.
pub(crate) fn link(
    parts: &mut Parts<'_>,
    pane: PaneId,
    harness: Option<Harness>,
    session: Option<String>,
) -> Result<RunId, crate::error::RpcError> {
    use crate::error::RpcError;
    if !parts.state.layout.panes.contains_key(&pane) {
        return Err(RpcError::new(
            ErrorCode::NotFound,
            format!("no pane {pane}"),
        ));
    }
    let id = match parts.state.runs.live.get(&pane).copied() {
        Some(id) => {
            let run = parts
                .state
                .runs
                .by_id
                .get_mut(&id)
                .ok_or_else(|| RpcError::internal("a live run is missing"))?;
            if let Some(named) = harness
                && named != run.harness
            {
                return Err(RpcError::new(
                    ErrorCode::HarnessMismatch,
                    format!(
                        "the run in this pane is {}, not {}",
                        harness_name(run.harness),
                        harness_name(named)
                    ),
                ));
            }
            run.binding = promote();
            id
        }
        None => {
            let harness = harness.ok_or_else(|| {
                RpcError::invalid_params("the pane has no run, so the call must name a harness")
            })?;
            create(
                parts,
                pane,
                Found {
                    harness,
                    manifest: harness_name(harness).to_owned(),
                    via: "link",
                    binding: Binding::Bound,
                    process: None,
                    yolo: false,
                    hooks: false,
                },
            )
            .ok_or_else(|| RpcError::new(ErrorCode::NotFound, format!("no pane {pane}")))?
        }
    };
    if let Some(session) = session
        && let Some(run) = parts.state.runs.by_id.get_mut(&id)
    {
        run.sessions.identify(&session);
    }
    reresolve(parts, id);
    record(parts, id, Vec::new());
    update_hints(parts);
    Ok(id)
}

/// The person puts a run at another place on the board (`agent.move`). The store gets the order
/// with the row of the run, and every window gets it in `agents_for`.
pub(crate) fn move_on_board(parts: &mut Parts<'_>, id: RunId, index: usize) {
    if parts.state.runs.move_on_board(id, index) {
        record(parts, id, Vec::new());
    }
}

/// The user breaks the link of a run (`agent.unlink`). The run stays and is shown, and nothing
/// that it asks can be answered from here until a signal of its pane or a new link binds it.
pub(crate) fn unlink(parts: &mut Parts<'_>, id: RunId) {
    let Some(run) = parts.state.runs.by_id.get_mut(&id) else {
        return;
    };
    if matches!(run.binding, Binding::Unlinked) {
        return;
    }
    run.binding = Binding::Unlinked;
    reresolve(parts, id);
    record(parts, id, Vec::new());
    update_hints(parts);
    let open: Vec<InteractionId> = parts
        .state
        .interactions
        .open_of_run(id)
        .into_iter()
        .map(|i| i.id)
        .collect();
    for interaction in open {
        crate::interaction::apply::transition(parts, interaction, |i| {
            i.unbind();
            ((), crate::interaction::model::Step::default())
        });
    }
}

/// A hook with the token of a pane: the run of the pane is bound or made, and its adapter reads
/// the event.
pub(crate) fn signal(
    parts: &mut Parts<'_>,
    hosts: &Arc<super::AdapterHosts>,
    pane: PaneId,
    harness: Harness,
    event: String,
    payload: serde_json::Value,
    at: Timestamp,
) {
    let Some(run) = bind_by_signal(parts, pane, harness) else {
        return;
    };
    let Some(start) = adapter_start(parts, run) else {
        return;
    };
    hosts.deliver(
        start,
        AdapterSignal::Raw {
            harness_event: event,
            payload,
        },
        at,
    );
}

/// A hook that a shim holds, with the token of a pane: like [`signal`], and the ticket gets the
/// reply when the adapter and the actor have seen the event.
#[allow(clippy::too_many_arguments)]
pub(crate) fn gate(
    parts: &mut Parts<'_>,
    hosts: &Arc<super::AdapterHosts>,
    pane: PaneId,
    harness: Harness,
    event: String,
    payload: serde_json::Value,
    at: Timestamp,
    ticket: super::host::GateTicket,
) {
    let Some(run) = bind_by_signal(parts, pane, harness) else {
        let _ = ticket.reply.send(GateReply::NoDecision("no_run"));
        return;
    };
    let Some(start) = adapter_start(parts, run) else {
        let _ = ticket.reply.send(GateReply::NoDecision("no_adapter"));
        return;
    };
    hosts.deliver_gate(
        start,
        AdapterSignal::Raw {
            harness_event: event,
            payload,
        },
        at,
        ticket,
    );
}

/// What the host of the adapters needs to start the adapter of a run.
pub(crate) fn adapter_start(parts: &Parts<'_>, id: RunId) -> Option<super::host::Start> {
    let run = parts.state.runs.by_id.get(&id)?;
    let manifests = loaded(parts);
    let manifest = manifest(&manifests, run)?;
    // The workspace of an agent is the root of its workspace, else the directory where its pane
    // started. A pane that has neither gets the directory of the first hook.
    let layout = &parts.state.layout;
    let workspace = run
        .pane
        .and_then(|pane| layout.panes.get(&pane))
        .and_then(|entry| {
            layout
                .workspace_index(entry.workspace)
                .and_then(|at| layout.workspaces[at].root.clone())
                .or_else(|| entry.cwd.clone())
        })
        .map(std::path::PathBuf::from);
    Some(super::host::Start {
        run: id,
        pane: run.pane,
        harness: run.harness,
        manifest: Arc::new(manifest.clone()),
        workspace,
    })
}

/// Works out the capabilities of a run again, after the version, the binding or the transports
/// changed.
fn reresolve(parts: &mut Parts<'_>, id: RunId) {
    let manifests = loaded(parts);
    let coverage = Arc::clone(&parts.state.runs.coverage);
    if let Some(run) = parts.state.runs.by_id.get_mut(&id) {
        let resolved = resolve(&coverage, &manifests, run);
        run.capabilities = resolved.capabilities;
        run.unvalidated = resolved.unvalidated;
    }
}

/// The version of the harness of a run is known, or the probe failed. The capabilities follow the
/// version.
pub(crate) fn set_version(parts: &mut Parts<'_>, id: RunId, found: Result<Version, String>) {
    let manifests = loaded(parts);
    let coverage = Arc::clone(&parts.state.runs.coverage);
    let Some(run) = parts.state.runs.by_id.get_mut(&id) else {
        return;
    };
    if !run.live() {
        return;
    }
    run.version = found.as_ref().ok().cloned();
    let resolved = resolve(&coverage, &manifests, run);
    run.capabilities = resolved.capabilities;
    let validated = resolved.unvalidated.is_none();
    run.unvalidated = resolved.unvalidated;
    let changes = run.arbiter.apply(Input::new(
        Source::Derived,
        Timestamp(now_ms()),
        Observation::Version { validated },
    ));
    let mut events = payloads(run, &changes);
    let cancelled = cancels_of(&changes);
    if let (false, Some(version)) = (validated, &run.version) {
        events.push(EventPayload::AgentHarnessVersionUnvalidated(
            AgentHarnessVersionUnvalidated {
                harness: run.harness,
                version: version.to_string(),
            },
        ));
    }
    if let Err(reason) = &found {
        tracing::debug!(run = %id, %reason, "the version of a harness is not known");
    }
    record(parts, id, events);
    cancel_interactions(parts, cancelled);
    refresh_unvalidated(parts);
}

/// Tells `health` which versions of the harnesses run without a golden recording, so that the
/// doctor lists them.
fn refresh_unvalidated(parts: &Parts<'_>) {
    let mut found: Vec<UnvalidatedHarness> = Vec::new();
    for run in parts.state.runs.by_id.values().filter(|run| run.live()) {
        if run.unvalidated.is_none() {
            continue;
        }
        let Some(version) = &run.version else {
            continue;
        };
        let entry = UnvalidatedHarness {
            harness: run.harness,
            version: version.to_string(),
        };
        if !found.contains(&entry) {
            found.push(entry);
        }
    }
    found.sort_by(|a, b| {
        (harness_name(a.harness), &a.version).cmp(&(harness_name(b.harness), &b.version))
    });
    parts.state.health.set_unvalidated_harnesses(found);
}

/// What the screen of a run showed, as the watcher of the pane reads it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ScreenSeen {
    /// The state of the highest rule, and its confidence.
    pub state: Option<(nh_agents::screen::ScreenState, f32)>,
    /// The dialog of the highest rule that opens an interaction.
    pub dialog: Option<nh_agents::screen::ScreenMatch>,
}

/// The screen manifest for the live run of a pane, when the harness has one.
pub(crate) fn screen_target(parts: &Parts<'_>, pane: PaneId) -> Option<(RunId, String, String)> {
    let run = parts.state.runs.of_pane(pane).filter(|run| run.live())?;
    let loaded = loaded(parts);
    let manifest = manifest(&loaded, run)?;
    let screen = manifest.screen.as_ref()?;
    Some((run.id, screen.manifest.clone(), manifest.id.clone()))
}

/// The screen of a run changed what the rules read. The state goes to the arbiter as
/// a screen observation. A dialog opens a provisional interaction, and a screen without a dialog
/// ends the provisional ones.
pub(crate) fn screen(parts: &mut Parts<'_>, id: RunId, seen: &ScreenSeen) {
    let at = Timestamp(now_ms());
    if let Some((state, confidence)) = seen.state {
        let execution = match state {
            nh_agents::screen::ScreenState::Working => Execution::Working,
            nh_agents::screen::ScreenState::Idle => Execution::Idle,
        };
        if screen_turn(parts, id, &execution, at) {
            update_hints(parts);
        }
        input(
            parts,
            id,
            Input::new(Source::Screen, at, Observation::Execution(execution))
                .with_confidence(confidence),
        );
    }
    // The manifest validates on the live screen once one of its rules read a known state or a
    // dialog. A dialog that only the heuristic found does not count.
    let validates = seen.state.is_some()
        || seen
            .dialog
            .as_ref()
            .is_some_and(|d| d.rule_id != nh_agents::screen::engine::UNKNOWN_DIALOG_RULE);
    if validates
        && let Some(run) = parts.state.runs.by_id.get_mut(&id)
        && !run.screen_validates
    {
        run.screen_validates = true;
        reresolve(parts, id);
        record(parts, id, Vec::new());
    }
    match &seen.dialog {
        Some(found) => crate::interaction::screen::opened(parts, id, found, at),
        None => crate::interaction::screen::gone(parts, id, at),
    }
}

/// Follows the turns that the screen sees. A turn starts when the screen reads `working` after the
/// run settled: the screen of a harness that starts can look busy for a moment. Returns true when a
/// turn ended and no hook had come.
fn screen_turn(parts: &mut Parts<'_>, id: RunId, execution: &Execution, at: Timestamp) -> bool {
    let Some(run) = parts.state.runs.by_id.get_mut(&id) else {
        return false;
    };
    match execution {
        Execution::Working => {
            if run.screen_turn.is_none() && at.0 - run.started_at.0 >= SETTLE_AFTER_MS {
                run.screen_turn = Some(at);
            }
            false
        }
        _ => {
            let ended = run.screen_turn.take().is_some();
            if ended && !run.hooks && !run.silent_turn {
                run.silent_turn = true;
                return true;
            }
            false
        }
    }
}

/// The process of a Codex run has no argument that turns the app-server daemon off.
pub(crate) fn set_shared_daemon(parts: &mut Parts<'_>, id: RunId, shared: bool) {
    if let Some(run) = parts.state.runs.by_id.get_mut(&id) {
        run.shared_daemon = shared;
    }
}

/// Gives an input to the arbiter of a run, and records the changes.
pub(crate) fn input(parts: &mut Parts<'_>, id: RunId, input: Input) {
    let viewers = viewers_of(parts, id);
    let Some(run) = parts.state.runs.by_id.get_mut(&id) else {
        return;
    };
    let before = run.arbiter.execution().value.clone();
    let at = input.at;
    let changes = run.arbiter.apply(input);
    let mut turn = None;
    if run.arbiter.execution().value != before {
        run.read.changed(viewers);
        turn = Some(super::turn::note(run, before == Execution::Working, at));
    }
    let events = payloads(run, &changes);
    let cancelled = cancels_of(&changes);
    if events.is_empty() && changes.is_empty() && turn.is_none() {
        return;
    }
    record(parts, id, events);
    cancel_interactions(parts, cancelled);
    if let Some(note) = turn {
        super::turn::follow(parts, id, note);
    }
}

/// The signals that the adapter of a run reported. Each effect goes to the run in order, and the
/// result is one row and one transaction.
pub(crate) fn apply_signals(
    parts: &mut Parts<'_>,
    id: RunId,
    signals: &[AdapterSignal],
    at: Timestamp,
) {
    let viewers = viewers_of(parts, id);
    let Some(run) = parts.state.runs.by_id.get_mut(&id) else {
        return;
    };
    if !run.live() {
        return;
    }
    let mut events = Vec::new();
    let mut cancelled = Vec::new();
    let mut touched = false;
    // The execution before the signals, and whether it was `working` at some point of them.
    let initial = run.arbiter.execution().value.clone();
    let mut was_working = initial == Execution::Working;
    for effect in signals.iter().flat_map(effects) {
        touched = true;
        match effect {
            Effect::Identify {
                session_id,
                transcript_path,
                model,
            } => {
                if model.is_some() {
                    run.model = model;
                }
                if transcript_path.is_some() {
                    run.transcript_path = transcript_path;
                }
                if run.sessions.identify(&session_id) != Identified::Same {
                    events.push(EventPayload::AgentIdentified(AgentIdentified {
                        run: id,
                        harness: run.harness,
                        session_id,
                        pane: run.pane,
                    }));
                }
            }
            Effect::PermissionMode { yolo } => run.yolo_mode = yolo,
            Effect::Observe(observation) => {
                let before = run.arbiter.execution().value.clone();
                let changes = run
                    .arbiter
                    .apply(Input::new(Source::Structured, at, observation));
                if run.arbiter.execution().value != before {
                    run.read.changed(viewers.iter().copied());
                }
                was_working |= run.arbiter.execution().value == Execution::Working;
                events.extend(payloads(run, &changes));
                cancelled.extend(cancels_of(&changes));
            }
        }
    }
    // A turn that started and ended within the signals also counts.
    let last = run.arbiter.execution().value.clone();
    let turn = (last != initial || (was_working && last == Execution::Idle))
        .then(|| super::turn::note(run, was_working, at));
    if touched {
        record(parts, id, events);
        cancel_interactions(parts, cancelled);
    }
    if let Some(note) = turn {
        super::turn::follow(parts, id, note);
    }
}

/// The adapter of a run failed. The health says so, and the run goes on with the other sources.
pub(crate) fn degrade(parts: &mut Parts<'_>, id: RunId, reason: &str) {
    input(
        parts,
        id,
        Input::new(
            Source::Derived,
            Timestamp(now_ms()),
            Observation::Degraded(Some(reason.to_owned())),
        ),
    );
}

/// The windows that look at the pane of a run: the window has the focus of the OS, can be seen, and
/// the pane is its active element.
fn viewers_of(parts: &Parts<'_>, id: RunId) -> Vec<WindowId> {
    let Some(pane) = parts.state.runs.by_id.get(&id).and_then(|run| run.pane) else {
        return Vec::new();
    };
    parts
        .state
        .layout
        .windows
        .iter()
        .filter(|w| w.facts.focused && w.facts.visible && w.facts.active_pane == Some(pane))
        .map(|w| w.id)
        .collect()
}

/// The run has been `starting` for a while and nobody said anything.
pub(crate) fn settle(parts: &mut Parts<'_>, id: RunId) {
    let now = Timestamp(now_ms());
    let Some(run) = parts.state.runs.by_id.get(&id) else {
        return;
    };
    if !run.live() || now.0 - run.started_at.0 < SETTLE_AFTER_MS {
        return;
    }
    input(
        parts,
        id,
        Input::new(Source::Process, now, Observation::Settled),
    );
    update_hints(parts);
}

/// The doctor says which runs have no hooks: their state comes from the process list
/// only. The note goes when the hooks arrive or the run ends.
///
/// Claude Code runs `SessionStart` when it starts, so five seconds of silence are enough. Codex
/// runs its first hook with the first prompt, so only a turn that ended without a hook
/// says that the hooks do not come.
pub(crate) fn update_hints(parts: &Parts<'_>) {
    let now = now_ms();
    let mut texts: Vec<String> = parts
        .state
        .runs
        .by_id
        .values()
        .filter(|run| run.live() && !run.hooks)
        .filter_map(|run| {
            let place = run
                .pane
                .and_then(|pane| parts.state.layout.panes.get(&pane))
                .map_or_else(|| "a pane".to_owned(), |pane| pane.handle.clone());
            let (name, id) = match run.harness {
                Harness::Claude if now - run.started_at.0 >= SETTLE_AFTER_MS => {
                    ("Claude Code", "claude")
                }
                Harness::Codex if run.silent_turn && run.shared_daemon => {
                    return Some(format!(
                        "Codex in {place} started without `--no-daemon`, so it can attach to the shared app-server of Codex, and then its hooks do not reach neohar. Quit it and start `codex` again in a pane of neohar: there `codex` is the shim of neohar, which adds the flag."
                    ));
                }
                Harness::Codex if run.silent_turn => ("Codex", "codex"),
                _ => return None,
            };
            Some(format!(
                "{name} in {place} is detected from the process list only. Run `nh integration install {id}` for the exact state."
            ))
        })
        .collect();
    texts.sort();
    parts.state.health.set_warnings(
        "integration",
        texts,
        Some("The hooks show what the harness does, and they let neohar answer its dialogs."),
    );
}

/// The process of the run is gone, or the child of the pane ended. The source says which.
pub(crate) fn end(parts: &mut Parts<'_>, id: RunId, source: Source, code: Option<i32>) {
    let now = Timestamp(now_ms());
    let Some(run) = parts.state.runs.by_id.get_mut(&id) else {
        return;
    };
    if !run.live() {
        return;
    }
    let changes = run
        .arbiter
        .apply(Input::new(source, now, Observation::Exited { code }));
    run.ended_at = Some(now);
    // A run that ends does not wait after its turn.
    let turn = super::turn::note(run, false, now);
    let pane = run.pane;
    let events = payloads(run, &changes);
    let cancelled = cancels_of(&changes);
    if let Some(pane) = pane
        && parts.state.runs.live.get(&pane) == Some(&id)
    {
        parts.state.runs.live.remove(&pane);
    }
    record(parts, id, events);
    cancel_interactions(parts, cancelled);
    super::turn::follow(parts, id, turn);
    refresh_unvalidated(parts);
    update_hints(parts);
    if let Some(adapters) = &parts.state.adapters {
        adapters.stop(id, nh_agents::adapter::StopReason::ProcessExited);
    }
}

/// The live run of a pane ends.
pub(crate) fn end_in_pane(parts: &mut Parts<'_>, pane: PaneId, source: Source, code: Option<i32>) {
    if let Some(id) = parts.state.runs.live.get(&pane).copied() {
        end(parts, id, source, code);
    }
}

/// The pane was closed: its run ends, and the runs of the pane leave the projections. The table
/// keeps them as history.
pub(crate) fn pane_closed(parts: &mut Parts<'_>, pane: PaneId) {
    end_in_pane(parts, pane, Source::Holder, None);
    let windows: Vec<WindowId> = parts.state.layout.windows.iter().map(|w| w.id).collect();
    for old in parts.state.runs.prune_ended(pane) {
        project::remove_run(parts.projections, old, &windows);
    }
}

/// The layout changed: a window that is new gets its projection of the agents and counts as having
/// seen the runs so far, and a window that is gone loses both.
pub(crate) fn sync_windows(
    hub: &crate::projection::ProjectionHub,
    runs: &mut Runs,
    old: &crate::layout::model::Layout,
    new: &crate::layout::model::Layout,
) {
    for window in &new.windows {
        if old.window_index(window.id).is_none() {
            runs.join_window(window.id);
            project::publish_window(hub, runs, window.id);
        }
    }
    for window in &old.windows {
        if new.window_index(window.id).is_none() {
            runs.leave_window(&window.id);
            project::forget_window(hub, window.id);
        }
    }
}

/// Another harness process took the place of the run of a pane.
pub(crate) fn replace(
    parts: &mut Parts<'_>,
    pane: PaneId,
    detection: &Detection,
    yolo: bool,
) -> Option<RunId> {
    end_in_pane(parts, pane, Source::Process, None);
    start(parts, pane, detection, yolo)
}

/// A window looked at a pane (`window.focus_report`).
pub(crate) fn viewed(parts: &mut Parts<'_>, pane: PaneId, window: WindowId) {
    let Some(id) = parts.state.runs.live.get(&pane).copied() else {
        return;
    };
    let Some(run) = parts.state.runs.by_id.get_mut(&id) else {
        return;
    };
    let before = run.read_state(&[window]);
    run.read.viewed(window);
    if run.read_state(&[window]) != before {
        publish(parts, id);
    }
}
