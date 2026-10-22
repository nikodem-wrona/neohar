//! The transcript of Claude Code: `~/.claude/projects/<cwd-slug>/<session>.jsonl`.
//!
//! Fields that the golden recordings show (Claude Code 2.1.292): the type of the line is `user`,
//! `assistant`, `system` or `cost-state`, and a dozen types that do not matter here (`attachment`,
//! `mode`, `permission-mode`, `file-history-snapshot`, `last-prompt`, `queue-operation`, …).
//! One message of the model can span several lines, one for each block, and each line repeats the
//! same `message.id` and the same `usage`.

use serde_json::Value;

use super::{Block, Entry, Kind, ModelCost, TokenUsage, moment, number, preview, text};

/// The tools that start a subagent. The first name is the one of 2.1.292.
const SUBAGENT_TOOLS: &[&str] = &["Task", "Agent"];

pub(super) fn parse(line: &Value, offset: u64) -> Vec<Entry> {
    let id = text(line, "uuid")
        .map(str::to_owned)
        .unwrap_or_else(|| format!("l{offset}"));
    let at = moment(line);
    let sidechain = line
        .get("isSidechain")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let entry = |id: String, kind: Kind| Entry {
        id,
        at,
        sidechain,
        kind,
    };
    match text(line, "type") {
        Some("user") => user(line, &id, &entry),
        Some("assistant") => assistant(line, &id, &entry),
        Some("system") => match text(line, "subtype") {
            Some("turn_duration") => vec![entry(
                id,
                Kind::TurnEnd {
                    duration_ms: line.get("durationMs").and_then(Value::as_u64),
                },
            )],
            Some("compact_boundary") => vec![entry(id, Kind::Compaction)],
            _ => Vec::new(),
        },
        Some("cost-state") => {
            let models = line
                .get("modelUsage")
                .and_then(Value::as_object)
                .map(|models| {
                    models
                        .iter()
                        .filter_map(|(model, figures)| {
                            Some(ModelCost {
                                model: model.clone(),
                                usd: figures.get("costUSD")?.as_f64()?,
                            })
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            if models.is_empty() {
                return Vec::new();
            }
            vec![entry(format!("l{offset}"), Kind::Cost { models })]
        }
        _ => Vec::new(),
    }
}

fn user(line: &Value, id: &str, entry: &dyn Fn(String, Kind) -> Entry) -> Vec<Entry> {
    // The text that the harness adds to the conversation is not a prompt of the person.
    let flag = |key: &str| line.get(key).and_then(Value::as_bool).unwrap_or(false);
    if flag("isMeta") || flag("isCompactSummary") {
        return Vec::new();
    }
    let human = line
        .get("origin")
        .and_then(|o| text(o, "kind"))
        .is_none_or(|kind| kind == "human");
    let Some(message) = line.get("message") else {
        return Vec::new();
    };
    match message.get("content") {
        Some(Value::String(prompt)) if human && !prompt.trim().is_empty() => {
            vec![entry(
                id.to_owned(),
                Kind::Prompt {
                    text: prompt.clone(),
                },
            )]
        }
        Some(Value::Array(blocks)) => {
            let mut entries = Vec::new();
            for (index, block) in blocks.iter().enumerate() {
                match text(block, "type") {
                    Some("tool_result") => {
                        let Some(tool_use) = text(block, "tool_use_id") else {
                            continue;
                        };
                        let output = result_text(block.get("content"));
                        let failed = block
                            .get("is_error")
                            .and_then(Value::as_bool)
                            .unwrap_or(false);
                        let denied = failed && is_denial(&output);
                        entries.push(entry(
                            format!("{id}.{index}"),
                            Kind::ToolResult {
                                tool_use: tool_use.to_owned(),
                                failed,
                                denied,
                                text: output,
                            },
                        ));
                    }
                    Some("text") if human => {
                        if let Some(prompt) = text(block, "text").filter(|t| !t.trim().is_empty()) {
                            entries.push(entry(
                                format!("{id}.{index}"),
                                Kind::Prompt {
                                    text: prompt.to_owned(),
                                },
                            ));
                        }
                    }
                    _ => {}
                }
            }
            entries
        }
        _ => Vec::new(),
    }
}

/// The text of the content of a tool result: a string, or a list of blocks with `text`.
fn result_text(content: Option<&Value>) -> String {
    match content {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(blocks)) => blocks
            .iter()
            .filter_map(|block| text(block, "text"))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

/// The text of a result when the person, or a rule, refused the call. The words are those of
/// 2.1.292 (`permission-deny`, `plan-deny`) `[verify]`.
fn is_denial(output: &str) -> bool {
    output.starts_with("The user doesn't want to proceed")
        || output.starts_with("Permission to use")
        || output.contains("hook error: Denied")
        || output.starts_with("Denied")
}

fn assistant(line: &Value, id: &str, entry: &dyn Fn(String, Kind) -> Entry) -> Vec<Entry> {
    let Some(message) = line.get("message") else {
        return Vec::new();
    };
    let model = text(message, "model")
        // The harness writes its own error replies with this name and no real call behind them.
        .filter(|model| *model != "<synthetic>")
        .map(str::to_owned);
    let usage = model
        .as_ref()
        .and_then(|_| message.get("usage"))
        .map(usage_of)
        .filter(|usage| !usage.is_zero());
    let mut blocks = Vec::new();
    if let Some(Value::Array(content)) = message.get("content") {
        for block in content {
            match text(block, "type") {
                Some("text") => {
                    if let Some(reply) = text(block, "text").filter(|t| !t.trim().is_empty()) {
                        blocks.push(Block::Text(reply.to_owned()));
                    }
                }
                Some("tool_use") => {
                    let (Some(call), Some(name)) = (text(block, "id"), text(block, "name")) else {
                        continue;
                    };
                    let input = block.get("input").cloned().unwrap_or(Value::Null);
                    blocks.push(Block::ToolUse {
                        id: call.to_owned(),
                        name: name.to_owned(),
                        summary: tool_summary(&input),
                        subagent: SUBAGENT_TOOLS
                            .contains(&name)
                            .then(|| text(&input, "subagent_type").unwrap_or(name).to_owned()),
                    });
                }
                _ => {}
            }
        }
    }
    if usage.is_none() && blocks.is_empty() {
        return Vec::new();
    }
    vec![entry(
        id.to_owned(),
        Kind::Assistant {
            message: text(message, "id").map(str::to_owned),
            model,
            usage,
            blocks,
        },
    )]
}

fn usage_of(usage: &Value) -> TokenUsage {
    let one_hour = usage
        .get("cache_creation")
        .map(|c| number(c, "ephemeral_1h_input_tokens"))
        .unwrap_or(0);
    TokenUsage {
        input: number(usage, "input_tokens"),
        output: number(usage, "output_tokens"),
        cache_read: number(usage, "cache_read_input_tokens"),
        cache_write: number(usage, "cache_creation_input_tokens"),
        cache_write_1h: one_hour,
    }
}

/// One line about the input of a tool: the first of the usual fields that holds a text.
pub(super) fn tool_summary(input: &Value) -> String {
    const KEYS: &[&str] = &[
        "command",
        "file_path",
        "path",
        "pattern",
        "url",
        "description",
        "query",
        "prompt",
    ];
    KEYS.iter()
        .find_map(|key| text(input, key))
        .map(|summary| preview(summary, 160))
        .unwrap_or_default()
}
