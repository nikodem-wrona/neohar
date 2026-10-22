//! What the core does with a signal of an adapter.
//!
//! The mapping is pure, so that a table of tests can read it: a signal gives the observations for
//! the arbiter and a few facts that the run keeps. The core applies the effects in
//! order. A signal that changes no state gives none.

use nh_proto::api::model::Execution;

use super::{AdapterSignal, StartSource};
use crate::arbiter::Observation;

/// One effect of a signal.
#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    /// Something for the arbiter, from the structured source.
    Observe(Observation),
    /// The session id of the run, with the facts that come with it.
    Identify {
        session_id: String,
        transcript_path: Option<String>,
        model: Option<String>,
    },
    /// The permission mode of the harness: whether the run has approval dialogs.
    PermissionMode { yolo: bool },
}

/// Whether a permission mode has no approval dialog for the tools.
pub fn mode_is_yolo(mode: &str) -> bool {
    matches!(mode, "bypassPermissions" | "dontAsk")
}

/// The effects of a signal, in order.
pub fn effects(signal: &AdapterSignal) -> Vec<Effect> {
    use Observation::{Execution as State, SessionEnded, TurnEnded};
    match signal {
        AdapterSignal::SessionStarted {
            session_id,
            transcript_path,
            model,
            source,
        } => {
            let mut out = vec![Effect::Identify {
                session_id: session_id.clone(),
                transcript_path: transcript_path
                    .as_ref()
                    .map(|path| path.display().to_string()),
                model: model.clone(),
            }];
            // A new session is idle until a prompt comes. `SessionStart` after a compaction
            // comes in the middle of a turn, so it says nothing about the state.
            if matches!(
                source,
                StartSource::Startup | StartSource::Resume | StartSource::Clear
            ) {
                out.push(Effect::Observe(State(Execution::Idle)));
            }
            out
        }
        AdapterSignal::TurnStarted { .. }
        | AdapterSignal::ToolStarted { .. }
        | AdapterSignal::SubagentStarted { .. } => vec![Effect::Observe(State(Execution::Working))],
        AdapterSignal::Compacting { phase } => match phase {
            // The state stays `working` while the context is compacted.
            super::Phase::Start => vec![Effect::Observe(State(Execution::Working))],
            super::Phase::End => Vec::new(),
        },
        AdapterSignal::TurnEnded { .. } => vec![Effect::Observe(TurnEnded)],
        AdapterSignal::RateLimited { resets_at, .. } => {
            vec![Effect::Observe(State(Execution::RateLimited {
                resets_at: *resets_at,
            }))]
        }
        AdapterSignal::Error {
            retrying: false, ..
        } => vec![Effect::Observe(State(Execution::Error))],
        // `/clear` ends a session and starts another one in the same process. The run stays.
        AdapterSignal::SessionEnded { reason } if reason == "clear" => Vec::new(),
        AdapterSignal::SessionEnded { .. } => vec![Effect::Observe(SessionEnded)],
        AdapterSignal::PermissionMode { mode } => vec![Effect::PermissionMode {
            yolo: mode_is_yolo(mode),
        }],
        // The interactions of the core read the interaction signals. The rest changes no state.
        AdapterSignal::ToolEnded { .. }
        | AdapterSignal::FileChanged { .. }
        | AdapterSignal::InteractionOpen { .. }
        | AdapterSignal::InteractionResolved { .. }
        | AdapterSignal::Notice { .. }
        | AdapterSignal::Usage { .. }
        | AdapterSignal::Error { retrying: true, .. }
        | AdapterSignal::SubagentEnded { .. }
        | AdapterSignal::Raw { .. } => Vec::new(),
    }
}
