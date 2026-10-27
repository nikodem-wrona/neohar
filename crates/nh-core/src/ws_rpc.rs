//! JSON-RPC 2.0 over a WebSocket: the protocol of `api.sock` on one socket.
//!
//! A text message is a request. `projection.subscribe {name, since}` answers `{subscription}` and
//! then sends the notification `projection.message {subscription, message}` for every message of
//! the projection. `projection.unsubscribe {subscription}` ends a subscription of this connection.
//! When the connection ends, its subscriptions end. The calls of one connection run one after the
//! other, in the order that they came, because the order of `pane.input` matters.
//!
//! The queues have a limit, as on `api.sock` ([`crate::subscriptions`]). A client that reads too
//! slowly fills the queue of a subscription. Then the hub drops the sink, and the client gets
//! `{type: "resync"}` and subscribes again.
//!
//! Two servers use it: the dev WebSocket (`crate::dev_ws`, development builds only) and
//! the listener of the phone (crate `nh-web`), which a release build has. Each gives a
//! [`RpcBackend`] that calls the core with the origin of its caller.

use std::{
    future::Future,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::mpsc,
};
use tokio_tungstenite::{WebSocketStream, tungstenite::Message};

use crate::{
    ErrorCode, MessageSink, RpcError,
    subscriptions::{PROJECTION_QUEUE, QueueSink},
};

/// How many messages for the client wait for the socket. A task that has a message for a client
/// that reads slowly waits for room, so a client that stops reading cannot make the core keep
/// messages without limit.
const OUT_QUEUE: usize = 512;

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

/// A notification of a subscription.
fn notification(id: &str, message: Value) -> Message {
    let text = json!({
        "jsonrpc": "2.0",
        "method": "projection.message",
        "params": { "subscription": id, "message": message },
    })
    .to_string();
    Message::Text(text.into())
}

/// Moves the messages of a subscription to the socket. It starts after the answer to the subscribe
/// call, so the client knows the id of each message. When the queue of the subscription was full,
/// the hub dropped the sink: the client then gets `{type: "resync"}` and subscribes again.
async fn forward(
    id: String,
    mut messages: mpsc::Receiver<Value>,
    overflow: Arc<AtomicBool>,
    out: mpsc::Sender<Message>,
) {
    while let Some(message) = messages.recv().await {
        if out.send(notification(&id, message)).await.is_err() {
            return;
        }
    }
    if overflow.load(Ordering::SeqCst) {
        let _ = out
            .send(notification(&id, json!({ "type": "resync" })))
            .await;
    }
}

/// Queues a message for the socket. It waits while the client reads slowly, and gives up when the
/// connection must stop.
async fn send<F: Future<Output = ()>>(
    out: &mpsc::Sender<Message>,
    message: Message,
    stop: Pin<&mut F>,
) -> bool {
    tokio::select! {
        sent = out.send(message) => sent.is_ok(),
        () = stop => false,
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
    let (out, mut queue) = mpsc::channel::<Message>(OUT_QUEUE);
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
                if !send(&out, rpc_reply(&Value::Null, Err(failure)), stop.as_mut()).await {
                    break;
                }
                continue;
            }
        };
        let answer = match request.method.as_str() {
            "projection.subscribe" => {
                match serde_json::from_value::<SubscribeParams>(request.params.clone()) {
                    Ok(params) => {
                        let (queue, messages) = mpsc::channel(PROJECTION_QUEUE);
                        let overflow = Arc::new(AtomicBool::new(false));
                        let sink = Box::new(QueueSink {
                            queue,
                            overflow: Arc::clone(&overflow),
                        });
                        match core.subscribe(&params.name, params.since, sink).await {
                            Ok(id) => {
                                // The answer goes first, then the messages of the queue.
                                let reply =
                                    rpc_reply(&request.id, Ok(json!({ "subscription": id })));
                                if !send(&out, reply, stop.as_mut()).await {
                                    break;
                                }
                                tokio::spawn(forward(id.clone(), messages, overflow, out.clone()));
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
        if !send(&out, rpc_reply(&request.id, answer), stop.as_mut()).await {
            break;
        }
    }
    for id in subscriptions {
        core.unsubscribe(&id).await;
    }
    drop(out);
    writer.abort();
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use tokio::io::duplex;
    use tokio_tungstenite::tungstenite::protocol::Role;

    use super::*;

    /// A core with one projection, whose sink the test keeps.
    #[derive(Default)]
    struct Backend {
        sink: Mutex<Option<Box<dyn MessageSink>>>,
    }

    #[async_trait::async_trait]
    impl RpcBackend for Backend {
        async fn call(&self, _method: &str, _params: Value) -> Result<Value, RpcError> {
            Ok(json!({}))
        }

        async fn subscribe(
            &self,
            _name: &str,
            _since: Option<u64>,
            sink: Box<dyn MessageSink>,
        ) -> Result<String, RpcError> {
            sink.send(json!({ "type": "snapshot" }));
            *self.sink.lock().expect("the lock") = Some(sink);
            Ok("s1".to_owned())
        }

        async fn unsubscribe(&self, _subscription: &str) -> bool {
            true
        }
    }

    async fn next_text<S>(ws: &mut WebSocketStream<S>) -> Value
    where
        S: AsyncRead + AsyncWrite + Unpin,
    {
        loop {
            let message = tokio::time::timeout(std::time::Duration::from_secs(5), ws.next())
                .await
                .expect("a message in time")
                .expect("the socket is open")
                .expect("a message");
            if let Message::Text(text) = message {
                return serde_json::from_str(&text).expect("JSON");
            }
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_client_that_stops_reading_fills_a_queue_with_a_limit_and_then_resyncs() {
        let backend = Arc::new(Backend::default());
        let (server_io, client_io) = duplex(16 * 1024);
        let server = WebSocketStream::from_raw_socket(server_io, Role::Server, None).await;
        tokio::spawn(serve(
            Arc::clone(&backend) as Arc<dyn RpcBackend>,
            server,
            std::future::pending(),
        ));
        let mut client = WebSocketStream::from_raw_socket(client_io, Role::Client, None).await;
        let subscribe = json!({ "jsonrpc": "2.0", "id": 1, "method": "projection.subscribe",
            "params": { "name": "agents" } });
        client
            .send(Message::Text(subscribe.to_string().into()))
            .await
            .expect("sends");
        assert_eq!(next_text(&mut client).await["result"]["subscription"], "s1");
        assert_eq!(
            next_text(&mut client).await["params"]["message"]["type"],
            "snapshot"
        );

        // The client reads no more. The sink refuses a message before the core keeps much more
        // than the two queues.
        let patch = json!({ "type": "patch", "data": "x".repeat(1024) });
        let mut taken = 0;
        while backend
            .sink
            .lock()
            .expect("the lock")
            .as_ref()
            .expect("a sink")
            .send(patch.clone())
        {
            taken += 1;
            assert!(taken < 4 * (PROJECTION_QUEUE + OUT_QUEUE), "no limit");
            if taken % 64 == 0 {
                tokio::task::yield_now().await;
            }
        }
        // As the hub does with a sink that refuses a message.
        drop(backend.sink.lock().expect("the lock").take());

        // The client reads again: the patches that were taken, then `resync`.
        let mut patches = 0;
        loop {
            let message = next_text(&mut client).await;
            match message["params"]["message"]["type"].as_str() {
                Some("patch") => patches += 1,
                Some("resync") => break,
                other => panic!("unexpected message {other:?}"),
            }
        }
        assert_eq!(patches, taken);
    }
}
