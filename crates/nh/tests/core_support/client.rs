//! A client of `api.sock` for the tests: JSON-RPC lines over a Unix socket.

use std::{collections::VecDeque, path::Path, time::Duration};

use serde_json::{Value, json};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{
        UnixStream,
        unix::{OwnedReadHalf, OwnedWriteHalf},
    },
};

const TIMEOUT: Duration = Duration::from_secs(15);

pub struct Client {
    read: BufReader<OwnedReadHalf>,
    write: OwnedWriteHalf,
    next_id: i64,
    /// Lines that came while `call` waited for a reply. `recv` returns them first.
    backlog: VecDeque<Value>,
}

impl Client {
    pub async fn connect(socket: &Path) -> Self {
        let stream = UnixStream::connect(socket)
            .await
            .expect("the socket accepts");
        let (read, write) = stream.into_split();
        Self {
            read: BufReader::new(read),
            write,
            next_id: 0,
            backlog: VecDeque::new(),
        }
    }

    /// Writes bytes as they are.
    pub async fn send_raw(&mut self, bytes: &[u8]) {
        self.write.write_all(bytes).await.expect("a write");
    }

    /// Writes one JSON value as a line.
    pub async fn send(&mut self, value: &Value) {
        let mut line = serde_json::to_vec(value).expect("JSON");
        line.push(b'\n');
        self.send_raw(&line).await;
    }

    /// Sends a request and returns its id.
    pub async fn request(&mut self, method: &str, params: Value) -> i64 {
        self.next_id += 1;
        let id = self.next_id;
        self.send(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))
            .await;
        id
    }

    /// Reads the next line, or `None` when the server closes the connection or nothing comes within
    /// `within`.
    pub async fn try_recv(&mut self, within: Duration) -> Option<Value> {
        if let Some(line) = self.backlog.pop_front() {
            return Some(line);
        }
        self.read_line(within).await
    }

    async fn read_line(&mut self, within: Duration) -> Option<Value> {
        let mut line = String::new();
        match tokio::time::timeout(within, self.read.read_line(&mut line)).await {
            Ok(Ok(n)) if n > 0 => Some(serde_json::from_str(&line).expect("a JSON line")),
            _ => None,
        }
    }

    /// Reads the next line. Fails the test when nothing comes.
    pub async fn recv(&mut self) -> Value {
        self.try_recv(TIMEOUT).await.expect("a line from the core")
    }

    /// Sends a request and reads the reply with the same id. Any other line stays for `recv`.
    pub async fn call(&mut self, method: &str, params: Value) -> Value {
        let id = self.request(method, params).await;
        loop {
            let line = self.read_line(TIMEOUT).await.expect("a line from the core");
            if line["id"] == json!(id) {
                return line;
            }
            self.backlog.push_back(line);
        }
    }

    /// The `result` of a call that must work.
    pub async fn ok(&mut self, method: &str, params: Value) -> Value {
        let reply = self.call(method, params).await;
        assert!(reply.get("error").is_none(), "{method}: {reply}");
        reply["result"].clone()
    }
}

/// The pair `(integer, string code)` of an error reply.
pub fn error_of(reply: &Value) -> (i64, String) {
    (
        reply["error"]["code"].as_i64().expect("an integer code"),
        reply["error"]["data"]["code"]
            .as_str()
            .expect("a string code")
            .to_owned(),
    )
}
