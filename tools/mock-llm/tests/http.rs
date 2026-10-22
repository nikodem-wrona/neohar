//! The mock model over HTTP: both protocols, the log, and the unknown endpoints.
//!
//! The tests send the requests of `fixtures/` with a small client on a TCP socket. The client
//! needs no HTTP library: the mock closes the connection after each answer.

use std::{
    collections::HashMap,
    net::SocketAddr,
    time::{Duration, Instant},
};

use mock_llm::{LogSink, Scenario, Server, serve};
use serde_json::{Value, json};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

const SCENARIO: &str = r#"
default_reply = "Default."

[[turn]]
blocks = [{ type = "text", text = "Hello from the mock model." }]
input_tokens = 100
output_tokens = 6

[[turn]]
match_user = "a.txt"
blocks = [{ type = "text", text = "Two files: a.txt and b.txt." }]

[[turn]]
match_user = "list the files"
blocks = [
  { type = "text", text = "I will look." },
  { type = "tool_use", name = "Bash", input = { command = "ls" } },
]
"#;

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/tests/fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap_or_else(|e| panic!("fixture {name}: {e}"))
}

struct Answer {
    status: u16,
    headers: HashMap<String, String>,
    body: Vec<u8>,
}

impl Answer {
    fn json(&self) -> Value {
        serde_json::from_slice(&self.body)
            .unwrap_or_else(|e| panic!("not JSON ({e}): {}", String::from_utf8_lossy(&self.body)))
    }

    /// The events of an SSE body, as `(event, data)`.
    fn events(&self) -> Vec<(String, Value)> {
        String::from_utf8_lossy(&self.body)
            .split("\n\n")
            .filter(|block| !block.trim().is_empty())
            .map(|block| {
                let mut name = String::new();
                let mut data = String::new();
                for line in block.lines() {
                    if let Some(rest) = line.strip_prefix("event:") {
                        name = rest.trim().to_owned();
                    } else if let Some(rest) = line.strip_prefix("data:") {
                        data.push_str(rest.trim());
                    }
                }
                (
                    name,
                    serde_json::from_str(&data).unwrap_or_else(|e| panic!("data {data:?}: {e}")),
                )
            })
            .collect()
    }
}

/// Decodes a body with `Transfer-Encoding: chunked`.
fn dechunk(mut raw: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        let Some(end) = raw.windows(2).position(|w| w == b"\r\n") else {
            return out;
        };
        let size = usize::from_str_radix(String::from_utf8_lossy(&raw[..end]).trim(), 16)
            .unwrap_or_else(|e| panic!("a chunk size: {e}"));
        if size == 0 {
            return out;
        }
        out.extend_from_slice(&raw[end + 2..end + 2 + size]);
        raw = &raw[end + 2 + size + 2..];
    }
}

async fn http(
    addr: SocketAddr,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: &[u8],
) -> Answer {
    let mut stream = TcpStream::connect(addr).await.expect("the server accepts");
    let mut head = format!(
        "{method} {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\nContent-Length: {}\r\n",
        body.len()
    );
    for (name, value) in headers {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    head.push_str("\r\n");
    stream
        .write_all(head.as_bytes())
        .await
        .expect("write the head");
    stream.write_all(body).await.expect("write the body");
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).await.expect("read the answer");
    let split = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .expect("a head");
    let head = String::from_utf8_lossy(&raw[..split]).into_owned();
    let mut lines = head.lines();
    let status = lines
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|s| s.parse().ok())
        .expect("a status line");
    let headers: HashMap<String, String> = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_owned()))
        .collect();
    let rest = &raw[split + 4..];
    let body = if headers
        .get("transfer-encoding")
        .is_some_and(|v| v == "chunked")
    {
        dechunk(rest)
    } else {
        rest.to_vec()
    };
    Answer {
        status,
        headers,
        body,
    }
}

async fn start(scenario: &str) -> (Server, std::sync::Arc<std::sync::Mutex<Vec<Value>>>) {
    let (sink, log) = LogSink::memory();
    let server = serve(
        Scenario::parse(scenario).expect("a good scenario"),
        sink,
        SocketAddr::from(([127, 0, 0, 1], 0)),
    )
    .await
    .expect("the server starts");
    (server, log)
}

async fn post(server: &Server, path: &str, fixture_name: &str) -> Answer {
    http(
        server.addr(),
        "POST",
        path,
        &[
            ("content-type", "application/json"),
            ("x-api-key", "mock"),
            ("anthropic-version", "2023-06-01"),
        ],
        &fixture(fixture_name),
    )
    .await
}

#[tokio::test]
async fn an_anthropic_stream_has_the_events_in_order_and_the_text_of_the_turn() {
    let (server, _log) = start(SCENARIO).await;
    let answer = post(&server, "/v1/messages", "anthropic-prompt.json").await;
    assert_eq!(answer.status, 200);
    assert!(answer.headers["content-type"].starts_with("text/event-stream"));
    assert!(answer.headers.contains_key("request-id"));
    let events = answer.events();
    let names: Vec<&str> = events.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names.first(), Some(&"message_start"));
    assert_eq!(names.last(), Some(&"message_stop"));
    assert!(names.contains(&"content_block_delta"));
    let text: String = events
        .iter()
        .filter(|(n, _)| n == "content_block_delta")
        .map(|(_, d)| d["delta"]["text"].as_str().unwrap_or("").to_owned())
        .collect();
    assert_eq!(text, "Hello from the mock model.");
    // The model name of the request comes back, and the turn's numbers are the usage.
    assert_eq!(events[0].1["message"]["model"], "claude-sonnet-4-5");
    assert_eq!(events[0].1["message"]["usage"]["input_tokens"], 100);
    server.stop().await;
}

#[tokio::test]
async fn a_tool_result_gets_the_turn_that_wants_it_and_a_request_for_a_tool_gets_a_tool_call() {
    let (server, _log) = start(SCENARIO).await;
    // The first main request takes turn 0.
    post(&server, "/v1/messages", "anthropic-prompt.json").await;
    // "a.txt" is in the tool result, so turn 1 fits.
    let answer = post(&server, "/v1/messages", "anthropic-tool-result.json").await;
    let text: String = answer
        .events()
        .iter()
        .filter(|(n, _)| n == "content_block_delta")
        .map(|(_, d)| d["delta"]["text"].as_str().unwrap_or("").to_owned())
        .collect();
    assert_eq!(text, "Two files: a.txt and b.txt.");
    let report = server.report();
    assert_eq!(
        (report.turns_used, report.turns_total, report.unmatched_main),
        (2, 3, 0)
    );
    server.stop().await;
}

#[tokio::test]
async fn a_request_without_tools_is_a_call_on_the_side_and_uses_no_turn() {
    let (server, log) = start(SCENARIO).await;
    let answer = post(&server, "/v1/messages", "anthropic-title.json").await;
    assert_eq!(answer.status, 200);
    let message = answer.json();
    assert_eq!(message["content"][0]["text"], "Default.");
    assert_eq!(message["model"], "claude-haiku-4-5");
    assert_eq!(server.report().turns_used, 0);
    {
        let log = log.lock().expect("lock");
        assert_eq!(
            (log[0]["main"].clone(), log[0]["turn"].clone()),
            (json!(false), Value::Null)
        );
    }
    server.stop().await;
}

#[tokio::test]
async fn a_main_request_that_no_turn_fits_gets_the_default_and_is_counted() {
    let (server, _log) = start("default_reply = \"Nothing left.\"").await;
    let answer = post(&server, "/v1/messages", "anthropic-prompt.json").await;
    let text: String = answer
        .events()
        .iter()
        .filter(|(n, _)| n == "content_block_delta")
        .map(|(_, d)| d["delta"]["text"].as_str().unwrap_or("").to_owned())
        .collect();
    assert_eq!(text, "Nothing left.");
    assert_eq!(server.report().unmatched_main, 1);
    server.stop().await;
}

#[tokio::test]
async fn the_responses_api_streams_text_and_returns_a_whole_response_without_stream() {
    let (server, _log) = start(SCENARIO).await;
    let answer = post(&server, "/v1/responses", "openai-prompt.json").await;
    assert_eq!(answer.status, 200);
    let events = answer.events();
    let names: Vec<&str> = events.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names.first(), Some(&"response.created"));
    assert_eq!(names.last(), Some(&"response.completed"));
    let done = &events.last().expect("an event").1["response"];
    assert_eq!(
        done["output"][0]["content"][0]["text"],
        "Hello from the mock model."
    );
    assert_eq!(done["usage"]["input_tokens"], 100);
    // The second request has no stream flag, and "a.txt" is in the tool output: turn 1.
    let whole = post(&server, "/v1/responses", "openai-tool-output.json")
        .await
        .json();
    assert_eq!(whole["object"], "response");
    assert_eq!(
        whole["output"][0]["content"][0]["text"],
        "Two files: a.txt and b.txt."
    );
    server.stop().await;
}

#[tokio::test]
async fn a_tool_turn_in_the_responses_api_is_a_function_call() {
    let (server, _log) = start(
        "[[turn]]\nblocks = [{ type = \"tool_use\", name = \"shell\", input = { command = [\"ls\"] } }]",
    )
    .await;
    let answer = post(&server, "/v1/responses", "openai-prompt.json").await;
    let done = answer.events().pop().expect("an event").1["response"].clone();
    assert_eq!(done["output"][0]["type"], "function_call");
    assert_eq!(done["output"][0]["name"], "shell");
    assert_eq!(done["output"][0]["arguments"], "{\"command\":[\"ls\"]}");
    server.stop().await;
}

#[tokio::test]
async fn count_tokens_answers_with_a_number() {
    let (server, log) = start(SCENARIO).await;
    let answer = post(
        &server,
        "/v1/messages/count_tokens",
        "anthropic-prompt.json",
    )
    .await;
    assert_eq!(answer.status, 200);
    assert!(
        answer.json()["input_tokens"]
            .as_u64()
            .is_some_and(|n| n > 0)
    );
    assert_eq!(log.lock().expect("lock").len(), 1, "the call is in the log");
    assert_eq!(server.report().turns_used, 0, "and it uses no turn");
    server.stop().await;
}

#[tokio::test]
async fn every_request_is_logged_with_its_body_and_a_secret_header_is_hidden() {
    let (server, log) = start(SCENARIO).await;
    post(&server, "/v1/messages", "anthropic-prompt.json").await;
    let entries: Vec<Value> = log.lock().expect("lock").clone();
    let entry = &entries[0];
    assert_eq!(
        (
            entry["seq"].clone(),
            entry["method"].clone(),
            entry["path"].clone()
        ),
        (json!(1), json!("POST"), json!("/v1/messages"))
    );
    assert_eq!(entry["api"], "anthropic");
    assert_eq!(entry["stream"], true);
    assert_eq!(entry["main"], true);
    assert_eq!(entry["turn"], 0);
    assert_eq!(
        entry["last_user"],
        "<system-reminder>context</system-reminder>\nsay hello"
    );
    assert_eq!(entry["body"]["model"], "claude-sonnet-4-5");
    assert_eq!(entry["headers"]["x-api-key"], "<redacted>");
    assert_eq!(entry["headers"]["anthropic-version"], "2023-06-01");
    server.stop().await;
}

#[tokio::test]
async fn an_unknown_endpoint_gets_a_404_and_a_line_in_the_log() {
    let (server, log) = start(SCENARIO).await;
    let answer = http(
        server.addr(),
        "POST",
        "/v1/surprise?x=1",
        &[("content-type", "application/json")],
        b"{\"what\":\"ever\"}",
    )
    .await;
    assert_eq!(answer.status, 404);
    assert_eq!(answer.json()["error"]["type"], "not_found_error");
    let entries: Vec<Value> = log.lock().expect("lock").clone();
    assert_eq!(entries[0]["path"], "/v1/surprise");
    assert_eq!(entries[0]["query"], "x=1");
    assert_eq!(entries[0]["body"], json!({ "what": "ever" }));
    assert_eq!(entries[0]["api"], Value::Null);
    server.stop().await;
}

#[tokio::test]
async fn a_body_that_is_no_json_gets_a_400_and_is_logged_as_text() {
    let (server, log) = start(SCENARIO).await;
    let answer = http(server.addr(), "POST", "/v1/messages", &[], b"not json").await;
    assert_eq!(answer.status, 400);
    assert_eq!(log.lock().expect("lock")[0]["body"], "not json");
    server.stop().await;
}

#[tokio::test]
async fn the_ready_endpoint_answers_and_is_not_logged() {
    let (server, log) = start(SCENARIO).await;
    let answer = http(server.addr(), "GET", "/__mock/ready", &[], b"").await;
    assert_eq!(answer.status, 200);
    assert!(log.lock().expect("lock").is_empty());
    server.stop().await;
}

#[tokio::test]
async fn the_delay_between_events_spaces_a_stream_out() {
    let (server, _log) =
        start("chunk_delay_ms = 15\n[[turn]]\nblocks = [{ type = \"text\", text = \"hi\" }]").await;
    let started = Instant::now();
    let answer = post(&server, "/v1/messages", "anthropic-prompt.json").await;
    let events = answer.events().len();
    // 15 ms before each of the events: at least half of that, to leave room for the clock.
    assert!(
        started.elapsed() >= Duration::from_millis(7 * events as u64),
        "{events} events"
    );
    server.stop().await;
}
