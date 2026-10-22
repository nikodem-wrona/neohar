//! The HTTP server of the mock model.
//!
//! One handler takes every request, so that the log sees all of them. The routes are
//! `POST /v1/messages` and `POST /v1/messages/count_tokens` (Anthropic) and `POST /v1/responses`
//! (OpenAI). Any other request gets a 404 and a line in the log. `GET /__mock/ready` answers 200
//! and is not logged: a driver uses it to see that the server runs.

use std::{
    convert::Infallible,
    io,
    net::SocketAddr,
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use axum::{
    Json, Router,
    body::to_bytes,
    extract::{Request, State},
    http::{HeaderMap, StatusCode, header},
    response::{
        IntoResponse, Response,
        sse::{Event, Sse},
    },
};
use futures_util::stream;
use serde_json::{Value, json};
use tokio::{sync::oneshot, task::JoinHandle};

use crate::{
    anthropic,
    log::{LogSink, is_secret},
    openai, request,
    scenario::{Dispatcher, Reply, Scenario},
};

/// The largest request body that the mock reads.
const MAX_BODY: usize = 64 << 20;

struct Inner {
    dispatcher: Mutex<Dispatcher>,
    log: LogSink,
    /// The number of requests, for `seq` in the log.
    requests: AtomicU64,
    /// The number of replies, for the ids in them.
    replies: AtomicU64,
    unmatched_main: AtomicU64,
    started: Instant,
}

type Shared = Arc<Inner>;

/// What a run of the server did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Report {
    pub requests: u64,
    pub turns_used: usize,
    pub turns_total: usize,
    /// Main requests that no turn fit. They got the default reply.
    pub unmatched_main: u64,
}

/// A running server.
pub struct Server {
    addr: SocketAddr,
    state: Shared,
    shutdown: Option<oneshot::Sender<()>>,
    task: JoinHandle<()>,
}

/// Starts the server on `bind` (port 0 picks a free one).
pub async fn serve(scenario: Scenario, log: LogSink, bind: SocketAddr) -> io::Result<Server> {
    let listener = tokio::net::TcpListener::bind(bind).await?;
    let addr = listener.local_addr()?;
    let state = Arc::new(Inner {
        dispatcher: Mutex::new(Dispatcher::new(scenario)),
        log,
        requests: AtomicU64::new(0),
        replies: AtomicU64::new(0),
        unmatched_main: AtomicU64::new(0),
        started: Instant::now(),
    });
    let app = Router::new()
        .fallback(handle)
        .with_state(Arc::clone(&state));
    let (shutdown, stop) = oneshot::channel::<()>();
    let task = tokio::spawn(async move {
        let _ = axum::serve(listener, app)
            .with_graceful_shutdown(async {
                let _ = stop.await;
            })
            .await;
    });
    Ok(Server {
        addr,
        state,
        shutdown: Some(shutdown),
        task,
    })
}

impl Server {
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// The base URL that a harness takes as its API address.
    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    pub fn report(&self) -> Report {
        let (turns_used, turns_total) = self
            .state
            .dispatcher
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .progress();
        Report {
            requests: self.state.requests.load(Ordering::Relaxed),
            turns_used,
            turns_total,
            unmatched_main: self.state.unmatched_main.load(Ordering::Relaxed),
        }
    }

    /// Stops the server and waits until it ended.
    pub async fn stop(mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        let _ = self.task.await;
    }
}

fn headers_json(headers: &HeaderMap) -> Value {
    let mut map = serde_json::Map::new();
    for (name, value) in headers {
        let shown = if is_secret(name.as_str()) {
            "<redacted>".to_owned()
        } else {
            String::from_utf8_lossy(value.as_bytes()).into_owned()
        };
        map.insert(name.as_str().to_owned(), Value::String(shown));
    }
    Value::Object(map)
}

fn error(status: StatusCode, kind: &str, message: &str) -> Response {
    (
        status,
        Json(json!({ "type": "error", "error": { "type": kind, "message": message } })),
    )
        .into_response()
}

/// A stream of SSE events, with `delay` between two of them.
fn sse(events: Vec<(String, Value)>, delay: Duration) -> Response {
    let events = stream::unfold(events.into_iter(), move |mut rest| async move {
        let (name, data) = rest.next()?;
        if !delay.is_zero() {
            tokio::time::sleep(delay).await;
        }
        let event = Event::default()
            .event(name)
            .json_data(&data)
            .unwrap_or_else(|_| Event::default().data("{}"));
        Some((Ok::<_, Infallible>(event), rest))
    });
    Sse::new(events).into_response()
}

async fn handle(State(state): State<Shared>, request: Request) -> Response {
    let (parts, body) = request.into_parts();
    let method = parts.method.as_str().to_owned();
    let path = parts.uri.path().to_owned();
    if method == "GET" && path == "/__mock/ready" {
        return StatusCode::OK.into_response();
    }
    let bytes = match to_bytes(body, MAX_BODY).await {
        Ok(bytes) => bytes,
        Err(_) => {
            return error(
                StatusCode::PAYLOAD_TOO_LARGE,
                "request_too_large",
                "the body is too large",
            );
        }
    };
    let seq = state.requests.fetch_add(1, Ordering::Relaxed) + 1;
    let json: Option<Value> = serde_json::from_slice(&bytes).ok();
    let mut record = json!({
        "seq": seq,
        "at_ms": u64::try_from(state.started.elapsed().as_millis()).unwrap_or(u64::MAX),
        "method": method,
        "path": path,
        "query": parts.uri.query(),
        "headers": headers_json(&parts.headers),
    });
    record["body"] = match (&json, bytes.is_empty()) {
        (Some(value), _) => value.clone(),
        (None, true) => Value::Null,
        (None, false) => Value::String(String::from_utf8_lossy(&bytes).into_owned()),
    };

    let api = match (method.as_str(), path.as_str()) {
        ("POST", "/v1/messages") => Some(true),
        ("POST", "/v1/responses") => Some(false),
        ("POST", "/v1/messages/count_tokens") => {
            record["api"] = json!("anthropic");
            let answer = json
                .as_ref()
                .map_or_else(|| json!({ "input_tokens": 1 }), anthropic::count_tokens);
            state.log.write(&record);
            return Json(answer).into_response();
        }
        _ => None,
    };
    let (Some(anthropic_api), Some(body)) = (api, json.as_ref()) else {
        record["api"] = Value::Null;
        state.log.write(&record);
        return if api.is_some() {
            error(
                StatusCode::BAD_REQUEST,
                "invalid_request_error",
                "the body is not JSON",
            )
        } else {
            error(
                StatusCode::NOT_FOUND,
                "not_found_error",
                &format!("mock-llm has no endpoint {method} {path}"),
            )
        };
    };

    let parsed = if anthropic_api {
        request::anthropic(body)
    } else {
        request::openai(body)
    };
    let (mut reply, delay): (Reply, u64) = {
        let mut dispatcher = state
            .dispatcher
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        (
            dispatcher.pick(parsed.main, &parsed.last_user),
            dispatcher.chunk_delay_ms(),
        )
    };
    let failures = reply.expand(&String::from_utf8_lossy(&bytes));
    if !failures.is_empty() {
        record["expansion_failures"] = json!(failures);
    }
    if parsed.main && reply.turn.is_none() {
        state.unmatched_main.fetch_add(1, Ordering::Relaxed);
    }
    record["api"] = json!(if anthropic_api { "anthropic" } else { "openai" });
    record["model"] = json!(parsed.model);
    record["stream"] = json!(parsed.stream);
    record["main"] = json!(parsed.main);
    record["last_user"] = json!(parsed.last_user.chars().take(200).collect::<String>());
    record["turn"] = json!(reply.turn);
    state.log.write(&record);

    let n = state.replies.fetch_add(1, Ordering::Relaxed) + 1;
    let model = if parsed.model.is_empty() {
        anthropic::DEFAULT_MODEL.to_owned()
    } else {
        parsed.model.clone()
    };
    let input_tokens = anthropic::input_tokens(body);
    let delay = Duration::from_millis(delay);
    let mut response = match (anthropic_api, parsed.stream) {
        (true, true) => sse(
            anthropic::events(n, &model, &reply, input_tokens)
                .into_iter()
                .map(|(name, data)| (name.to_owned(), data))
                .collect(),
            delay,
        ),
        (true, false) => Json(anthropic::message(n, &model, &reply, input_tokens)).into_response(),
        (false, true) => sse(openai::events(n, &model, &reply, input_tokens), delay),
        (false, false) => {
            Json(openai::response(n, &model, &reply, input_tokens, true)).into_response()
        }
    };
    if let Ok(id) = format!("req_mock_{n:04}").parse() {
        response.headers_mut().insert("request-id", id);
    }
    response
        .headers_mut()
        .entry(header::CACHE_CONTROL)
        .or_insert(header::HeaderValue::from_static("no-cache"));
    response
}
