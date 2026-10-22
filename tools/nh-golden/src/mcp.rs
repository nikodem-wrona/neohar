//! A tiny MCP server over standard input and output, for the `Elicitation` scenarios.
//!
//! `nh-golden mcp-elicit` speaks the Model Context Protocol as JSON lines. It has one tool, `ask`.
//! A call of `ask` makes the server send an `elicitation/create` request to the client (Claude
//! Code) and wait for the answer. The tool result then says what the client answered. A scenario
//! starts the server with `--mcp-config`, and the tool is `mcp__golden__ask` for the model.

use std::io::{self, BufRead, Write};

use serde_json::{Value, json};

/// The protocol version that the server offers when the client names none.
const DEFAULT_PROTOCOL: &str = "2025-06-18";

/// What the server says in the elicitation request.
pub const QUESTION: &str = "Which color do you want?";

fn send(out: &mut impl Write, value: &Value) -> io::Result<()> {
    writeln!(out, "{value}")?;
    out.flush()
}

fn reply(out: &mut impl Write, id: &Value, result: Value) -> io::Result<()> {
    send(
        out,
        &json!({ "jsonrpc": "2.0", "id": id, "result": result }),
    )
}

fn error(out: &mut impl Write, id: &Value, code: i64, message: &str) -> io::Result<()> {
    send(
        out,
        &json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } }),
    )
}

/// The request that asks the client a question.
fn elicitation_request(id: i64) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "elicitation/create",
        "params": {
            "message": QUESTION,
            "requestedSchema": {
                "type": "object",
                "properties": {
                    "color": {
                        "type": "string",
                        "title": "Color",
                        "enum": ["red", "blue"],
                    },
                },
                "required": ["color"],
            },
        },
    })
}

/// Reads lines from `input` until the response to `id` arrives, and returns its `result` (or its
/// `error`). A message of another kind is answered when it is a request, and skipped otherwise.
fn await_response(
    input: &mut impl BufRead,
    out: &mut impl Write,
    id: i64,
) -> io::Result<Option<Value>> {
    let mut line = String::new();
    loop {
        line.clear();
        if input.read_line(&mut line)? == 0 {
            return Ok(None);
        }
        let Ok(message) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if message["id"] == id && message.get("method").is_none() {
            return Ok(Some(
                message
                    .get("result")
                    .or_else(|| message.get("error"))
                    .cloned()
                    .unwrap_or(Value::Null),
            ));
        }
        if message.get("method") == Some(&json!("ping")) {
            reply(out, &message["id"], json!({}))?;
        }
    }
}

/// Runs the server until the input ends.
pub fn serve(mut input: impl BufRead, mut out: impl Write) -> io::Result<()> {
    let mut next_id = 1000;
    let mut line = String::new();
    loop {
        line.clear();
        if input.read_line(&mut line)? == 0 {
            return Ok(());
        }
        let Ok(message) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let (Some(method), id) = (message["method"].as_str(), &message["id"]) else {
            continue;
        };
        if id.is_null() {
            // A notification, such as `notifications/initialized`.
            continue;
        }
        match method {
            "initialize" => {
                let version = message["params"]["protocolVersion"]
                    .as_str()
                    .unwrap_or(DEFAULT_PROTOCOL);
                reply(
                    &mut out,
                    id,
                    json!({
                        "protocolVersion": version,
                        "capabilities": { "tools": {} },
                        "serverInfo": { "name": "golden", "version": "0.0.0" },
                    }),
                )?;
            }
            "ping" => reply(&mut out, id, json!({}))?,
            "tools/list" => reply(
                &mut out,
                id,
                json!({ "tools": [{
                    "name": "ask",
                    "description": "Ask the user which color they want.",
                    "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false },
                }] }),
            )?,
            "tools/call" if message["params"]["name"] == "ask" => {
                next_id += 1;
                send(&mut out, &elicitation_request(next_id))?;
                let answer = await_response(&mut input, &mut out, next_id)?;
                let text = match answer {
                    Some(answer) => format!("The client answered: {answer}"),
                    None => "The client closed the connection".to_owned(),
                };
                reply(
                    &mut out,
                    id,
                    json!({ "content": [{ "type": "text", "text": text }], "isError": false }),
                )?;
            }
            _ => error(&mut out, id, -32601, "method not found")?,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(input: &[Value]) -> Vec<Value> {
        let text: String = input.iter().map(|v| format!("{v}\n")).collect();
        let mut out = Vec::new();
        serve(text.as_bytes(), &mut out).expect("the server runs");
        String::from_utf8(out)
            .expect("UTF-8")
            .lines()
            .map(|l| serde_json::from_str(l).expect("JSON"))
            .collect()
    }

    #[test]
    fn the_server_initializes_and_lists_its_tool() {
        let out = run(&[
            json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "2025-03-26" } }),
            json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
            json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }),
            json!({ "jsonrpc": "2.0", "id": 3, "method": "nothing" }),
        ]);
        assert_eq!(out[0]["result"]["protocolVersion"], "2025-03-26");
        assert_eq!(out[1]["result"]["tools"][0]["name"], "ask");
        assert_eq!(out[2]["error"]["code"], -32601);
        assert_eq!(out.len(), 3, "a notification gets no answer");
    }

    #[test]
    fn a_call_asks_the_client_and_returns_the_answer() {
        let out = run(&[
            json!({ "jsonrpc": "2.0", "id": 5, "method": "tools/call", "params": { "name": "ask", "arguments": {} } }),
            json!({ "jsonrpc": "2.0", "id": 1001, "result": { "action": "accept", "content": { "color": "red" } } }),
        ]);
        assert_eq!(out[0]["method"], "elicitation/create");
        assert_eq!(out[0]["params"]["message"], QUESTION);
        assert_eq!(out[0]["id"], 1001);
        assert_eq!(out[1]["id"], 5);
        let text = out[1]["result"]["content"][0]["text"]
            .as_str()
            .expect("a text");
        assert!(
            text.contains("\"action\":\"accept\"") && text.contains("red"),
            "{text}"
        );
    }
}
