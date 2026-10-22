//! The OpenAI Responses API as the mock model speaks it (`POST /v1/responses`).
//!
//! Like [`crate::anthropic`], the functions build values and know nothing about HTTP. A stream is
//! a list of `(event name, data)` pairs. Each `data` has a `type` and a `sequence_number`.

use serde_json::{Value, json};

use crate::{
    anthropic::{chunks, estimate_tokens},
    scenario::{Block, Reply},
};

/// How many characters of text or of the arguments of a function go in one delta.
const CHUNK: usize = 24;

fn response_id(n: u64) -> String {
    format!("resp_mock_{n:04}")
}

fn item_id(prefix: &str, n: u64, index: usize) -> String {
    format!("{prefix}_mock_{n:04}_{index}")
}

/// The arguments of a function call, as the string that the API carries.
fn arguments(input: &Value) -> String {
    input.to_string()
}

/// One output item, finished.
fn item(n: u64, index: usize, block: &Block) -> Value {
    match block {
        Block::Text { text } => json!({
            "id": item_id("msg", n, index), "type": "message", "status": "completed",
            "role": "assistant",
            "content": [{ "type": "output_text", "text": text, "annotations": [] }],
        }),
        Block::ToolUse { name, input } => json!({
            "id": item_id("fc", n, index), "type": "function_call", "status": "completed",
            "call_id": item_id("call", n, index), "name": name, "arguments": arguments(input),
        }),
    }
}

fn output_tokens(reply: &Reply) -> u64 {
    reply.output_tokens.unwrap_or_else(|| {
        let chars: usize = reply
            .blocks
            .iter()
            .map(|block| match block {
                Block::Text { text } => text.chars().count(),
                Block::ToolUse { input, .. } => arguments(input).len(),
            })
            .sum();
        estimate_tokens(chars)
    })
}

fn usage(input: u64, output: u64) -> Value {
    json!({
        "input_tokens": input, "input_tokens_details": { "cached_tokens": 0 },
        "output_tokens": output, "output_tokens_details": { "reasoning_tokens": 0 },
        "total_tokens": input + output,
    })
}

/// The whole response as one JSON value. A request with `stream: false` gets it, and the last event
/// of a stream carries it.
pub fn response(n: u64, model: &str, reply: &Reply, input_tokens: u64, done: bool) -> Value {
    let input = reply.input_tokens.unwrap_or(input_tokens);
    let (status, output, usage) = if done {
        (
            "completed",
            reply
                .blocks
                .iter()
                .enumerate()
                .map(|(index, block)| item(n, index, block))
                .collect::<Vec<_>>(),
            usage(input, output_tokens(reply)),
        )
    } else {
        ("in_progress", Vec::new(), Value::Null)
    };
    json!({
        "id": response_id(n), "object": "response", "created_at": 0, "status": status,
        "model": model, "output": output, "usage": usage,
        "parallel_tool_calls": true, "tool_choice": "auto", "tools": [],
    })
}

/// The reply as the events of a stream.
pub fn events(n: u64, model: &str, reply: &Reply, input_tokens: u64) -> Vec<(String, Value)> {
    let mut out: Vec<(String, Value)> = Vec::new();
    let mut push = |name: &str, mut data: Value| {
        data["type"] = json!(name);
        data["sequence_number"] = json!(out.len());
        out.push((name.to_owned(), data));
    };
    push(
        "response.created",
        json!({ "response": response(n, model, reply, input_tokens, false) }),
    );
    push(
        "response.in_progress",
        json!({ "response": response(n, model, reply, input_tokens, false) }),
    );
    for (index, block) in reply.blocks.iter().enumerate() {
        match block {
            Block::Text { text } => {
                let id = item_id("msg", n, index);
                push(
                    "response.output_item.added",
                    json!({
                        "output_index": index,
                        "item": { "id": id, "type": "message", "status": "in_progress",
                                  "role": "assistant", "content": [] },
                    }),
                );
                push(
                    "response.content_part.added",
                    json!({
                        "item_id": id, "output_index": index, "content_index": 0,
                        "part": { "type": "output_text", "text": "", "annotations": [] },
                    }),
                );
                for piece in chunks(text, CHUNK) {
                    push(
                        "response.output_text.delta",
                        json!({ "item_id": id, "output_index": index, "content_index": 0, "delta": piece }),
                    );
                }
                push(
                    "response.output_text.done",
                    json!({ "item_id": id, "output_index": index, "content_index": 0, "text": text }),
                );
                push(
                    "response.content_part.done",
                    json!({
                        "item_id": id, "output_index": index, "content_index": 0,
                        "part": { "type": "output_text", "text": text, "annotations": [] },
                    }),
                );
            }
            Block::ToolUse { name, input } => {
                let id = item_id("fc", n, index);
                push(
                    "response.output_item.added",
                    json!({
                        "output_index": index,
                        "item": { "id": id, "type": "function_call", "status": "in_progress",
                                  "call_id": item_id("call", n, index), "name": name,
                                  "arguments": "" },
                    }),
                );
                let args = arguments(input);
                for piece in chunks(&args, CHUNK) {
                    push(
                        "response.function_call_arguments.delta",
                        json!({ "item_id": id, "output_index": index, "delta": piece }),
                    );
                }
                push(
                    "response.function_call_arguments.done",
                    json!({ "item_id": id, "output_index": index, "arguments": args }),
                );
            }
        }
        push(
            "response.output_item.done",
            json!({ "output_index": index, "item": item(n, index, block) }),
        );
    }
    push(
        "response.completed",
        json!({ "response": response(n, model, reply, input_tokens, true) }),
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reply(blocks: Vec<Block>) -> Reply {
        Reply {
            blocks,
            input_tokens: Some(11),
            output_tokens: Some(4),
            turn: Some(0),
        }
    }

    fn names(events: &[(String, Value)]) -> Vec<&str> {
        events.iter().map(|(name, _)| name.as_str()).collect()
    }

    #[test]
    fn a_text_stream_has_the_events_of_the_responses_api_in_order() {
        let r = reply(vec![Block::Text {
            text: "Hello from the mock model.".into(),
        }]);
        let e = events(1, "gpt-x", &r, 0);
        assert_eq!(
            names(&e),
            [
                "response.created",
                "response.in_progress",
                "response.output_item.added",
                "response.content_part.added",
                "response.output_text.delta",
                "response.output_text.delta",
                "response.output_text.done",
                "response.content_part.done",
                "response.output_item.done",
                "response.completed"
            ]
        );
        for (i, (name, data)) in e.iter().enumerate() {
            assert_eq!(data["type"], name.as_str());
            assert_eq!(data["sequence_number"], i);
        }
        let done = &e.last().expect("an event").1["response"];
        assert_eq!(done["status"], "completed");
        assert_eq!(
            done["output"][0]["content"][0]["text"],
            "Hello from the mock model."
        );
        assert_eq!(done["usage"], usage(11, 4));
        assert_eq!(e[0].1["response"]["status"], "in_progress");
        assert_eq!(e[0].1["response"]["usage"], Value::Null);
    }

    #[test]
    fn a_function_call_streams_its_arguments_and_finishes_with_a_call_id() {
        let input =
            json!({ "command": ["bash", "-lc", "echo a command line that needs several deltas"] });
        let r = reply(vec![Block::ToolUse {
            name: "shell".into(),
            input: input.clone(),
        }]);
        let e = events(3, "m", &r, 0);
        assert_eq!(e[2].1["item"]["type"], "function_call");
        assert_eq!(e[2].1["item"]["arguments"], "");
        let joined: String = e
            .iter()
            .filter(|(n, _)| n == "response.function_call_arguments.delta")
            .map(|(_, d)| d["delta"].as_str().unwrap_or("").to_owned())
            .collect();
        assert_eq!(serde_json::from_str::<Value>(&joined).expect("json"), input);
        let finished = e
            .iter()
            .find(|(n, _)| n == "response.output_item.done")
            .expect("a done event");
        assert_eq!(finished.1["item"]["call_id"], "call_mock_0003_0");
        assert_eq!(finished.1["item"]["arguments"], joined);
        assert_eq!(finished.1["item"]["status"], "completed");
    }

    #[test]
    fn the_non_streaming_response_lists_every_item() {
        let r = reply(vec![
            Block::Text { text: "a".into() },
            Block::ToolUse {
                name: "shell".into(),
                input: json!({}),
            },
        ]);
        let v = response(5, "m", &r, 0, true);
        assert_eq!(v["id"], "resp_mock_0005");
        assert_eq!(v["output"].as_array().map(Vec::len), Some(2));
        assert_eq!(v["usage"]["total_tokens"], 15);
    }
}
