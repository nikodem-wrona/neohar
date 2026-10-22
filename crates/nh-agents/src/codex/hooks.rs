//! The adapter of Codex in TUI mode.
//!
//! Codex sends the hooks of Claude Code with fewer events. The recordings of Codex 0.156.1 show:
//!
//! - `SessionStart` fires with the first prompt, not when the interface starts.
//! - `PreToolUse` and `PostToolUse` carry `tool_name: "Bash"`, `tool_input: {command}` and a
//!   `tool_use_id` (`call_…`).
//! - `PermissionRequest` has `tool_input: {command, description}` and no `tool_use_id`, so the
//!   native ref is the one of the `PreToolUse` before it, as for Claude Code. A deny shows the model
//!   `exec_command failed … Rejected("<message>")`, and no `PostToolUse` follows.
//! - Under `-a never` and `danger-full-access` every hook fires except `PermissionRequest`, and the
//!   `permission_mode` is `bypassPermissions`, which makes the run `yolo`.
//!
//! The adapter has no question and no plan review: Codex raises those through its app-server
//! protocol, which the observer reads. It decides nothing: the interactions of the core decide.

use async_trait::async_trait;
use nh_proto::api::model::InteractionKind;
use serde_json::{Value, json};

use crate::adapter::{
    Adapter, AdapterCaps, AdapterCtx, AdapterSignal, Phase, Resolution, Result, SignalReply,
    StopReason, Transport,
    kit::{Kit, agent, preview, text},
};

#[derive(Default)]
pub struct CodexHooks {
    kit: Kit,
}

impl CodexHooks {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl Adapter for CodexHooks {
    fn capabilities(&self) -> AdapterCaps {
        AdapterCaps {
            transports: vec![Transport::Hooks, Transport::Transcript, Transport::Screen],
            observe: true,
            gate: true,
            answer_native: vec![InteractionKind::Approval],
            answer_keystroke: true,
            reconcile: false,
            resume: true,
            steer: false,
            survive_disconnect: true,
            turns_items: true,
            usage: true,
            subagents: true,
        }
    }

    async fn start(&mut self, ctx: AdapterCtx) -> Result<()> {
        self.kit.start(ctx);
        Ok(())
    }

    async fn on_signal(&mut self, signal: AdapterSignal) -> Result<SignalReply> {
        let AdapterSignal::Raw {
            harness_event,
            payload,
        } = signal
        else {
            return Ok(SignalReply::Continue);
        };
        if self.kit.ends_other_session(&harness_event, &payload) {
            // An old thread ended. The run goes on in its own session, so only the raw signal.
            self.kit.emit(AdapterSignal::Raw {
                harness_event,
                payload,
            });
            return Ok(SignalReply::Continue);
        }
        self.kit.session_of(&harness_event, &payload);
        self.kit.mode_of(&payload);
        match harness_event.as_str() {
            "UserPromptSubmit" => self.kit.emit(AdapterSignal::TurnStarted {
                turn_ref: text(&payload, "turn_id").map(str::to_owned),
                prompt_preview: text(&payload, "prompt").map(preview),
            }),
            "PreToolUse" => {
                self.kit.tool_started(&payload);
            }
            "PermissionRequest" => self.permission_request(&payload),
            "PostToolUse" => {
                if let Some((id, _)) = self.kit.tool_ended(&payload, true, &[]) {
                    // The command ran, so the request for it ended: allowed by the user in the
                    // dialog or by neohar. The core knows which.
                    self.kit.resolve(&id, Resolution::Allowed);
                }
            }
            "Stop" => {
                self.kit.turn_over();
                self.kit.emit(AdapterSignal::TurnEnded {
                    turn_ref: text(&payload, "turn_id").map(str::to_owned),
                    stop_reason: None,
                    last_message_preview: text(&payload, "last_assistant_message").map(preview),
                });
            }
            "Interrupt" => {
                self.kit.turn_over();
                self.kit.emit(AdapterSignal::TurnEnded {
                    turn_ref: text(&payload, "turn_id").map(str::to_owned),
                    stop_reason: Some("interrupt".to_owned()),
                    last_message_preview: None,
                });
            }
            "SubagentStart" => {
                if let Some(id) = text(&payload, "agent_id") {
                    self.kit.emit(AdapterSignal::SubagentStarted {
                        native_id: id.to_owned(),
                        kind: text(&payload, "agent_type").map(str::to_owned),
                    });
                }
            }
            "SubagentStop" => {
                if let Some(id) = text(&payload, "agent_id") {
                    self.kit.emit(AdapterSignal::SubagentEnded {
                        native_id: id.to_owned(),
                    });
                }
            }
            "PreCompact" => self.kit.emit(AdapterSignal::Compacting {
                phase: Phase::Start,
            }),
            "PostCompact" => self
                .kit
                .emit(AdapterSignal::Compacting { phase: Phase::End }),
            "SessionEnd" => self.kit.emit(AdapterSignal::SessionEnded {
                reason: text(&payload, "reason").unwrap_or("ended").to_owned(),
            }),
            // `SessionStart` is the session, and the events that come later have no signal.
            _ => {}
        }
        self.kit.emit(AdapterSignal::Raw {
            harness_event,
            payload,
        });
        Ok(SignalReply::Continue)
    }

    fn resume_handle(&self) -> Option<Vec<String>> {
        let session = self.kit.session()?;
        let argv = &self.kit.ctx()?.manifest.resume.as_ref()?.argv;
        Some(
            argv.iter()
                .map(|arg| arg.replace("{resume_id}", session))
                .collect(),
        )
    }

    async fn stop(&mut self, _reason: StopReason) -> Result<()> {
        self.kit.turn_over();
        Ok(())
    }
}

impl CodexHooks {
    fn permission_request(&mut self, payload: &Value) {
        let Some((native_ref, tool, input)) = self.kit.request_ref(payload) else {
            return;
        };
        if self.kit.is_open(&native_ref) {
            return;
        }
        self.kit.open_interaction(
            InteractionKind::Approval,
            native_ref,
            json!({
                "tool": tool,
                "command": input.get("command"),
                "description": input.get("description"),
                "paths": Vec::<String>::new(),
                "input": input,
                "permission_mode": text(payload, "permission_mode"),
                "agent": agent(payload),
            }),
        );
    }
}

#[cfg(test)]
mod tests;
