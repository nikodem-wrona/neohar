//! `/stream` of a phone: one binary message for each frame of a stream of the
//! core, for the kinds `render`, `peek` and `scrollback`.
//!
//! The listener sends a WebSocket ping every second and measures the time to its pong. The sink
//! gives that round trip to the render server, so a phone on a slow path gets more frames in
//! flight, as a window over a link does. The stream ends when the phone closes it, the
//! device is removed or the listener stops.

use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use futures_util::{SinkExt, StreamExt, future::BoxFuture};
use nh_core::{CoreHandle, ErrorCode, FrameSink, RpcError};
use nh_proto::api::ids::WindowId;
use serde_json::{Value, json};
use tokio::sync::{broadcast, mpsc, watch};
use tokio_tungstenite::{WebSocketStream, tungstenite::Message};
use ulid::Ulid;

/// How often the listener measures the round trip of a stream.
const PING_EVERY: Duration = Duration::from_secs(1);

/// The kinds of stream that a phone may open.
const KINDS: [&str; 3] = ["render", "peek", "scrollback"];

/// The params of a stream, checked for the phone window: a render stream is for the window of the
/// phone, and it gets that window when it names none. A stream of another kind names a pane.
pub(crate) fn checked_params(
    kind: &str,
    mut params: Value,
    window: WindowId,
) -> Result<Value, RpcError> {
    if !KINDS.contains(&kind) {
        return Err(RpcError::new(
            ErrorCode::NotFound,
            format!("a phone has no stream {kind:?}"),
        ));
    }
    if kind != "render" {
        return Ok(params);
    }
    let Some(object) = params.as_object_mut() else {
        return Err(RpcError::invalid_params(
            "the params of a stream are an object",
        ));
    };
    match object.get("window").and_then(Value::as_str) {
        None => {
            object.insert("window".to_owned(), json!(window.to_string()));
        }
        Some(named) if named == window.to_string() => {}
        Some(_) => {
            return Err(RpcError::new(
                ErrorCode::PermissionDenied,
                "a phone renders its own window only",
            ));
        }
    }
    Ok(params)
}

/// The sink of a stream: it queues the frames for the writer and knows the last round trip.
struct Frames {
    out: mpsc::UnboundedSender<Vec<u8>>,
    /// The last round trip in microseconds. 0: not measured yet.
    rtt_us: Arc<AtomicU64>,
}

impl FrameSink for Frames {
    fn send(&self, frame: Vec<u8>) -> bool {
        self.out.send(frame).is_ok()
    }

    fn is_closed(&self) -> bool {
        self.out.is_closed()
    }

    fn round_trip(&self) -> Option<Duration> {
        match self.rtt_us.load(Ordering::Relaxed) {
            0 => None,
            us => Some(Duration::from_micros(us)),
        }
    }
}

/// What ends a stream besides the phone: the removal of the device and the stop of the listener.
pub(crate) struct Ends {
    pub core: CoreHandle,
    pub device: Ulid,
    pub token: String,
    pub removed: broadcast::Receiver<Ulid>,
    pub stop: watch::Receiver<bool>,
}

/// What opens the stream with the sink of the socket: the core of the listener, or a machine.
pub(crate) type Open =
    Box<dyn FnOnce(Arc<dyn FrameSink>) -> BoxFuture<'static, Result<(), RpcError>> + Send>;

/// Serves one stream until the phone closes it, the sink ends, or `ends` fires.
pub(crate) async fn serve<S>(ws: WebSocketStream<S>, open: Open, ends: Ends)
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let (mut write, mut read) = ws.split();
    let (out, mut frames) = mpsc::unbounded_channel::<Vec<u8>>();
    let rtt_us = Arc::new(AtomicU64::new(0));
    let sink = Arc::new(Frames {
        out,
        rtt_us: Arc::clone(&rtt_us),
    });
    if let Err(error) = open(sink).await {
        let _ = write
            .send(Message::Text(json!({ "error": error }).to_string().into()))
            .await;
        let _ = write.close().await;
        return;
    }
    let ended = crate::rpc::ended(ends.core, ends.device, ends.token, ends.removed, ends.stop);
    tokio::pin!(ended);
    let mut ping = tokio::time::interval(PING_EVERY);
    ping.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let started = Instant::now();
    loop {
        tokio::select! {
            () = &mut ended => break,
            frame = frames.recv() => {
                let Some(frame) = frame else { break };
                if write.feed(Message::Binary(frame.into())).await.is_err() {
                    break;
                }
                // The frames that wait go in one flush.
                let mut failed = false;
                while let Ok(next) = frames.try_recv() {
                    if write.feed(Message::Binary(next.into())).await.is_err() {
                        failed = true;
                        break;
                    }
                }
                if failed || write.flush().await.is_err() {
                    break;
                }
            }
            _ = ping.tick() => {
                let sent = u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX);
                if write.send(Message::Ping(sent.to_be_bytes().to_vec().into())).await.is_err() {
                    break;
                }
            }
            message = read.next() => match message {
                Some(Ok(Message::Pong(payload))) => {
                    if let Ok(bytes) = <[u8; 8]>::try_from(payload.as_ref()) {
                        let sent = u64::from_be_bytes(bytes);
                        let now = u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX);
                        rtt_us.store(now.saturating_sub(sent).max(1), Ordering::Relaxed);
                    }
                }
                Some(Ok(Message::Close(_)) | Err(_)) | None => break,
                Some(Ok(_)) => {}
            },
        }
    }
    let _ = write.close().await;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window() -> WindowId {
        WindowId::new(Ulid::from(7u128))
    }

    #[test]
    fn a_render_stream_is_for_the_window_of_the_phone() {
        let given = checked_params("render", json!({}), window()).expect("allowed");
        assert_eq!(given["window"], json!(window().to_string()));
        assert!(
            checked_params(
                "render",
                json!({ "window": window().to_string(), "x": 1 }),
                window()
            )
            .is_ok()
        );
        let other = WindowId::new(Ulid::from(8u128)).to_string();
        assert_eq!(
            checked_params("render", json!({ "window": other }), window())
                .expect_err("another window")
                .code,
            ErrorCode::PermissionDenied
        );
        assert_eq!(
            checked_params("bench", json!({}), window())
                .expect_err("not for a phone")
                .code,
            ErrorCode::NotFound
        );
        assert!(checked_params("peek", json!({ "pane": "x" }), window()).is_ok());
    }
}
