//! The rollout of Codex: `~/.codex/sessions/…/rollout-*.jsonl`.
//!
//! Fields that the golden recordings show (Codex 0.156.1): each line has `timestamp`, `ordinal`,
//! `type` and `payload`. The types that matter are `turn_context`, `event_msg` (with
//! `task_started`, `task_complete`, `item_completed` and `token_count`), `token_usage_record` and
//! `response_item` (with `function_call` and `function_call_output`).
//!
//! Codex counts the cached tokens inside `input_tokens`. The parser takes them out, so a figure
//! means the same for both harnesses.

use nh_proto::api::time::Timestamp;
use serde_json::Value;

use super::{
    Block, Entry, Kind, LimitWindow, TokenUsage, claude::tool_summary, moment, number, text,
};

pub(super) fn parse(line: &Value, offset: u64) -> Vec<Entry> {
    let id = line
        .get("ordinal")
        .and_then(Value::as_u64)
        .map(|ordinal| format!("o{ordinal}"))
        .unwrap_or_else(|| format!("l{offset}"));
    let at = moment(line);
    let entry = |kind: Kind| Entry {
        id: id.clone(),
        at,
        sidechain: false,
        kind,
    };
    let Some(payload) = line.get("payload") else {
        return Vec::new();
    };
    match text(line, "type") {
        Some("turn_context") => text(payload, "model")
            .map(|model| {
                vec![entry(Kind::Model {
                    model: model.to_owned(),
                })]
            })
            .unwrap_or_default(),
        Some("token_usage_record") => {
            let (Some(key), Some(usage)) = (text(payload, "response_id"), payload.get("usage"))
            else {
                return Vec::new();
            };
            vec![entry(Kind::Usage {
                key: key.to_owned(),
                model: None,
                usage: usage_of(usage),
            })]
        }
        Some("event_msg") => event(payload, &entry),
        Some("response_item") => response_item(payload, &entry),
        _ => Vec::new(),
    }
}

fn event(payload: &Value, entry: &dyn Fn(Kind) -> Entry) -> Vec<Entry> {
    match text(payload, "type") {
        Some("task_started") => vec![entry(Kind::TurnStart)],
        Some("task_complete") => vec![entry(Kind::TurnEnd {
            duration_ms: payload.get("duration_ms").and_then(Value::as_u64),
        })],
        Some("item_completed") => {
            let Some(item) = payload.get("item") else {
                return Vec::new();
            };
            match text(item, "type") {
                Some("UserMessage") => {
                    let prompt = joined_text(item.get("content"));
                    if prompt.trim().is_empty() {
                        return Vec::new();
                    }
                    vec![entry(Kind::Prompt { text: prompt })]
                }
                Some("AgentMessage") => {
                    let reply = joined_text(item.get("content"));
                    if reply.trim().is_empty() {
                        return Vec::new();
                    }
                    vec![entry(Kind::Assistant {
                        message: text(item, "id").map(str::to_owned),
                        model: None,
                        usage: None,
                        blocks: vec![Block::Text(reply)],
                    })]
                }
                _ => Vec::new(),
            }
        }
        Some("token_count") => {
            let windows = limits(payload.get("rate_limits"));
            if windows.is_empty() {
                Vec::new()
            } else {
                vec![entry(Kind::Limits { windows })]
            }
        }
        _ => Vec::new(),
    }
}

fn response_item(payload: &Value, entry: &dyn Fn(Kind) -> Entry) -> Vec<Entry> {
    match text(payload, "type") {
        Some("function_call") => {
            let (Some(call), Some(name)) = (text(payload, "call_id"), text(payload, "name")) else {
                return Vec::new();
            };
            // The arguments are a text that holds JSON.
            let input = text(payload, "arguments")
                .and_then(|arguments| serde_json::from_str::<Value>(arguments).ok())
                .unwrap_or(Value::Null);
            let summary = match text(&input, "cmd") {
                Some(cmd) => super::preview(cmd, 160),
                None => tool_summary(&input),
            };
            vec![entry(Kind::Assistant {
                message: None,
                model: None,
                usage: None,
                blocks: vec![Block::ToolUse {
                    id: call.to_owned(),
                    name: name.to_owned(),
                    summary,
                    subagent: None,
                }],
            })]
        }
        Some("function_call_output") => {
            let Some(call) = text(payload, "call_id") else {
                return Vec::new();
            };
            let output = match payload.get("output") {
                Some(Value::String(output)) => output.clone(),
                Some(other) => other.to_string(),
                None => String::new(),
            };
            // The text of a command has the line `Process exited with code N`.
            let failed = output
                .lines()
                .find_map(|line| line.strip_prefix("Process exited with code "))
                .and_then(|code| code.trim().parse::<i64>().ok())
                .is_some_and(|code| code != 0);
            vec![entry(Kind::ToolResult {
                tool_use: call.to_owned(),
                failed,
                denied: false,
                text: output_body(&output),
            })]
        }
        _ => Vec::new(),
    }
}

/// The part of the output of a command after the line `Output:`; the header is for the harness.
fn output_body(output: &str) -> String {
    match output.split_once("\nOutput:\n") {
        Some((_, body)) => body.to_owned(),
        None => output.to_owned(),
    }
}

fn joined_text(content: Option<&Value>) -> String {
    match content {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(parts)) => parts
            .iter()
            .filter_map(|part| text(part, "text"))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

fn usage_of(usage: &Value) -> TokenUsage {
    let cached = number(usage, "cached_input_tokens");
    let written = number(usage, "cache_write_input_tokens");
    TokenUsage {
        input: number(usage, "input_tokens")
            .saturating_sub(cached)
            .saturating_sub(written),
        output: number(usage, "output_tokens"),
        cache_read: cached,
        cache_write: written,
        cache_write_1h: 0,
    }
}

/// The windows of `rate_limits`: `primary` and `secondary` when they are not null.
/// `[verify]` The recordings have both null, so the shape is the one of the Codex source.
fn limits(rate_limits: Option<&Value>) -> Vec<LimitWindow> {
    let Some(rate_limits) = rate_limits else {
        return Vec::new();
    };
    ["primary", "secondary"]
        .iter()
        .filter_map(|key| {
            let window = rate_limits.get(*key).filter(|w| w.is_object())?;
            let used = window.get("used_percent").and_then(Value::as_f64)?;
            let minutes = window.get("window_minutes").and_then(Value::as_u64);
            Some(LimitWindow {
                name: window_name(minutes, key),
                used_percent: used as f32,
                resets_at: window
                    .get("resets_at")
                    .and_then(Value::as_i64)
                    .map(|seconds| Timestamp(seconds * 1000)),
            })
        })
        .collect()
}

fn window_name(minutes: Option<u64>, fallback: &str) -> String {
    match minutes {
        Some(300) => "5h".to_owned(),
        Some(10_080) => "weekly".to_owned(),
        Some(m) if m % 60 == 0 => format!("{}h", m / 60),
        Some(m) => format!("{m}m"),
        None => fallback.to_owned(),
    }
}
