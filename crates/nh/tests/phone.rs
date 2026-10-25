//! The listener of the phone against a real core: the
//! pairing, the checks of `Host`, `Origin` and the cookie, the scope `phone`, the removal of a
//! device, the blobs, and no secret in the log.
//!
//! Each test starts the listener of a development run on a free port, so its origin is
//! `http://127.0.0.1:<port>` (`phone_support`).

mod core_support;
mod phone_support;

use std::{
    io::Write as _,
    sync::{Arc, Mutex, PoisonError},
    time::Duration,
};

use core_support::{Harness, Messages, TIMEOUT};
use futures_util::StreamExt;
use nh_core::{ErrorCode, Tuning};
use nh_web::{PhoneService, TailscaleCli};
use phone_support::{Rig, call, connect, error_code, http, refused_with};
use serde_json::{Value, json};
use tokio::{net::TcpStream, time::timeout};
use tokio_tungstenite::tungstenite::Message;

/// The state of `phone_pairing` in a message of the projection.
fn pairing_state(message: &Value) -> Option<String> {
    let value = if message["type"] == "snapshot" {
        message["data"]["items"]["_"].clone()
    } else {
        message["ops"]
            .as_array()?
            .iter()
            .rev()
            .find(|op| op["op"] == "set" && op["key"] == "_")?["value"]
            .clone()
    };
    value["state"].as_str().map(str::to_owned)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_phone_pairs_with_the_long_code_or_the_short_code_once() {
    let rig = Rig::start();
    let mut pairing = Messages::subscribe(&rig.h.handle, "phone_pairing");
    let (long, short) = rig.offer().await;
    pairing
        .wait_for("an offer", |m| {
            pairing_state(m).as_deref() == Some("waiting")
        })
        .await;
    let answer = rig.pair(&long).await;
    assert_eq!(answer.status, 200, "{}", answer.body);
    let cookie = answer.header("set-cookie").expect("a cookie");
    assert!(
        cookie.contains("HttpOnly") && cookie.contains("SameSite=Strict"),
        "{cookie}"
    );
    assert_eq!(answer.json()["name"], "Test iPhone");
    pairing
        .wait_for("the pairing", |m| {
            pairing_state(m).as_deref() == Some("paired")
        })
        .await;
    // The code of an offer serves one phone.
    assert_eq!(rig.pair(&long).await.status, 403);
    assert_eq!(rig.pair(&short).await.status, 403);

    // The short code of a new offer, typed in small letters.
    let (_, short) = rig.offer().await;
    let answer = rig.pair(&short.to_ascii_lowercase()).await;
    assert_eq!(answer.status, 200, "{}", answer.body);
    let phones = rig
        .h
        .call("projection.versions", json!({ "names": ["phones"] }))
        .await;
    assert!(phones["versions"]["phones"].is_u64(), "{phones}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_expired_offer_and_the_sixth_wrong_code_refuse_the_pairing() {
    let rig = Rig::with(Harness::start_tuned(Tuning {
        start_holders: false,
        phone_offer_ttl: Duration::ZERO,
        ..Tuning::default()
    }));
    let (long, _) = rig.offer().await;
    assert_eq!(
        rig.pair(&long).await.status,
        403,
        "the time of the offer ran out"
    );

    let rig = Rig::start();
    let (long, _) = rig.offer().await;
    for _ in 0..5 {
        assert_eq!(rig.pair("ZZZZ-ZZZZ").await.status, 403);
    }
    assert_eq!(
        rig.pair(&long).await.status,
        403,
        "five wrong codes ended the offer for this client"
    );
    // No offer: every code fails.
    rig.h.call("phone.pair_cancel", json!({})).await;
    assert_eq!(rig.pair(&long).await.status, 403);
}

/// A pairing with the tailnet user that `tailscale serve` names.
async fn pair_as(rig: &Rig, code: &str, login: &str) -> u16 {
    let body = json!({ "code": code, "name": "Test iPhone" }).to_string();
    http(
        rig.port,
        "POST",
        "/api/pair",
        &[
            ("Host", &rig.host()),
            ("Origin", &rig.origin()),
            ("Tailscale-User-Login", login),
        ],
        Some(body.as_bytes()),
    )
    .await
    .status
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn another_device_of_the_tailnet_cannot_end_the_offer_alone() {
    let rig = Rig::start();
    let (long, _) = rig.offer().await;
    for _ in 0..5 {
        assert_eq!(pair_as(&rig, "ZZZZ-ZZZZ", "mallory@example.com").await, 403);
    }
    assert_eq!(pair_as(&rig, &long, "mallory@example.com").await, 403);
    // The phone of the person, with its own login, still pairs.
    assert_eq!(pair_as(&rig, &long, "ada@example.com").await, 200);

    // Fifty wrong codes of all clients end the offer.
    let (long, _) = rig.offer().await;
    for n in 0..nh_core::phones::WRONG_CODES_TOTAL {
        assert_eq!(
            pair_as(&rig, "ZZZZ-ZZZZ", &format!("c{}", n / 5)).await,
            403
        );
    }
    assert_eq!(pair_as(&rig, &long, "ada@example.com").await, 403);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_listener_checks_the_host_the_origin_and_the_cookie() {
    let rig = Rig::start();
    let (token, _) = rig.paired().await;

    // Host.
    let page = http(rig.port, "GET", "/", &[("Host", &rig.host())], None).await;
    assert_eq!(page.status, 200);
    assert!(
        page.header("content-security-policy")
            .is_some_and(|csp| csp.contains("frame-ancestors 'none'"))
    );
    assert_eq!(page.header("x-content-type-options"), Some("nosniff"));
    let rebound = http(rig.port, "GET", "/", &[("Host", "evil.example")], None).await;
    assert_eq!(rebound.status, 403);

    // Cookie.
    let me = http(rig.port, "GET", "/api/me", &[("Host", &rig.host())], None).await;
    assert_eq!(me.status, 401);
    let cookie = format!("nh_device={token}");
    let me = http(
        rig.port,
        "GET",
        "/api/me",
        &[("Host", &rig.host()), ("Cookie", &cookie)],
        None,
    )
    .await;
    assert_eq!(me.status, 200);
    assert_eq!(me.json()["name"], "Test iPhone");

    // The WebSocket.
    assert_eq!(refused_with(rig.rpc(None).await), 401);
    let other_origin = connect(
        rig.port,
        &rig.host(),
        "https://evil.example",
        "/rpc",
        Some(&token),
    )
    .await;
    assert_eq!(refused_with(other_origin), 403);
    let other_host = connect(
        rig.port,
        "evil.example",
        &rig.origin(),
        "/rpc",
        Some(&token),
    )
    .await;
    assert_eq!(refused_with(other_host), 403);
    assert!(rig.rpc(Some(&token)).await.is_ok());

    // A pairing from another origin.
    let body = json!({ "code": "x", "name": "n" }).to_string();
    let foreign = http(
        rig.port,
        "POST",
        "/api/pair",
        &[("Host", &rig.host()), ("Origin", "https://evil.example")],
        Some(body.as_bytes()),
    )
    .await;
    assert_eq!(foreign.status, 403);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_phone_has_full_access_less_the_pairing_and_the_native_windows() {
    let rig = Rig::start();
    let (token, device) = rig.paired().await;
    let mut ws = rig.rpc(Some(&token)).await.expect("the socket");

    // A phone has the methods of the UI: the scope passes, and the params decide.
    for (id, method) in [
        (1, "interaction.answer"),
        (2, "agent.reply"),
        (3, "timeline.item"),
        (4, "pane.input"),
        (10, "config.set"),
        (11, "changes.follow"),
    ] {
        let answer = call(&mut ws, id, method, json!({})).await;
        assert_eq!(
            error_code(&answer),
            Some("invalid_params"),
            "{method}: {answer}"
        );
    }
    for (id, method, code) in [
        (12, "window.open", "unsupported"),
        (13, "fs.open", "unsupported"),
        (15, "phone.pair_begin", "permission_denied"),
        (16, "phone.publish", "permission_denied"),
    ] {
        let answer = call(&mut ws, id, method, json!({})).await;
        assert_eq!(error_code(&answer), Some(code), "{method}: {answer}");
    }

    let agents = call(
        &mut ws,
        5,
        "projection.subscribe",
        json!({ "name": "agents" }),
    )
    .await;
    assert!(agents["result"]["subscription"].is_string(), "{agents}");
    let pairing = call(
        &mut ws,
        6,
        "projection.subscribe",
        json!({ "name": "phone_pairing" }),
    )
    .await;
    assert_eq!(error_code(&pairing), Some("permission_denied"), "{pairing}");
    // A connection ends only its own subscriptions: not the one of a window of the desktop.
    let desktop = rig
        .h
        .handle
        .subscribe_tracked("agents", None, Box::new(Quiet))
        .expect("a subscription");
    let foreign = call(
        &mut ws,
        7,
        "projection.unsubscribe",
        json!({ "subscription": desktop }),
    )
    .await;
    assert_eq!(error_code(&foreign), Some("not_found"), "{foreign}");
    assert!(
        rig.h.handle.unsubscribe_tracked(&desktop),
        "the subscription of the desktop stays"
    );

    // A phone renames itself, and it can remove another phone, as the UI can.
    let renamed = call(
        &mut ws,
        8,
        "phone.rename",
        json!({ "device": device, "name": "Pocket" }),
    )
    .await;
    assert!(renamed["result"].is_object(), "{renamed}");
    let other = rig.paired().await.1;
    let removed = call(&mut ws, 9, "phone.remove", json!({ "device": other })).await;
    assert!(removed["result"].is_object(), "{removed}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_removed_phone_loses_its_socket_and_its_cookie() {
    let rig = Rig::start();
    let (token, device) = rig.paired().await;
    let mut ws = rig.rpc(Some(&token)).await.expect("the socket");
    let first = call(
        &mut ws,
        1,
        "projection.versions",
        json!({ "names": ["agents"] }),
    )
    .await;
    assert!(first["result"].is_object(), "{first}");

    rig.h
        .call("phone.remove", json!({ "device": device }))
        .await;
    let closed = timeout(Duration::from_secs(1), async {
        loop {
            match ws.next().await {
                None | Some(Err(_)) | Some(Ok(Message::Close(_))) => return,
                Some(Ok(_)) => {}
            }
        }
    })
    .await;
    assert!(closed.is_ok(), "the socket is still open after 1 s");
    let cookie = format!("nh_device={token}");
    let me = http(
        rig.port,
        "GET",
        "/api/me",
        &[("Host", &rig.host()), ("Cookie", &cookie)],
        None,
    )
    .await;
    assert_eq!(me.status, 401);
    assert_eq!(refused_with(rig.rpc(Some(&token)).await), 401);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_phone_unpairs_itself_and_gets_images_but_no_other_blob() {
    let rig = Rig::start();
    let (token, _) = rig.paired().await;
    let cookie = format!("nh_device={token}");
    let (png, _) = rig
        .h
        .handle
        .put_upload("shot.png", b"\x89PNG\r\n\x1a\nfake".to_vec())
        .await
        .expect("a blob");
    let (text, _) = rig
        .h
        .handle
        .put_upload("notes.txt", b"hello".to_vec())
        .await
        .expect("a blob");
    let image = http(
        rig.port,
        "GET",
        &format!("/blob/{png}"),
        &[("Host", &rig.host()), ("Cookie", &cookie)],
        None,
    )
    .await;
    assert_eq!(image.status, 200);
    assert_eq!(image.header("content-type"), Some("image/png"));
    let other = http(
        rig.port,
        "GET",
        &format!("/blob/{text}"),
        &[("Host", &rig.host()), ("Cookie", &cookie)],
        None,
    )
    .await;
    assert_eq!(other.status, 404);
    let anonymous = http(
        rig.port,
        "GET",
        &format!("/blob/{png}"),
        &[("Host", &rig.host())],
        None,
    )
    .await;
    assert_eq!(anonymous.status, 401);

    let gone = http(
        rig.port,
        "POST",
        "/api/unpair",
        &[
            ("Host", &rig.host()),
            ("Origin", &rig.origin()),
            ("Cookie", &cookie),
        ],
        None,
    )
    .await;
    assert_eq!(gone.status, 204);
    assert!(
        gone.header("set-cookie")
            .is_some_and(|c| c.contains("Max-Age=0"))
    );
    let me = http(
        rig.port,
        "GET",
        "/api/me",
        &[("Host", &rig.host()), ("Cookie", &cookie)],
        None,
    )
    .await;
    assert_eq!(me.status, 401);
}

/// The status of the listener in a message of `phone_status`.
fn status_of(message: &Value) -> Option<Value> {
    if message["type"] == "snapshot" {
        return Some(message["data"]["items"]["_"].clone()).filter(|v| !v.is_null());
    }
    message["ops"]
        .as_array()?
        .iter()
        .rev()
        .find(|op| op["op"] == "set" && op["key"] == "_")
        .map(|op| op["value"].clone())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_listener_follows_the_settings_and_has_no_origin_without_phone_url_or_tailscale() {
    let h = Harness::start_layout();
    let mut status = Messages::subscribe(&h.handle, "phone_status");
    status
        .wait_for("the listener off", |m| {
            status_of(m).is_some_and(|s| s["listening"] == false)
        })
        .await;
    // No Tailscale: the test must never reach the one of the machine.
    let _phone = PhoneService::start_with(
        h.handle.clone(),
        TailscaleCli::At(h.dir.join("no-tailscale")),
    );
    let free = std::net::TcpListener::bind("127.0.0.1:0").expect("a free port");
    let port = free.local_addr().expect("an address").port();
    drop(free);
    h.call("config.set", json!({ "key": "phone.port", "value": port }))
        .await;
    h.call(
        "config.set",
        json!({ "key": "phone.enabled", "value": true }),
    )
    .await;
    // First the listener reads Tailscale, then it has the hint of a machine without it.
    let on = status
        .wait_for("the listener on, without Tailscale", |m| {
            status_of(m)
                .is_some_and(|s| s["listening"] == true && s["tailscale"]["state"] == "missing")
        })
        .await;
    let on = status_of(&on).expect("a status");
    assert_eq!(on["port"], json!(port));
    assert!(on.get("origin").is_none(), "no phone.url: no origin");
    assert!(
        on["hint"]
            .as_str()
            .is_some_and(|hint| hint.contains("phone.url")),
        "{on}"
    );
    // Without an origin no host passes, and an offer cannot have a URL.
    let page = http(
        port,
        "GET",
        "/",
        &[("Host", &format!("127.0.0.1:{port}"))],
        None,
    )
    .await;
    assert_eq!(page.status, 403);
    let offer = h.handle.rpc("phone.pair_begin", json!({})).await;
    assert_eq!(offer.expect_err("no origin").code, ErrorCode::Unsupported);

    h.call(
        "config.set",
        json!({ "key": "phone.url", "value": format!("http://127.0.0.1:{port}/") }),
    )
    .await;
    status
        .wait_for("the origin", |m| {
            status_of(m).is_some_and(|s| s["origin"] == format!("http://127.0.0.1:{port}"))
        })
        .await;
    let page = http(
        port,
        "GET",
        "/",
        &[("Host", &format!("127.0.0.1:{port}"))],
        None,
    )
    .await;
    assert_eq!(page.status, 200);
    let offer = h.call("phone.pair_begin", json!({})).await;
    assert!(
        offer["url"]
            .as_str()
            .is_some_and(|url| url.starts_with(&format!("http://127.0.0.1:{port}/pair?code=")))
    );

    h.call(
        "config.set",
        json!({ "key": "phone.enabled", "value": false }),
    )
    .await;
    status
        .wait_for("the listener off again", |m| {
            status_of(m).is_some_and(|s| s["listening"] == false)
        })
        .await;
    let closed = timeout(TIMEOUT, async {
        while TcpStream::connect(("127.0.0.1", port)).await.is_ok() {
            tokio::task::yield_now().await;
        }
    })
    .await;
    assert!(closed.is_ok(), "the port still takes connections");
}

/// A sink that drops the messages.
struct Quiet;

impl nh_core::MessageSink for Quiet {
    fn send(&self, _message: Value) -> bool {
        true
    }
}

/// The lines of the log, at every level, kept in memory.
#[derive(Clone, Default)]
struct Lines(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for Lines {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_log_has_no_token_and_no_code() {
    // Every level, and no redaction: the code itself must not log a secret.
    let lines = Lines::default();
    let writer = lines.clone();
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_ansi(false)
        .with_writer(move || writer.clone())
        .try_init();
    let rig = Rig::start();
    let (long, short) = rig.offer().await;
    assert_eq!(rig.pair("WRONG-CODE").await.status, 403);
    let answer = rig.pair(&long).await;
    let token = answer.cookie().expect("a cookie");
    let mut ws = rig.rpc(Some(&token)).await.expect("the socket");
    call(&mut ws, 1, "pane.input", json!({})).await;
    drop(ws);
    let refused = connect(
        rig.port,
        "evil.example",
        &rig.origin(),
        "/rpc",
        Some(&token),
    )
    .await;
    assert!(refused.is_err());
    let _ = std::io::stdout().flush();
    let log = String::from_utf8_lossy(&lines.0.lock().unwrap_or_else(PoisonError::into_inner))
        .into_owned();
    assert!(!log.is_empty(), "the test sees the log");
    assert!(!log.contains(&token), "the token is in the log");
    assert!(!log.contains(&long), "the long code is in the log");
    assert!(
        !log.contains(&short.replace('-', "")),
        "the short code is in the log"
    );
    assert!(!log.contains(&short), "the short code is in the log");
}

/// Small limits, so that a test reaches each one quickly.
const SMALL: nh_web::Limits = nh_web::Limits {
    connections: 3,
    unpaired: 2,
    header_wait: Duration::from_millis(600),
    pair_body_wait: Duration::from_millis(600),
};

/// Sends one request on a connection that stays open, and returns the status of the answer.
async fn request_on(stream: &mut TcpStream, path: &str, headers: &[(String, String)]) -> u16 {
    try_request_on(stream, path, headers)
        .await
        .expect("the listener closed the connection")
}

/// [`request_on`], or `None` when the listener closes the connection without an answer.
async fn try_request_on(
    stream: &mut TcpStream,
    path: &str,
    headers: &[(String, String)],
) -> Option<u16> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let mut request = format!("GET {path} HTTP/1.1\r\n");
    for (name, value) in headers {
        request.push_str(&format!("{name}: {value}\r\n"));
    }
    request.push_str("\r\n");
    stream.write_all(request.as_bytes()).await.ok()?;
    let mut raw = Vec::new();
    let mut chunk = [0u8; 4096];
    // The head, then the body of `Content-Length`.
    loop {
        let n = timeout(TIMEOUT, stream.read(&mut chunk))
            .await
            .expect("an answer in time")
            .ok()?;
        if n == 0 {
            return None;
        }
        raw.extend_from_slice(&chunk[..n]);
        let Some(end) = raw.windows(4).position(|w| w == b"\r\n\r\n") else {
            continue;
        };
        let head = String::from_utf8_lossy(&raw[..end]).to_ascii_lowercase();
        let length = head
            .lines()
            .find_map(|line| line.strip_prefix("content-length:"))
            .and_then(|value| value.trim().parse::<usize>().ok())
            .unwrap_or(0);
        if raw.len() >= end + 4 + length {
            return head.split(' ').nth(1).and_then(|code| code.parse().ok());
        }
    }
}

/// A new connection that gets an answer to `GET <path>`. A connection that ended a moment ago can
/// still hold its permit, so it tries again until [`TIMEOUT`].
async fn answered(port: u16, path: &str, headers: &[(String, String)]) -> (TcpStream, u16) {
    let deadline = std::time::Instant::now() + TIMEOUT;
    loop {
        let mut stream = TcpStream::connect(("127.0.0.1", port))
            .await
            .expect("a connection");
        if let Some(status) = try_request_on(&mut stream, path, headers).await {
            return (stream, status);
        }
        assert!(std::time::Instant::now() < deadline, "no connection got in");
        tokio::task::yield_now().await;
    }
}

/// Reads until the listener closes the connection, and returns the time that took.
async fn closed_after(stream: &mut TcpStream) -> Duration {
    use tokio::io::AsyncReadExt;

    let started = std::time::Instant::now();
    let mut sink = [0u8; 1024];
    loop {
        match timeout(TIMEOUT, stream.read(&mut sink))
            .await
            .expect("the listener closes the connection in time")
        {
            Ok(0) | Err(_) => return started.elapsed(),
            Ok(_) => {}
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unpaired_clients_cannot_hold_the_connections_of_the_core() {
    let h = Harness::start_layout();
    let (phone, port) =
        PhoneService::start_dev_with(h.handle.clone(), 0, SMALL).expect("the listener");
    let rig = Rig { h, phone, port };
    let connect = || TcpStream::connect(("127.0.0.1", port));

    // Two clients that send nothing take the permits of unpaired clients. A third one is closed at
    // once, long before the wait for its headers ends.
    let mut first = connect().await.expect("a connection");
    let mut second = connect().await.expect("a connection");
    let mut third = connect().await.expect("a connection");
    assert!(closed_after(&mut third).await < SMALL.header_wait);
    // The silent ones are closed when the wait for their headers ends.
    assert!(closed_after(&mut first).await >= SMALL.header_wait / 2);
    closed_after(&mut second).await;

    // The permits came back: a client gets an answer again.
    let host = vec![("Host".to_owned(), rig.host())];
    let (mut again, status) = answered(port, "/", &host).await;
    assert_eq!(status, 200);
    // A connection that waits after its answer is closed too.
    closed_after(&mut again).await;

    // A pairing whose body does not come ends after its wait.
    let mut slow = connect().await.expect("a connection");
    {
        use tokio::io::AsyncWriteExt;
        let head = format!(
            "POST /api/pair HTTP/1.1\r\nHost: {}\r\nOrigin: {}\r\nContent-Type: application/json\r\n\
             Content-Length: 100\r\n\r\n{{\"code\"",
            rig.host(),
            rig.origin()
        );
        slow.write_all(head.as_bytes())
            .await
            .expect("the start of the request goes");
    }
    let mut answer = Vec::new();
    {
        use tokio::io::AsyncReadExt;
        let _ = timeout(TIMEOUT, slow.read_to_end(&mut answer))
            .await
            .expect("an answer in time");
    }
    let answer = String::from_utf8_lossy(&answer);
    assert!(answer.starts_with("HTTP/1.1 408"), "{answer}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_connection_of_a_paired_phone_does_not_count_as_unpaired() {
    let h = Harness::start_layout();
    let (phone, port) =
        PhoneService::start_dev_with(h.handle.clone(), 0, SMALL).expect("the listener");
    let rig = Rig { h, phone, port };
    let (token, _) = rig.paired().await;
    let connect = || TcpStream::connect(("127.0.0.1", port));

    // The phone shows its cookie, and its connection gives back the permit of an unpaired client.
    let (_paired, status) = answered(port, "/api/me", &rig.headers(&token)).await;
    assert_eq!(status, 200);
    // Two unpaired clients still get in, and each gets an answer.
    let host = vec![("Host".to_owned(), rig.host())];
    let mut first = connect().await.expect("a connection");
    let mut second = connect().await.expect("a connection");
    assert_eq!(request_on(&mut first, "/", &host).await, 200);
    assert_eq!(request_on(&mut second, "/", &host).await, 200);
    // The limit of all connections holds: the fourth one is closed at once.
    let mut fourth = connect().await.expect("a connection");
    assert!(closed_after(&mut fourth).await < SMALL.header_wait);
}
