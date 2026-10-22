//! The state arbiter.
//!
//! The arbiter is the only part that writes the facets of a run. It is a pure state machine: it
//! gets an [`Input`] (a source, a time, a confidence and an [`Observation`]) and gives the list of
//! [`Change`]s that the core records as events. It has no timer and reads no clock, so silence
//! never changes a state (rule 3), and a test sets the time of each input.
//!
//! The rules, with where each is:
//!
//! 1. A process that ends overrides at once: [`Observation::Exited`].
//! 2. An open interaction leaves `pending` only by one of these paths: [`Observation::Closed`],
//!    the end of a turn or of the session, the death of the process, or a provisional interaction
//!    that the screen no longer shows. A screen that shows nothing closes no other interaction.
//! 3. A source of lower precedence only adds information: [`Arbiter::execution_from_screen`] and
//!    [`Arbiter::screen_opens`].
//! 4. A structured channel that is gone is explicit: [`Observation::TransportLost`].
//! 5. Among sources, the structured one wins over the screen and the screen over the process.
//!    Inside one source a signal that is older than the state is dropped.
//! 6. An unvalidated version caps every confidence at 0.8.

use nh_proto::api::{
    common::Source,
    events::Facet,
    ids::InteractionId,
    model::{
        AdapterHealth, Execution, ExecutionFacet, Facets, HealthFacet, InteractionKind, Liveness,
        LivenessFacet, Provenance, ReadState,
    },
    time::Timestamp,
};
use serde::Serialize;
use serde_json::{Value, json};

/// The confidence cap of rule 6.
pub const UNVALIDATED_CAP: f32 = 0.8;

/// The confidence of the first state of a run, which only the process told.
const PROCESS_CONFIDENCE: f32 = 0.5;

/// Something that was seen, and from where.
#[derive(Clone, Debug, PartialEq)]
pub struct Input {
    pub source: Source,
    pub at: Timestamp,
    /// From 0 to 1. A hook gives 1, and a screen rule gives its own number.
    pub confidence: f32,
    pub observation: Observation,
}

impl Input {
    pub fn new(source: Source, at: Timestamp, observation: Observation) -> Self {
        Self {
            source,
            at,
            confidence: 1.0,
            observation,
        }
    }

    pub fn with_confidence(mut self, confidence: f32) -> Self {
        self.confidence = confidence;
        self
    }
}

/// What was seen.
#[derive(Clone, Debug, PartialEq)]
pub enum Observation {
    /// The process lives (holder, process).
    Alive,
    /// The process ended (holder, process).
    Exited { code: Option<i32> },
    /// The run has been `starting` for long enough with no word from the harness: the process
    /// source says `unknown` instead (the core decides how long).
    Settled,
    /// The execution state, as a hook or a protocol message, or a screen rule, says it.
    Execution(Execution),
    /// A turn ended: `idle`, and no interaction stays open (rule 2).
    TurnEnded,
    /// The session ended in the harness.
    SessionEnded,
    /// An interaction opened. From the screen it is provisional.
    Opened {
        id: InteractionId,
        kind: InteractionKind,
    },
    /// An interaction ended by an answer, a native resolution or a dismissal.
    /// From the screen it closes only a provisional interaction, because the dialog is gone.
    Closed { id: InteractionId },
    /// The structured channel is gone (end of the protocol stream, no shim).
    TransportLost,
    /// The channel is back. The state heals with the next structured signal.
    TransportHealed,
    /// The adapter works in part.
    Degraded(Option<String>),
    /// The version of the harness is in a validated range, or it is not (rule 6).
    Version { validated: bool },
}

/// Why interactions were closed without an answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CancelReason {
    ProcessExited,
    TurnEnded,
    SessionEnded,
    /// A structured interaction took the place of a provisional one from the screen.
    Superseded,
    /// The screen no longer shows the dialog of a provisional interaction.
    DialogGone,
}

/// What the core records after an input.
#[derive(Clone, Debug, PartialEq)]
pub enum Change {
    /// `agent.state_changed`.
    State {
        facet: Facet,
        from: Value,
        to: Value,
        source: Source,
        confidence: f32,
    },
    /// `adapter.disagreement`.
    Disagreement {
        facet: Facet,
        structured: Value,
        other: Value,
    },
    /// The interactions to close in the table, with the reason.
    Cancel {
        ids: Vec<InteractionId>,
        reason: CancelReason,
    },
    /// The structured channel is gone: the core asks the harness for the real state if the run has
    /// the capability `reconcile`.
    Reconcile,
}

/// The counters of the disagreement telemetry.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counters {
    /// The signals of a lower source that could be compared with a structured state.
    pub observations: u32,
    /// How many of them said something else.
    pub disagreements: u32,
}

#[derive(Clone, Debug)]
struct Open {
    id: InteractionId,
    kind: InteractionKind,
    provisional: bool,
}

/// What the screen last said about the execution.
#[derive(Clone, Debug)]
struct ScreenSaid {
    state: Execution,
    confidence: f32,
    at: Timestamp,
}

/// The facets of one run and the rules that change them.
#[derive(Clone, Debug)]
pub struct Arbiter {
    liveness: LivenessFacet,
    execution: ExecutionFacet,
    health: HealthFacet,
    pending: Vec<Open>,
    /// A structured signal set the execution state, and the channel is not known to be gone.
    structured: bool,
    transport_lost: bool,
    degraded: Option<String>,
    unvalidated: bool,
    screen: Option<ScreenSaid>,
    counters: Counters,
}

fn rank(source: Source) -> u8 {
    match source {
        Source::Structured | Source::Transcript | Source::User | Source::Derived => 3,
        Source::Screen => 2,
        Source::Process | Source::Holder => 1,
    }
}

fn json_of<T: Serialize>(value: &T) -> Value {
    serde_json::to_value(value).unwrap_or(Value::Null)
}

impl Arbiter {
    /// A run that was just found. The process lives, the harness has not said anything.
    pub fn new(at: Timestamp, source: Source) -> Self {
        let provenance = |confidence| Provenance {
            source,
            confidence,
            at,
        };
        Self {
            liveness: LivenessFacet {
                value: Liveness::Alive,
                provenance: provenance(1.0),
            },
            execution: ExecutionFacet {
                value: Execution::Starting,
                provenance: provenance(PROCESS_CONFIDENCE),
            },
            health: HealthFacet {
                value: AdapterHealth::Healthy,
                provenance: provenance(1.0),
            },
            pending: Vec::new(),
            structured: false,
            transport_lost: false,
            degraded: None,
            unvalidated: false,
            screen: None,
            counters: Counters::default(),
        }
    }

    /// The arbiter of a run that the core loaded from its table after a restart. The facets are as
    /// they were. The list of open interactions is empty, because the table of interactions says
    /// which are open, and the screen state is empty until the screen detector speaks.
    pub fn restore(facets: &Facets) -> Self {
        let health = &facets.adapter_health;
        Self {
            liveness: facets.liveness.clone(),
            execution: facets.execution.clone(),
            health: health.clone(),
            pending: Vec::new(),
            // A state that a hook set keeps its authority until the channel is known to be gone.
            structured: rank(facets.execution.provenance.source) == 3,
            transport_lost: health.value == AdapterHealth::Disconnected,
            degraded: match &health.value {
                AdapterHealth::Degraded { reason } => Some(reason.clone()),
                _ => None,
            },
            unvalidated: health.value == AdapterHealth::UnvalidatedVersion,
            screen: None,
            counters: Counters::default(),
        }
    }

    /// The facets as the projection shows them. The read state is per client, so the caller gives it.
    pub fn facets(&self, read_state: ReadState) -> Facets {
        Facets {
            liveness: self.liveness.clone(),
            execution: self.execution.clone(),
            pending: self.pending.iter().map(|open| open.id).collect(),
            adapter_health: self.health.clone(),
            read_state,
        }
    }

    pub fn liveness(&self) -> &LivenessFacet {
        &self.liveness
    }

    pub fn execution(&self) -> &ExecutionFacet {
        &self.execution
    }

    pub fn health(&self) -> &HealthFacet {
        &self.health
    }

    pub fn pending(&self) -> Vec<InteractionId> {
        self.pending.iter().map(|open| open.id).collect()
    }

    /// Whether the interaction is one that the screen opened and no hook confirmed.
    pub fn is_provisional(&self, id: InteractionId) -> bool {
        self.pending
            .iter()
            .any(|open| open.id == id && open.provisional)
    }

    pub fn counters(&self) -> Counters {
        self.counters
    }

    /// Whether the execution state is a guess of the screen or the process, and not the word of
    /// the harness. The UI marks it with `~`.
    pub fn inferred(&self) -> bool {
        matches!(
            self.execution.provenance.source,
            Source::Screen | Source::Process
        ) && !matches!(self.execution.value, Execution::Exited)
    }

    /// Applies one input and gives the changes.
    pub fn apply(&mut self, input: Input) -> Vec<Change> {
        let Input {
            source,
            at,
            confidence,
            observation,
        } = input;
        let confidence = confidence.clamp(0.0, 1.0);
        let mut out = Vec::new();
        match observation {
            Observation::Exited { code } => self.exited(source, at, code, &mut out),
            _ if matches!(self.liveness.value, Liveness::Exited { .. }) => {
                // The process is gone. Nothing that arrives later changes the facets (rule 1).
            }
            Observation::Alive => self.alive(source, at, &mut out),
            Observation::Settled => self.settled(source, at, &mut out),
            Observation::Execution(state) => {
                self.execution_from(source, at, confidence, state, &mut out);
            }
            Observation::TurnEnded => {
                if rank(source) == 3 && !self.late(at) {
                    self.execution_from(source, at, confidence, Execution::Idle, &mut out);
                    self.cancel_all(source, CancelReason::TurnEnded, &mut out);
                }
            }
            Observation::SessionEnded => {
                if rank(source) == 3 && !self.late(at) {
                    self.set_execution(source, at, confidence, Execution::Exited, &mut out);
                    self.cancel_all(source, CancelReason::SessionEnded, &mut out);
                }
            }
            Observation::Opened { id, kind } => {
                if source == Source::Screen {
                    self.screen_opens(id, kind, confidence, &mut out);
                } else if rank(source) == 3 {
                    self.opens(id, kind, source, confidence, &mut out);
                }
            }
            Observation::Closed { id } => self.closes(id, source, confidence, &mut out),
            Observation::TransportLost => self.transport_lost(source, at, &mut out),
            Observation::TransportHealed => {
                self.transport_lost = false;
                self.set_health(source, at, &mut out);
            }
            Observation::Degraded(reason) => {
                self.degraded = reason;
                self.set_health(source, at, &mut out);
            }
            Observation::Version { validated } => {
                self.unvalidated = !validated;
                self.cap_confidence();
                self.set_health(source, at, &mut out);
            }
        }
        out
    }

    /// Confidence of a derived state (rule 6).
    fn capped(&self, confidence: f32) -> f32 {
        if self.unvalidated {
            confidence.min(UNVALIDATED_CAP)
        } else {
            confidence
        }
    }

    fn cap_confidence(&mut self) {
        if self.unvalidated {
            for confidence in [
                &mut self.liveness.provenance.confidence,
                &mut self.execution.provenance.confidence,
            ] {
                *confidence = confidence.min(UNVALIDATED_CAP);
            }
        }
    }

    /// Rule 1.
    fn exited(&mut self, source: Source, at: Timestamp, code: Option<i32>, out: &mut Vec<Change>) {
        if matches!(self.liveness.value, Liveness::Exited { .. }) {
            return;
        }
        let from = json_of(&self.liveness.value);
        self.liveness = LivenessFacet {
            value: Liveness::Exited { code },
            provenance: Provenance {
                source,
                confidence: self.capped(1.0),
                at,
            },
        };
        out.push(Change::State {
            facet: Facet::Liveness,
            from,
            to: json_of(&self.liveness.value),
            source,
            confidence: self.liveness.provenance.confidence,
        });
        self.cancel_all(source, CancelReason::ProcessExited, out);
        self.set_execution(source, at, 1.0, Execution::Exited, out);
        self.structured = false;
    }

    fn alive(&mut self, source: Source, at: Timestamp, out: &mut Vec<Change>) {
        if self.liveness.value == Liveness::Starting {
            let from = json_of(&self.liveness.value);
            self.liveness = LivenessFacet {
                value: Liveness::Alive,
                provenance: Provenance {
                    source,
                    confidence: self.capped(1.0),
                    at,
                },
            };
            out.push(Change::State {
                facet: Facet::Liveness,
                from,
                to: json_of(&self.liveness.value),
                source,
                confidence: self.liveness.provenance.confidence,
            });
        }
    }

    fn settled(&mut self, source: Source, at: Timestamp, out: &mut Vec<Change>) {
        if self.execution.value == Execution::Starting
            && rank(self.execution.provenance.source) == 1
        {
            self.set_execution(source, at, PROCESS_CONFIDENCE, Execution::Unknown, out);
        }
    }

    /// Whether a structured signal at `at` is older than the state that a structured signal set.
    fn late(&self, at: Timestamp) -> bool {
        self.structured && at < self.execution.provenance.at
    }

    /// Whether a structured channel is alive: it said something, and it is not known to be gone.
    fn structured_active(&self) -> bool {
        self.structured && !self.transport_lost
    }

    /// Rules 3 and 5 for an execution state.
    fn execution_from(
        &mut self,
        source: Source,
        at: Timestamp,
        confidence: f32,
        state: Execution,
        out: &mut Vec<Change>,
    ) {
        let rank = rank(source);
        if rank == 3 {
            // Structured. An older signal than the state is a late one. It is dropped.
            if self.late(at) {
                return;
            }
            self.structured = true;
            if self.transport_lost {
                // A later structured signal heals the state (rule 4).
                self.transport_lost = false;
                self.set_health(source, at, out);
            }
            self.set_execution(source, at, confidence, state, out);
        } else if rank == 2 {
            self.screen = Some(ScreenSaid {
                state: state.clone(),
                confidence,
                at,
            });
            if self.structured_active() {
                // Rule 3: it adds information. It counts as an observation, and it counts as a
                // disagreement when it says something else.
                self.counters.observations += 1;
                if self.execution.value != state {
                    self.counters.disagreements += 1;
                }
            } else if self.execution.provenance.source != Source::Screen
                || at >= self.execution.provenance.at
            {
                self.set_execution(source, at, confidence, state, out);
            }
        }
        // The process and the holder do not tell the execution state.
    }

    fn set_execution(
        &mut self,
        source: Source,
        at: Timestamp,
        confidence: f32,
        state: Execution,
        out: &mut Vec<Change>,
    ) {
        let confidence = self.capped(confidence);
        if self.execution.value == state {
            // The same state from a source that knows better only raises the provenance.
            if rank(source) >= rank(self.execution.provenance.source) {
                self.execution.provenance = Provenance {
                    source,
                    confidence,
                    at,
                };
            }
            return;
        }
        let from = json_of(&self.execution.value);
        self.execution = ExecutionFacet {
            value: state,
            provenance: Provenance {
                source,
                confidence,
                at,
            },
        };
        out.push(Change::State {
            facet: Facet::Execution,
            from,
            to: json_of(&self.execution.value),
            source,
            confidence,
        });
    }

    /// A structured interaction opens (rule 2).
    fn opens(
        &mut self,
        id: InteractionId,
        kind: InteractionKind,
        source: Source,
        confidence: f32,
        out: &mut Vec<Change>,
    ) {
        if self.pending.iter().any(|open| open.id == id) {
            return;
        }
        // A provisional interaction of the same kind was the same dialog. The structured one
        // replaces it.
        let superseded: Vec<InteractionId> = self
            .pending
            .iter()
            .filter(|open| open.provisional && open.kind == kind)
            .map(|open| open.id)
            .collect();
        let before = self.pending();
        self.pending
            .retain(|open| !(open.provisional && open.kind == kind));
        self.pending.push(Open {
            id,
            kind,
            provisional: false,
        });
        self.pending_changed(before, source, confidence, out);
        if !superseded.is_empty() {
            out.push(Change::Cancel {
                ids: superseded,
                reason: CancelReason::Superseded,
            });
        }
    }

    /// Rule 3: the screen shows a dialog.
    fn screen_opens(
        &mut self,
        id: InteractionId,
        kind: InteractionKind,
        confidence: f32,
        out: &mut Vec<Change>,
    ) {
        if self
            .pending
            .iter()
            .any(|open| open.id == id || open.kind == kind)
        {
            // The dialog is one that is open already.
            return;
        }
        let structured_says = json_of(&self.execution.value);
        let active = self.structured_active();
        let before = self.pending();
        self.pending.push(Open {
            id,
            kind,
            provisional: true,
        });
        self.pending_changed(before, Source::Screen, confidence, out);
        if active {
            self.counters.observations += 1;
            self.counters.disagreements += 1;
            out.push(Change::Disagreement {
                facet: Facet::Execution,
                structured: structured_says,
                other: json!({ "pending": kind }),
            });
        }
    }

    fn closes(
        &mut self,
        id: InteractionId,
        source: Source,
        confidence: f32,
        out: &mut Vec<Change>,
    ) {
        let Some(open) = self.pending.iter().find(|open| open.id == id) else {
            return;
        };
        if rank(source) < 3 {
            // The screen closes only what it opened: the dialog is gone (rule 2).
            if source != Source::Screen || !open.provisional {
                return;
            }
        }
        let provisional = open.provisional && source == Source::Screen;
        let before = self.pending();
        self.pending.retain(|open| open.id != id);
        self.pending_changed(before, source, confidence, out);
        if provisional {
            out.push(Change::Cancel {
                ids: vec![id],
                reason: CancelReason::DialogGone,
            });
        }
    }

    fn cancel_all(&mut self, source: Source, reason: CancelReason, out: &mut Vec<Change>) {
        if self.pending.is_empty() {
            return;
        }
        let before = self.pending();
        self.pending.clear();
        out.push(Change::State {
            facet: Facet::Pending,
            from: json_of(&before),
            to: json_of(&Vec::<InteractionId>::new()),
            source,
            confidence: self.capped(1.0),
        });
        out.push(Change::Cancel {
            ids: before,
            reason,
        });
    }

    fn pending_changed(
        &mut self,
        before: Vec<InteractionId>,
        source: Source,
        confidence: f32,
        out: &mut Vec<Change>,
    ) {
        out.push(Change::State {
            facet: Facet::Pending,
            from: json_of(&before),
            to: json_of(&self.pending()),
            source,
            confidence: self.capped(confidence),
        });
    }

    /// Rule 4.
    fn transport_lost(&mut self, source: Source, at: Timestamp, out: &mut Vec<Change>) {
        if self.transport_lost {
            return;
        }
        self.transport_lost = true;
        self.set_health(source, at, out);
        if rank(self.execution.provenance.source) == 3
            && !matches!(self.execution.value, Execution::Exited)
        {
            // The state comes from the next source: the screen, and else the process.
            match self.screen.clone() {
                Some(said) => {
                    self.force_execution(Source::Screen, said.at, said.confidence, said.state, out)
                }
                None => self.force_execution(
                    Source::Process,
                    at,
                    PROCESS_CONFIDENCE,
                    Execution::Unknown,
                    out,
                ),
            }
        }
        out.push(Change::Reconcile);
    }

    /// Sets the execution state whatever the source of the old one was.
    fn force_execution(
        &mut self,
        source: Source,
        at: Timestamp,
        confidence: f32,
        state: Execution,
        out: &mut Vec<Change>,
    ) {
        let confidence = self.capped(confidence);
        let from = json_of(&self.execution.value);
        let changed = self.execution.value != state;
        self.execution = ExecutionFacet {
            value: state,
            provenance: Provenance {
                source,
                confidence,
                at,
            },
        };
        if changed {
            out.push(Change::State {
                facet: Facet::Execution,
                from,
                to: json_of(&self.execution.value),
                source,
                confidence,
            });
        }
    }

    /// The health is the worst of the reasons: a lost channel, a part that fails, a version that
    /// nobody checked.
    fn set_health(&mut self, source: Source, at: Timestamp, out: &mut Vec<Change>) {
        let value = if self.transport_lost {
            AdapterHealth::Disconnected
        } else if let Some(reason) = &self.degraded {
            AdapterHealth::Degraded {
                reason: reason.clone(),
            }
        } else if self.unvalidated {
            AdapterHealth::UnvalidatedVersion
        } else {
            AdapterHealth::Healthy
        };
        if value == self.health.value {
            return;
        }
        let from = json_of(&self.health.value);
        self.health = HealthFacet {
            value,
            provenance: Provenance {
                source,
                confidence: 1.0,
                at,
            },
        };
        out.push(Change::State {
            facet: Facet::AdapterHealth,
            from,
            to: json_of(&self.health.value),
            source,
            confidence: 1.0,
        });
    }
}

#[cfg(test)]
mod tests;
