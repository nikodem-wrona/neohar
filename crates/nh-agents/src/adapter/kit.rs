//! What the adapters of the hooks of both harnesses share.
//!
//! Claude Code and Codex send hooks with the same fields: a session, a turn, tools that start and
//! end, and requests for permission. The [`Kit`] holds what an adapter must remember between two
//! hooks, and it makes the signals that do not depend on the harness:
//!
//! - **The session.** The first name, or a new one, gives `SessionStarted`. A hook other than
//!   `SessionStart` names a session that began before the adapter saw it, so the source is `seen`
//!   and the state stays as it is. A `SessionEnd` of another session is not the end of the run: Codex
//!   ends an idle thread while the process runs on a new one.
//! - **The calls just before.** A `PermissionRequest` has no `tool_use_id`. Its native ref is the
//!   `tool_use_id` of the `PreToolUse` that came just before with the same tool and the same input.
//!   The kit keeps the last 16 calls and uses each once.
//! - **The interactions it opened.** The end of a call resolves what the call opened.

use std::{collections::VecDeque, path::PathBuf};

use nh_proto::api::model::{GateMode, InteractionKind};
use serde_json::{Value, json};

use super::{AdapterCtx, AdapterSignal, FileOp, Resolution, StartSource};

/// How many calls the kit keeps to find the native ref of a request.
const KEEP_PRE: usize = 16;

/// The longest preview of a prompt or of a message, in characters.
const PREVIEW: usize = 200;

/// A call that `PreToolUse` announced.
pub struct Call {
    pub id: String,
    pub tool: String,
    pub input: Value,
}

struct Open {
    native_ref: String,
    kind: InteractionKind,
}

#[derive(Default)]
pub struct Kit {
    ctx: Option<AdapterCtx>,
    session: Option<String>,
    mode: Option<String>,
    calls: VecDeque<Call>,
    open: Vec<Open>,
}

/// A string field of a payload, when it has one that is not empty.
pub fn text<'a>(payload: &'a Value, key: &str) -> Option<&'a str> {
    payload
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
}

/// The first `PREVIEW` characters of a text, with an ellipsis when it is longer.
pub fn preview(text: &str) -> String {
    let mut chars = text.chars();
    let head: String = chars.by_ref().take(PREVIEW).collect();
    if chars.next().is_some() {
        format!("{head}…")
    } else {
        head
    }
}

/// The part of a payload that says which subagent the hook comes from.
pub fn agent(payload: &Value) -> Option<Value> {
    let id = text(payload, "agent_id")?;
    Some(json!({"id": id, "type": text(payload, "agent_type")}))
}

/// Whether the input of a request is the input of a call. The inputs are equal for Claude Code. A
/// request of Codex adds a `description` (the reason for the command) to the input of the call, so
/// two inputs with the same `command` match as well.
fn same_input(call: &Value, request: &Value) -> bool {
    call == request
        || matches!((call.get("command"), request.get("command")), (Some(a), Some(b)) if a == b)
}

impl Kit {
    pub fn start(&mut self, ctx: AdapterCtx) {
        self.ctx = Some(ctx);
    }

    pub fn ctx(&self) -> Option<&AdapterCtx> {
        self.ctx.as_ref()
    }

    pub fn session(&self) -> Option<&str> {
        self.session.as_deref()
    }

    pub fn emit(&self, signal: AdapterSignal) {
        if let Some(ctx) = &self.ctx {
            ctx.emitter.emit(signal);
        }
    }

    /// Every hook names its session. The first name, or a new one, is a session that starts.
    pub fn session_of(&mut self, event: &str, payload: &Value) {
        let Some(id) = text(payload, "session_id") else {
            return;
        };
        if self.session.as_deref() == Some(id) {
            return;
        }
        self.session = Some(id.to_owned());
        self.emit(AdapterSignal::SessionStarted {
            session_id: id.to_owned(),
            transcript_path: text(payload, "transcript_path").map(PathBuf::from),
            source: if event == "SessionStart" {
                text(payload, "source").map_or(StartSource::Startup, StartSource::parse)
            } else {
                StartSource::Other("seen".to_owned())
            },
            model: text(payload, "model").map(str::to_owned),
        });
    }

    /// Whether the hook is a `SessionEnd` of a session that is not the session of the run. Codex
    /// shuts down an old idle thread while the process runs on a new one. Before the run
    /// has a session, every `SessionEnd` is its own.
    pub fn ends_other_session(&self, event: &str, payload: &Value) -> bool {
        event == "SessionEnd"
            && matches!(
                (self.session.as_deref(), text(payload, "session_id")),
                (Some(run), Some(id)) if run != id
            )
    }

    /// The permission mode is in the hooks that belong to a turn. A change of it is a signal.
    pub fn mode_of(&mut self, payload: &Value) {
        let Some(mode) = text(payload, "permission_mode") else {
            return;
        };
        if self.mode.as_deref() != Some(mode) {
            self.mode = Some(mode.to_owned());
            self.emit(AdapterSignal::PermissionMode {
                mode: mode.to_owned(),
            });
        }
    }

    /// A tool starts. Returns the call, which the kit also remembers for a request that follows.
    pub fn tool_started(&mut self, payload: &Value) -> Option<(String, String, Value)> {
        let (id, tool) = (text(payload, "tool_use_id")?, text(payload, "tool_name")?);
        let input = payload.get("tool_input").cloned().unwrap_or(Value::Null);
        self.emit(AdapterSignal::ToolStarted {
            call_id: id.to_owned(),
            tool: tool.to_owned(),
            input: input.clone(),
        });
        self.calls.push_back(Call {
            id: id.to_owned(),
            tool: tool.to_owned(),
            input: input.clone(),
        });
        while self.calls.len() > KEEP_PRE {
            self.calls.pop_front();
        }
        Some((id.to_owned(), tool.to_owned(), input))
    }

    /// The native ref of a request for permission: the call before it with the same tool and
    /// input, used once. A request with no call before it gets a ref made from what it says, so
    /// that the same request is the same interaction.
    pub fn request_ref(&mut self, payload: &Value) -> Option<(String, String, Value)> {
        let tool = text(payload, "tool_name")?;
        let input = payload.get("tool_input").cloned().unwrap_or(Value::Null);
        let found = self
            .calls
            .iter()
            .rposition(|call| call.tool == tool && same_input(&call.input, &input))
            .and_then(|at| self.calls.remove(at));
        let native_ref = match found {
            Some(call) => call.id,
            None => format!(
                "request:{}:{}",
                text(payload, "prompt_id")
                    .or_else(|| text(payload, "turn_id"))
                    .unwrap_or("-"),
                blake3::hash(
                    &serde_json::to_vec(&[json!(tool), input.clone()]).unwrap_or_default()
                )
                .to_hex()
                .chars()
                .take(16)
                .collect::<String>()
            ),
        };
        Some((native_ref, tool.to_owned(), input))
    }

    pub fn is_open(&self, native_ref: &str) -> bool {
        self.open.iter().any(|open| open.native_ref == native_ref)
    }

    /// Opens an interaction, and tells the core.
    pub fn open_interaction(&mut self, kind: InteractionKind, native_ref: String, payload: Value) {
        self.open.push(Open {
            native_ref: native_ref.clone(),
            kind,
        });
        self.emit(AdapterSignal::InteractionOpen {
            kind,
            native_ref,
            payload,
            gate: GateMode::Gate,
        });
    }

    /// Closes the interaction of a native ref, and tells the core.
    pub fn resolve(&mut self, native_ref: &str, resolution: Resolution) {
        let Some(at) = self
            .open
            .iter()
            .position(|open| open.native_ref == native_ref)
        else {
            return;
        };
        self.open.remove(at);
        self.emit(AdapterSignal::InteractionResolved {
            native_ref: native_ref.to_owned(),
            resolution,
        });
    }

    /// The oldest open interaction of a kind.
    pub fn first_open(&self, kind: InteractionKind) -> Option<String> {
        self.open
            .iter()
            .find(|open| open.kind == kind)
            .map(|open| open.native_ref.clone())
    }

    /// The native refs of the open interactions that start with `prefix`.
    pub fn open_with_prefix(&self, prefix: &str) -> Vec<String> {
        self.open
            .iter()
            .filter(|open| open.native_ref.starts_with(prefix))
            .map(|open| open.native_ref.clone())
            .collect()
    }

    /// A tool ended. The call is forgotten, a file change is reported, and the interaction that the
    /// call opened is resolved. Returns the tool and the id of the call.
    pub fn tool_ended(
        &mut self,
        payload: &Value,
        ok: bool,
        file_tools: &[(&str, &str)],
    ) -> Option<(String, String)> {
        let (id, tool) = (text(payload, "tool_use_id")?, text(payload, "tool_name")?);
        self.calls.retain(|call| call.id != id);
        self.emit(AdapterSignal::ToolEnded {
            call_id: id.to_owned(),
            tool: tool.to_owned(),
            ok,
            summary: None,
        });
        if ok
            && let Some(path) = file_tools.iter().find_map(|(name, key)| {
                (*name == tool)
                    .then(|| payload.get("tool_input").and_then(|input| text(input, key)))
                    .flatten()
            })
        {
            let created = payload
                .get("tool_response")
                .and_then(|response| text(response, "type"))
                == Some("create");
            self.emit(AdapterSignal::FileChanged {
                path: PathBuf::from(path),
                op: if created {
                    FileOp::Create
                } else {
                    FileOp::Edit
                },
            });
        }
        Some((id.to_owned(), tool.to_owned()))
    }

    /// The turn is over. The kit forgets the calls and the interactions of the turn; the core
    /// closes what it had open for it (rule 2 of the arbiter).
    pub fn turn_over(&mut self) {
        self.calls.clear();
        self.open.clear();
    }
}
