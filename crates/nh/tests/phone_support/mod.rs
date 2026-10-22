//! A core with the listener of the phone in a development run, and a few lines of HTTP and
//! WebSocket client for the tests of the phone. The origin of the listener is `http://127.0.0.1:<port>`.
//! The HTTP client writes one request over a TCP stream, and the WebSocket client is the one of
//! `tokio-tungstenite`.

#![allow(dead_code)]

use futures_util::{SinkExt, StreamExt};
use nh_web::PhoneService;
use serde_json::{Value, json};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    time::timeout,
};
use tokio_tungstenite::{
    WebSocketStream, client_async,
    tungstenite::{self, Message, client::IntoClientRequest, http::HeaderValue},
};

use crate::core_support::{Harness, TIMEOUT};

/// A core with the listener of a development run.
pub struct Rig {
    pub h: Harness,
    pub phone: PhoneService,
    pub port: u16,
}

impl Rig {
    pub fn start() -> Self {
        Self::with(Harness::start_layout())
    }

    pub fn with(h: Harness) -> Self {
        let (phone, port) = PhoneService::start_dev(h.handle.clone(), 0).expect("the listener");
        Self { h, phone, port }
    }

    pub fn host(&self) -> String {
        format!("127.0.0.1:{}", self.port)
    }

    pub fn origin(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// A new offer: the long code of its URL and its short code.
    pub async fn offer(&self) -> (String, String) {
        let started = self.h.call("phone.pair_begin", json!({})).await;
        let url = started["url"].as_str().expect("a URL").to_owned();
        assert!(
            url.starts_with(&format!("{}/pair?code=", self.origin())),
            "{url}"
        );
        let long = url.split_once("code=").expect("a code").1.to_owned();
        let short = started["code"].as_str().expect("a short code").to_owned();
        (long, short)
    }

    pub async fn pair(&self, code: &str) -> Answer {
        let body = json!({ "code": code, "name": "Test iPhone" }).to_string();
        http(
            self.port,
            "POST",
            "/api/pair",
            &[("Host", &self.host()), ("Origin", &self.origin())],
            Some(body.as_bytes()),
        )
        .await
    }

    /// Pairs a phone and returns its token and its device id.
    pub async fn paired(&self) -> (String, String) {
        let (long, _) = self.offer().await;
        let answer = self.pair(&long).await;
        assert_eq!(answer.status, 200, "{}", answer.body);
        let token = answer.cookie().expect("a cookie");
        let device = answer.json()["device"]
            .as_str()
            .expect("a device")
            .to_owned();
        (token, device)
    }

    /// The JSON-RPC socket of a phone.
    pub async fn rpc(&self, token: Option<&str>) -> Result<Ws, tungstenite::Error> {
        connect(self.port, &self.host(), &self.origin(), "/rpc", token).await
    }

    /// The socket of one stream of a phone.
    pub async fn stream(
        &self,
        token: Option<&str>,
        kind: &str,
        params: &Value,
    ) -> Result<Ws, tungstenite::Error> {
        let path = format!(
            "/stream?kind={kind}&params={}",
            percent(&params.to_string())
        );
        connect(self.port, &self.host(), &self.origin(), &path, token).await
    }

    /// The headers of a request of the paired phone with this token.
    pub fn headers(&self, token: &str) -> Vec<(String, String)> {
        vec![
            ("Host".to_owned(), self.host()),
            ("Origin".to_owned(), self.origin()),
            ("Cookie".to_owned(), format!("nh_device={token}")),
        ]
    }
}

/// The percent encoding of `encodeURIComponent`.
pub fn percent(text: &str) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&byte) {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// An answer of the listener.
pub struct Answer {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: String,
    pub bytes: Vec<u8>,
}

impl Answer {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    /// The token of the `Set-Cookie` of a pairing.
    pub fn cookie(&self) -> Option<String> {
        let value = self.header("set-cookie")?;
        let token = value.strip_prefix("nh_device=")?.split(';').next()?;
        Some(token.to_owned())
    }

    pub fn json(&self) -> Value {
        serde_json::from_str(&self.body).unwrap_or(Value::Null)
    }
}

/// One HTTP/1.1 request with `Connection: close`.
pub async fn http(
    port: u16,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: Option<&[u8]>,
) -> Answer {
    let mut stream = TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("the listener takes a connection");
    let mut request = format!("{method} {path} HTTP/1.1\r\nConnection: close\r\n");
    for (name, value) in headers {
        request.push_str(&format!("{name}: {value}\r\n"));
    }
    let mut bytes = request.into_bytes();
    if let Some(body) = body {
        bytes.extend_from_slice(
            format!(
                "Content-Type: application/json\r\nContent-Length: {}\r\n\r\n",
                body.len()
            )
            .as_bytes(),
        );
        bytes.extend_from_slice(body);
    } else {
        bytes.extend_from_slice(b"\r\n");
    }
    stream.write_all(&bytes).await.expect("the request goes");
    let mut raw = Vec::new();
    timeout(TIMEOUT, stream.read_to_end(&mut raw))
        .await
        .expect("an answer in time")
        .expect("the answer reads");
    let split = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .unwrap_or(raw.len());
    let head = String::from_utf8_lossy(&raw[..split]).into_owned();
    let body_bytes = raw.get(split + 4..).unwrap_or_default().to_vec();
    let mut lines = head.lines();
    let status = lines
        .next()
        .and_then(|line| line.split(' ').nth(1))
        .and_then(|code| code.parse().ok())
        .expect("a status line");
    let headers = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(n, v)| (n.trim().to_owned(), v.trim().to_owned()))
        .collect();
    Answer {
        status,
        headers,
        body: String::from_utf8_lossy(&body_bytes).into_owned(),
        bytes: body_bytes,
    }
}

pub type Ws = WebSocketStream<TcpStream>;

pub async fn connect(
    port: u16,
    host: &str,
    origin: &str,
    path: &str,
    token: Option<&str>,
) -> Result<Ws, tungstenite::Error> {
    let stream = TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("the listener takes a connection");
    let mut request = format!("ws://{host}{path}")
        .into_client_request()
        .expect("a request");
    let headers = request.headers_mut();
    headers.insert("Origin", HeaderValue::from_str(origin).expect("a value"));
    if let Some(token) = token {
        headers.insert(
            "Cookie",
            HeaderValue::from_str(&format!("nh_device={token}")).expect("a value"),
        );
    }
    client_async(request, stream).await.map(|(ws, _)| ws)
}

/// The status of a refused upgrade.
pub fn refused_with(result: Result<Ws, tungstenite::Error>) -> u16 {
    match result {
        Err(tungstenite::Error::Http(response)) => response.status().as_u16(),
        Err(other) => panic!("not an HTTP refusal: {other}"),
        Ok(_) => panic!("the upgrade passed"),
    }
}

/// A JSON-RPC call over the socket. Notifications on the way are skipped.
pub async fn call(ws: &mut Ws, id: u64, method: &str, params: Value) -> Value {
    let request = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
    ws.send(Message::Text(request.to_string().into()))
        .await
        .expect("the request goes");
    loop {
        let message = timeout(TIMEOUT, ws.next())
            .await
            .expect("an answer in time")
            .expect("the socket is open")
            .expect("a message");
        if let Message::Text(text) = message {
            let value: Value = serde_json::from_str(&text).expect("JSON");
            if value["id"] == json!(id) {
                return value;
            }
        }
    }
}

/// Waits for a notification of a subscription that `test` accepts, and returns its message.
pub async fn notification(ws: &mut Ws, what: &str, test: impl Fn(&Value) -> bool) -> Value {
    let found = timeout(TIMEOUT, async {
        loop {
            let message = ws
                .next()
                .await
                .expect("the socket is open")
                .expect("a message");
            if let Message::Text(text) = message {
                let value: Value = serde_json::from_str(&text).expect("JSON");
                if value["method"] == "projection.message" && test(&value["params"]["message"]) {
                    return value["params"]["message"].clone();
                }
            }
        }
    })
    .await;
    found.unwrap_or_else(|_| panic!("waited for {what}"))
}

pub fn error_code(reply: &Value) -> Option<&str> {
    reply["error"]["data"]["code"].as_str()
}
