//! The adapter of Claude Code in TUI mode.
//!
//! One function for each row of the table of hooks. The adapter reads the payload that the hook
//! wrote to stdin and says what happened with [`AdapterSignal`]s. It decides nothing: the answer to
//! a blocking hook comes from the interactions of the core, so every call returns `Continue`.
//!
//! What it must remember between two hooks is in the [`Kit`] that both harnesses share. Claude
//! Code adds one thing: `AskUserQuestion` opens its interaction at `PreToolUse`, and a dialog that
//! Claude Code shows for it fires `PermissionRequest` as well. The adapter opens one interaction
//! for the pair. A `PostToolUse` that follows resolves it.
//!
//! The events that the golden recordings do not have (`PermissionDenied`, `PostToolUseFailure`,
//! `StopFailure`, `SubagentStart`, `SubagentStop`, `PreCompact`, `PostCompact`, `CwdChanged`) are
//! read from the documentation of the hooks, and so are the fields that they use.

use async_trait::async_trait;
use nh_proto::api::model::InteractionKind;
use serde_json::{Value, json};

use crate::adapter::{
    Adapter, AdapterCaps, AdapterCtx, AdapterSignal, NoticeKind, Phase, Resolution, Result,
    SignalReply, StopReason, Transport,
    kit::{Kit, agent, preview, text},
};

/// The tools that change a file, and the key of the path in their input.
const FILE_TOOLS: &[(&str, &str)] = &[
    ("Edit", "file_path"),
    ("Write", "file_path"),
    ("MultiEdit", "file_path"),
    ("NotebookEdit", "notebook_path"),
];

#[derive(Default)]
pub struct ClaudeHooks {
    kit: Kit,
}

impl ClaudeHooks {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl Adapter for ClaudeHooks {
    fn capabilities(&self) -> AdapterCaps {
        AdapterCaps {
            transports: vec![Transport::Hooks, Transport::Transcript, Transport::Screen],
            observe: true,
            gate: true,
            answer_native: vec![InteractionKind::Approval, InteractionKind::PlanReview],
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
            "SessionStart" => {}
            "UserPromptSubmit" => self.kit.emit(AdapterSignal::TurnStarted {
                turn_ref: text(&payload, "prompt_id").map(str::to_owned),
                prompt_preview: text(&payload, "prompt").map(preview),
            }),
            "PreToolUse" => self.pre_tool_use(&payload),
            "PermissionRequest" => self.permission_request(&payload),
            "PermissionDenied" => self.permission_denied(&payload),
            "PostToolUse" => self.post_tool_use(&payload, true),
            "PostToolUseFailure" => self.post_tool_use(&payload, false),
            "Notification" => self.notification(&payload),
            "Stop" => {
                self.kit.turn_over();
                self.kit.emit(AdapterSignal::TurnEnded {
                    turn_ref: text(&payload, "prompt_id").map(str::to_owned),
                    stop_reason: None,
                    last_message_preview: text(&payload, "last_assistant_message").map(preview),
                });
            }
            "StopFailure" => self.stop_failure(&payload),
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
            "Elicitation" => self.elicitation(&payload),
            "SessionEnd" => self.kit.emit(AdapterSignal::SessionEnded {
                reason: text(&payload, "reason").unwrap_or("ended").to_owned(),
            }),
            // `CwdChanged` and the events that come later have no signal.
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

impl ClaudeHooks {
    fn pre_tool_use(&mut self, payload: &Value) {
        let Some((id, tool, input)) = self.kit.tool_started(payload) else {
            return;
        };
        if tool == "AskUserQuestion" {
            // The question opens now. The dialog of Claude Code, if it comes, is the same one.
            self.kit.open_interaction(
                InteractionKind::Question,
                id,
                json!({
                    "tool": tool,
                    "questions": input.get("questions").cloned().unwrap_or(Value::Null),
                    "agent": agent(payload),
                }),
            );
        }
    }

    fn permission_request(&mut self, payload: &Value) {
        let Some((native_ref, tool, input)) = self.kit.request_ref(payload) else {
            return;
        };
        if self.kit.is_open(&native_ref) {
            // The question that `PreToolUse` opened: the same dialog.
            return;
        }
        if tool == "ExitPlanMode" {
            self.kit.open_interaction(
                InteractionKind::PlanReview,
                native_ref,
                json!({
                    "tool": tool,
                    "plan_md": input.get("plan"),
                    "plan_file": input.get("planFilePath"),
                    "agent": agent(payload),
                }),
            );
            return;
        }
        let paths: Vec<&str> = ["file_path", "notebook_path", "path"]
            .iter()
            .filter_map(|key| input.get(key).and_then(Value::as_str))
            .collect();
        self.kit.open_interaction(
            InteractionKind::Approval,
            native_ref,
            json!({
                "tool": tool,
                "command": input.get("command"),
                "paths": paths,
                "input": input,
                "permission_mode": text(payload, "permission_mode"),
                "suggestions": payload.get("permission_suggestions"),
                "agent": agent(payload),
            }),
        );
    }

    fn permission_denied(&mut self, payload: &Value) {
        // The hook names the call when it can. Without it, the oldest open approval is the one.
        let native_ref = text(payload, "tool_use_id")
            .map(str::to_owned)
            .or_else(|| self.kit.first_open(InteractionKind::Approval));
        if let Some(native_ref) = native_ref {
            self.kit.resolve(&native_ref, Resolution::Denied);
        }
    }

    fn post_tool_use(&mut self, payload: &Value, ok: bool) {
        let Some((id, tool)) = self.kit.tool_ended(payload, ok, FILE_TOOLS) else {
            return;
        };
        // The tool ran, so whatever was open for it ended: by an answer of the user in the dialog,
        // or by neohar. The core knows which.
        let resolution = if tool == "AskUserQuestion" && ok {
            Resolution::Answered
        } else if ok {
            Resolution::Allowed
        } else {
            Resolution::Unknown
        };
        self.kit.resolve(&id, resolution);
        // An MCP tool that asked the user for something (`Elicitation`) is over with the tool.
        if tool.starts_with("mcp__") {
            self.resolve_elicitations();
        }
    }

    /// Resolves the interactions that an MCP elicitation opened.
    fn resolve_elicitations(&mut self) {
        for native_ref in self.kit.open_with_prefix("elicitation:") {
            self.kit.resolve(&native_ref, Resolution::Answered);
        }
    }

    fn notification(&mut self, payload: &Value) {
        let kind = text(payload, "notification_type").unwrap_or("");
        let message = text(payload, "message").unwrap_or(kind).to_owned();
        match kind {
            "idle_prompt" => self.kit.emit(AdapterSignal::TurnEnded {
                turn_ref: None,
                stop_reason: Some("idle_prompt".to_owned()),
                last_message_preview: None,
            }),
            kind if kind.starts_with("quota_auto_resume") => {
                self.kit.emit(AdapterSignal::RateLimited {
                    resets_at: None,
                    scope: Some(kind.to_owned()),
                });
            }
            "elicitation_response" => self.resolve_elicitations(),
            _ => {}
        }
        self.kit.emit(AdapterSignal::Notice {
            kind: NoticeKind::Info,
            message,
        });
    }

    fn stop_failure(&mut self, payload: &Value) {
        let kind = text(payload, "error_type")
            .or_else(|| text(payload, "error"))
            .or_else(|| text(payload, "reason"))
            .unwrap_or("error");
        if kind.contains("rate_limit") {
            self.kit.emit(AdapterSignal::RateLimited {
                resets_at: None,
                scope: None,
            });
        } else {
            self.kit.emit(AdapterSignal::Error {
                kind: kind.to_owned(),
                message: text(payload, "message").unwrap_or(kind).to_owned(),
                retrying: false,
            });
        }
    }

    fn elicitation(&mut self, payload: &Value) {
        let native_ref = format!(
            "elicitation:{}:{}",
            text(payload, "prompt_id").unwrap_or("-"),
            text(payload, "mcp_server_name").unwrap_or("-")
        );
        self.kit.open_interaction(
            InteractionKind::Question,
            native_ref,
            json!({
                "source": "elicitation",
                "server": text(payload, "mcp_server_name"),
                "message": text(payload, "message"),
                "mode": text(payload, "mode"),
                "schema": payload.get("requested_schema"),
            }),
        );
    }
}

#[cfg(test)]
mod tests;
