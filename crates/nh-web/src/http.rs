//! The routes of the listener and their checks.
//!
//! | Route | Cookie | Origin |
//! |---|---|---|
//! | `GET /`, `/pair`, the files of the app, `/manifest.webmanifest`, `/theme.css` | no | no |
//! | `POST /api/pair` | no | yes |
//! | `GET /api/me` | yes | no |
//! | `POST /api/unpair` | yes | yes |
//! | `GET /rpc` (WebSocket) | yes | yes |
//! | `GET /stream` (WebSocket) | yes | yes |
//! | `GET /blob/<hash>` | yes | no |
//! | `POST /blob` | yes | yes |
//! | `GET /font/faces.css`, `GET /font/file` | yes | no |
//!
//! `/rpc`, `/stream` and both routes of `/blob` take `machine=<label>`: the request goes to that
//! machine of the host ([`crate::machines`]). No label is the core of the listener.
//!
//! Every request needs the host of the public origin in `Host`, so that a page of another site
//! cannot reach the port through DNS rebinding. A development run also takes `127.0.0.1:<port>`.
//! Nothing here logs a token, a code or a query.
//!
//! **Limits.** Every device of the tailnet and every local user can open connections, so the
//! listener bounds what they hold in the process of the core ([`crate::Limits`]): at most 128
//! connections, of which at most 16 have not yet shown the cookie of a paired phone; 15 s for the
//! headers of each request, also between two requests of a connection; and 10 s for the body of
//! `POST /api/pair`. A connection over a limit is closed at once. A WebSocket of a paired phone
//! counts no more after its upgrade.

use std::{
    convert::Infallible,
    sync::{
        Arc, Mutex, PoisonError, RwLock,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use bytes::Bytes;
use http_body_util::{BodyExt, Full, Limited};
use hyper::{
    HeaderMap, Method, Request, Response, StatusCode,
    body::Incoming,
    header::{self, HeaderValue},
    server::conn::http1,
    service::service_fn,
};
use hyper_util::rt::{TokioIo, TokioTimer};
use nh_core::{
    ClientInfo, CoreHandle, ErrorCode, Origin,
    phones::{PairRefusal, PhoneIdentity},
};
use nh_proto::api::ids::WindowId;
use serde::Deserialize;
use serde_json::json;
use tokio::{
    net::{TcpListener, TcpStream},
    sync::{OwnedSemaphorePermit, Semaphore, broadcast, watch},
};
use tokio_tungstenite::{
    WebSocketStream,
    tungstenite::{handshake::derive_accept_key, protocol::Role},
};

use crate::{
    files,
    fonts::{self, FontSource},
    machines::{self, MachineBackend, PhoneMachine, PhoneMachines},
    rpc, stream,
};

/// The name of the cookie of a phone.
const COOKIE: &str = "nh_device";

/// How long a browser keeps the cookie: 400 days, the most that browsers allow.
const COOKIE_AGE: u32 = 34_560_000;

/// The largest body of `POST /api/pair`.
const PAIR_BODY_MAX: usize = 4096;

/// The pause after a failed accept, for example when the process has no free file descriptor.
const ACCEPT_PAUSE: Duration = Duration::from_millis(100);

/// The policy of every page. `'wasm-unsafe-eval'` lets the page compile the decoder of
/// the frames, and it allows no script from a string.
const CSP: &str = "default-src 'self'; connect-src 'self'; img-src 'self' data: blob:; \
style-src 'self' 'unsafe-inline'; script-src 'self' 'wasm-unsafe-eval'; font-src 'self'; \
frame-ancestors 'none'";

/// The answer to every refused pairing, so that a guess learns nothing.
/// The numbers of the phone connections of this process. `phone.visible` holds for one connection.
static CONNECTIONS: AtomicU64 = AtomicU64::new(1);

const PAIR_REFUSED: &str = "The code is wrong, or the offer ended. Ask the Mac for a new code.";

/// The image types that a phone gets from the blob store.
const IMAGES: [&str; 4] = ["image/png", "image/jpeg", "image/gif", "image/webp"];

type Body = Full<Bytes>;

/// The font source that the host gives the listener. `nh daemon` gives none.
pub(crate) type FontSlot = RwLock<Option<Arc<dyn FontSource>>>;

/// The other machines that the host gives the listener. `nh daemon` gives none.
pub(crate) type MachineSlot = RwLock<Option<Arc<dyn PhoneMachines>>>;

/// One listener.
pub(crate) struct Server {
    pub core: CoreHandle,
    pub checks: Checks,
    pub stop: watch::Receiver<bool>,
    pub fonts: Arc<FontSlot>,
    pub machines: Arc<MachineSlot>,
    pub admission: Admission,
}

/// The limits of the connections of one listener, with their permits.
pub(crate) struct Admission {
    limits: crate::Limits,
    all: Arc<Semaphore>,
    unpaired: Arc<Semaphore>,
}

impl Admission {
    pub(crate) fn new(limits: crate::Limits) -> Self {
        Self {
            limits,
            all: Arc::new(Semaphore::new(limits.connections)),
            unpaired: Arc::new(Semaphore::new(limits.unpaired)),
        }
    }

    /// The permits of a new connection, or `None` when it is over a limit.
    fn admit(&self) -> Option<(OwnedSemaphorePermit, OwnedSemaphorePermit)> {
        let all = Arc::clone(&self.all).try_acquire_owned().ok()?;
        let unpaired = Arc::clone(&self.unpaired).try_acquire_owned().ok()?;
        Some((all, unpaired))
    }
}

/// What the checks of a request compare with.
pub(crate) struct Checks {
    pub port: u16,
    /// The public origin. `None`: no request passes the check of `Host` or `Origin`, except in a
    /// development run. A new `phone.url` changes it while the listener runs.
    pub origin: RwLock<Option<String>>,
    pub dev: bool,
}

/// Completes when the listener stops.
async fn stopped(mut stop: watch::Receiver<bool>) {
    while !*stop.borrow_and_update() {
        if stop.changed().await.is_err() {
            return;
        }
    }
}

/// Takes the connections until the listener stops.
pub(crate) async fn accept(listener: TcpListener, server: Arc<Server>) {
    let stop = stopped(server.stop.clone());
    tokio::pin!(stop);
    loop {
        tokio::select! {
            () = &mut stop => return,
            accepted = listener.accept() => {
                let stream = match accepted {
                    Ok((stream, _)) => stream,
                    Err(error) => {
                        // Without a pause, an error that stays (no free file descriptor) turns
                        // the loop into a busy loop.
                        tracing::warn!(%error, "the phone listener cannot accept a connection");
                        tokio::time::sleep(ACCEPT_PAUSE).await;
                        continue;
                    }
                };
                let Some(permits) = server.admission.admit() else {
                    tracing::debug!("a phone connection over the limit is closed");
                    drop(stream);
                    continue;
                };
                tokio::spawn(connection(stream, Arc::clone(&server), permits));
            }
        }
    }
}

async fn connection(
    stream: TcpStream,
    server: Arc<Server>,
    (_all, unpaired): (OwnedSemaphorePermit, OwnedSemaphorePermit),
) {
    let _ = stream.set_nodelay(true);
    let stop = stopped(server.stop.clone());
    let header_wait = server.admission.limits.header_wait;
    // The permit of an unpaired connection goes back at the first request with a valid cookie.
    let unpaired = Arc::new(Mutex::new(Some(unpaired)));
    let service = service_fn(move |request| {
        let (server, unpaired) = (Arc::clone(&server), Arc::clone(&unpaired));
        async move {
            release_when_paired(&server, &unpaired, request.headers()).await;
            Ok::<_, Infallible>(handle(&server, request).await)
        }
    });
    let served = http1::Builder::new()
        .timer(TokioTimer::new())
        .header_read_timeout(header_wait)
        .serve_connection(TokioIo::new(stream), service)
        .with_upgrades();
    tokio::select! {
        result = served => {
            if let Err(error) = result {
                tracing::debug!(%error, "a phone connection ended with an error");
            }
        }
        () = stop => {}
    }
}

/// Gives back the permit of an unpaired connection when a request has the cookie of a paired
/// phone. It looks up the cookie only while the connection holds the permit.
async fn release_when_paired(
    server: &Server,
    unpaired: &Mutex<Option<OwnedSemaphorePermit>>,
    headers: &HeaderMap,
) {
    let holds = unpaired
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .is_some();
    if !holds {
        return;
    }
    let Some(token) = token_of(headers) else {
        return;
    };
    if server.core.phone_of_token(&token).await.is_some() {
        drop(
            unpaired
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .take(),
        );
    }
}

/// The host and port of an origin, as `Host` carries them.
fn authority_of(origin: &str) -> Option<&str> {
    origin.split_once("://").map(|(_, rest)| rest)
}

/// The host and port without the default port of the scheme.
fn plain_authority(authority: &str, https: bool) -> &str {
    let default = if https { ":443" } else { ":80" };
    authority.strip_suffix(default).unwrap_or(authority)
}

impl Checks {
    pub(crate) fn origin(&self) -> Option<String> {
        self.origin
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub(crate) fn set_origin(&self, origin: Option<String>) {
        *self.origin.write().unwrap_or_else(PoisonError::into_inner) = origin;
    }

    fn host_allowed(&self, headers: &HeaderMap) -> bool {
        let Some(host) = headers.get(header::HOST).and_then(|v| v.to_str().ok()) else {
            return false;
        };
        let host = host.to_ascii_lowercase();
        if self.dev
            && (host == format!("127.0.0.1:{}", self.port)
                || host == format!("localhost:{}", self.port))
        {
            return true;
        }
        let Some(origin) = self.origin() else {
            return false;
        };
        let https = origin.starts_with("https://");
        authority_of(&origin).is_some_and(|authority| {
            plain_authority(authority, https) == plain_authority(&host, https)
        })
    }

    fn origin_allowed(&self, headers: &HeaderMap) -> bool {
        let Some(given) = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()) else {
            return false;
        };
        self.origin()
            .is_some_and(|origin| origin.eq_ignore_ascii_case(given))
    }

    fn cookie(&self, token: &str) -> String {
        let secure = if self.dev { "" } else { "; Secure" };
        format!("{COOKIE}={token}; HttpOnly{secure}; SameSite=Strict; Path=/; Max-Age={COOKIE_AGE}")
    }

    fn cookie_cleared(&self) -> String {
        let secure = if self.dev { "" } else { "; Secure" };
        format!("{COOKIE}=; HttpOnly{secure}; SameSite=Strict; Path=/; Max-Age=0")
    }
}

/// The token of the cookie of a phone: 64 hex digits.
fn token_of(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .filter_map(|pair| pair.trim().strip_prefix(COOKIE)?.strip_prefix('='))
        .find(|token| token.len() == 64 && token.bytes().all(|b| b.is_ascii_hexdigit()))
        .map(str::to_owned)
}

/// The value of a key of the query, without a decode: the values here are base64url.
fn query_value<'a>(query: Option<&'a str>, key: &str) -> Option<&'a str> {
    query?
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find(|(k, _)| *k == key)
        .map(|(_, v)| v)
}

fn response(status: StatusCode, kind: &str, body: impl Into<Bytes>) -> Response<Body> {
    let mut response = Response::new(Full::new(body.into()));
    *response.status_mut() = status;
    if let Ok(kind) = HeaderValue::from_str(kind) {
        response.headers_mut().insert(header::CONTENT_TYPE, kind);
    }
    response
}

fn json_response(status: StatusCode, body: &serde_json::Value) -> Response<Body> {
    response(status, "application/json", body.to_string())
}

fn refused(status: StatusCode, text: &str) -> Response<Body> {
    json_response(status, &json!({ "error": text }))
}

/// The headers of every answer.
fn finish(mut response: Response<Body>, cache: &'static str) -> Response<Body> {
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(CSP),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    headers
        .entry(header::CACHE_CONTROL)
        .or_insert(HeaderValue::from_static(cache));
    response
}

async fn handle(server: &Server, request: Request<Incoming>) -> Response<Body> {
    if !server.checks.host_allowed(request.headers()) {
        tracing::debug!(
            path = request.uri().path(),
            "a phone request with a wrong host"
        );
        return finish(refused(StatusCode::FORBIDDEN, "not allowed"), "no-store");
    }
    let path = request.uri().path().to_owned();
    match (request.method().clone(), path.as_str()) {
        (Method::GET, "/" | "/index.html" | "/pair") => finish(
            response(
                StatusCode::OK,
                files::kind_of("index.html"),
                page(request.uri().query()),
            ),
            "no-cache",
        ),
        (Method::GET, "/manifest.webmanifest") => finish(
            manifest(request.uri().query(), &theme(&server.core)),
            "no-cache",
        ),
        (Method::GET, "/theme.css") => finish(
            response(
                StatusCode::OK,
                files::kind_of("theme.css"),
                theme_css(&theme(&server.core)),
            ),
            "no-cache",
        ),
        (Method::POST, "/api/pair") => finish(pair(server, request).await, "no-store"),
        (Method::GET, "/api/me") => finish(me(server, request.headers()).await, "no-store"),
        (Method::POST, "/api/unpair") => {
            finish(unpair(server, request.headers()).await, "no-store")
        }
        (Method::GET, "/rpc") => finish(upgrade(server, request).await, "no-store"),
        (Method::GET, "/stream") => finish(stream_upgrade(server, request).await, "no-store"),
        (Method::POST, "/blob") => finish(blob_put(server, request).await, "no-store"),
        (Method::GET, other) if other.starts_with("/font/") => {
            let query = request.uri().query().map(str::to_owned);
            finish(
                font(server, request.headers(), other, query.as_deref()).await,
                "private, max-age=3600",
            )
        }
        (Method::GET, other) if other.starts_with("/blob/") => finish(
            blob(
                server,
                request.headers(),
                request.uri().query(),
                &other["/blob/".len()..],
            )
            .await,
            "no-store",
        ),
        (Method::GET, other) => match files::file(other.trim_start_matches('/')) {
            Some(bytes) => {
                let cache = if other.starts_with("/assets/") {
                    "public, max-age=31536000, immutable"
                } else {
                    "no-cache"
                };
                finish(
                    response(StatusCode::OK, files::kind_of(other), bytes),
                    cache,
                )
            }
            None => finish(refused(StatusCode::NOT_FOUND, "not found"), "no-store"),
        },
        _ => finish(
            refused(StatusCode::METHOD_NOT_ALLOWED, "method not allowed"),
            "no-store",
        ),
    }
}

/// The code of a pairing in the query, when it is base64url or a short code.
fn code_of(query: Option<&str>) -> Option<&str> {
    query_value(query, "code").filter(|code| {
        !code.is_empty()
            && code.len() <= 64
            && code
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    })
}

/// The link of the page to its manifest, as `apps/web/index.html` has it.
const MANIFEST_LINK: &str = "href=\"/manifest.webmanifest\"";

/// The page of the app. With a code in the query, the link to the manifest names the code, so the
/// app that the person adds to the home screen starts with it.
fn page(query: Option<&str>) -> Bytes {
    let index = files::index();
    let Some(code) = code_of(query) else {
        return Bytes::from_static(index);
    };
    let Ok(text) = std::str::from_utf8(index) else {
        return Bytes::from_static(index);
    };
    Bytes::from(text.replacen(
        MANIFEST_LINK,
        &format!("href=\"/manifest.webmanifest?code={code}\""),
        1,
    ))
}

/// The colours of the section `theme`, with the names of the variables of `tokens.css`:
/// `bg_raised` is `--bg-raised`. A value that is not a plain CSS colour is left out, so a value
/// cannot add rules to the page.
fn theme(core: &CoreHandle) -> Vec<(&'static str, String)> {
    let config = core.config().config();
    let theme = &config.theme;
    [
        ("bg", &theme.bg),
        ("bg-raised", &theme.bg_raised),
        ("bg-hover", &theme.bg_hover),
        ("line", &theme.line),
        ("text", &theme.text),
        ("text-dim", &theme.text_dim),
        ("accent", &theme.accent),
        ("danger", &theme.danger),
    ]
    .into_iter()
    .filter(|(_, value)| plain_colour(value))
    .map(|(name, value)| (name, value.trim().to_owned()))
    .collect()
}

/// The functions of a CSS colour. Any other function, such as `url()`, is not a colour.
const COLOUR_FUNCTIONS: [&str; 10] = [
    "rgb", "rgba", "hsl", "hsla", "hwb", "lab", "lch", "oklab", "oklch", "color",
];

/// A CSS colour such as `#1c1c1c`, `black` or `rgb(0 0 0 / 50%)`: no character that ends a value or
/// a rule, and no function but one of a colour.
fn plain_colour(value: &str) -> bool {
    let value = value.trim();
    let plain = !value.is_empty()
        && value.len() <= 64
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || " #(),.%-/".contains(c));
    if !plain {
        return false;
    }
    match value.split_once('(') {
        None => !value.contains(')'),
        Some((name, rest)) => {
            COLOUR_FUNCTIONS.contains(&name.to_ascii_lowercase().as_str())
                && rest
                    .strip_suffix(')')
                    .is_some_and(|inner| !inner.contains(['(', ')']))
        }
    }
}

/// The style sheet of the theme. `html:root` has no layer, so it wins over the defaults of
/// `tokens.css`, which are in a layer.
fn theme_css(colours: &[(&'static str, String)]) -> String {
    let mut css = String::from("html:root {\n");
    for (name, value) in colours {
        css.push_str(&format!("  --{name}: {value};\n"));
    }
    css.push_str("}\n");
    css
}

/// The manifest of the home-screen app. With a code in the query, the app starts with it, so its
/// first launch pairs. Its colours are the background of the theme.
fn manifest(query: Option<&str>, colours: &[(&'static str, String)]) -> Response<Body> {
    let start = code_of(query).map_or_else(|| "/".to_owned(), |code| format!("/?code={code}"));
    let background = colours
        .iter()
        .find(|(name, _)| *name == "bg")
        .map_or("#000000", |(_, value)| value.as_str());
    let icons: Vec<_> = files::icons()
        .into_iter()
        .map(|(name, size)| json!({ "src": format!("/{name}"), "sizes": format!("{size}x{size}"), "type": "image/png" }))
        .collect();
    let body = json!({
        "name": "neohar",
        "short_name": "neohar",
        "id": "/",
        "start_url": start,
        "scope": "/",
        "display": "standalone",
        "background_color": background,
        "theme_color": background,
        "icons": icons,
    });
    response(
        StatusCode::OK,
        files::kind_of("x.webmanifest"),
        body.to_string(),
    )
}

#[derive(Deserialize)]
struct PairBody {
    code: String,
    #[serde(default)]
    name: String,
}

async fn pair(server: &Server, request: Request<Incoming>) -> Response<Body> {
    if !server.checks.origin_allowed(request.headers()) {
        return refused(StatusCode::FORBIDDEN, "not allowed");
    }
    let body = tokio::time::timeout(
        server.admission.limits.pair_body_wait,
        Limited::new(request.into_body(), PAIR_BODY_MAX).collect(),
    )
    .await;
    let body = match body {
        Ok(Ok(body)) => body,
        Ok(Err(_)) => return refused(StatusCode::PAYLOAD_TOO_LARGE, "the body is too large"),
        Err(_) => return refused(StatusCode::REQUEST_TIMEOUT, "the body did not come in time"),
    };
    let Ok(body) = serde_json::from_slice::<PairBody>(&body.to_bytes()) else {
        return refused(StatusCode::BAD_REQUEST, "the body is {code, name}");
    };
    match server.core.phone_pair(&body.code, &body.name).await {
        Ok(paired) => {
            let mut answer = json_response(
                StatusCode::OK,
                &json!({ "device": paired.device.to_string(), "name": paired.name }),
            );
            if let Ok(cookie) = HeaderValue::from_str(&server.checks.cookie(&paired.token)) {
                answer.headers_mut().insert(header::SET_COOKIE, cookie);
            }
            answer
        }
        Err(PairRefusal::Store(error)) => {
            tracing::warn!(%error, "a pairing failed in the store");
            refused(
                StatusCode::INTERNAL_SERVER_ERROR,
                "the Mac cannot keep the phone",
            )
        }
        Err(_) => refused(StatusCode::FORBIDDEN, PAIR_REFUSED),
    }
}

async fn me(server: &Server, headers: &HeaderMap) -> Response<Body> {
    let Some(token) = token_of(headers) else {
        return refused(StatusCode::UNAUTHORIZED, "not paired");
    };
    match server.core.phone_of_token(&token).await {
        Some(phone) => json_response(
            StatusCode::OK,
            &json!({ "device": phone.device.to_string(), "name": phone.name }),
        ),
        None => refused(StatusCode::UNAUTHORIZED, "not paired"),
    }
}

async fn unpair(server: &Server, headers: &HeaderMap) -> Response<Body> {
    if !server.checks.origin_allowed(headers) {
        return refused(StatusCode::FORBIDDEN, "not allowed");
    }
    let Some(token) = token_of(headers) else {
        return refused(StatusCode::UNAUTHORIZED, "not paired");
    };
    let Some(phone) = server.core.phone_of_token(&token).await else {
        return refused(StatusCode::UNAUTHORIZED, "not paired");
    };
    if let Err(error) = server.core.phone_unpair(phone.device).await {
        return refused(StatusCode::INTERNAL_SERVER_ERROR, &error.message);
    }
    let mut answer = response(StatusCode::NO_CONTENT, "text/plain", Bytes::new());
    if let Ok(cookie) = HeaderValue::from_str(&server.checks.cookie_cleared()) {
        answer.headers_mut().insert(header::SET_COOKIE, cookie);
    }
    answer
}

/// Whether a request asks for a WebSocket of version 13 (RFC 6455).
fn wants_websocket(headers: &HeaderMap) -> bool {
    let has = |name: header::HeaderName, word: &str| {
        headers
            .get_all(name)
            .iter()
            .filter_map(|v| v.to_str().ok())
            .flat_map(|v| v.split(','))
            .any(|v| v.trim().eq_ignore_ascii_case(word))
    };
    has(header::UPGRADE, "websocket")
        && has(header::CONNECTION, "upgrade")
        && headers
            .get(header::SEC_WEBSOCKET_VERSION)
            .is_some_and(|v| v == "13")
}

async fn upgrade(server: &Server, mut request: Request<Incoming>) -> Response<Body> {
    if !server.checks.origin_allowed(request.headers()) {
        return refused(StatusCode::FORBIDDEN, "not allowed");
    }
    if !wants_websocket(request.headers()) {
        return refused(StatusCode::BAD_REQUEST, "/rpc is a WebSocket");
    }
    let Some(key) = request.headers().get(header::SEC_WEBSOCKET_KEY).cloned() else {
        return refused(StatusCode::BAD_REQUEST, "no Sec-WebSocket-Key");
    };
    let Some(token) = token_of(request.headers()) else {
        return refused(StatusCode::UNAUTHORIZED, "not paired");
    };
    // The removals from now on, so that a removal after the check below is not missed.
    let removed = server.core.phone_revocations();
    let Some(phone) = server.core.phone_of_token(&token).await else {
        return refused(StatusCode::UNAUTHORIZED, "not paired");
    };
    match machine_of(server, request.uri().query()) {
        Ok(Some(machine)) => {
            let phone = Phone {
                identity: phone,
                token,
                removed,
            };
            return machine_upgrade(server, request, &key, phone, machine).await;
        }
        Ok(None) => {}
        Err(answer) => return *answer,
    }
    let window = match phone_window(&server.core, phone.device).await {
        Ok(window) => window,
        Err(answer) => return *answer,
    };
    let on_upgrade = hyper::upgrade::on(&mut request);
    let core = server.core.clone();
    let stop = server.stop.clone();
    tokio::spawn(async move {
        let upgraded = match on_upgrade.await {
            Ok(upgraded) => upgraded,
            Err(error) => {
                tracing::debug!(%error, "the upgrade of a phone connection failed");
                return;
            }
        };
        let ws = WebSocketStream::from_raw_socket(TokioIo::new(upgraded), Role::Server, None).await;
        core.phone_seen(phone.device).await;
        // While the socket is open, the phone is a client, and its window is present.
        let attachment = core.attach_phone(phone.device, &phone.name);
        let connection = CONNECTIONS.fetch_add(1, Ordering::Relaxed);
        let backend = Arc::new(rpc::PhoneBackend {
            core: core.clone(),
            device: phone.device,
            connection,
            window,
            links: std::sync::Mutex::default(),
        });
        let ended = rpc::ended(core.clone(), phone.device, token, removed, stop);
        nh_core::ws_rpc::serve(backend, ws, ended).await;
        drop(attachment);
        // The app of a connection that ended is not visible.
        core.phone_left(phone.device, connection);
    });
    switching(&key)
}

/// A paired phone whose socket opens, and the removals from the time of its check.
struct Phone {
    identity: PhoneIdentity,
    token: String,
    removed: broadcast::Receiver<ulid::Ulid>,
}

impl Phone {
    fn info(&self) -> ClientInfo {
        ClientInfo {
            device: self.identity.device,
            name: self.identity.name.clone(),
        }
    }
}

/// The machine that a request names with `machine=`, or `None` for the core of the listener.
/// A label that is no machine of the host, or a machine whose link is down, gives the
/// answer that refuses the request.
fn machine_of(
    server: &Server,
    query: Option<&str>,
) -> Result<Option<Arc<dyn PhoneMachine>>, Box<Response<Body>>> {
    let Some(label) = query_value(query, "machine").and_then(fonts::form_decode) else {
        return Ok(None);
    };
    if label == "local" {
        return Ok(None);
    }
    let machines = server
        .machines
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    let Some(machines) = machines else {
        return Err(Box::new(refused(
            StatusCode::NOT_FOUND,
            "this core reaches no other machine",
        )));
    };
    machines.machine(&label).map(Some).map_err(|error| {
        let status = if error.code == ErrorCode::NotFound {
            StatusCode::NOT_FOUND
        } else {
            StatusCode::SERVICE_UNAVAILABLE
        };
        Box::new(refused(status, &error.message))
    })
}

/// The window of the phone on a machine, or the answer that refuses the request.
async fn machine_window(
    machine: &dyn PhoneMachine,
    phone: &ClientInfo,
) -> Result<WindowId, Box<Response<Body>>> {
    machines::phone_window(machine, phone)
        .await
        .map_err(|error| {
            tracing::warn!(%error, "the phone window of a machine is not there");
            Box::new(refused(StatusCode::SERVICE_UNAVAILABLE, &error.message))
        })
}

/// `/rpc?machine=<label>`: the JSON-RPC of a phone with a machine of the host. The
/// machine attaches the phone while a stream of it is open, so the listener attaches nothing
/// here.
async fn machine_upgrade(
    server: &Server,
    mut request: Request<Incoming>,
    key: &HeaderValue,
    phone: Phone,
    machine: Arc<dyn PhoneMachine>,
) -> Response<Body> {
    let info = phone.info();
    let window = match machine_window(machine.as_ref(), &info).await {
        Ok(window) => window,
        Err(answer) => return *answer,
    };
    let on_upgrade = hyper::upgrade::on(&mut request);
    let core = server.core.clone();
    let stop = server.stop.clone();
    tokio::spawn(async move {
        let upgraded = match on_upgrade.await {
            Ok(upgraded) => upgraded,
            Err(error) => {
                tracing::debug!(%error, "the upgrade of a phone connection failed");
                return;
            }
        };
        let ws = WebSocketStream::from_raw_socket(TokioIo::new(upgraded), Role::Server, None).await;
        let device = info.device;
        core.phone_seen(device).await;
        let connection = CONNECTIONS.fetch_add(1, Ordering::Relaxed);
        let backend = Arc::new(MachineBackend {
            core: core.clone(),
            machine,
            phone: info,
            connection,
            window,
        });
        let ended = rpc::ended(core.clone(), device, phone.token, phone.removed, stop);
        nh_core::ws_rpc::serve(backend, ws, ended).await;
        core.phone_left(device, connection);
    });
    switching(key)
}

/// The phone window of a device: the core makes it when there is none.
async fn phone_window(
    core: &CoreHandle,
    device: ulid::Ulid,
) -> Result<WindowId, Box<Response<Body>>> {
    let reply = core
        .call_as(Origin::phone(device), "phone.window", json!({}))
        .await
        .map_err(|error| {
            tracing::warn!(%error, "the phone window is not there");
            Box::new(refused(StatusCode::SERVICE_UNAVAILABLE, &error.message))
        })?;
    reply
        .get("window")
        .and_then(|window| serde_json::from_value::<WindowId>(window.clone()).ok())
        .ok_or_else(|| {
            Box::new(refused(
                StatusCode::INTERNAL_SERVER_ERROR,
                "the core gave no phone window",
            ))
        })
}

/// The answer that switches a request to a WebSocket.
fn switching(key: &HeaderValue) -> Response<Body> {
    let mut answer = response(StatusCode::SWITCHING_PROTOCOLS, "text/plain", Bytes::new());
    answer.headers_mut().remove(header::CONTENT_TYPE);
    let headers = answer.headers_mut();
    headers.insert(header::UPGRADE, HeaderValue::from_static("websocket"));
    headers.insert(header::CONNECTION, HeaderValue::from_static("Upgrade"));
    if let Ok(accept) = HeaderValue::from_str(&derive_accept_key(key.as_bytes())) {
        headers.insert(header::SEC_WEBSOCKET_ACCEPT, accept);
    }
    answer
}

/// `/stream?kind=&params=`: one stream of the core on a binary socket.
async fn stream_upgrade(server: &Server, mut request: Request<Incoming>) -> Response<Body> {
    if !server.checks.origin_allowed(request.headers()) {
        return refused(StatusCode::FORBIDDEN, "not allowed");
    }
    if !wants_websocket(request.headers()) {
        return refused(StatusCode::BAD_REQUEST, "/stream is a WebSocket");
    }
    let Some(key) = request.headers().get(header::SEC_WEBSOCKET_KEY).cloned() else {
        return refused(StatusCode::BAD_REQUEST, "no Sec-WebSocket-Key");
    };
    let Some(token) = token_of(request.headers()) else {
        return refused(StatusCode::UNAUTHORIZED, "not paired");
    };
    let removed = server.core.phone_revocations();
    let Some(phone) = server.core.phone_of_token(&token).await else {
        return refused(StatusCode::UNAUTHORIZED, "not paired");
    };
    let query = request.uri().query();
    let kind = query_value(query, "kind").and_then(fonts::form_decode);
    let params = match query_value(query, "params") {
        None => Some(json!({})),
        Some(text) => fonts::form_decode(text).and_then(|text| serde_json::from_str(&text).ok()),
    };
    let (Some(kind), Some(params)) = (kind, params) else {
        return refused(
            StatusCode::BAD_REQUEST,
            "/stream takes kind and params (JSON)",
        );
    };
    let machine = match machine_of(server, query) {
        Ok(machine) => machine,
        Err(answer) => return *answer,
    };
    let info = ClientInfo {
        device: phone.device,
        name: phone.name.clone(),
    };
    let window = match &machine {
        Some(machine) => machine_window(machine.as_ref(), &info).await,
        None => phone_window(&server.core, phone.device).await,
    };
    let window = match window {
        Ok(window) => window,
        Err(answer) => return *answer,
    };
    let params = match stream::checked_params(&kind, params, window) {
        Ok(params) => params,
        Err(error) if error.code == ErrorCode::PermissionDenied => {
            return refused(StatusCode::FORBIDDEN, &error.message);
        }
        Err(error) => return refused(StatusCode::BAD_REQUEST, &error.message),
    };
    let core = server.core.clone();
    let open: stream::Open = match machine {
        Some(machine) => Box::new(move |sink| {
            Box::pin(async move {
                machine
                    .stream(&info, &kind, params, Some(window), sink)
                    .await
            })
        }),
        None => {
            let core = core.clone();
            Box::new(move |sink| Box::pin(async move { core.stream(&kind, params, sink) }))
        }
    };
    let on_upgrade = hyper::upgrade::on(&mut request);
    let ends = stream::Ends {
        core: core.clone(),
        device: phone.device,
        token,
        removed,
        stop: server.stop.clone(),
    };
    tokio::spawn(async move {
        let upgraded = match on_upgrade.await {
            Ok(upgraded) => upgraded,
            Err(error) => {
                tracing::debug!(%error, "the upgrade of a phone stream failed");
                return;
            }
        };
        let ws = WebSocketStream::from_raw_socket(TokioIo::new(upgraded), Role::Server, None).await;
        stream::serve(ws, open, ends).await;
    });
    switching(&key)
}

/// The phone of the cookie of a request, or the answer that refuses it.
async fn paired(
    server: &Server,
    headers: &HeaderMap,
) -> Result<nh_core::phones::PhoneIdentity, Box<Response<Body>>> {
    let Some(token) = token_of(headers) else {
        return Err(Box::new(refused(StatusCode::UNAUTHORIZED, "not paired")));
    };
    server
        .core
        .phone_of_token(&token)
        .await
        .ok_or_else(|| Box::new(refused(StatusCode::UNAUTHORIZED, "not paired")))
}

/// `POST /blob?name=`: the bytes of one file that the person attaches. The answer is
/// `{hash}`, for `pane.drop_blobs`.
async fn blob_put(server: &Server, request: Request<Incoming>) -> Response<Body> {
    if !server.checks.origin_allowed(request.headers()) {
        return refused(StatusCode::FORBIDDEN, "not allowed");
    }
    if let Err(answer) = paired(server, request.headers()).await {
        return *answer;
    }
    let machine = match machine_of(server, request.uri().query()) {
        Ok(machine) => machine,
        Err(answer) => return *answer,
    };
    let name = query_value(request.uri().query(), "name")
        .and_then(fonts::form_decode)
        .unwrap_or_else(|| "file".to_owned());
    let limit = usize::try_from(nh_core::UPLOAD_MAX_BYTES).unwrap_or(usize::MAX);
    let Ok(body) = Limited::new(request.into_body(), limit).collect().await else {
        return refused(
            StatusCode::PAYLOAD_TOO_LARGE,
            &format!("a file has at most {limit} bytes"),
        );
    };
    let bytes = body.to_bytes().to_vec();
    let put = match machine {
        Some(machine) => machine.put_blob(&name, bytes).await,
        None => server
            .core
            .put_upload(&name, bytes)
            .await
            .map(|(hash, _)| hash),
    };
    match put {
        Ok(hash) => json_response(StatusCode::OK, &json!({ "hash": hash })),
        Err(error) if error.code == ErrorCode::TooLarge => {
            refused(StatusCode::PAYLOAD_TOO_LARGE, &error.message)
        }
        Err(error) => refused(StatusCode::INTERNAL_SERVER_ERROR, &error.message),
    }
}

/// `/font/faces.css` and `/font/file`: the fonts that the person installed on the Mac.
/// Without a font source the sheet is empty and no file is there.
async fn font(
    server: &Server,
    headers: &HeaderMap,
    path: &str,
    query: Option<&str>,
) -> Response<Body> {
    if let Err(answer) = paired(server, headers).await {
        return *answer;
    }
    let Some(request) = fonts::request(path, query) else {
        return refused(StatusCode::NOT_FOUND, "not found");
    };
    let source = server
        .fonts
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    // CoreText and the files take time: they go to the blocking pool.
    let found = tokio::task::spawn_blocking(move || match request {
        fonts::Request::Faces { families } => {
            let css = source.map_or_else(String::new, |source| {
                fonts::stylesheet(source.as_ref(), &families, "/font/file")
            });
            Some((css.into_bytes(), "text/css; charset=utf-8"))
        }
        fonts::Request::File {
            family,
            bold,
            italic,
        } => source.and_then(|source| fonts::file(source.as_ref(), &family, bold, italic)),
    })
    .await
    .ok()
    .flatten();
    match found {
        Some((bytes, kind)) => response(StatusCode::OK, kind, bytes),
        None => refused(StatusCode::NOT_FOUND, "not found"),
    }
}

async fn blob(
    server: &Server,
    headers: &HeaderMap,
    query: Option<&str>,
    hash: &str,
) -> Response<Body> {
    let Some(token) = token_of(headers) else {
        return refused(StatusCode::UNAUTHORIZED, "not paired");
    };
    if server.core.phone_of_token(&token).await.is_none() {
        return refused(StatusCode::UNAUTHORIZED, "not paired");
    }
    let found = match machine_of(server, query) {
        Ok(Some(machine)) => machine.blob(hash).await,
        Ok(None) => server.core.blob(hash).await,
        Err(answer) => return *answer,
    };
    match found {
        Ok(Some((bytes, kind))) if IMAGES.contains(&kind.as_str()) => {
            response(StatusCode::OK, &kind, bytes)
        }
        Ok(_) => refused(StatusCode::NOT_FOUND, "not found"),
        Err(error) => refused(StatusCode::INTERNAL_SERVER_ERROR, &error.message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(pairs: &[(header::HeaderName, &str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (name, value) in pairs {
            map.append(name.clone(), HeaderValue::from_str(value).expect("a value"));
        }
        map
    }

    fn server(origin: Option<&str>, dev: bool) -> Checks {
        Checks {
            port: 47_310,
            origin: RwLock::new(origin.map(str::to_owned)),
            dev,
        }
    }

    #[test]
    fn the_host_must_be_the_host_of_the_public_origin() {
        let s = server(Some("https://mac.example.ts.net:8443"), false);
        assert!(s.host_allowed(&headers(&[(header::HOST, "mac.example.ts.net:8443")])));
        assert!(s.host_allowed(&headers(&[(header::HOST, "MAC.example.ts.net:8443")])));
        assert!(!s.host_allowed(&headers(&[(header::HOST, "evil.example:8443")])));
        assert!(!s.host_allowed(&headers(&[(header::HOST, "127.0.0.1:47310")])));
        assert!(!s.host_allowed(&HeaderMap::new()));
        let plain = server(Some("https://mac.example.ts.net"), false);
        assert!(plain.host_allowed(&headers(&[(header::HOST, "mac.example.ts.net:443")])));
        let dev = server(None, true);
        assert!(dev.host_allowed(&headers(&[(header::HOST, "127.0.0.1:47310")])));
        assert!(!server(None, false).host_allowed(&headers(&[(header::HOST, "127.0.0.1:47310")])));
    }

    #[test]
    fn the_origin_must_be_the_public_origin() {
        let s = server(Some("https://mac.example.ts.net:8443"), false);
        assert!(s.origin_allowed(&headers(&[(
            header::ORIGIN,
            "https://mac.example.ts.net:8443"
        )])));
        assert!(!s.origin_allowed(&headers(&[(header::ORIGIN, "https://evil.example")])));
        assert!(!s.origin_allowed(&HeaderMap::new()));
    }

    #[test]
    fn the_token_comes_from_the_cookie_and_has_64_hex_digits() {
        let token = "a".repeat(64);
        let found = token_of(&headers(&[(
            header::COOKIE,
            &format!("x=1; nh_device={token}; y=2"),
        )]));
        assert_eq!(found.as_deref(), Some(token.as_str()));
        assert_eq!(
            token_of(&headers(&[(header::COOKIE, "nh_device=abc")])),
            None
        );
        assert_eq!(
            token_of(&headers(&[(
                header::COOKIE,
                &format!("nh_devicex={token}")
            )])),
            None
        );
        assert_eq!(token_of(&HeaderMap::new()), None);
    }

    #[test]
    fn the_cookie_is_secure_except_in_a_development_run() {
        let s = server(Some("https://mac.example.ts.net:8443"), false);
        let cookie = s.cookie("t");
        assert!(
            cookie.contains("HttpOnly; Secure; SameSite=Strict; Path=/"),
            "{cookie}"
        );
        assert!(!server(None, true).cookie("t").contains("Secure"));
        assert!(s.cookie_cleared().contains("Max-Age=0"));
    }

    async fn read(query: Option<&str>) -> serde_json::Value {
        let bytes = manifest(query, &[("bg", "#101010".to_owned())])
            .into_body()
            .collect()
            .await
            .expect("a body")
            .to_bytes();
        serde_json::from_slice(&bytes).expect("JSON")
    }

    #[tokio::test]
    async fn the_manifest_starts_with_a_code_only_when_it_is_base64url() {
        assert_eq!(
            read(Some("code=abc_DEF-1")).await["start_url"],
            "/?code=abc_DEF-1"
        );
        assert_eq!(read(Some("code=a<b")).await["start_url"], "/");
        assert_eq!(read(None).await["start_url"], "/");
        assert_eq!(read(None).await["display"], "standalone");
        assert_eq!(read(None).await["theme_color"], "#101010");
        let plain = manifest(None, &[])
            .into_body()
            .collect()
            .await
            .expect("a body");
        let plain: serde_json::Value = serde_json::from_slice(&plain.to_bytes()).expect("JSON");
        assert_eq!(plain["background_color"], "#000000");
    }

    #[test]
    fn a_colour_of_the_theme_cannot_add_a_rule() {
        assert!(plain_colour("#1c1c1c"));
        assert!(plain_colour(" black "));
        assert!(plain_colour("rgb(0 0 0 / 50%)"));
        assert!(!plain_colour(""));
        assert!(!plain_colour("red; } body { display: none"));
        assert!(!plain_colour("url(https://example.com/x.png)"));
        assert!(!plain_colour("url(x.png)"));
        assert!(!plain_colour("rgb(0 0 0) url(x.png)"));
        assert!(!plain_colour("red !important"));
        let css = theme_css(&[("bg", "#000000".to_owned()), ("text-dim", "tan".to_owned())]);
        assert_eq!(
            css,
            "html:root {\n  --bg: #000000;\n  --text-dim: tan;\n}\n"
        );
    }

    #[test]
    fn the_page_names_the_code_in_its_link_to_the_manifest() {
        // Without the built app, the page has no link and stays as it is.
        let plain = page(Some("code=abc_DEF-1"));
        if files::file("index.html").is_some() {
            let text = std::str::from_utf8(&plain).expect("UTF-8");
            assert!(
                text.contains("href=\"/manifest.webmanifest?code=abc_DEF-1\""),
                "{text}"
            );
        } else {
            assert_eq!(plain.as_ref(), files::index());
        }
        assert_eq!(page(Some("code=a<b")).as_ref(), files::index());
        assert_eq!(page(None).as_ref(), files::index());
    }
}
