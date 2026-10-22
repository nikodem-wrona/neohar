//! The adapter of a harness.
//!
//! An adapter turns what a harness says into what the core understands. A hook shim delivers a
//! hook event as it got it (`adapter.signal`), and the adapter reads the payload and says what
//! happened with [`AdapterSignal`]s: a session started, a turn began, a tool ran, a dialog opened.
//! The core gives those to the arbiter and to the interactions.
//!
//! The adapter has no access to the state of the core. It gets an [`AdapterCtx`] when it starts,
//! and it reports through the [`Emitter`] of that context. A call that a blocking hook waits for
//! returns a [`SignalReply`].
//!
//! [`hooks::ManifestAdapter`] reads the `[states]` table of a manifest, so that both harnesses
//! show their execution state as soon as their hooks fire. The adapters of Claude Code and Codex
//! read the payloads in full.

pub mod effects;
pub mod hooks;
pub mod kit;

use std::{
    path::PathBuf,
    sync::{Arc, mpsc},
};

use async_trait::async_trait;
use nh_proto::api::{
    calls::agent::Decision,
    common::Harness,
    ids::{PaneId, RunId},
    model::{GateMode, InteractionKind, InteractionValue},
    time::Timestamp,
};
use serde_json::Value;

use crate::manifest::Manifest;

/// How signals reach the core.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Transport {
    Hooks,
    Transcript,
    Screen,
    Process,
    StreamJson,
    AppServer,
}

/// What an adapter can do, before the resolver intersects it with the version ranges and the golden
/// coverage.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AdapterCaps {
    pub transports: Vec<Transport>,
    pub observe: bool,
    pub gate: bool,
    pub answer_native: Vec<InteractionKind>,
    pub answer_keystroke: bool,
    pub reconcile: bool,
    pub resume: bool,
    pub steer: bool,
    pub survive_disconnect: bool,
    /// The adapter reports turns and items for the timeline.
    pub turns_items: bool,
    pub usage: bool,
    pub subagents: bool,
}

/// Why a session started (`SessionStart` of Claude Code has the field `source`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StartSource {
    Startup,
    Resume,
    Clear,
    Compact,
    Other(String),
}

impl StartSource {
    pub fn parse(text: &str) -> Self {
        match text {
            "startup" => Self::Startup,
            "resume" => Self::Resume,
            "clear" => Self::Clear,
            "compact" => Self::Compact,
            other => Self::Other(other.to_owned()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileOp {
    Create,
    Edit,
    Delete,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoticeKind {
    Info,
    Warning,
}

/// The start or the end of a compaction of the context.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Start,
    End,
}

/// How an interaction ended in the harness itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Resolution {
    Allowed,
    Denied,
    Answered,
    /// The harness does not say which.
    Unknown,
}

/// What an adapter says about a run.
#[derive(Clone, Debug, PartialEq)]
pub enum AdapterSignal {
    SessionStarted {
        session_id: String,
        transcript_path: Option<PathBuf>,
        source: StartSource,
        model: Option<String>,
    },
    TurnStarted {
        turn_ref: Option<String>,
        prompt_preview: Option<String>,
    },
    TurnEnded {
        turn_ref: Option<String>,
        stop_reason: Option<String>,
        last_message_preview: Option<String>,
    },
    ToolStarted {
        call_id: String,
        tool: String,
        input: Value,
    },
    ToolEnded {
        call_id: String,
        tool: String,
        ok: bool,
        summary: Option<String>,
    },
    FileChanged {
        path: PathBuf,
        op: FileOp,
    },
    InteractionOpen {
        kind: InteractionKind,
        native_ref: String,
        payload: Value,
        gate: GateMode,
    },
    InteractionResolved {
        native_ref: String,
        resolution: Resolution,
    },
    Notice {
        kind: NoticeKind,
        message: String,
    },
    Usage {
        input: u64,
        output: u64,
        cache_read: u64,
        cache_write: u64,
        cost_usd: Option<f64>,
        model: Option<String>,
    },
    RateLimited {
        resets_at: Option<Timestamp>,
        scope: Option<String>,
    },
    Error {
        kind: String,
        message: String,
        retrying: bool,
    },
    SubagentStarted {
        native_id: String,
        kind: Option<String>,
    },
    SubagentEnded {
        native_id: String,
    },
    Compacting {
        phase: Phase,
    },
    /// The permission mode of the harness, as a hook names it (`default`, `plan`, `acceptEdits`,
    /// `bypassPermissions`). A mode without approval dialogs makes the run `yolo`.
    PermissionMode {
        mode: String,
    },
    SessionEnded {
        reason: String,
    },
    /// The event as the harness sent it. The core hands every event to the adapter in this form,
    /// and the golden tests and the logs use it.
    Raw {
        harness_event: String,
        payload: Value,
    },
}

/// The JSON that a blocking hook prints to stdout, made by the core.
#[derive(Clone, Debug, PartialEq)]
pub struct HookDecision {
    pub output: String,
    pub idempotency_key: String,
}

/// A handle for a decision that comes later.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WaitToken(pub String);

/// What a blocking hook gets back.
#[derive(Clone, Debug, PartialEq)]
pub enum SignalReply {
    /// Nothing to say. The shim prints nothing, and the harness goes on or shows its own dialog.
    Continue,
    /// A decision at once (the policy fast path).
    Decision(HookDecision),
    /// The decision comes later. The shim keeps the hook open.
    Wait(WaitToken),
}

/// Why the run stops its adapter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StopReason {
    /// The process of the run ended.
    ProcessExited,
    /// The core ends.
    Shutdown,
    /// The adapter failed and the host drops it.
    Failed,
}

/// The id that lets a delivery repeat without a second effect.
pub type IdempotencyKey = String;

/// What happened to a decision that an adapter delivered by its own channel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeliveryOutcome {
    /// The channel took the decision.
    Delivered,
    /// The channel refused it, with the reason.
    Failed(String),
    /// The adapter cannot say whether the decision arrived.
    Unknown,
    /// The adapter has no channel of its own for this kind: the hook shim carries the decision.
    NotMine,
}

/// What the harness reports after a restart, as the truth about its pending requests.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReconcileReport {
    /// The `native_ref` of each request that the harness still waits on.
    pub pending: Vec<String>,
}

/// A prompt that the user sends to a run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PromptInput {
    pub text: String,
}

/// What an adapter reports through [`Emitter::emit`]. The core reads these in order.
#[derive(Clone)]
pub struct Emitter {
    sender: mpsc::Sender<AdapterSignal>,
}

impl Emitter {
    /// A channel for the signals of an adapter: the emitter for the adapter, and the receiver for
    /// the host.
    pub fn channel() -> (Self, mpsc::Receiver<AdapterSignal>) {
        let (sender, receiver) = mpsc::channel();
        (Self { sender }, receiver)
    }

    /// Reports a signal. A host that is gone drops it.
    pub fn emit(&self, signal: AdapterSignal) {
        let _ = self.sender.send(signal);
    }
}

/// What an adapter gets when it starts.
#[derive(Clone)]
pub struct AdapterCtx {
    pub run: RunId,
    pub pane: Option<PaneId>,
    pub harness: Harness,
    /// The manifest of the run, as it was when the adapter started.
    pub manifest: Arc<Manifest>,
    pub emitter: Emitter,
}

/// A failure of an adapter call. The host marks the adapter `degraded` and goes on.
#[derive(Debug, thiserror::Error)]
pub enum AdapterError {
    #[error("{0}")]
    Failed(String),
}

pub type Result<T> = std::result::Result<T, AdapterError>;

/// One instance for each run. A host runs it in a task of its own, so a call can wait
/// without blocking the others, and a panic ends only this adapter.
#[async_trait]
pub trait Adapter: Send + 'static {
    /// What this adapter can do. The resolver intersects it with the manifest and the golden
    /// coverage.
    fn capabilities(&self) -> AdapterCaps;

    /// The run exists. The adapter keeps the context and reports through its emitter.
    async fn start(&mut self, ctx: AdapterCtx) -> Result<()>;

    /// A hook event, or a message of a protocol, as it came (`AdapterSignal::Raw`). The adapter
    /// reads it, reports what happened through its emitter, and says what a blocking hook gets.
    async fn on_signal(&mut self, signal: AdapterSignal) -> Result<SignalReply>;

    /// A rule of the screen manifest matched. A structured adapter only adds information from it
    /// (rule 3 of the arbiter).
    async fn on_screen(&mut self, _matched: &str) -> Result<()> {
        Ok(())
    }

    /// Delivers a decision by the channel of the adapter. The call is safe to repeat with the same
    /// key. An adapter whose decisions travel in a hook reply says [`DeliveryOutcome::NotMine`].
    async fn deliver(
        &mut self,
        _interaction: &InteractionValue,
        _decision: &Decision,
        _key: IdempotencyKey,
    ) -> Result<DeliveryOutcome> {
        Ok(DeliveryOutcome::NotMine)
    }

    /// Asks the harness for the truth about its requests. Only an adapter with the capability
    /// `reconcile` has an answer.
    async fn reconcile(&mut self) -> Result<Option<ReconcileReport>> {
        Ok(None)
    }

    /// Sends a prompt (a protocol message, or bracketed paste and Enter in a TUI).
    async fn prompt(&mut self, _prompt: PromptInput) -> Result<()> {
        Err(AdapterError::Failed(
            "this adapter cannot send a prompt".to_owned(),
        ))
    }

    /// Interrupts the turn (Esc, an interrupt message).
    async fn interrupt(&mut self) -> Result<()> {
        Err(AdapterError::Failed(
            "this adapter cannot interrupt".to_owned(),
        ))
    }

    /// The resume handle of the run, if the adapter knows one.
    fn resume_handle(&self) -> Option<Vec<String>> {
        None
    }

    /// The run ends. The adapter lets go of what it holds.
    async fn stop(&mut self, _reason: StopReason) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests;
