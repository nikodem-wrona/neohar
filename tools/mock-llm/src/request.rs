//! What the mock model needs to know about a request body.

use serde_json::Value;

/// The facts of a request that decide the reply.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Parsed {
    pub model: String,
    pub stream: bool,
    /// The request offers tools, so it is a turn of the conversation and not a call on the side
    /// (a title, a summary, a quota check).
    pub main: bool,
    /// The text of the last user message or tool result.
    pub last_user: String,
}

fn text_parts(content: &Value, kinds: &[&str]) -> Vec<String> {
    match content {
        Value::String(text) => vec![text.clone()],
        Value::Array(parts) => parts
            .iter()
            .flat_map(|part| {
                let kind = part.get("type").and_then(Value::as_str).unwrap_or("");
                if kinds.contains(&kind) {
                    part.get("text")
                        .and_then(Value::as_str)
                        .map(|t| vec![t.to_owned()])
                        .unwrap_or_default()
                } else if kind == "tool_result" {
                    part.get("content")
                        .map(|c| text_parts(c, kinds))
                        .unwrap_or_default()
                } else {
                    Vec::new()
                }
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn has_tools(body: &Value) -> bool {
    body.get("tools")
        .and_then(Value::as_array)
        .is_some_and(|tools| !tools.is_empty())
}

fn common(body: &Value) -> (String, bool, bool) {
    (
        body.get("model")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned(),
        body.get("stream").and_then(Value::as_bool).unwrap_or(false),
        has_tools(body),
    )
}

/// A request to `POST /v1/messages` (Anthropic Messages API).
pub fn anthropic(body: &Value) -> Parsed {
    let (model, stream, main) = common(body);
    let last_user = body
        .get("messages")
        .and_then(Value::as_array)
        .and_then(|messages| {
            messages
                .iter()
                .rev()
                .find(|m| m.get("role").and_then(Value::as_str) == Some("user"))
        })
        .and_then(|message| message.get("content"))
        .map(|content| text_parts(content, &["text"]).join("\n"))
        .unwrap_or_default();
    Parsed {
        model,
        stream,
        main,
        last_user,
    }
}

/// A request to `POST /v1/responses` (OpenAI Responses API).
pub fn openai(body: &Value) -> Parsed {
    let (model, stream, main) = common(body);
    let last_user = match body.get("input") {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(items)) => items
            .iter()
            .rev()
            .find_map(|item| {
                let kind = item.get("type").and_then(Value::as_str);
                let role = item.get("role").and_then(Value::as_str);
                if kind == Some("function_call_output") {
                    // The output of a tool is a string, or a list of parts.
                    item.get("output")
                        .map(|out| text_parts(out, &["text", "output_text"]).join("\n"))
                } else if role == Some("user") && matches!(kind, None | Some("message")) {
                    item.get("content")
                        .map(|c| text_parts(c, &["input_text", "text"]).join("\n"))
                } else {
                    None
                }
            })
            .unwrap_or_default(),
        _ => String::new(),
    };
    Parsed {
        model,
        stream,
        main,
        last_user,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn anthropic_reads_the_last_user_text_and_the_tool_result() {
        let body = json!({
            "model": "claude-x", "stream": true, "tools": [{ "name": "Bash" }],
            "messages": [
                { "role": "user", "content": "first" },
                { "role": "assistant", "content": [{ "type": "text", "text": "ok" }] },
                { "role": "user", "content": [
                    { "type": "tool_result", "tool_use_id": "t", "content": "file.txt" },
                    { "type": "text", "text": "and more" },
                ] },
            ],
        });
        let parsed = anthropic(&body);
        assert_eq!(
            parsed,
            Parsed {
                model: "claude-x".into(),
                stream: true,
                main: true,
                last_user: "file.txt\nand more".into()
            }
        );
    }

    #[test]
    fn a_request_without_tools_is_auxiliary_and_a_missing_body_part_is_empty() {
        let body =
            json!({ "model": "m", "messages": [{ "role": "user", "content": "write a title" }] });
        let parsed = anthropic(&body);
        assert!(!parsed.main && !parsed.stream);
        assert_eq!(parsed.last_user, "write a title");
        assert_eq!(anthropic(&json!({})).last_user, "");
        let empty_tools = json!({ "tools": [], "messages": [] });
        assert!(!anthropic(&empty_tools).main);
    }

    #[test]
    fn openai_reads_a_string_input_a_user_message_and_a_tool_output() {
        assert_eq!(openai(&json!({ "input": "hello" })).last_user, "hello");
        let items = json!({
            "model": "gpt-x", "tools": [{ "type": "function", "name": "shell" }],
            "input": [
                { "type": "message", "role": "user", "content": [{ "type": "input_text", "text": "run it" }] },
                { "type": "function_call", "call_id": "c1", "name": "shell", "arguments": "{}" },
                { "type": "function_call_output", "call_id": "c1", "output": "done" },
            ],
        });
        let parsed = openai(&items);
        assert!(parsed.main);
        assert_eq!(parsed.last_user, "done");
        let only_user = json!({ "input": [{ "role": "user", "content": "plain" }] });
        assert_eq!(openai(&only_user).last_user, "plain");
    }
}
