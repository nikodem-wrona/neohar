//! An interaction and its delivery record as a state machine.
//!
//! This module is pure. Each transition takes the time as an argument, changes the interaction,
//! and gives the events to append and the effects that the caller must run after the commit. It
//! reads no clock, uses no lock and does no I/O, so the property test can play any order of
//! answers, acks, native resolutions, lost connections, deadlines and restarts against it.
//!
//! ```text
//!                 answer                 lease / sent              ack + native resolution
//!      open ──────────────► decision_recorded ───────► delivering ───────────────────► delivered
//!        │                        │   ▲                    │
//!        │ native resolution      │   │ answer (rev + 1)   ├─ connection lost, no ack ──► delivery_unknown
//!        ▼                        ▼   │                    └─ native resolution, no ack ► resolved_elsewhere
//!   resolved_elsewhere     (the same, with a decision that nobody used)
//! ```
//!
//! The final states are `delivered`, `failed`, `resolved_elsewhere`, `dismissed` and `cancelled`.
//! `delivery_unknown` is not final: the shim can come back (`gate_resume`), and the signal that
//! follows in the harness settles it. Nothing leaves a final state.

use nh_agents::adapter::Resolution;
use nh_proto::api::{
    calls::agent::{ApprovalDecision, Behavior, Decision, PlanDecision, QuestionDecision},
    common::{Risk, Via},
    events::{
        EventPayload, InteractionCancelled, InteractionDecided, InteractionDelivery,
        InteractionDeliveryFailed, InteractionEnded, InteractionOpened,
    },
    ids::{InteractionId, PaneId, RunId},
    model::{
        AnswerChannel, DeliveryInfo, GateMode, InteractionKind, InteractionPayload,
        InteractionState, InteractionValue,
    },
    time::Timestamp,
};

use crate::policy::fingerprint::Facts;

/// Where an interaction came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Source {
    Hook,
    Protocol,
    Screen,
    /// The core itself, for example the notice that offers to resume agents.
    Core,
}

impl Source {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Hook => "hook",
            Self::Protocol => "protocol",
            Self::Screen => "screen",
            Self::Core => "core",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "hook" => Some(Self::Hook),
            "protocol" => Some(Self::Protocol),
            "screen" => Some(Self::Screen),
            "core" => Some(Self::Core),
            _ => None,
        }
    }
}

/// Who holds the lease of a delivery.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Holder {
    /// The hook process, which waits for the decision.
    Shim,
}

impl Holder {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Shim => "shim",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Lease {
    pub holder: Holder,
    /// The connection of the holder. A lease does not outlive the core that made it.
    pub conn: u64,
    /// The process of the holder, when the system tells it. A recording of a harness kills it to
    /// test a delivery that its shim does not finish.
    pub pid: Option<i32>,
    pub expires_at: Timestamp,
}

/// What happened when the core tried to deliver (`attempts`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Attempt {
    pub at: Timestamp,
    pub channel: AnswerChannel,
    pub via: Option<Via>,
    /// `sent`, `acked`, `resumed`, `lost`, `released` or `deadline`.
    pub outcome: &'static str,
}

/// The delivery record.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Delivery {
    /// The revision of the decision. It is 0 until a client decides, and it grows when a client
    /// replaces a decision that was not delivered yet.
    pub rev: u32,
    pub decision: Option<Decision>,
    pub decided_by: Option<String>,
    pub via: Option<Via>,
    /// `blake3(interaction id, rev)`: a delivery that repeats has the same key.
    pub key: Option<String>,
    pub channel: Option<AnswerChannel>,
    pub lease: Option<Lease>,
    /// The shim confirmed that it printed the decision.
    pub acked: bool,
    pub attempts: Vec<Attempt>,
    pub fail_reason: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Interaction {
    pub id: InteractionId,
    pub handle: String,
    pub kind: InteractionKind,
    pub run: Option<RunId>,
    pub pane: Option<PaneId>,
    pub native_ref: Option<String>,
    pub title: String,
    pub risk: Risk,
    pub payload: InteractionPayload,
    pub state: InteractionState,
    pub mode: GateMode,
    pub answerable: bool,
    pub channels: Vec<AnswerChannel>,
    pub source: Source,
    pub confidence: f64,
    pub opened_at: Timestamp,
    pub deadline: Option<Timestamp>,
    pub resolved_at: Option<Timestamp>,
    pub delivery: Delivery,
    /// What a rule or a suggestion needs to know about an approval.
    pub facts: Option<Facts>,
    /// The dialog of the harness can be answered by verified keystrokes: the run is
    /// bound and has the capability, and the dialog is on the screen of the pane. It stays so
    /// when the core releases a held hook.
    pub keystroke: bool,
}

/// What the caller gives to open an interaction.
#[derive(Clone, Debug)]
pub(crate) struct OpenSpec {
    pub kind: InteractionKind,
    pub run: Option<RunId>,
    pub pane: Option<PaneId>,
    pub native_ref: Option<String>,
    pub title: String,
    pub risk: Risk,
    pub payload: InteractionPayload,
    pub mode: GateMode,
    pub answerable: bool,
    pub channels: Vec<AnswerChannel>,
    pub source: Source,
    pub confidence: f64,
    pub deadline: Option<Timestamp>,
    pub facts: Option<Facts>,
    /// A rule of the policy decides at once.
    pub by_policy: bool,
    /// See [`Interaction::keystroke`].
    pub keystroke: bool,
}

/// What the caller must do after it stored the step.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Effect {
    /// A decision is recorded. Deliver it by the lease, if there is one.
    Deliver,
    /// Tell the shim that holds the hook, on connection `conn`, that there is no decision, and why.
    Release { conn: u64, reason: &'static str },
}

/// The result of a transition.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Step {
    pub events: Vec<EventPayload>,
    pub effects: Vec<Effect>,
}

impl Step {
    pub fn is_empty(&self) -> bool {
        self.events.is_empty() && self.effects.is_empty()
    }
}

/// Why a client's request was refused.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Refusal {
    /// The interaction has a decision that is on its way or delivered (`interaction.already_answered`).
    AlreadyAnswered {
        by: Option<String>,
        via: Option<Via>,
        decision: Option<Decision>,
    },
    /// The interaction ended (`interaction_closed`).
    Closed(InteractionState),
    /// A client cannot answer it: the run is not bound, or no channel exists for this kind.
    NotAnswerable,
    Invalid(String),
}

/// The key that makes a delivery safe to repeat.
pub(crate) fn idempotency_key(id: InteractionId, rev: u32) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(id.to_string().as_bytes());
    hasher.update(&rev.to_le_bytes());
    hasher.finalize().to_hex().to_string()
}

pub(crate) fn is_final(state: InteractionState) -> bool {
    matches!(
        state,
        InteractionState::Delivered
            | InteractionState::Failed
            | InteractionState::Superseded
            | InteractionState::ResolvedElsewhere
            | InteractionState::Dismissed
            | InteractionState::Cancelled
    )
}

impl Interaction {
    /// Opens an interaction. The event `interaction.opened` is the first of its life.
    pub fn open(id: InteractionId, handle: String, spec: OpenSpec, now: Timestamp) -> (Self, Step) {
        let interaction = Self {
            id,
            handle,
            kind: spec.kind,
            run: spec.run,
            pane: spec.pane,
            native_ref: spec.native_ref,
            title: spec.title,
            risk: spec.risk,
            payload: spec.payload,
            state: InteractionState::Open,
            mode: spec.mode,
            answerable: spec.answerable,
            channels: spec.channels,
            source: spec.source,
            confidence: spec.confidence,
            opened_at: now,
            deadline: spec.deadline,
            resolved_at: None,
            delivery: Delivery::default(),
            facts: spec.facts,
            keystroke: spec.keystroke,
        };
        let step = Step {
            events: vec![EventPayload::InteractionOpened(InteractionOpened {
                interaction: id,
                kind: interaction.kind,
                run: interaction.run,
                pane: interaction.pane,
                risk: interaction.risk,
                by_policy: spec.by_policy,
            })],
            effects: Vec::new(),
        };
        (interaction, step)
    }

    pub fn is_final(&self) -> bool {
        is_final(self.state)
    }

    /// The interaction as the API shows it.
    pub fn value(&self) -> InteractionValue {
        InteractionValue {
            id: self.id,
            handle: self.handle.clone(),
            kind: self.kind,
            state: self.state,
            mode: self.mode,
            run: self.run,
            pane: self.pane,
            title: self.title.clone(),
            risk: self.risk,
            opened_at: self.opened_at,
            deadline: self.deadline,
            payload: self.payload.clone(),
            delivery: DeliveryInfo {
                rev: self.delivery.rev,
                attempts: u32::try_from(self.delivery.attempts.len()).unwrap_or(u32::MAX),
                via: self.delivery.via,
                decided_by: self.delivery.decided_by.clone(),
                fail_reason: self.delivery.fail_reason.clone(),
            },
            answerable: self.answerable && !self.is_final(),
            channels: self.channels.clone(),
            always_rule: self
                .always_rule()
                .filter(|_| self.answerable && !self.is_final())
                .map(str::to_owned),
        }
    }

    /// The rule that "Allow always" adds: the pattern of an approval that a rule may cover
    /// ([`Facts`]). The decision must go through the hook: a key in the dialog of the harness
    /// would pick the rule of the harness, which the person did not see.
    pub fn always_rule(&self) -> Option<&str> {
        if self.kind != InteractionKind::Approval || !self.channels.contains(&AnswerChannel::Native)
        {
            return None;
        }
        self.facts
            .as_ref()
            .filter(|facts| facts.eligible)
            .and_then(|facts| facts.pattern.as_deref())
    }

    fn ended(&mut self, state: InteractionState, now: Timestamp) {
        self.state = state;
        self.resolved_at = Some(now);
        self.delivery.lease = None;
    }

    fn attempt(&mut self, now: Timestamp, outcome: &'static str) {
        self.delivery.attempts.push(Attempt {
            at: now,
            channel: self.delivery.channel.unwrap_or(AnswerChannel::Native),
            via: self.delivery.via,
            outcome,
        });
    }

    fn release_effect(&self, reason: &'static str) -> Vec<Effect> {
        match self.delivery.lease {
            Some(lease) => vec![Effect::Release {
                conn: lease.conn,
                reason,
            }],
            None => Vec::new(),
        }
    }

    // --- decisions -----------------------------------------------------------------------------

    /// A client decides (`interaction.answer`). The first decision wins. The client that decided
    /// may replace its decision while the state is `decision_recorded`, and no other client may.
    pub fn answer(&mut self, decision: Decision, by: &str, via: Via) -> Result<Step, Refusal> {
        match self.state {
            InteractionState::Open => {}
            // The client that decided may replace its decision until the decision is sent. Another
            // client is late: the first decision wins.
            InteractionState::DecisionRecorded
                if self.delivery.decided_by.as_deref() == Some(by) => {}
            InteractionState::DecisionRecorded
            | InteractionState::Delivering
            | InteractionState::Delivered
            | InteractionState::DeliveryUnknown => {
                return Err(Refusal::AlreadyAnswered {
                    by: self.delivery.decided_by.clone(),
                    via: self.delivery.via,
                    decision: self.delivery.decision.clone(),
                });
            }
            other => return Err(Refusal::Closed(other)),
        }
        if !self.answerable {
            return Err(Refusal::NotAnswerable);
        }
        self.validate(&decision)?;
        let rev = self.delivery.rev + 1;
        self.delivery.rev = rev;
        self.delivery.decision = Some(decision);
        self.delivery.decided_by = Some(by.to_owned());
        self.delivery.via = Some(via);
        self.delivery.key = Some(idempotency_key(self.id, rev));
        self.delivery.channel = Some(self.channel_for_decision());
        self.delivery.acked = false;
        self.state = InteractionState::DecisionRecorded;
        Ok(Step {
            events: vec![EventPayload::InteractionDecided(InteractionDecided {
                interaction: self.id,
                rev,
                by: by.to_owned(),
                via,
            })],
            effects: vec![Effect::Deliver],
        })
    }

    /// The channel that carries the decision: native when the kind has it, else keystrokes.
    fn channel_for_decision(&self) -> AnswerChannel {
        if self.channels.contains(&AnswerChannel::Native) {
            AnswerChannel::Native
        } else {
            AnswerChannel::Keystroke
        }
    }

    /// Checks a decision against the kind, the options and the channels of the interaction.
    fn validate(&self, decision: &Decision) -> Result<(), Refusal> {
        let invalid = |text: &str| Err(Refusal::Invalid(text.to_owned()));
        match (&self.payload, decision) {
            (InteractionPayload::Approval(_), Decision::Approval(d)) => {
                validate_approval(d)?;
                if d.always && self.always_rule().is_none() {
                    return invalid(
                        "no rule covers only this request, so it has no \"always\": allow it once",
                    );
                }
                Ok(())
            }
            (InteractionPayload::Question(payload), Decision::Question(d)) => {
                validate_question(payload, d)
            }
            (InteractionPayload::PlanReview(_), Decision::PlanReview(d)) => {
                // Approval of a plan has no native channel in a TUI.
                if d.approve && !self.channels.contains(&AnswerChannel::Keystroke) {
                    return invalid(
                        "this plan can be approved only with keystrokes, and they are not available",
                    );
                }
                validate_plan(d)
            }
            (InteractionPayload::Notice(_), _) => invalid("a notice has no decision; dismiss it"),
            _ => invalid("the decision is not for this kind of interaction"),
        }
    }

    // --- delivery ------------------------------------------------------------------------------

    /// A shim holds the hook for this interaction (`adapter.gate`, `gate_resume`).
    pub fn hold(&mut self, lease: Lease) {
        self.delivery.lease = Some(lease);
    }

    /// The decision went to the holder of the lease.
    pub fn sent(&mut self, now: Timestamp) -> Step {
        if !matches!(
            self.state,
            InteractionState::DecisionRecorded | InteractionState::DeliveryUnknown
        ) || self.delivery.decision.is_none()
        {
            return Step::default();
        }
        self.state = InteractionState::Delivering;
        self.delivery.acked = false;
        self.attempt(now, "sent");
        Step {
            events: vec![EventPayload::InteractionDeliveryStarted(
                InteractionDelivery {
                    interaction: self.id,
                    rev: self.delivery.rev,
                },
            )],
            effects: Vec::new(),
        }
    }

    /// The shim printed the decision (`adapter.delivery_ack`).
    pub fn acked(&mut self, key: &str, applied: bool, now: Timestamp) -> bool {
        if self.state != InteractionState::Delivering || self.delivery.key.as_deref() != Some(key) {
            return false;
        }
        self.delivery.acked = applied;
        self.attempt(now, if applied { "acked" } else { "not_applied" });
        true
    }

    /// The connection of the lease ended.
    pub fn lease_lost(&mut self, now: Timestamp) -> Step {
        if self.delivery.lease.is_none() || self.is_final() {
            return Step::default();
        }
        self.delivery.lease = None;
        if self.state == InteractionState::Delivering && !self.delivery.acked {
            self.state = InteractionState::DeliveryUnknown;
            self.attempt(now, "lost");
            return Step {
                events: vec![EventPayload::InteractionDeliveryUnknown(
                    InteractionDelivery {
                        interaction: self.id,
                        rev: self.delivery.rev,
                    },
                )],
                effects: Vec::new(),
            };
        }
        Step::default()
    }

    /// The harness says what happened to the request (`InteractionResolved`).
    pub fn native_resolved(&mut self, resolution: &Resolution, now: Timestamp) -> Step {
        let rev = self.delivery.rev;
        match self.state {
            InteractionState::Open | InteractionState::DecisionRecorded => {
                let effects = self.release_effect("resolved_elsewhere");
                self.ended(InteractionState::ResolvedElsewhere, now);
                Step {
                    events: vec![self.resolved_elsewhere_event()],
                    effects,
                }
            }
            InteractionState::Delivering => {
                if self.delivery.acked {
                    self.ended(InteractionState::Delivered, now);
                    Step {
                        events: vec![self.delivered_event(rev)],
                        effects: Vec::new(),
                    }
                } else {
                    // The harness went on without the ack of our shim: someone else answered.
                    self.ended(InteractionState::ResolvedElsewhere, now);
                    Step {
                        events: vec![self.resolved_elsewhere_event()],
                        effects: Vec::new(),
                    }
                }
            }
            InteractionState::DeliveryUnknown => {
                match self.consistent(resolution) {
                    Some(true) => {
                        self.ended(InteractionState::Delivered, now);
                        Step {
                            events: vec![self.delivered_event(rev)],
                            effects: Vec::new(),
                        }
                    }
                    Some(false) => {
                        self.ended(InteractionState::ResolvedElsewhere, now);
                        Step {
                            events: vec![self.resolved_elsewhere_event()],
                            effects: Vec::new(),
                        }
                    }
                    // The harness does not say how it ended: the answer stays unconfirmed.
                    None => Step::default(),
                }
            }
            _ => Step::default(),
        }
    }

    /// Whether the resolution of the harness is the one that the decision asked for. `None` when
    /// the harness did not say.
    fn consistent(&self, resolution: &Resolution) -> Option<bool> {
        let decision = self.delivery.decision.as_ref()?;
        let wanted = match decision {
            Decision::Approval(d) => match d.behavior {
                Behavior::Allow => Resolution::Allowed,
                Behavior::Deny => Resolution::Denied,
            },
            Decision::Question(_) => Resolution::Answered,
            Decision::PlanReview(d) => {
                if d.approve {
                    Resolution::Allowed
                } else {
                    Resolution::Denied
                }
            }
        };
        match resolution {
            Resolution::Unknown => None,
            other => Some(*other == wanted),
        }
    }

    fn delivered_event(&self, rev: u32) -> EventPayload {
        EventPayload::InteractionDelivered(InteractionDelivery {
            interaction: self.id,
            rev,
        })
    }

    fn resolved_elsewhere_event(&self) -> EventPayload {
        EventPayload::InteractionResolvedElsewhere(InteractionEnded {
            interaction: self.id,
        })
    }

    /// The shim connects again (`adapter.gate_resume`). A decision that exists goes to it with the
    /// same key.
    pub fn resumed(&mut self, lease: Lease, now: Timestamp) -> Step {
        if self.is_final() {
            return Step::default();
        }
        self.delivery.lease = Some(lease);
        if matches!(
            self.state,
            InteractionState::DecisionRecorded
                | InteractionState::Delivering
                | InteractionState::DeliveryUnknown
        ) && self.delivery.decision.is_some()
        {
            // The old attempt is void: the new shim has not printed anything.
            self.attempt(now, "resumed");
            if self.state == InteractionState::Delivering {
                self.delivery.acked = false;
            }
            return Step {
                events: Vec::new(),
                effects: vec![Effect::Deliver],
            };
        }
        Step::default()
    }

    // --- endings -------------------------------------------------------------------------------

    /// The deadline passed (the gate timeout or the deadline of the harness).
    pub fn deadline_passed(&mut self, now: Timestamp) -> Step {
        let rev = self.delivery.rev;
        match self.state {
            InteractionState::Open | InteractionState::DecisionRecorded => {
                let effects = self.release_effect("deadline");
                self.attempt(now, "deadline");
                self.delivery.fail_reason = Some("deadline".to_owned());
                self.ended(InteractionState::Failed, now);
                Step {
                    events: vec![EventPayload::InteractionDeliveryFailed(
                        InteractionDeliveryFailed {
                            interaction: self.id,
                            rev,
                            reason: "deadline".to_owned(),
                        },
                    )],
                    effects,
                }
            }
            InteractionState::Delivering if !self.delivery.acked => {
                self.delivery.lease = None;
                self.state = InteractionState::DeliveryUnknown;
                self.attempt(now, "deadline");
                Step {
                    events: vec![EventPayload::InteractionDeliveryUnknown(
                        InteractionDelivery {
                            interaction: self.id,
                            rev,
                        },
                    )],
                    effects: Vec::new(),
                }
            }
            _ => Step::default(),
        }
    }

    /// The harness rejected the decision, or the dialog changed.
    pub fn failed(&mut self, reason: &str, now: Timestamp) -> Step {
        if !matches!(
            self.state,
            InteractionState::DecisionRecorded
                | InteractionState::Delivering
                | InteractionState::DeliveryUnknown
        ) {
            return Step::default();
        }
        let rev = self.delivery.rev;
        self.delivery.fail_reason = Some(reason.to_owned());
        self.ended(InteractionState::Failed, now);
        Step {
            events: vec![EventPayload::InteractionDeliveryFailed(
                InteractionDeliveryFailed {
                    interaction: self.id,
                    rev,
                    reason: reason.to_owned(),
                },
            )],
            effects: Vec::new(),
        }
    }

    /// The keys went in and the dialog went, and nothing confirmed it in time.
    pub fn unconfirmed(&mut self, now: Timestamp) -> Step {
        if self.state != InteractionState::Delivering {
            return Step::default();
        }
        self.state = InteractionState::DeliveryUnknown;
        self.attempt(now, "lost");
        Step {
            events: vec![EventPayload::InteractionDeliveryUnknown(
                InteractionDelivery {
                    interaction: self.id,
                    rev: self.delivery.rev,
                },
            )],
            effects: Vec::new(),
        }
    }

    /// The dialog went from the screen after the keys: that is the resolution when no hook says
    /// anything.
    pub fn screen_resolved(&mut self, now: Timestamp) -> Step {
        if self.state != InteractionState::Delivering || !self.delivery.acked {
            return Step::default();
        }
        let rev = self.delivery.rev;
        self.ended(InteractionState::Delivered, now);
        Step {
            events: vec![self.delivered_event(rev)],
            effects: Vec::new(),
        }
    }

    /// The interaction cannot go on: the process of the run ended, or the turn ended.
    pub fn cancel(&mut self, reason: &str, now: Timestamp) -> Step {
        if self.is_final() {
            return Step::default();
        }
        let effects = self.release_effect("cancelled");
        self.ended(InteractionState::Cancelled, now);
        Step {
            events: vec![EventPayload::InteractionCancelled(InteractionCancelled {
                interaction: self.id,
                reason: reason.to_owned(),
            })],
            effects,
        }
    }

    /// The user closes the interaction with no answer.
    pub fn dismiss(&mut self, now: Timestamp) -> Result<Step, Refusal> {
        match self.state {
            InteractionState::Open => {
                let effects = self.release_effect("dismissed");
                self.ended(InteractionState::Dismissed, now);
                Ok(Step {
                    events: vec![EventPayload::InteractionDismissed(InteractionEnded {
                        interaction: self.id,
                    })],
                    effects,
                })
            }
            InteractionState::DecisionRecorded
            | InteractionState::Delivering
            | InteractionState::Delivered
            | InteractionState::DeliveryUnknown => Err(Refusal::AlreadyAnswered {
                by: self.delivery.decided_by.clone(),
                via: self.delivery.via,
                decision: self.delivery.decision.clone(),
            }),
            other => Err(Refusal::Closed(other)),
        }
    }

    /// The pane gets the focus while the hook is held: the shim prints nothing, the harness shows
    /// its own dialog, and the interaction goes on in `observe` mode.
    pub fn release_to_observe(&mut self, now: Timestamp) -> Step {
        if self.state != InteractionState::Open || self.mode != GateMode::Gate {
            return Step::default();
        }
        self.mode = GateMode::Observe;
        // The hook is not held any more, so no decision can reach it.
        self.channels
            .retain(|channel| *channel != AnswerChannel::Native);
        // The harness shows its own dialog now, and a key can answer it.
        if self.keystroke && !self.channels.contains(&AnswerChannel::Keystroke) {
            self.channels.push(AnswerChannel::Keystroke);
            self.answerable = true;
        }
        self.answerable = self.answerable && !self.channels.is_empty();
        let effects = self.release_effect("focused");
        if !effects.is_empty() {
            self.attempt(now, "released");
        }
        self.delivery.lease = None;
        self.deadline = None;
        Step {
            events: Vec::new(),
            effects,
        }
    }

    /// The run of the interaction is not bound to its pane any more (`agent.unlink`). No client
    /// may answer it, and the hook that a shim holds goes on to its own deadline.
    pub fn unbind(&mut self) {
        self.answerable = false;
        self.keystroke = false;
        self.channels.clear();
    }
}

fn validate_approval(d: &ApprovalDecision) -> Result<(), Refusal> {
    let invalid = |text: &str| Err(Refusal::Invalid(text.to_owned()));
    if d.always && d.behavior == Behavior::Deny {
        return invalid("only an allow can be \"always\"");
    }
    if d.persist && !d.always {
        return invalid("persist needs always");
    }
    if d.stop && d.behavior == Behavior::Allow {
        return invalid("only a deny can stop the turn");
    }
    if d.message.is_some() && d.behavior == Behavior::Allow {
        return invalid("only a deny has a message");
    }
    Ok(())
}

fn validate_question(
    payload: &nh_proto::api::model::QuestionPayload,
    decision: &QuestionDecision,
) -> Result<(), Refusal> {
    let invalid = |text: String| Err(Refusal::Invalid(text));
    if decision.answers.is_empty() {
        return invalid("a question needs at least one answer".to_owned());
    }
    for answer in &decision.answers {
        let Some(question) = payload
            .questions
            .iter()
            .find(|q| q.question == answer.question)
        else {
            return invalid(format!("no question \"{}\"", answer.question));
        };
        if answer.selected.is_empty() && answer.text.as_deref().is_none_or(str::is_empty) {
            return invalid(format!("no answer for \"{}\"", answer.question));
        }
        if !question.multi_select && answer.selected.len() > 1 {
            return invalid(format!("\"{}\" takes one option", answer.question));
        }
        for label in &answer.selected {
            if !question.options.iter().any(|o| &o.label == label) {
                return invalid(format!(
                    "\"{label}\" is not an option of \"{}\"",
                    answer.question
                ));
            }
        }
    }
    Ok(())
}

fn validate_plan(d: &PlanDecision) -> Result<(), Refusal> {
    if !d.approve && d.feedback.as_deref().is_none_or(|f| f.trim().is_empty()) {
        return Err(Refusal::Invalid(
            "requesting changes needs the feedback".to_owned(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
