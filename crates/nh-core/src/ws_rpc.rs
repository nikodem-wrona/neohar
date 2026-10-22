//! JSON-RPC 2.0 over a WebSocket: the protocol of `api.sock` on one socket.
//!
//! A text message is a request. `projection.subscribe {name, since}` answers `{subscription}` and
//! then sends the notification `projection.message {subscription, message}` for every message of
//! the projection. `projection.unsubscribe {subscription}` ends a subscription of this connection.
//! When the connection ends, its subscriptions end. The calls of one connection run one after the
//! other, in the order that they came, because the order of `pane.input` matters.
//!
//! Two servers use it: the dev WebSocket (`crate::dev_ws`, development builds only) and
//! the listener of the phone (crate `nh-web`), which a release build has. Each gives a
//! [`RpcBackend`] that calls the core with the origin of its caller.

use std::{future::Future, sync::Arc};

use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::mpsc,
};
use tokio_tungstenite::{WebSocketStream, tungstenite::Message};

use crate::{ErrorCode, MessageSink, RpcError};

/// What a connection calls: the commands of the transport and the end of a
/// subscription.
#[async_trait::async_trait]
pub trait RpcBackend: Send + Sync + 'static {
    /// `call(method, params)`, with the origin of the caller of this backend.
    async fn call(&self, method: &str, params: Value) -> Result<Value, RpcError>;
    /// `subscribe(projection)`. The sink gets the snapshot and then the patches. The answer is the
    /// id that [`RpcBackend::unsubscribe`] takes.
    async fn subscribe(
        &self,
        name: &str,
        since: Option<u64>,
        sink: Box<dyn MessageSink>,
    ) -> Result<String, RpcError>;
    /// Ends a subscription. Returns whether it existed.
    async fn unsubscribe(&self, subscription: &str) -> bool;
}

/// A request of JSON-RPC.
#[derive(Deserialize)]
struct RpcRequest {
    #[serde(default)]
    id: Value,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Deserialize)]
struct SubscribeParams {
    name: String,
    #[serde(default)]
    since: Option<u64>,
}

/// The sink of a subscription. The snapshot goes to the sink while `subscribe` runs, before the id
/// is known, so the sink holds the messages until the id is set.
struct Notifier {
    out: mpsc::UnboundedSender<Message>,
    state: std::sync::Mutex<NotifierState>,
}

#[derive(Default)]
struct NotifierState {
    id: Option<String>,
    held: Vec<Value>,
}

impl Notifier {
    fn notification(id: &str, message: Value) -> Message {
        let text = json!({
            "jsonrpc": "2.0",
            "method": "projection.message",
            "params": { "subscription": id, "message": message },
        })
        .to_string();
        Message::Text(text.into())
    }

    /// Gives the sink its id and sends what it held.
    fn set_id(&self, id: &str) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.id = Some(id.to_owned());
        for message in std::mem::take(&mut state.held) {
            let _ = self.out.send(Self::notification(id, message));
        }
    }
}

impl MessageSink for Notifier {
    fn send(&self, message: Value) -> bool {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match &state.id {
            Some(id) => self.out.send(Self::notification(id, message)).is_ok(),
            None => {
                state.held.push(message);
                !self.out.is_closed()
            }
        }
    }
}

/// The hub wants a boxed sink, and the connection keeps a handle to set the id.
struct SharedNotifier(Arc<Notifier>);

impl MessageSink for SharedNotifier {
    fn send(&self, message: Value) -> bool {
        self.0.send(message)
    }
}

fn rpc_reply(id: &Value, answer: Result<Value, RpcError>) -> Message {
    let body = match answer {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err(error) => {
            let mut data = json!({ "code": error.code });
            if !error.details.is_null() {
                data["details"] = error.details.clone();
            }
            json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {
                    "code": error.code.rpc_code(),
                    "message": error.message,
                    "data": data,
                },
            })
        }
    };
    Message::Text(body.to_string().into())
}

/// Serves one connection until the client closes it or `stop` completes. Then it ends the
/// subscriptions of the connection and closes the socket.
pub async fn serve<S>(
    core: Arc<dyn RpcBackend>,
    ws: WebSocketStream<S>,
    stop: impl Future<Output = ()> + Send,
) where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let (mut write, mut read) = ws.split();
    let (out, mut queue) = mpsc::unbounded_channel::<Message>();
    let writer = tokio::spawn(async move {
        while let Some(message) = queue.recv().await {
            if write.send(message).await.is_err() {
                return;
            }
        }
    });
    let mut subscriptions: Vec<String> = Vec::new();
    tokio::pin!(stop);
    loop {
        let message = tokio::select! {
            () = &mut stop => break,
            next = read.next() => match next {
                Some(Ok(message)) => message,
                _ => break,
            },
        };
        let Message::Text(text) = message else {
            continue;
        };
        let request = match serde_json::from_str::<RpcRequest>(&text) {
            Ok(request) => request,
            Err(error) => {
                let failure = RpcError::new(ErrorCode::ParseError, error.to_string());
                if out.send(rpc_reply(&Value::Null, Err(failure))).is_err() {
                    break;
                }
                continue;
            }
        };
        let answer = match request.method.as_str() {
            "projection.subscribe" => {
                match serde_json::from_value::<SubscribeParams>(request.params.clone()) {
                    Ok(params) => {
                        let notifier = Arc::new(Notifier {
                            out: out.clone(),
                            state: std::sync::Mutex::default(),
                        });
                        let sink = Box::new(SharedNotifier(Arc::clone(&notifier)));
                        match core.subscribe(&params.name, params.since, sink).await {
                            Ok(id) => {
                                // The answer goes first, then the messages that the sink held.
                                let reply =
                                    rpc_reply(&request.id, Ok(json!({ "subscription": id })));
                                if out.send(reply).is_err() {
                                    break;
                                }
                                notifier.set_id(&id);
                                subscriptions.push(id);
                                continue;
                            }
                            Err(error) => Err(error),
                        }
                    }
                    Err(error) => Err(RpcError::invalid_params(error.to_string())),
                }
            }
            "projection.unsubscribe" => {
                let id = request.params["subscription"].as_str().unwrap_or_default();
                // A connection ends only its own subscriptions.
                let known = subscriptions.iter().any(|s| s == id);
                subscriptions.retain(|s| s != id);
                if known && core.unsubscribe(id).await {
                    Ok(json!({}))
                } else {
                    Err(RpcError::new(
                        ErrorCode::NotFound,
                        format!("no subscription {id}"),
                    ))
                }
            }
            method => core.call(method, request.params).await,
        };
        if out.send(rpc_reply(&request.id, answer)).is_err() {
            break;
        }
    }
    for id in subscriptions {
        core.unsubscribe(&id).await;
    }
    drop(out);
    writer.abort();
}
