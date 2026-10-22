//! The Anthropic Messages API as the mock model speaks it (`POST /v1/messages`).
//!
//! The functions here build values and know nothing about HTTP, so that a test can check each
//! event. A stream is a list of `(event name, data)` pairs.

use serde_json::{Value, json};

use crate::scenario::{Block, Reply};

/// How many characters of text or of the arguments of a tool go in one delta.
const CHUNK: usize = 24;

/// The model name that the mock reports when the request names none.
pub const DEFAULT_MODEL: &str = "mock-model";

/// An estimate of the tokens in some text: four characters for one token.
pub fn estimate_tokens(chars: usize) -> u64 {
    (chars as u64).div_ceil(4).max(1)
}

/// Splits text into pieces of at most `size` characters, never in the middle of one.
pub(crate) fn chunks(text: &str, size: usize) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    chars.chunks(size).map(|c| c.iter().collect()).collect()
}

fn id(prefix: &str, message: u64, block: usize) -> String {
    format!("{prefix}_mock_{message:04}_{block}")
}

/// The reply as a `content` list, with the ids of the tool calls.
fn content(message: u64, reply: &Reply) -> Vec<Value> {
    reply
        .blocks
        .iter()
        .enumerate()
        .map(|(index, block)| match block {
            Block::Text { text } => json!({ "type": "text", "text": text }),
            Block::ToolUse { name, input } => json!({
                "type": "tool_use", "id": id("toolu", message, index), "name": name, "input": input,
            }),
        })
        .collect()
}

fn stop_reason(reply: &Reply) -> &'static str {
    if reply.calls_tool() {
        "tool_use"
    } else {
        "end_turn"
    }
}

fn output_tokens(reply: &Reply) -> u64 {
    reply.output_tokens.unwrap_or_else(|| {
        let chars: usize = reply
            .blocks
            .iter()
            .map(|block| match block {
                Block::Text { text } => text.chars().count(),
                Block::ToolUse { input, .. } => input.to_string().len(),
            })
            .sum();
        estimate_tokens(chars)
    })
}

fn usage(input: u64, output: u64) -> Value {
    json!({
        "input_tokens": input, "output_tokens": output,
        "cache_creation_input_tokens": 0, "cache_read_input_tokens": 0,
    })
}

/// The whole reply as one JSON value (a request with `stream: false`).
pub fn message(message: u64, model: &str, reply: &Reply, input_tokens: u64) -> Value {
    json!({
        "id": id("msg", message, 0), "type": "message", "role": "assistant", "model": model,
        "content": content(message, reply),
        "stop_reason": stop_reason(reply), "stop_sequence": null,
        "usage": usage(reply.input_tokens.unwrap_or(input_tokens), output_tokens(reply)),
    })
}

/// The reply as the events of a stream.
pub fn events(
    message_number: u64,
    model: &str,
    reply: &Reply,
    input_tokens: u64,
) -> Vec<(&'static str, Value)> {
    let input = reply.input_tokens.unwrap_or(input_tokens);
    let mut out = vec![(
        "message_start",
        json!({
            "type": "message_start",
            "message": {
                "id": id("msg", message_number, 0), "type": "message", "role": "assistant",
                "model": model, "content": [], "stop_reason": null, "stop_sequence": null,
                "usage": usage(input, 1),
            },
        }),
    )];
    out.push(("ping", json!({ "type": "ping" })));
    for (index, block) in reply.blocks.iter().enumerate() {
        match block {
            Block::Text { text } => {
                out.push((
                    "content_block_start",
                    json!({
                        "type": "content_block_start", "index": index,
                        "content_block": { "type": "text", "text": "" },
                    }),
                ));
                for piece in chunks(text, CHUNK) {
                    out.push((
                        "content_block_delta",
                        json!({
                            "type": "content_block_delta", "index": index,
                            "delta": { "type": "text_delta", "text": piece },
                        }),
                    ));
                }
            }
            Block::ToolUse { name, input } => {
                out.push((
                    "content_block_start",
                    json!({
                        "type": "content_block_start", "index": index,
                        "content_block": {
                            "type": "tool_use", "id": id("toolu", message_number, index),
                            "name": name, "input": {},
                        },
                    }),
                ));
                for piece in chunks(&input.to_string(), CHUNK) {
                    out.push((
                        "content_block_delta",
                        json!({
                            "type": "content_block_delta", "index": index,
                            "delta": { "type": "input_json_delta", "partial_json": piece },
                        }),
                    ));
                }
            }
        }
        out.push((
            "content_block_stop",
            json!({ "type": "content_block_stop", "index": index }),
        ));
    }
    out.push((
        "message_delta",
        json!({
            "type": "message_delta",
            "delta": { "stop_reason": stop_reason(reply), "stop_sequence": null },
            "usage": { "output_tokens": output_tokens(reply) },
        }),
    ));
    out.push(("message_stop", json!({ "type": "message_stop" })));
    out
}

/// The answer of `POST /v1/messages/count_tokens`: the characters of the request body, four for
/// one token.
pub fn count_tokens(body: &Value) -> Value {
    json!({ "input_tokens": estimate_tokens(body.to_string().chars().count()) })
}

/// The tokens that a request is worth, for the usage of the reply.
pub fn input_tokens(body: &Value) -> u64 {
    estimate_tokens(body.to_string().chars().count())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scenario::Reply;

    fn reply(blocks: Vec<Block>) -> Reply {
        Reply {
            blocks,
            input_tokens: Some(10),
            output_tokens: Some(5),
            turn: Some(0),
        }
    }

    fn names(events: &[(&'static str, Value)]) -> Vec<&'static str> {
        events.iter().map(|(name, _)| *name).collect()
    }

    #[test]
    fn a_text_reply_is_a_message_with_one_block() {
        let r = reply(vec![Block::Text {
            text: "Hello there.".into(),
        }]);
        let m = message(1, "claude-x", &r, 99);
        assert_eq!(m["type"], "message");
        assert_eq!(m["role"], "assistant");
        assert_eq!(m["model"], "claude-x");
        assert_eq!(
            m["content"],
            json!([{ "type": "text", "text": "Hello there." }])
        );
        assert_eq!(m["stop_reason"], "end_turn");
        assert_eq!(
            m["usage"]["input_tokens"], 10,
            "the turn's number wins over the estimate"
        );
        assert_eq!(m["usage"]["output_tokens"], 5);
    }

    #[test]
    fn a_tool_call_ends_with_tool_use_and_has_an_id() {
        let r = reply(vec![
            Block::Text {
                text: "I will look.".into(),
            },
            Block::ToolUse {
                name: "Bash".into(),
                input: json!({ "command": "ls" }),
            },
        ]);
        let m = message(7, "m", &r, 0);
        assert_eq!(m["stop_reason"], "tool_use");
        assert_eq!(m["content"][1]["id"], "toolu_mock_0007_1");
        assert_eq!(m["content"][1]["input"], json!({ "command": "ls" }));
    }

    #[test]
    fn a_stream_has_the_events_of_the_api_in_order() {
        let r = reply(vec![Block::Text {
            text: "abcdefghijklmnopqrstuvwxyz0123456789".into(),
        }]);
        let e = events(1, "m", &r, 0);
        assert_eq!(
            names(&e),
            [
                "message_start",
                "ping",
                "content_block_start",
                "content_block_delta",
                "content_block_delta",
                "content_block_stop",
                "message_delta",
                "message_stop"
            ]
        );
        // 36 characters make two deltas of at most 24.
        let text: String = e
            .iter()
            .filter(|(n, _)| *n == "content_block_delta")
            .map(|(_, d)| d["delta"]["text"].as_str().unwrap_or("").to_owned())
            .collect();
        assert_eq!(text, "abcdefghijklmnopqrstuvwxyz0123456789");
        assert_eq!(e[0].1["message"]["usage"]["output_tokens"], 1);
        assert_eq!(e[6].1["delta"]["stop_reason"], "end_turn");
        assert_eq!(e[6].1["usage"]["output_tokens"], 5);
    }

    #[test]
    fn the_arguments_of_a_tool_stream_as_partial_json_that_joins_to_the_input() {
        let input =
            json!({ "command": "echo a long command line that needs several deltas", "n": 3 });
        let r = reply(vec![Block::ToolUse {
            name: "Bash".into(),
            input: input.clone(),
        }]);
        let e = events(2, "m", &r, 0);
        assert_eq!(e[2].1["content_block"]["type"], "tool_use");
        assert_eq!(e[2].1["content_block"]["input"], json!({}));
        let joined: String = e
            .iter()
            .filter(|(n, _)| *n == "content_block_delta")
            .map(|(_, d)| d["delta"]["partial_json"].as_str().unwrap_or("").to_owned())
            .collect();
        assert_eq!(serde_json::from_str::<Value>(&joined).expect("json"), input);
        let pieces = e
            .iter()
            .filter(|(n, _)| *n == "content_block_delta")
            .count();
        assert!(pieces > 1);
        assert_eq!(e[e.len() - 2].1["delta"]["stop_reason"], "tool_use");
    }

    #[test]
    fn chunks_never_split_a_character_and_the_estimate_is_at_least_one() {
        let pieces = chunks("é\u{1f980}éé", 2);
        assert_eq!(pieces, ["é\u{1f980}", "éé"]);
        assert_eq!(estimate_tokens(0), 1);
        assert_eq!(estimate_tokens(9), 3);
        assert_eq!(count_tokens(&json!({})), json!({ "input_tokens": 1 }));
    }

    #[test]
    fn without_numbers_in_the_turn_the_mock_estimates_the_usage() {
        let r = Reply {
            blocks: vec![Block::Text {
                text: "12345678".into(),
            }],
            input_tokens: None,
            output_tokens: None,
            turn: None,
        };
        let m = message(1, "m", &r, 42);
        assert_eq!(m["usage"]["input_tokens"], 42);
        assert_eq!(m["usage"]["output_tokens"], 2);
    }
}
