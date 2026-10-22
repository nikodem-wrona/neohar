//! The push to a phone against a real core and a fake push service on `127.0.0.1`: the
//! subscription, the encrypted payload, the VAPID JWT, the end of a subscription on a
//! 410, and the three tries on a 503. The turn end of a fake agent with no window in focus reaches
//! the service. No test sends to a real push service.

mod core_support;

use std::{
    collections::VecDeque,
    sync::{Arc, Mutex, PoisonError},
    time::{Duration, Instant},
};

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use core_support::{Harness, TIMEOUT};
use futures_util::{SinkExt, StreamExt};
use nh_core::ErrorCode;
use nh_proto::api::projections::Projection;
use nh_web::{
    PhoneService,
    push::{NO_CONTACT, encrypt},
};
use p256::{
    SecretKey,
    ecdsa::{Signature, VerifyingKey, signature::Verifier},
    elliptic_curve::Generate,
};
use serde_json::{Value, json};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    time::timeout,
};
use tokio_tungstenite::{
    WebSocketStream, client_async,
    tungstenite::{Message, client::IntoClientRequest, http::HeaderValue},
};

/// A push that the fake service took.
#[derive(Clone, Debug)]
struct Push {
    path: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Push {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

/// A push service on a free port of `127.0.0.1`. It answers each push with the next status of its
/// script, else 201.
struct FakePushService {
    origin: String,
    pushes: Arc<Mutex<Vec<Push>>>,
    script: Arc<Mutex<VecDeque<u16>>>,
}

impl FakePushService {
    async fn start() -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("a free port");
        let origin = format!(
            "http://127.0.0.1:{}",
            listener.local_addr().expect("an address").port()
        );
        let pushes = Arc::new(Mutex::new(Vec::new()));
        let script = Arc::new(Mutex::new(VecDeque::new()));
        let (taken, answers) = (Arc::clone(&pushes), Arc::clone(&script));
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let (taken, answers) = (Arc::clone(&taken), Arc::clone(&answers));
                tokio::spawn(serve(stream, taken, answers));
            }
        });
        Self {
            origin,
            pushes,
            script,
        }
    }

    fn answer_with(&self, statuses: &[u16]) {
        self.script
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .extend(statuses);
    }

    fn pushes(&self) -> Vec<Push> {
        self.pushes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

async fn serve(
    mut stream: TcpStream,
    taken: Arc<Mutex<Vec<Push>>>,
    answers: Arc<Mutex<VecDeque<u16>>>,
) {
    let mut raw = Vec::new();
    let mut chunk = [0u8; 4096];
    let (head, length) = loop {
        let Ok(n) = stream.read(&mut chunk).await else {
            return;
        };
        if n == 0 {
            return;
        }
        raw.extend_from_slice(&chunk[..n]);
        if let Some(end) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&raw[..end]).into_owned();
            let length = head
                .lines()
                .filter_map(|line| line.split_once(':'))
                .find(|(n, _)| n.trim().eq_ignore_ascii_case("content-length"))
                .and_then(|(_, v)| v.trim().parse::<usize>().ok())
                .unwrap_or(0);
            raw.drain(..end + 4);
            break (head, length);
        }
    };
    while raw.len() < length {
        let Ok(n) = stream.read(&mut chunk).await else {
            return;
        };
        if n == 0 {
            return;
        }
        raw.extend_from_slice(&chunk[..n]);
    }
    let mut lines = head.lines();
    let path = lines
        .next()
        .and_then(|line| line.split(' ').nth(1))
        .unwrap_or("")
        .to_owned();
    let headers = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(n, v)| (n.trim().to_owned(), v.trim().to_owned()))
        .collect();
    taken
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(Push {
            path,
            headers,
            body: raw,
        });
    let status = answers
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .pop_front()
        .unwrap_or(201);
    let answer = format!(
        "HTTP/1.1 {status} Fake\r\nContent-Length: 15\r\nConnection: close\r\n\r\n{{\"fake\": \"yes\"}}"
    );
    let _ = stream.write_all(answer.as_bytes()).await;
    let _ = stream.shutdown().await;
}

/// The keys of a subscription that the browser of a phone would make.
struct Browser {
    secret: SecretKey,
    auth: [u8; 16],
}

impl Browser {
    fn new() -> Self {
        Self {
            secret: SecretKey::generate(),
            auth: *b"0123456789abcdef",
        }
    }

    fn subscription(&self, endpoint: &str) -> Value {
        json!({
            "endpoint": endpoint,
            "p256dh": URL_SAFE_NO_PAD.encode(encrypt::sec1(&self.secret.public_key())),
            "auth": URL_SAFE_NO_PAD.encode(self.auth),
        })
    }

    fn open(&self, push: &Push) -> Value {
        let plain = encrypt::decrypt(&self.secret, &self.auth, &push.body).expect("decrypts");
        serde_json::from_slice(&plain).expect("the payload is JSON")
    }
}

/// A core with the listener of a development run, a fake push service, and a paired phone on the
/// socket of the phone app.
struct Rig {
    h: Harness,
    _phone: PhoneService,
    service: FakePushService,
    device: String,
    ws: WebSocketStream<TcpStream>,
    next_id: u64,
}

impl Rig {
    async fn start() -> Self {
        Self::with_core(Harness::start_layout()).await
    }

    async fn with_core(h: Harness) -> Self {
        let (phone, port) = PhoneService::start_dev(h.handle.clone(), 0).expect("the listener");
        let service = FakePushService::start().await;
        phone.allow_test_push(&service.origin, Duration::from_millis(20));
        let (token, device) = pair(&h, port).await;
        let ws = connect(port, &token).await;
        Self {
            h,
            _phone: phone,
            service,
            device,
            ws,
            next_id: 1,
        }
    }

    /// A call of the phone: the reply with its `result` or its `error`.
    async fn phone_call(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        let request = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        self.ws
            .send(Message::Text(request.to_string().into()))
            .await
            .expect("the request goes");
        loop {
            let message = timeout(TIMEOUT, self.ws.next())
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

    async fn subscribe(&mut self, browser: &Browser, path: &str) {
        let endpoint = format!("{}{path}", self.service.origin);
        let reply = self
            .phone_call("phone.push_subscribe", browser.subscription(&endpoint))
            .await;
        assert!(reply["error"].is_null(), "{reply}");
    }

    /// Whether the row of the phone in `phones` shows push on.
    async fn push_on(&self) -> bool {
        let phones = self.h.handle.projections().snapshot(Projection::Phones);
        phones
            .items
            .get(&self.device)
            .and_then(|row| row["push"].as_bool())
            .unwrap_or(false)
    }

    async fn test_push(&self) -> Result<Value, nh_core::RpcError> {
        self.h
            .handle
            .rpc("phone.push_test", json!({ "device": self.device }))
            .await
    }
}

/// Pairs a phone over HTTP. Returns its token and its device id.
async fn pair(h: &Harness, port: u16) -> (String, String) {
    let started = h.call("phone.pair_begin", json!({})).await;
    let url = started["url"].as_str().expect("a URL");
    let code = url.split_once("code=").expect("a code").1;
    let body = json!({ "code": code, "name": "Test iPhone" }).to_string();
    let mut stream = TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("the listener takes a connection");
    let request = format!(
        "POST /api/pair HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nOrigin: http://127.0.0.1:{port}\r\n\
         Connection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    stream
        .write_all(request.as_bytes())
        .await
        .expect("the request goes");
    let mut raw = Vec::new();
    timeout(TIMEOUT, stream.read_to_end(&mut raw))
        .await
        .expect("an answer in time")
        .expect("the answer reads");
    let text = String::from_utf8_lossy(&raw).into_owned();
    let token = text
        .lines()
        .find_map(|line| line.strip_prefix("set-cookie: nh_device="))
        .or_else(|| {
            text.lines()
                .find_map(|line| line.strip_prefix("Set-Cookie: nh_device="))
        })
        .and_then(|rest| rest.split(';').next())
        .expect("a cookie")
        .to_owned();
    let (_, body) = text.split_once("\r\n\r\n").expect("a body");
    let device = serde_json::from_str::<Value>(body).expect("JSON")["device"]
        .as_str()
        .expect("a device")
        .to_owned();
    (token, device)
}

async fn connect(port: u16, token: &str) -> WebSocketStream<TcpStream> {
    let stream = TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("the listener takes a connection");
    let mut request = format!("ws://127.0.0.1:{port}/rpc")
        .into_client_request()
        .expect("a request");
    let headers = request.headers_mut();
    let origin = format!("http://127.0.0.1:{port}");
    headers.insert("Origin", HeaderValue::from_str(&origin).expect("a value"));
    headers.insert(
        "Cookie",
        HeaderValue::from_str(&format!("nh_device={token}")).expect("a value"),
    );
    client_async(request, stream).await.expect("the upgrade").0
}

async fn until(what: &str, mut pred: impl AsyncFnMut() -> bool) {
    let deadline = Instant::now() + TIMEOUT;
    while !pred().await {
        assert!(Instant::now() < deadline, "waited for {what}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_test_push_reaches_the_push_service_encrypted_and_signed() {
    let mut rig = Rig::start().await;
    let browser = Browser::new();
    assert!(!rig.push_on().await);
    rig.subscribe(&browser, "/push/one").await;
    until("push on in phones", async || rig.push_on().await).await;

    rig.test_push().await.expect("the test push goes");
    let pushes = rig.service.pushes();
    assert_eq!(pushes.len(), 1);
    let push = &pushes[0];
    assert_eq!(push.path, "/push/one");
    assert_eq!(push.header("TTL"), Some("3600"));
    assert_eq!(push.header("Urgency"), Some("normal"));
    assert_eq!(push.header("Content-Encoding"), Some("aes128gcm"));
    assert_eq!(push.header("Topic"), None, "a test push has no topic");
    assert_eq!(
        browser.open(push),
        json!({
            "title": "neohar",
            "body": "Test notification",
            "tag": "test",
            "url": "#/phone",
        })
    );

    // The JWT: the origin of the endpoint, the contact, 12 h, and the key of `phone_status`.
    let vapid_key = rig
        .h
        .handle
        .phone_status()
        .vapid_key
        .expect("phone_status has the VAPID key");
    let authorization = push.header("Authorization").expect("Authorization");
    let (jwt, key) = authorization
        .strip_prefix("vapid t=")
        .and_then(|rest| rest.split_once(", k="))
        .expect("vapid t=…, k=…");
    assert_eq!(key, vapid_key);
    let parts: Vec<&str> = jwt.split('.').collect();
    let claims: Value =
        serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts[1]).expect("b64")).expect("JSON");
    assert_eq!(claims["aud"], json!(rig.service.origin));
    assert_eq!(claims["sub"], json!(NO_CONTACT));
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("a clock")
        .as_secs();
    let exp = claims["exp"].as_u64().expect("exp");
    assert!(exp > now && exp <= now + 12 * 3600 + 5, "{exp} {now}");
    let verifying =
        VerifyingKey::from_sec1_bytes(&URL_SAFE_NO_PAD.decode(key).expect("b64")).expect("a key");
    let signature =
        Signature::from_slice(&URL_SAFE_NO_PAD.decode(parts[2]).expect("b64")).expect("ES256");
    verifying
        .verify(format!("{}.{}", parts[0], parts[1]).as_bytes(), &signature)
        .expect("the JWT verifies with the VAPID key");

    // The phone can test itself, and it can end its subscription.
    let device = rig.device.clone();
    let reply = rig
        .phone_call("phone.push_test", json!({ "device": device }))
        .await;
    assert!(reply["error"].is_null(), "{reply}");
    assert_eq!(rig.service.pushes().len(), 2);
    let reply = rig.phone_call("phone.push_unsubscribe", json!({})).await;
    assert!(reply["error"].is_null(), "{reply}");
    until("push off in phones", async || !rig.push_on().await).await;
    let error = rig.test_push().await.expect_err("no subscription");
    assert_eq!(error.code, ErrorCode::NotFound);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_410_ends_the_subscription() {
    let mut rig = Rig::start().await;
    let browser = Browser::new();
    rig.subscribe(&browser, "/push/gone").await;
    until("push on in phones", async || rig.push_on().await).await;
    rig.service.answer_with(&[410]);
    let error = rig
        .test_push()
        .await
        .expect_err("the service no longer knows it");
    assert_eq!(error.code, ErrorCode::NotFound, "{error:?}");
    assert_eq!(rig.service.pushes().len(), 1, "a 410 gets no second try");
    until("push off in phones", async || !rig.push_on().await).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_503_gets_three_tries() {
    let mut rig = Rig::start().await;
    let browser = Browser::new();
    rig.subscribe(&browser, "/push/busy").await;

    rig.service.answer_with(&[503, 503, 201]);
    rig.test_push().await.expect("the third try goes");
    assert_eq!(rig.service.pushes().len(), 3);

    rig.service.answer_with(&[503, 503, 503, 201]);
    let error = rig.test_push().await.expect_err("three tries fail");
    assert_eq!(error.code, ErrorCode::Busy, "{error:?}");
    assert_eq!(rig.service.pushes().len(), 6, "three more tries, not four");
    assert!(rig.push_on().await, "a busy service keeps the subscription");

    // Each try is the same encrypted body.
    let pushes = rig.service.pushes();
    assert_eq!(pushes[3].body, pushes[5].body);
    assert_eq!(browser.open(&pushes[5])["body"], "Test notification");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_subscription_needs_an_endpoint_of_the_list_and_good_keys() {
    let mut rig = Rig::start().await;
    let browser = Browser::new();
    for endpoint in [
        "https://example.com/push",
        "http://127.0.0.1:1/push",
        "https://17.253.144.10/push",
        "https://web.push.apple.com:8443/push",
    ] {
        let reply = rig
            .phone_call("phone.push_subscribe", browser.subscription(endpoint))
            .await;
        assert_eq!(
            reply["error"]["data"]["code"], "invalid_params",
            "{endpoint}: {reply}"
        );
    }
    let mut bad = browser.subscription(&format!("{}/push", rig.service.origin));
    bad["p256dh"] = json!("AAAA");
    let reply = rig.phone_call("phone.push_subscribe", bad).await;
    assert_eq!(reply["error"]["data"]["code"], "invalid_params", "{reply}");
    assert!(!rig.push_on().await);

    // Only a phone subscribes, for itself.
    let error = rig
        .h
        .handle
        .rpc(
            "phone.push_subscribe",
            browser.subscription(&format!("{}/push", rig.service.origin)),
        )
        .await
        .expect_err("the UI and the CLI do not subscribe");
    assert_eq!(error.code, ErrorCode::PermissionDenied, "{error:?}");
    // A phone has full access: it can test another phone, and a device that is
    // not paired is not found.
    let other = "01M4F1W7EM2GN3EV6NHXZJEGGA";
    let reply = rig
        .phone_call("phone.push_test", json!({ "device": other }))
        .await;
    assert_eq!(reply["error"]["data"]["code"], "not_found", "{reply}");
    assert!(rig.service.pushes().is_empty());
}

/// A hook of Claude Code with the token of a pane, as the shim sends it.
async fn signal(h: &Harness, token: &str, event: &str, payload: Value) {
    let socket = h.api_socket();
    let line = json!({
        "jsonrpc": "2.0", "id": 1, "method": "adapter.signal",
        "params": {"token": token, "harness": "claude", "event": event, "payload": payload},
    })
    .to_string()
        + "\n";
    tokio::task::spawn_blocking(move || {
        use std::io::{BufRead, BufReader, Write};
        let mut stream = std::os::unix::net::UnixStream::connect(socket).expect("connect");
        stream.write_all(line.as_bytes()).expect("write");
        let mut reply = String::new();
        BufReader::new(&mut stream)
            .read_line(&mut reply)
            .expect("a reply");
        assert!(reply.contains("\"result\""), "the signal failed: {reply}");
    })
    .await
    .expect("the task");
}

/// A turn of the run in the pane: a prompt, then its end.
async fn turn(h: &Harness, token: &str, prompt: &str) {
    let submit = json!({"session_id": "s1", "prompt": prompt});
    signal(h, token, "UserPromptSubmit", submit).await;
    signal(h, token, "Stop", json!({"session_id": "s1"})).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_turn_end_with_no_window_in_focus_reaches_the_push_service() {
    let dir = std::env::temp_dir().join(format!("nh-phone-turn-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a directory");
    let token_file = dir.join("token");
    // The pane writes its token for the hooks, then waits.
    let script = format!(
        "printf %s \"$NEOHAR_PANE_TOKEN\" > '{}'; exec cat",
        token_file.display()
    );
    let h = Harness::start_tuned(nh_core::Tuning {
        turn_settle: Duration::from_millis(200),
        notify_window: Duration::from_millis(100),
        ..nh_core::Tuning::default()
    })
    .with_command_pane(&["/bin/sh", "-c", &script])
    .await;
    let mut rig = Rig::with_core(h).await;
    let browser = Browser::new();
    rig.subscribe(&browser, "/push/turn").await;
    until("push on in phones", async || rig.push_on().await).await;
    let mut token = String::new();
    until("the token of the pane", async || {
        token = std::fs::read_to_string(&token_file).unwrap_or_default();
        !token.is_empty()
    })
    .await;
    // A run of Claude Code starts in the pane, works on a prompt, and ends its turn. No window of
    // a desktop client exists, so none has the focus.
    let session = json!({"session_id": "s1", "cwd": "/work", "source": "startup"});
    signal(&rig.h, &token, "SessionStart", session).await;
    turn(&rig.h, &token, "fix the login bug").await;
    until("a push", async || !rig.service.pushes().is_empty()).await;
    let pushes = rig.service.pushes();
    let push = &pushes[0];
    assert_eq!(push.path, "/push/turn");
    assert_eq!(push.header("Urgency"), Some("normal"));
    let run = push
        .header("Topic")
        .expect("the run is the topic")
        .to_owned();
    let message = browser.open(push);
    assert_eq!(message["body"], json!("Finished its turn"));
    assert_eq!(message["url"], json!(format!("#/run/{run}")));
    assert_eq!(message["badge"], json!(1));
    let title = message["title"].as_str().expect("a title");
    assert!(title.starts_with("Claude Code"), "{title}");
    assert!(
        !message.to_string().contains("login bug"),
        "no text of the prompt: {message}"
    );
    // A visible phone gets no push for the next turn.
    let reply = rig
        .phone_call("phone.visible", json!({"visible": true}))
        .await;
    assert!(reply["error"].is_null(), "{reply}");
    turn(&rig.h, &token, "again").await;
    tokio::time::sleep(Duration::from_millis(800)).await;
    assert_eq!(rig.service.pushes().len(), 1, "the phone is visible");
    // The app is hidden again: the next turn pushes.
    let reply = rig
        .phone_call("phone.visible", json!({"visible": false}))
        .await;
    assert!(reply["error"].is_null(), "{reply}");
    turn(&rig.h, &token, "once more").await;
    until("a second push", async || rig.service.pushes().len() == 2).await;
    // The end of a connection forgets that its app was visible.
    let reply = rig
        .phone_call("phone.visible", json!({"visible": true}))
        .await;
    assert!(reply["error"].is_null(), "{reply}");
    rig.ws.close(None).await.expect("the socket closes");
    tokio::time::sleep(Duration::from_millis(300)).await;
    turn(&rig.h, &token, "and again").await;
    until("a third push", async || rig.service.pushes().len() == 3).await;
    let _ = std::fs::remove_dir_all(&dir);
}
