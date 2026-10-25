//! The listener of the phone: the phone app, its JSON-RPC WebSocket and the
//! pairing, on a port of `127.0.0.1`.
//!
//! The host of the core starts a [`PhoneService`]: the app that has the core in its process, and
//! `nh daemon`. The service follows the section `[phone]` of the configuration. When `enabled` is
//! on, it serves `127.0.0.1:<port>`. `tailscale serve` puts the port on the tailnet with HTTPS, so
//! neohar has no TLS and no network listener of its own. A change of `enabled`, `port` or `url`
//! starts or stops the listener at once.
//!
//! The public origin is `phone.url`, else the name of this Mac in the tailnet with
//! `phone.https_port`. The service reads Tailscale when the listener starts, after
//! a change of `[phone]`, every minute, and when the core asks: before `phone.pair_begin` and
//! `doctor.run`. `phone.publish` and `phone.unpublish` change the mapping of `tailscale serve`
//! through the [`nh_core::phones::PhoneNetwork`] that the service gives the core.
//!
//! **The mapping lives only as long as the listener.** Any local user can listen on a free port
//! of `127.0.0.1`. A mapping to a port that neohar does not hold would bring the cookie of a
//! paired phone to that user. So the service removes the mapping when the listener stops (a quit,
//! `phone.enabled` off, another port) and when the port is not free. `phone.publish` leaves the
//! file `<data>/phone/published`, and while it is there the service adds the mapping again each
//! time the listener serves. `phone.unpublish` removes the file. After a crash the mapping stays
//! until the next start of the listener.
//!
//! A phone pairs with a code of the offer of `phone.pair_begin` (`POST /api/pair`) and gets a
//! cookie with a token. Every route that is not a file of the app needs the cookie. `/rpc` and each
//! `POST` also need the public origin in `Origin`, and every request needs its host in `Host`.
//! The calls of a phone have the scope `phone`.
//!
//! The service also gives the core the sender of the push ([`push`]). `phone_status`
//! has the public VAPID key while the listener runs.
//!
//! The listener serves the whole desk: the render streams on `/stream`, the
//! fonts that the person installed from a [`FontSource`] that the host gives, and the files that
//! the person attaches on `POST /blob`. A phone has one window on the core, and the
//! listener puts it into the origin of each call. The other machines of the host come through
//! the [`PhoneMachines`] that the host gives.

mod files;
pub mod fonts;
mod http;
pub mod machines;
pub mod push;
mod rpc;
mod stream;
mod tailscale;

use std::{
    io,
    net::SocketAddr,
    sync::{Arc, Mutex, PoisonError, Weak},
    time::Duration,
};

use async_trait::async_trait;
pub use fonts::{FontFace, FontSource};
pub use machines::{PhoneMachine, PhoneMachines};
use nh_core::{
    CoreHandle, ErrorCode, MessageSink, RpcError,
    phones::{PhoneNetwork, PushOut},
};
use nh_proto::api::model::{PhoneStatusValue, TailscaleStatus};
use serde_json::Value;
pub use tailscale::TailscaleCli;
use tokio::{net::TcpListener, sync::watch};

/// The hint when the listener is off.
const OFF: &str = "The phone listener is off. Turn on phone.enabled.";

/// The hint when `phone.url` is not an address.
const BAD_URL: &str = "Set phone.url to the HTTPS address of this Mac, for example \
https://studio.example.ts.net:8443, or empty it to use Tailscale.";

/// The hint before the first read of Tailscale.
const READING: &str = "neohar reads the state of Tailscale.";

/// How often the service reads Tailscale while the listener uses it.
const POLL: Duration = Duration::from_secs(60);

/// The file in the directory of the phones that says that the person published the listener
/// with Tailscale and did not unpublish it.
const PUBLISHED_MARK: &str = "published";

/// How long a stop of the service waits for the removal of the mapping.
const WITHDRAW_WAIT: Duration = Duration::from_secs(5);

/// Where one listener serves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Listen {
    port: u16,
    /// A development run: the origin is `http://127.0.0.1:<port>`, and the cookie has no
    /// `Secure`, because the browser of a test speaks plain HTTP to the loopback address.
    dev: bool,
}

/// How a phone reaches a listener that runs.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Face {
    /// The public origin, for example `https://studio.example.ts.net:8443`. `None`: every request
    /// but the files fails its check.
    origin: Option<String>,
    /// What the person must do before a phone can reach the listener.
    hint: Option<String>,
    tailscale: Option<TailscaleStatus>,
}

impl Face {
    fn status(self, port: u16, vapid_key: Option<String>) -> PhoneStatusValue {
        let published = self.tailscale.as_ref().is_some_and(|t| t.published);
        let summary = match (&self.origin, &self.hint) {
            (Some(origin), None) if published => format!("Published with Tailscale at {origin}."),
            (Some(origin), None) => format!("Listens on 127.0.0.1:{port} for {origin}."),
            _ => format!("Listens on 127.0.0.1:{port}. No phone can reach it yet."),
        };
        PhoneStatusValue {
            listening: true,
            port: Some(port),
            origin: self.origin,
            hint: self.hint,
            summary,
            tailscale: self.tailscale,
            vapid_key,
        }
    }
}

/// A listener that runs.
struct Running {
    listen: Listen,
    /// The port that the listener has: `listen.port`, or a free port when that is 0.
    port: u16,
    stop: watch::Sender<bool>,
    server: Arc<http::Server>,
}

struct Inner {
    core: CoreHandle,
    /// `Some` for a development run, which ignores `[phone]` and Tailscale.
    dev_port: Option<u16>,
    cli: TailscaleCli,
    running: Mutex<Option<Running>>,
    /// The last facts of Tailscale. Facts for other ports count as none.
    tailscale: Mutex<Option<tailscale::Facts>>,
    /// One read or change of Tailscale at a time.
    busy: tokio::sync::Mutex<()>,
    pusher: Arc<push::Pusher>,
    /// The fonts that the host gives. Every listener of the service reads them.
    fonts: Arc<http::FontSlot>,
    /// The other machines that the host gives.
    machines: Arc<http::MachineSlot>,
}

/// The listener of the phone. Dropping it stops the listener and closes its connections.
pub struct PhoneService {
    inner: Arc<Inner>,
    /// What the core calls for `phone.publish`, `phone.unpublish` and a read of Tailscale.
    network: Arc<dyn PhoneNetwork>,
    /// What the core calls for a push.
    push_out: Arc<dyn PushOut>,
}

impl PhoneService {
    /// Follows `[phone]` of the configuration of `core`: it serves while `phone.enabled` is on.
    /// The CLI of Tailscale is [`TailscaleCli::from_env`]. Call it on the runtime of the core or
    /// from any thread.
    pub fn start(core: CoreHandle) -> Self {
        Self::start_with(core, TailscaleCli::from_env())
    }

    /// [`PhoneService::start`] with this CLI of Tailscale. The tests give a fake.
    pub fn start_with(core: CoreHandle, cli: TailscaleCli) -> Self {
        Self::with(core, None, cli)
    }

    /// A development run (`nh daemon --phone-port <port>`): it always serves, on `port` of
    /// `127.0.0.1` (0 is a free port), with the origin `http://127.0.0.1:<port>`. Returns the port.
    pub fn start_dev(core: CoreHandle, port: u16) -> io::Result<(Self, u16)> {
        let service = Self::with(core, Some(port), TailscaleCli::Search);
        match service.port() {
            Some(port) => Ok((service, port)),
            None => Err(io::Error::other(
                service
                    .inner
                    .core
                    .phone_status()
                    .hint
                    .unwrap_or_else(|| "the phone listener did not start".to_owned()),
            )),
        }
    }

    fn with(core: CoreHandle, dev_port: Option<u16>, cli: TailscaleCli) -> Self {
        let pusher = Arc::new(push::Pusher::new(core.phone_dir(), core.config()));
        let inner = Arc::new(Inner {
            core: core.clone(),
            dev_port,
            cli,
            running: Mutex::new(None),
            tailscale: Mutex::new(None),
            busy: tokio::sync::Mutex::new(()),
            pusher: Arc::clone(&pusher),
            fonts: Arc::default(),
            machines: Arc::default(),
        });
        inner.reconcile();
        let network: Arc<dyn PhoneNetwork> = Arc::new(Network(Arc::downgrade(&inner)));
        core.set_phone_network(Arc::clone(&network));
        let push_out: Arc<dyn PushOut> = pusher;
        core.set_push_out(Arc::clone(&push_out));
        if dev_port.is_none() {
            // A change of the settings is a message of `settings`.
            let sink = Box::new(Follow(Arc::downgrade(&inner)));
            if let Err(error) = core.subscribe("settings", None, sink) {
                tracing::warn!(%error, "the phone listener does not follow the settings");
            }
            core.runtime().spawn(poll(Arc::downgrade(&inner)));
        }
        Self {
            inner,
            network,
            push_out,
        }
    }

    /// The faces of the fonts that the person installed: `/font/faces.css` and
    /// `/font/file` come from `source`. The app gives the CoreText code. Without a source the sheet
    /// is empty.
    pub fn set_font_source(&self, source: Arc<dyn FontSource>) {
        *self
            .inner
            .fonts
            .write()
            .unwrap_or_else(PoisonError::into_inner) = Some(source);
    }

    /// The other machines of the host: `machine=<label>` on `/rpc`, `/stream` and
    /// `/blob` goes to them. The app gives its machines. Without them a label other than `local`
    /// gets 404.
    pub fn set_machines(&self, machines: Arc<dyn PhoneMachines>) {
        *self
            .inner
            .machines
            .write()
            .unwrap_or_else(PoisonError::into_inner) = Some(machines);
    }

    /// The port that the listener serves, or `None` when it does not run.
    pub fn port(&self) -> Option<u16> {
        self.inner.port()
    }

    /// A development build only: the push may also go to `origin`, `http://127.0.0.1:<port>`, the
    /// fake push service of a test, and the first pause between two tries is `pause`. A release
    /// build ignores it. `NEOHAR_PUSH_TEST_ORIGIN` sets the origin for `nh daemon`.
    pub fn allow_test_push(&self, origin: &str, pause: Duration) {
        self.inner.pusher.set_test(origin, pause);
    }
}

impl PhoneService {
    /// Stops the listener and removes its mapping of `tailscale serve`, and waits for that for
    /// at most [`WITHDRAW_WAIT`]. The app calls it when it quits, because its state is never
    /// dropped. A second call does nothing.
    pub fn shutdown(&self) {
        let Some(port) = self.inner.stop(OFF) else {
            return;
        };
        if self.inner.dev_port.is_some() {
            return;
        }
        let (done, wait) = std::sync::mpsc::channel();
        let inner = Arc::clone(&self.inner);
        self.inner.core.runtime().spawn(async move {
            inner.withdraw(port).await;
            let _ = done.send(());
        });
        if wait.recv_timeout(WITHDRAW_WAIT).is_err() {
            tracing::warn!(
                port,
                "the mapping of tailscale serve was not removed in time; it goes to a port that \
                 nothing of neohar holds now"
            );
        }
    }
}

impl Drop for PhoneService {
    fn drop(&mut self) {
        self.inner.core.release_phone_network(&self.network);
        self.inner.core.release_push_out(&self.push_out);
        self.shutdown();
    }
}

/// The sink of `settings`: each message makes the service look at `[phone]` again. The hub calls
/// the sink while it delivers, so the work goes to the runtime and the sink returns at once.
struct Follow(Weak<Inner>);

impl MessageSink for Follow {
    fn send(&self, _message: Value) -> bool {
        match self.0.upgrade() {
            Some(inner) => {
                let runtime = inner.core.runtime();
                runtime.spawn(async move {
                    inner.reconcile();
                    inner.refresh().await;
                });
                true
            }
            None => false,
        }
    }
}

/// Reads Tailscale every [`POLL`], so that the status follows a change that the person makes in
/// Tailscale. It ends with the service.
async fn poll(inner: Weak<Inner>) {
    loop {
        let Some(strong) = inner.upgrade() else {
            return;
        };
        strong.refresh().await;
        drop(strong);
        tokio::time::sleep(POLL).await;
    }
}

/// The part of the service that the core calls.
struct Network(Weak<Inner>);

impl Network {
    fn inner(&self) -> Result<Arc<Inner>, RpcError> {
        self.0
            .upgrade()
            .ok_or_else(|| RpcError::new(ErrorCode::Unsupported, "The phone listener stopped."))
    }
}

#[async_trait]
impl PhoneNetwork for Network {
    async fn refresh(&self) {
        if let Some(inner) = self.0.upgrade() {
            inner.refresh().await;
        }
    }

    async fn publish(&self) -> Result<(), RpcError> {
        self.inner()?.publish().await
    }

    async fn unpublish(&self) -> Result<(), RpcError> {
        self.inner()?.unpublish().await
    }
}

/// The public origin of `phone.url`: no `/` at the end, and nothing after the host and port.
fn origin_of(url: &str) -> Option<String> {
    let url = url.trim();
    let (scheme, rest) = url.split_once("://")?;
    if !matches!(scheme, "https" | "http") {
        return None;
    }
    let authority = rest.split('/').next()?;
    if authority.is_empty() {
        return None;
    }
    Some(format!("{scheme}://{}", authority.to_ascii_lowercase()))
}

fn unsupported(text: impl Into<String>) -> RpcError {
    RpcError::new(ErrorCode::Unsupported, text.into())
}

impl Inner {
    fn port(&self) -> Option<u16> {
        self.running
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .map(|running| running.port)
    }

    /// The ports that the facts of Tailscale are for: the port of the listener and the HTTPS port.
    /// `None` when the listener does not use Tailscale: it does not run, `phone.url` names the
    /// origin, or the run is a development run.
    fn tailscale_ports(&self) -> Option<(u16, u16)> {
        if self.dev_port.is_some() {
            return None;
        }
        let phone = self.core.config().config().phone.clone();
        if !phone.url.trim().is_empty() {
            return None;
        }
        Some((phone.https_port, self.port()?))
    }

    /// How a phone reaches the listener on `port`.
    fn face(&self, listen: Listen, port: u16) -> Face {
        if listen.dev {
            return Face {
                origin: Some(format!("http://127.0.0.1:{port}")),
                hint: None,
                tailscale: None,
            };
        }
        let phone = self.core.config().config().phone.clone();
        if !phone.url.trim().is_empty() {
            let origin = origin_of(&phone.url);
            return Face {
                hint: origin.is_none().then(|| BAD_URL.to_owned()),
                origin,
                tailscale: None,
            };
        }
        let facts = self
            .tailscale
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
            .filter(|facts| facts.for_ports(phone.https_port, port));
        match facts {
            None => Face {
                origin: None,
                hint: Some(READING.to_owned()),
                tailscale: None,
            },
            Some(facts) => Face {
                origin: facts.origin(),
                hint: facts.hint(),
                tailscale: Some(facts.status),
            },
        }
    }

    /// Starts, stops or keeps the listener, as the configuration asks, and updates the origin and
    /// `phone_status`.
    fn reconcile(self: &Arc<Self>) {
        let listen = match self.dev_port {
            Some(port) => Some(Listen { port, dev: true }),
            None => {
                let phone = self.core.config().config().phone.clone();
                phone.enabled.then_some(Listen {
                    port: phone.port,
                    dev: false,
                })
            }
        };
        let mut running = self.running.lock().unwrap_or_else(PoisonError::into_inner);
        if running.as_ref().map(|r| r.listen) != listen {
            if let Some(old) = running.take() {
                let _ = old.stop.send(true);
                // Nothing of neohar holds the old port now.
                if !old.listen.dev {
                    self.withdraw_later(old.port);
                }
            }
            if let Some(listen) = listen {
                match self.listen(listen) {
                    Ok(started) => {
                        tracing::info!(port = started.port, "the phone listener serves 127.0.0.1");
                        *running = Some(started);
                    }
                    Err(error) => {
                        tracing::warn!(port = listen.port, %error, "the phone listener does not start");
                        drop(running);
                        // Another process holds the port. A mapping to it would bring the
                        // requests of the phones, with their cookies, to that process.
                        if !listen.dev {
                            self.withdraw_later(listen.port);
                        }
                        self.set_status(status_off(
                            &format!("Port {} of 127.0.0.1 is not free: {error}", listen.port),
                            "The phone listener does not run.",
                        ));
                        return;
                    }
                }
            }
        }
        let status = match running.as_ref() {
            None => status_off(OFF, "The phone listener is off."),
            Some(current) => {
                let face = self.face(current.listen, current.port);
                current.server.checks.set_origin(face.origin.clone());
                // The key comes at the first run of the listener: a phone needs it to subscribe.
                let vapid_key = self
                    .pusher
                    .key()
                    .ok()
                    .map(|key| key.public_key().to_owned());
                face.status(current.port, vapid_key)
            }
        };
        drop(running);
        self.set_status(status);
    }

    /// Puts the status in `phone_status` when it changed.
    fn set_status(&self, status: PhoneStatusValue) {
        if self.core.phone_status() != status {
            self.core.set_phone_status(status);
        }
    }

    /// Stops the listener. Returns the port that it had, or `None` when it did not run.
    fn stop(&self, hint: &str) -> Option<u16> {
        let old = self
            .running
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()?;
        let _ = old.stop.send(true);
        self.core
            .set_phone_status(status_off(hint, "The phone listener is off."));
        Some(old.port)
    }

    /// Whether the person published the listener and did not unpublish it.
    fn wants_published(&self) -> bool {
        self.core.phone_dir().join(PUBLISHED_MARK).is_file()
    }

    /// Keeps or removes the mark of [`Inner::wants_published`].
    fn set_wants_published(&self, wanted: bool) {
        let dir = self.core.phone_dir();
        let mark = dir.join(PUBLISHED_MARK);
        let result = if wanted {
            std::fs::create_dir_all(&dir).and_then(|()| std::fs::write(&mark, b""))
        } else {
            match std::fs::remove_file(&mark) {
                Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
                other => other,
            }
        };
        if let Err(error) = result {
            tracing::warn!(%error, path = %mark.display(), "the mark of the publish is not kept");
        }
    }

    /// [`Inner::withdraw`] on the runtime of the core, without a wait.
    fn withdraw_later(self: &Arc<Self>, port: u16) {
        let inner = Arc::clone(self);
        self.core
            .runtime()
            .spawn(async move { inner.withdraw(port).await });
    }

    /// Removes the mapping of `phone.https_port` when it goes to `port` of `127.0.0.1`, which no
    /// listener of neohar holds now. The mark of the publish stays, so the mapping comes back with
    /// the listener. It does nothing in a development run or with `phone.url`.
    async fn withdraw(self: &Arc<Self>, port: u16) {
        let _one = self.busy.lock().await;
        if self.dev_port.is_some() {
            return;
        }
        let phone = self.core.config().config().phone.clone();
        if !phone.url.trim().is_empty() {
            return;
        }
        // A new listener on the same port serves again: its mapping stays.
        if self.port() == Some(port) {
            return;
        }
        let env = self.core.login_env();
        let facts = tailscale::read(&self.cli, &env, phone.https_port, port).await;
        if !facts.status.published {
            return;
        }
        tracing::info!(
            https_port = phone.https_port,
            port,
            "tailscale serve removes the mapping to a port that the phone listener does not hold"
        );
        if let Err(error) = tailscale::unpublish(&facts, &env).await {
            tracing::warn!(%error, port, "tailscale serve does not remove the mapping");
        }
    }

    /// Binds the port and starts the accept loop on the runtime of the core. The origin comes
    /// with the next look at the configuration.
    fn listen(&self, listen: Listen) -> io::Result<Running> {
        let bound = std::net::TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], listen.port)))?;
        bound.set_nonblocking(true)?;
        let port = bound.local_addr()?.port();
        let (stop, stopped) = watch::channel(false);
        let server = Arc::new(http::Server {
            core: self.core.clone(),
            checks: http::Checks {
                port,
                origin: std::sync::RwLock::new(None),
                dev: listen.dev,
            },
            stop: stopped,
            fonts: Arc::clone(&self.fonts),
            machines: Arc::clone(&self.machines),
        });
        let runtime = self.core.runtime();
        let listener = {
            let _guard = runtime.enter();
            TcpListener::from_std(bound)?
        };
        runtime.spawn(http::accept(listener, Arc::clone(&server)));
        Ok(Running {
            listen,
            port,
            stop,
            server,
        })
    }

    /// Keeps facts of Tailscale when they are for the ports of the listener, and looks at the
    /// configuration again.
    fn keep(self: &Arc<Self>, facts: Option<tailscale::Facts>) {
        let facts = facts.filter(|facts| {
            self.tailscale_ports()
                .is_some_and(|(https, port)| facts.for_ports(https, port))
        });
        *self
            .tailscale
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = facts;
        self.reconcile();
    }

    /// Reads Tailscale again, when the listener uses it. While the person wants the listener
    /// published, it adds the mapping again when it is missing: the service removes it each time
    /// the listener stops.
    async fn refresh(self: &Arc<Self>) {
        let _one = self.busy.lock().await;
        let Some((https, port)) = self.tailscale_ports() else {
            self.keep(None);
            return;
        };
        let env = self.core.login_env();
        let mut facts = tailscale::read(&self.cli, &env, https, port).await;
        if facts.status.published {
            // A mapping of an earlier version of neohar, which did not keep the mark.
            if !self.wants_published() {
                self.set_wants_published(true);
            }
        } else if self.wants_published() && facts.refusal().is_none() {
            tracing::info!(
                https_port = https,
                port,
                "tailscale serve publishes the phone listener again"
            );
            if let Err(error) = tailscale::publish(&facts, &env).await {
                tracing::warn!(%error, "tailscale serve does not publish the phone listener");
            }
            facts = tailscale::read(&self.cli, &env, https, port).await;
        }
        self.keep(Some(facts));
    }

    /// `phone.publish`: the mapping of `phone.https_port` to the listener.
    async fn publish(self: &Arc<Self>) -> Result<(), RpcError> {
        let _one = self.busy.lock().await;
        if self.dev_port.is_some() {
            return Err(unsupported(
                "A development run serves 127.0.0.1 only and does not publish.",
            ));
        }
        let phone = self.core.config().config().phone.clone();
        if !phone.enabled {
            return Err(unsupported(
                "The phone listener is off. Turn on phone.enabled first.",
            ));
        }
        if !phone.url.trim().is_empty() {
            return Err(unsupported(
                "phone.url names the address of this Mac, so neohar does not use Tailscale. \
                 Empty phone.url first.",
            ));
        }
        let Some(port) = self.port() else {
            let hint = self.core.phone_status().hint;
            return Err(unsupported(
                hint.unwrap_or_else(|| "The phone listener does not run.".to_owned()),
            ));
        };
        let env = self.core.login_env();
        let facts = tailscale::read(&self.cli, &env, phone.https_port, port).await;
        if facts.status.published {
            self.set_wants_published(true);
            self.keep(Some(facts));
            return Ok(());
        }
        if let Some(refusal) = facts.refusal() {
            self.keep(Some(facts));
            return Err(unsupported(refusal));
        }
        tracing::info!(
            https_port = phone.https_port,
            port,
            "tailscale serve publishes the phone listener"
        );
        let ran = tailscale::publish(&facts, &env).await;
        let after = tailscale::read(&self.cli, &env, phone.https_port, port).await;
        let published = after.status.published;
        self.keep(Some(after));
        match ran {
            Err(error) => Err(RpcError::internal(format!(
                "tailscale serve failed: {error}"
            ))),
            Ok(()) if !published => Err(RpcError::internal(format!(
                "tailscale serve ran, but its configuration has no mapping of port {} to \
                 127.0.0.1:{port}.",
                phone.https_port
            ))),
            Ok(()) => {
                self.set_wants_published(true);
                Ok(())
            }
        }
    }

    /// `phone.unpublish`: removes the mapping of `phone.https_port`, when it goes to the listener.
    /// It also works while the listener is off.
    async fn unpublish(self: &Arc<Self>) -> Result<(), RpcError> {
        let _one = self.busy.lock().await;
        if self.dev_port.is_some() {
            return Err(unsupported(
                "A development run serves 127.0.0.1 only and does not publish.",
            ));
        }
        // The listener does not publish itself again from now on.
        self.set_wants_published(false);
        let phone = self.core.config().config().phone.clone();
        let port = self.port().unwrap_or(phone.port);
        let env = self.core.login_env();
        let facts = tailscale::read(&self.cli, &env, phone.https_port, port).await;
        if !facts.status.published {
            // No mapping of neohar: nothing to remove, and a mapping of something else stays.
            self.keep(Some(facts));
            return Ok(());
        }
        tracing::info!(
            https_port = phone.https_port,
            "tailscale serve removes the mapping of the phone listener"
        );
        let ran = tailscale::unpublish(&facts, &env).await;
        let after = tailscale::read(&self.cli, &env, phone.https_port, port).await;
        let published = after.status.published;
        self.keep(Some(after));
        match ran {
            Err(error) => Err(RpcError::internal(format!(
                "tailscale serve failed: {error}"
            ))),
            Ok(()) if published => Err(RpcError::internal(format!(
                "tailscale serve ran, but port {} still maps to the listener.",
                phone.https_port
            ))),
            Ok(()) => Ok(()),
        }
    }
}

fn status_off(hint: &str, summary: &str) -> PhoneStatusValue {
    PhoneStatusValue {
        listening: false,
        port: None,
        origin: None,
        hint: Some(hint.to_owned()),
        summary: summary.to_owned(),
        tailscale: None,
        vapid_key: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_origin_of_a_url_is_its_scheme_and_its_host() {
        assert_eq!(
            origin_of("https://Studio.example.ts.net:8443/").as_deref(),
            Some("https://studio.example.ts.net:8443")
        );
        assert_eq!(
            origin_of(" https://mac.ts.net/pair?code=x ").as_deref(),
            Some("https://mac.ts.net")
        );
        assert_eq!(origin_of("mac.ts.net"), None);
        assert_eq!(origin_of("ftp://mac.ts.net"), None);
        assert_eq!(origin_of("https://"), None);
        assert_eq!(origin_of(""), None);
    }
}
