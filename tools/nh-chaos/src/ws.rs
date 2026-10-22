//! A client of the dev WebSocket of the core: JSON calls on the control connection.

use std::time::Duration;

use anyhow::{Context, Result, bail};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::{net::TcpStream, time::timeout};
use tokio_tungstenite::{WebSocketStream, client_async, tungstenite::Message};

/// How long one call may take.
const CALL_TIMEOUT: Duration = Duration::from_secs(30);

/// Where a host listens. The host writes it to a file when it is ready.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostInfo {
    pub pid: u32,
    pub port: u16,
    pub token: String,
}

/// One control connection.
pub struct Control {
    ws: WebSocketStream<TcpStream>,
    next_id: u64,
}

impl Control {
    pub async fn connect(info: &HostInfo) -> Result<Self> {
        let stream = TcpStream::connect(("127.0.0.1", info.port))
            .await
            .context("cannot connect to the dev WebSocket")?;
        stream.set_nodelay(true)?;
        let url = format!("ws://127.0.0.1:{}/control?token={}", info.port, info.token);
        let (ws, _) = client_async(url, stream)
            .await
            .context("the dev WebSocket refuses the handshake")?;
        Ok(Self { ws, next_id: 1 })
    }

    /// Calls a method and returns its result. An error answer of the core is an `Err`.
    pub async fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_id;
        self.next_id += 1;
        let request = json!({ "id": id, "call": { "method": method, "params": params } });
        self.ws
            .send(Message::Text(request.to_string().into()))
            .await
            .with_context(|| format!("cannot send {method}"))?;
        timeout(CALL_TIMEOUT, self.answer(id, method))
            .await
            .with_context(|| format!("{method} did not answer in {CALL_TIMEOUT:?}"))?
    }

    async fn answer(&mut self, id: u64, method: &str) -> Result<Value> {
        loop {
            let Some(message) = self.ws.next().await else {
                bail!("the connection ended during {method}");
            };
            let Message::Text(text) = message.context("the connection broke")? else {
                continue;
            };
            let reply: Value = serde_json::from_str(&text).context("a reply that is no JSON")?;
            if reply.get("id").and_then(Value::as_u64) != Some(id) {
                continue;
            }
            if let Some(error) = reply.get("error") {
                bail!("{method} failed: {error}");
            }
            return Ok(reply.get("result").cloned().unwrap_or(Value::Null));
        }
    }
}
