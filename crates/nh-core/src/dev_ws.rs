//! The WebSocket transport for development and for the tests.
//!
//! It is a test seam, not a product feature. It binds `127.0.0.1` only, it asks for a random
//! token in the URL of every connection, and it exists only in a build with the cargo feature
//! `dev-ws`. A release of the product never has it. A phone connects through the listener of
//! `nh-web`, which pairs the device.
//!
//! A client opens two kinds of connection. Both start with a `GET` that carries `token=<hex>` in
//! the query.
//!
//! **Control** (`/control`): text messages with JSON. The client sends one of
//!
//! ```text
//! {"id": 1, "call": {"method": "pane.resize", "params": {…}}}
//! {"id": 2, "subscribe": {"name": "panes"}}
//! ```
//!
//! and the server answers `{"id": 1, "result": …}` or `{"id": 1, "error": {code, message}}`. A
//! subscription sends `{"sub": 2, "message": …}` for every message of the projection. The server
//! runs the calls of one connection one after the other, in the order that they came, because the
//! order of `pane.input` matters. A call that takes long delays the calls after it on the same
//! connection.
//!
//! **RPC** (`/rpc`): JSON-RPC 2.0 in text messages, the protocol of `api.sock` with the
//! scope of the UI. [`crate::ws_rpc`] serves it, as it serves the listener of the phone.
//!
//! **Stream** (`/stream?kind=render&params=<percent-encoded JSON>`): the server sends one binary
//! message for each frame and nothing else, so the page needs no code to split or strip bytes.
//!
//! The server serves from a [`DevWsBackend`]. [`CoreHandle`] is one: [`CoreHandle::dev_ws_serve`]
//! serves the core in this process. A client of a link (crate `nh-remote`) is another,
//! so that the shell tests can run against a core behind `link.sock`.

use std::{io, net::SocketAddr, sync::Arc};

use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::{
    net::{TcpListener, TcpStream},
    runtime::Handle,
    sync::mpsc,
};
use tokio_tungstenite::{
    accept_hdr_async,
    tungstenite::{
        Message,
        handshake::server::{ErrorResponse, Request, Response},
        http::StatusCode,
    },
};

use crate::{CoreHandle, FrameSink, MessageSink, RpcError, ws_rpc::RpcBackend};

/// Where the dev WebSocket listens and the token that a client must send.
#[derive(Clone, Debug)]
pub struct DevWsInfo {
    pub port: u16,
    pub token: String,
}

/// What a connection asked for in its URL.
enum Route {
    Control,
    Rpc,
    Stream { kind: String, params: Value },
}

/// The request of a client on the control connection.
#[derive(Deserialize)]
struct ControlRequest {
    id: u64,
    call: Option<Call>,
    subscribe: Option<Subscribe>,
}

#[derive(Deserialize)]
struct Call {
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Deserialize)]
struct Subscribe {
    name: String,
    #[serde(default)]
    since: Option<u64>,
}

/// What the dev WebSocket serves from: the commands and subscriptions of [`RpcBackend`], and the
/// streams. The calls have the scope of the UI and no window.
#[async_trait::async_trait]
pub trait DevWsBackend: RpcBackend {
    /// `stream(kind, params)`. The frames go to the sink until it says that its receiver is gone.
    async fn stream(
        &self,
        kind: &str,
        params: Value,
        sink: Arc<dyn FrameSink>,
    ) -> Result<(), RpcError>;
}

#[async_trait::async_trait]
impl RpcBackend for CoreHandle {
    async fn call(&self, method: &str, params: Value) -> Result<Value, RpcError> {
        self.rpc(method, params).await
    }

    async fn subscribe(
        &self,
        name: &str,
        since: Option<u64>,
        sink: Box<dyn MessageSink>,
    ) -> Result<String, RpcError> {
        self.subscribe_tracked(name, since, sink)
    }

    async fn unsubscribe(&self, subscription: &str) -> bool {
        self.unsubscribe_tracked(subscription)
    }
}

#[async_trait::async_trait]
impl DevWsBackend for CoreHandle {
    async fn stream(
        &self,
        kind: &str,
        params: Value,
        sink: Arc<dyn FrameSink>,
    ) -> Result<(), RpcError> {
        CoreHandle::stream(self, kind, params, sink)
    }
}

impl CoreHandle {
    /// Starts the dev WebSocket of this core on a free port of `127.0.0.1` and returns where it
    /// listens. The token is random.
    ///
    /// The port is bound before this returns, so a client can connect at once. It does not wait
    /// for anything else, so any thread can call it.
    pub fn dev_ws_serve(&self) -> io::Result<DevWsInfo> {
        serve(Arc::new(self.clone()), &self.runtime())
    }
}

/// Starts the dev WebSocket for `backend` on a free port of `127.0.0.1`, with its tasks on
/// `runtime`, and returns where it listens. The token is random.
///
/// The port is bound before this returns, so a client can connect at once.
pub fn serve(backend: Arc<dyn DevWsBackend>, runtime: &Handle) -> io::Result<DevWsInfo> {
    let mut random = [0u8; 16];
    getrandom::fill(&mut random).map_err(io::Error::other)?;
    let token: String = random.iter().map(|b| format!("{b:02x}")).collect();
    let bound = std::net::TcpListener::bind(("127.0.0.1", 0))?;
    bound.set_nonblocking(true)?;
    let port = bound.local_addr()?.port();
    let info = DevWsInfo { port, token };
    let token = Arc::<str>::from(info.token.as_str());
    let enter = runtime.clone();
    runtime.spawn(async move {
        let Ok(listener) = TcpListener::from_std(bound) else {
            tracing::error!("the dev WebSocket cannot use its port");
            return;
        };
        loop {
            let Ok((stream, peer)) = listener.accept().await else {
                continue;
            };
            enter.spawn(connection(
                Arc::clone(&backend),
                stream,
                peer,
                Arc::clone(&token),
            ));
        }
    });
    Ok(info)
}

// The handshake callback must return the HTTP response of tungstenite as its error, which is large.
#[expect(
    clippy::result_large_err,
    reason = "the type of the handshake callback is fixed by tokio-tungstenite"
)]
async fn connection(
    core: Arc<dyn DevWsBackend>,
    stream: TcpStream,
    peer: SocketAddr,
    token: Arc<str>,
) {
    // One message at a time must not wait for the ack of the one before it.
    let _ = stream.set_nodelay(true);
    let mut route = None;
    let ws = accept_hdr_async(
        stream,
        |request: &Request, response: Response| match parse_route(request.uri(), &token) {
            Some(parsed) => {
                route = Some(parsed);
                Ok(response)
            }
            None => Err(refusal()),
        },
    )
    .await;
    let (Ok(ws), Some(route)) = (ws, route) else {
        tracing::debug!(%peer, "the dev WebSocket refused a connection");
        return;
    };
    match route {
        Route::Control => control(core, ws).await,
        Route::Rpc => crate::ws_rpc::serve(core, ws, std::future::pending()).await,
        Route::Stream { kind, params } => frames(core, ws, &kind, params).await,
    }
}

fn refusal() -> ErrorResponse {
    let mut response = ErrorResponse::new(Some("not allowed".to_owned()));
    *response.status_mut() = StatusCode::FORBIDDEN;
    response
}

/// Reads the path and the query of a request. Returns `None` for a wrong token or an unknown
/// path, so that the handshake fails.
fn parse_route(uri: &tokio_tungstenite::tungstenite::http::Uri, token: &str) -> Option<Route> {
    let mut given = None;
    let mut kind = None;
    let mut params = None;
    for pair in uri.query().unwrap_or("").split('&') {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        match key {
            "token" => given = Some(value.to_owned()),
            "kind" => kind = percent_decode(value),
            "params" => params = percent_decode(value),
            _ => {}
        }
    }
    if !constant_time_eq(given?.as_bytes(), token.as_bytes()) {
        return None;
    }
    match uri.path() {
        "/control" => Some(Route::Control),
        "/rpc" => Some(Route::Rpc),
        "/stream" => {
            let params = serde_json::from_str(params.as_deref().unwrap_or("{}")).ok()?;
            Some(Route::Stream {
                kind: kind?,
                params,
            })
        }
        _ => None,
    }
}

/// Compares two byte strings in a time that does not depend on where they differ.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Decodes `%XX` escapes. `+` stays a plus: the page uses `encodeURIComponent`.
fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = bytes.get(i + 1..i + 3)?;
            out.push(u8::from_str_radix(std::str::from_utf8(hex).ok()?, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

type Ws = tokio_tungstenite::WebSocketStream<TcpStream>;

/// The sink of a subscription: it queues the messages for the writer of the connection.
struct Subscription {
    id: u64,
    out: mpsc::UnboundedSender<Message>,
}

impl MessageSink for Subscription {
    fn send(&self, message: Value) -> bool {
        let text = json!({ "sub": self.id, "message": message }).to_string();
        self.out.send(Message::Text(text.into())).is_ok()
    }
}

async fn control(core: Arc<dyn DevWsBackend>, ws: Ws) {
    let (mut write, mut read) = ws.split();
    let (out, mut queue) = mpsc::unbounded_channel::<Message>();
    let writer = tokio::spawn(async move {
        while let Some(message) = queue.recv().await {
            if write.feed(message).await.is_err() {
                return;
            }
            while let Ok(next) = queue.try_recv() {
                if write.feed(next).await.is_err() {
                    return;
                }
            }
            if write.flush().await.is_err() {
                return;
            }
        }
    });
    while let Some(Ok(message)) = read.next().await {
        let Message::Text(text) = message else {
            continue;
        };
        let Ok(request) = serde_json::from_str::<ControlRequest>(&text) else {
            continue;
        };
        let answer = handle(&core, &out, &request).await;
        let reply = match answer {
            Ok(result) => json!({ "id": request.id, "result": result }),
            Err(error) => json!({ "id": request.id, "error": error }),
        };
        if out.send(Message::Text(reply.to_string().into())).is_err() {
            break;
        }
    }
    drop(out);
    writer.abort();
}

async fn handle(
    core: &Arc<dyn DevWsBackend>,
    out: &mpsc::UnboundedSender<Message>,
    request: &ControlRequest,
) -> Result<Value, RpcError> {
    if let Some(call) = &request.call {
        return core.call(&call.method, call.params.clone()).await;
    }
    if let Some(subscribe) = &request.subscribe {
        let sink = Subscription {
            id: request.id,
            out: out.clone(),
        };
        core.subscribe(&subscribe.name, subscribe.since, Box::new(sink))
            .await?;
        return Ok(json!({}));
    }
    Err(RpcError::invalid_params(
        "a request has `call` or `subscribe`",
    ))
}

/// The sink of a stream: it queues the frames for the writer of the connection.
struct Frames(mpsc::UnboundedSender<Vec<u8>>);

impl FrameSink for Frames {
    fn send(&self, frame: Vec<u8>) -> bool {
        self.0.send(frame).is_ok()
    }
}

async fn frames(core: Arc<dyn DevWsBackend>, mut ws: Ws, kind: &str, params: Value) {
    let (tx, mut rx) = mpsc::unbounded_channel::<Vec<u8>>();
    if let Err(error) = core.stream(kind, params, Arc::new(Frames(tx))).await {
        let _ = ws
            .send(Message::Text(json!({ "error": error }).to_string().into()))
            .await;
        return;
    }
    // A message that the client never reads cannot fill the memory of the core: the render
    // server holds at most two frames in flight for a pane.
    while let Some(frame) = rx.recv().await {
        if ws.feed(Message::Binary(frame.into())).await.is_err() {
            return;
        }
        while let Ok(next) = rx.try_recv() {
            if ws.feed(Message::Binary(next.into())).await.is_err() {
                return;
            }
        }
        if ws.flush().await.is_err() {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uri(text: &str) -> tokio_tungstenite::tungstenite::http::Uri {
        text.parse().expect("a URI")
    }

    #[test]
    fn a_route_needs_the_token_and_a_known_path() {
        assert!(matches!(
            parse_route(&uri("/control?token=abc"), "abc"),
            Some(Route::Control)
        ));
        assert!(parse_route(&uri("/control?token=abd"), "abc").is_none());
        assert!(parse_route(&uri("/control"), "abc").is_none());
        assert!(parse_route(&uri("/other?token=abc"), "abc").is_none());
    }

    #[test]
    fn a_stream_route_carries_the_kind_and_the_json_parameters() {
        let route = parse_route(
            &uri(
                "/stream?token=t&kind=render&params=%7B%22window%22%3A%2201ARZ3NDEKTSV4RRFFQ69G5FAV%22%7D",
            ),
            "t",
        );
        let Some(Route::Stream { kind, params }) = route else {
            panic!("a stream route");
        };
        assert_eq!(kind, "render");
        assert_eq!(params, json!({ "window": "01ARZ3NDEKTSV4RRFFQ69G5FAV" }));
    }

    #[test]
    fn percent_decoding_rejects_a_cut_escape() {
        assert_eq!(percent_decode("a%20b").as_deref(), Some("a b"));
        assert_eq!(percent_decode("a+b").as_deref(), Some("a+b"));
        assert_eq!(percent_decode("a%2"), None);
        assert_eq!(percent_decode("a%zz"), None);
    }

    #[test]
    fn tokens_compare_without_stopping_at_the_first_difference() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"ab"));
    }
}
