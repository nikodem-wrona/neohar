//! The text that a held hook prints for a decision.
//!
//! The core builds the JSON, and the shim prints it as it is. The shim has no knowledge of these
//! shapes, so a new field costs no change in the shim.

use nh_proto::api::{
    calls::agent::{ApprovalDecision, Behavior, Decision, PlanDecision, QuestionDecision},
    common::Harness,
    model::{ApprovalPayload, InteractionPayload, QuestionPayload},
};
use serde_json::{Value, json};

use super::model::Interaction;

/// The message of a deny that has none: the model sees it as the result of the tool.
const DENIED: &str = "Denied from neohar";

/// What the hook of `harness` prints for the decision of `interaction`, or `None` when the hook
/// cannot carry it: the approval of a plan has no native channel in a TUI, and an
/// elicitation of an MCP server has no answer format yet. `persist_allowed` is the setting
/// `approvals.claude.persist_always`.
pub(crate) fn hook_output(
    harness: Harness,
    interaction: &Interaction,
    persist_allowed: bool,
) -> Option<String> {
    let decision = interaction.delivery.decision.as_ref()?;
    match (&interaction.payload, decision) {
        (InteractionPayload::Approval(payload), Decision::Approval(d)) => {
            Some(approval(harness, payload, d, persist_allowed).to_string())
        }
        (InteractionPayload::PlanReview(_), Decision::PlanReview(d)) => {
            plan(d).map(|v| v.to_string())
        }
        (InteractionPayload::Question(payload), Decision::Question(d)) => {
            question(payload, d).map(|v| v.to_string())
        }
        _ => None,
    }
}

fn permission(decision: Value) -> Value {
    json!({"hookSpecificOutput": {"hookEventName": "PermissionRequest", "decision": decision}})
}

fn approval(
    harness: Harness,
    payload: &ApprovalPayload,
    decision: &ApprovalDecision,
    persist_allowed: bool,
) -> Value {
    match decision.behavior {
        Behavior::Allow => {
            let mut out = json!({"behavior": "allow"});
            // Only Claude Code takes a rule with the answer. For Codex, "always" is a
            // rule of neohar, and the answer to the hook is a plain allow.
            if decision.always && harness == Harness::Claude {
                let destination = if decision.persist && persist_allowed {
                    "localSettings"
                } else {
                    "session"
                };
                let mut rule = json!({"toolName": payload.tool});
                if let Some(prefix) = payload
                    .command
                    .as_ref()
                    .and_then(|c| nh_rich::command::rule_prefix(&c.argv))
                {
                    rule["ruleContent"] = json!(prefix);
                }
                out["updatedPermissions"] = json!([{
                    "type": "addRules",
                    "rules": [rule],
                    "behavior": "allow",
                    "destination": destination,
                }]);
            }
            permission(out)
        }
        Behavior::Deny => {
            let message = decision.message.as_deref().filter(|m| !m.trim().is_empty());
            let mut out = json!({"behavior": "deny", "message": message.unwrap_or(DENIED)});
            if decision.stop {
                out["interrupt"] = json!(true);
            }
            permission(out)
        }
    }
}

/// A plan is rejected with a deny. Its approval has no native channel.
fn plan(decision: &PlanDecision) -> Option<Value> {
    if decision.approve {
        return None;
    }
    let message = decision
        .feedback
        .as_deref()
        .filter(|m| !m.trim().is_empty())?;
    Some(permission(json!({"behavior": "deny", "message": message})))
}

/// The answer to `AskUserQuestion`: the hook `PreToolUse` allows the tool with the questions as
/// Claude sent them and an answer for each.
fn question(payload: &QuestionPayload, decision: &QuestionDecision) -> Option<Value> {
    // An elicitation of an MCP server is shown as a question with no options. It has no format.
    if payload.questions.iter().all(|q| q.options.is_empty()) {
        return None;
    }
    let questions: Vec<Value> = payload
        .questions
        .iter()
        .map(|q| {
            let mut out = json!({
                "question": q.question,
                "options": q.options.iter().map(|o| {
                    let mut option = json!({"label": o.label});
                    if let Some(description) = &o.description {
                        option["description"] = json!(description);
                    }
                    option
                }).collect::<Vec<_>>(),
                "multiSelect": q.multi_select,
            });
            if let Some(header) = &q.header {
                out["header"] = json!(header);
            }
            out
        })
        .collect();
    let mut answers = serde_json::Map::new();
    for answer in &decision.answers {
        let text = match answer.text.as_deref().filter(|t| !t.is_empty()) {
            Some(text) => text.to_owned(),
            None => answer.selected.join(", "),
        };
        answers.insert(answer.question.clone(), json!(text));
    }
    Some(json!({"hookSpecificOutput": {
        "hookEventName": "PreToolUse",
        "permissionDecision": "allow",
        "updatedInput": {"questions": questions, "answers": answers},
    }}))
}

#[cfg(test)]
mod tests;
