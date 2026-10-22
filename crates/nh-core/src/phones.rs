//! The paired phones: the offer of a pairing, the devices, and the status of the
//! listener.
//!
//! - **Offer.** `phone.pair_begin` makes one offer: a long code for the QR code and a short code
//!   that a person types. It is good for [`OFFER_TTL`] and for one phone, and [`WRONG_CODES`]
//!   wrong codes end it. The offer lives in memory only: a core that starts again has none.
//! - **Devices.** A pairing makes a device with a random token. The table `phone_devices` keeps the
//!   BLAKE3 hash of the token, never the token. The listener (crate `nh-web`) finds the device of a
//!   cookie with [`crate::CoreHandle::phone_of_token`].
//! - **Removal.** `phone.remove` removes the row and tells the listener, which closes the
//!   connections of the device ([`crate::CoreHandle::phone_revocations`]).
//! - **Status.** The listener tells the core where it listens
//!   ([`crate::CoreHandle::set_phone_status`]). The core needs the public origin for the URL of the
//!   QR code, and the doctor reports the state.
//! - **Network.** `nh-web` reads and changes Tailscale. This crate cannot depend
//!   on it, so the listener gives the core a [`PhoneNetwork`]
//!   ([`crate::CoreHandle::set_phone_network`]). `phone.publish` and `phone.unpublish` go to it,
//!   and the doctor and `phone.pair_begin` ask it to read Tailscale again first. Without one, the
//!   two methods answer `unsupported`.
//! - **Push.** A phone sends its push subscription with `phone.push_subscribe`, and the row of the
//!   device keeps it. The sender is in `nh-web`: the listener gives the core a
//!   [`PushOut`] ([`crate::CoreHandle::set_push_out`]). [`deliver`] sends one push to a phone and
//!   removes the subscription when the push service no longer knows it. [`PushMessage::notice`]
//!   makes the text of a push: the harness and the work of the run, and the reason in a few words.
//!   No text of the agent goes into a push. When a push goes is the rule of `crate::native::push`.
//! - **Window.** A phone has one row of `windows`. `phone.window` gives it, and
//!   the core makes it when there is none. While a connection of the phone is open, the device is
//!   a client of the core ([`crate::CoreHandle::attach_phone`]), so the window is present. A removal
//!   removes the windows of the device.
//! - **Visible.** The phone app calls `phone.visible` on each `visibilitychange`. The core keeps
//!   it in memory for each connection of the phone, and the end of a connection forgets it
//!   ([`crate::CoreHandle::phone_left`]). A visible phone gets no push.

use std::{
    collections::BTreeSet,
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use async_trait::async_trait;
use base64::Engine;
use nh_proto::api::{
    calls::phone::{
        PhonePairStarted, PhonePushSubscribe, PhonePushTest, PhoneRemove, PhoneRename, PhoneVisible,
    },
    events::{EventPayload, PhoneEvent, PhoneReplied},
    ids::DeviceId,
    ids::{RunId, WindowId},
    methods::{Call, Method},
    model::{AttentionReason, PairingState, PhonePairingValue, PhoneStatusValue, PhoneValue},
    projections::{Projection, SINGLETON_KEY},
    time::Timestamp,
};
use nh_store::{PhoneRow, PushRow};
use serde::Serialize;
use serde_json::Value;
use tokio::sync::broadcast;
use ulid::Ulid;

use crate::{
    CoreHandle, ErrorCode,
    actor::{Origin, Parts, Scope},
    error::RpcError,
    persist::now_ms,
    projection::ProjectionHub,
    runtime::Inner,
};

/// How long an offer waits for a phone.
pub const OFFER_TTL: Duration = Duration::from_secs(10 * 60);

/// The wrong codes that end an offer.
pub const WRONG_CODES: u8 = 5;

/// The longest name of a phone, in characters.
pub const NAME_MAX: usize = 64;

/// The letters of a short code: Crockford base32, with no `I`, `L`, `O` or `U`.
const SHORT_ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// Whether a phone may follow a projection: every projection of a window of the
/// desk, and a projection of a window only for its own phone window. `phone_pairing` holds the
/// codes, so a phone cannot follow it.
pub fn phone_may_follow(name: &str, window: WindowId) -> bool {
    match name.parse::<Projection>() {
        Ok(Projection::PhonePairing) | Err(_) => false,
        Ok(
            Projection::AgentsFor(of) | Projection::ChangesFor(of) | Projection::ChangesHead(of),
        ) => of == window,
        Ok(_) => true,
    }
}

/// The part of the listener that reads and changes Tailscale. `nh-web`
/// implements it and gives it to the core with [`crate::CoreHandle::set_phone_network`].
#[async_trait]
pub trait PhoneNetwork: Send + Sync + 'static {
    /// Reads the state of Tailscale again and updates `phone_status`. It returns when the status
    /// is up to date.
    async fn refresh(&self);

    /// `phone.publish`: adds the mapping of `tailscale serve` for the listener.
    async fn publish(&self) -> Result<(), RpcError>;

    /// `phone.unpublish`: removes the mapping of `tailscale serve` for the listener. It never
    /// removes a mapping that serves something else.
    async fn unpublish(&self) -> Result<(), RpcError>;
}

/// The sender of the push. `nh-web` implements it and gives it to the core with
/// [`crate::CoreHandle::set_push_out`].
#[async_trait]
pub trait PushOut: Send + Sync + 'static {
    /// Checks a subscription before the core keeps it: the endpoint is on the allowlist, and the
    /// keys are a P-256 key and a secret of 16 bytes. `Err` has the reason, for `invalid_params`.
    fn check(&self, subscription: &PushSubscription) -> Result<(), String>;

    /// Sends one push, and tries again when it fails. It returns when the push service took the
    /// push, refused it for good, or failed each try.
    async fn send(&self, job: PushJob) -> PushOutcome;
}

/// The push subscription of a phone, as the browser of the phone made it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PushSubscription {
    pub endpoint: String,
    /// The public key of the browser (P-256, uncompressed), base64url.
    pub p256dh: String,
    /// The authentication secret of the browser (16 bytes), base64url.
    pub auth: String,
}

impl From<PushRow> for PushSubscription {
    fn from(row: PushRow) -> Self {
        Self {
            endpoint: row.endpoint,
            p256dh: row.p256dh,
            auth: row.auth,
        }
    }
}

impl From<PushSubscription> for PushRow {
    fn from(subscription: PushSubscription) -> Self {
        Self {
            endpoint: subscription.endpoint,
            p256dh: subscription.p256dh,
            auth: subscription.auth,
        }
    }
}

/// How soon the push service must deliver a push (the header `Urgency`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PushUrgency {
    Normal,
    /// A question, an approval or a plan review.
    High,
}

/// The content of a push: the JSON that the service worker of the phone app reads.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PushMessage {
    pub title: String,
    pub body: String,
    /// A newer notification with the same tag replaces an older one on the phone.
    pub tag: String,
    /// The view that a tap opens, for example `#/run/<id>`.
    pub url: String,
    /// The count of the badge of the app, when the push sets it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub badge: Option<u32>,
}

/// The longest work of a run in the title of a push, in characters.
pub const WORK_MAX: usize = 60;

impl PushMessage {
    /// The push of a run that needs the person. The title is the harness and the work
    /// of the run ("Claude Code · fix the login bug"). The body tells in a few words what the
    /// notice is about. No text of the agent, of a question or of a command goes in.
    pub fn notice(
        harness: &str,
        work: Option<&str>,
        reason: AttentionReason,
        run: Ulid,
        badge: u32,
    ) -> Self {
        let work = work
            .map(|work| {
                work.split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
                    .chars()
                    .filter(|c| !c.is_control())
                    .collect::<String>()
            })
            .filter(|work| !work.is_empty());
        let title = match work {
            Some(work) if work.chars().count() > WORK_MAX => {
                let short: String = work.chars().take(WORK_MAX - 1).collect();
                format!("{harness} · {}…", short.trim_end())
            }
            Some(work) => format!("{harness} · {work}"),
            None => harness.to_owned(),
        };
        Self {
            title,
            body: reason_text(reason).to_owned(),
            tag: run.to_string(),
            url: format!("#/run/{run}"),
            badge: Some(badge),
        }
    }

    /// The push of a notice of another machine. The title is the label of the machine
    /// and the title of the notice, which names the harness and what it waits for ("studio ·
    /// Claude Code asks for permission"). A tap opens the desk of that machine. No text of the
    /// agent goes in: the body of the notice stays out.
    pub fn machine(machine: &str, title: &str, interaction: bool, tag: &str) -> Self {
        let title: String = title
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .filter(|c| !c.is_control())
            .collect();
        let title = if title.chars().count() > WORK_MAX {
            let short: String = title.chars().take(WORK_MAX - 1).collect();
            format!("{}…", short.trim_end())
        } else {
            title
        };
        let body = if interaction {
            "Waits for you"
        } else {
            "Finished its turn"
        };
        Self {
            title: format!("{machine} · {title}"),
            body: body.to_owned(),
            tag: tag.to_owned(),
            url: format!("#/machine/{machine}"),
            badge: None,
        }
    }

    /// The push of `phone.push_test`.
    pub fn test() -> Self {
        Self {
            title: "neohar".to_owned(),
            body: "Test notification".to_owned(),
            tag: "test".to_owned(),
            url: "#/phone".to_owned(),
            badge: None,
        }
    }
}

/// What a push says about a reason.
pub fn reason_text(reason: AttentionReason) -> &'static str {
    match reason {
        AttentionReason::TurnEnded => "Finished its turn",
        AttentionReason::Question => "Asks a question",
        AttentionReason::Approval => "Waits for an approval",
        AttentionReason::PlanReview => "Waits for a plan review",
        AttentionReason::Error => "Stopped with an error",
        AttentionReason::RateLimited => "Hit a rate limit",
    }
}

/// The urgency of the push of a reason: high for what waits for an answer.
pub fn reason_urgency(reason: AttentionReason) -> PushUrgency {
    match reason {
        AttentionReason::Question | AttentionReason::Approval | AttentionReason::PlanReview => {
            PushUrgency::High
        }
        AttentionReason::TurnEnded | AttentionReason::Error | AttentionReason::RateLimited => {
            PushUrgency::Normal
        }
    }
}

/// The sender of the push, shared by the methods of the phones and the rule of the actor.
/// The listener puts its sender in it, and takes it back when it stops.
#[derive(Default)]
pub struct PushSlot(Mutex<Option<Arc<dyn PushOut>>>);

impl PushSlot {
    pub(crate) fn get(&self) -> Option<Arc<dyn PushOut>> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub(crate) fn set(&self, out: Arc<dyn PushOut>) {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner) = Some(out);
    }

    /// Takes the sender back, when it is still `out`. A sender that replaced it stays.
    pub(crate) fn release(&self, out: &Arc<dyn PushOut>) {
        let mut current = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        let ours = current
            .as_ref()
            .is_some_and(|c| std::ptr::addr_eq(Arc::as_ptr(c), Arc::as_ptr(out)));
        if ours {
            *current = None;
        }
    }
}

/// One push to one phone.
#[derive(Clone, Debug)]
pub struct PushJob {
    pub device: Ulid,
    pub subscription: PushSubscription,
    pub message: PushMessage,
    pub urgency: PushUrgency,
    /// A newer push with the same topic replaces an older one that waits in the push service. The
    /// core uses the run.
    pub topic: Option<String>,
}

/// What the push service did with a push.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PushOutcome {
    /// The push service took the push (201).
    Delivered,
    /// The push service no longer knows the subscription (404 or 410). [`deliver`] removes it.
    Gone { status: u16 },
    /// The push did not go: the service refused it, or each try failed.
    Failed {
        /// The last status of the push service, or `None` when no answer came.
        status: Option<u16>,
        reason: String,
    },
}

/// The part of the phones that is not in the store: the status of the listener, the channel of
/// the removals, the network of the listener and the sender of the push.
pub(crate) struct PhoneSide {
    status: Mutex<PhoneStatusValue>,
    revoked: broadcast::Sender<Ulid>,
    offer_ttl: Duration,
    offers: AtomicU64,
    network: Mutex<Option<Arc<dyn PhoneNetwork>>>,
    push_out: Arc<PushSlot>,
    /// `<data>/phone`.
    dir: std::path::PathBuf,
}

impl PhoneSide {
    pub(crate) fn new(offer_ttl: Duration, dir: std::path::PathBuf, push: Arc<PushSlot>) -> Self {
        Self {
            status: Mutex::new(PhoneStatusValue {
                listening: false,
                port: None,
                origin: None,
                hint: Some("The phone listener is off. Turn on phone.enabled.".to_owned()),
                summary: "The phone listener is off.".to_owned(),
                tailscale: None,
                vapid_key: None,
            }),
            revoked: broadcast::channel(16).0,
            offer_ttl,
            offers: AtomicU64::new(0),
            network: Mutex::new(None),
            push_out: push,
            dir,
        }
    }

    pub(crate) fn push_out(&self) -> Option<Arc<dyn PushOut>> {
        self.push_out.get()
    }

    pub(crate) fn network(&self) -> Option<Arc<dyn PhoneNetwork>> {
        self.network
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Reads Tailscale again, when a listener gave the core its network.
    pub(crate) async fn refresh(&self) {
        if let Some(network) = self.network() {
            network.refresh().await;
        }
    }

    pub(crate) fn status(&self) -> PhoneStatusValue {
        self.status
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Keeps the status and puts it in `phone_status`.
    pub(crate) fn set_status(&self, status: PhoneStatusValue, hub: &ProjectionHub) {
        if let Ok(json) = serde_json::to_value(&status) {
            hub.set(Projection::PhoneStatus, SINGLETON_KEY, json);
        }
        *self.status.lock().unwrap_or_else(PoisonError::into_inner) = status;
    }
}

/// One paired phone.
#[derive(Clone, Debug)]
struct Device {
    id: Ulid,
    row: PhoneRow,
}

/// The offer of a pairing.
#[derive(Clone, Debug)]
struct Offer {
    /// Tells this offer from a later one, for the timer that ends it.
    number: u64,
    state: PairingState,
    long: String,
    /// Eight letters of [`SHORT_ALPHABET`], without the dash.
    short: String,
    url: String,
    qr: String,
    expires_at: i64,
    /// How long the offer is good for, as a person reads it.
    note: String,
    wrong: u8,
    device: Option<Ulid>,
}

/// What the actor keeps of the phones.
#[derive(Clone, Debug, Default)]
pub(crate) struct Phones {
    /// The phone paired last first.
    devices: Vec<Device>,
    offer: Option<Offer>,
    /// The connections of phones whose app is open and visible: the device and the number of the
    /// connection. Memory only.
    visible: BTreeSet<(Ulid, u64)>,
}

impl Phones {
    /// The phones of the store.
    pub(crate) fn load(store: &nh_store::Store) -> Result<Self, nh_store::StoreError> {
        let devices = store
            .phones()?
            .into_iter()
            .filter_map(|row| {
                let id = Ulid::from_string(&row.device).ok()?;
                Some(Device { id, row })
            })
            .collect();
        Ok(Self {
            devices,
            offer: None,
            visible: BTreeSet::new(),
        })
    }

    fn device(&self, id: Ulid) -> Option<&Device> {
        self.devices.iter().find(|device| device.id == id)
    }

    /// Whether the device is a paired phone. A window of a phone is not a window of a desktop
    /// client.
    pub(crate) fn is_phone(&self, id: Ulid) -> bool {
        self.device(id).is_some()
    }

    /// Whether a paired phone has a push subscription: for `phone.keep_awake`.
    pub(crate) fn subscribed(&self) -> bool {
        self.devices.iter().any(|device| device.row.push.is_some())
    }

    /// The phones that get a push now: a subscription, and no connection whose app is visible.
    pub(crate) fn push_targets(&self) -> Vec<(Ulid, PushSubscription)> {
        self.devices
            .iter()
            .filter(|device| !self.visible.iter().any(|(id, _)| *id == device.id))
            .filter_map(|device| {
                let push = device.row.push.clone()?;
                Some((device.id, PushSubscription::from(push)))
            })
            .collect()
    }

    /// Notes whether the app of a connection of a phone is visible.
    pub(crate) fn set_visible(&mut self, device: Ulid, connection: u64, visible: bool) {
        if visible {
            self.visible.insert((device, connection));
        } else {
            self.visible.remove(&(device, connection));
        }
    }

    /// A paired phone with an optional push subscription, for the tests of the rule.
    #[cfg(test)]
    pub(crate) fn add_for_test(&mut self, id: Ulid, push: Option<PushSubscription>) {
        self.devices.push(Device {
            id,
            row: PhoneRow {
                device: id.to_string(),
                name: format!("phone {id}"),
                token_hash: String::new(),
                paired_at: 0,
                seen_at: None,
                push: push.map(PushRow::from),
            },
        });
    }

    fn value(&self) -> PhonePairingValue {
        let empty = |state| PhonePairingValue {
            state,
            url: None,
            code: None,
            qr: None,
            expires_at: None,
            note: None,
            device: None,
        };
        match &self.offer {
            None => empty(PairingState::None),
            Some(offer) => match offer.state {
                PairingState::Waiting => PhonePairingValue {
                    state: PairingState::Waiting,
                    url: Some(offer.url.clone()),
                    code: Some(show_short(&offer.short)),
                    qr: Some(offer.qr.clone()),
                    expires_at: Some(Timestamp(offer.expires_at)),
                    note: Some(offer.note.clone()),
                    device: None,
                },
                PairingState::Paired => PhonePairingValue {
                    device: offer.device.map(DeviceId),
                    ..empty(PairingState::Paired)
                },
                state => empty(state),
            },
        }
    }
}

fn phone_value(device: &Device) -> PhoneValue {
    PhoneValue {
        device: DeviceId(device.id),
        name: device.row.name.clone(),
        paired_at: Timestamp(device.row.paired_at),
        seen_at: device.row.seen_at.map(Timestamp),
        push: device.row.push.is_some(),
    }
}

/// Puts the phones in `phones`. `gone` is a device that was removed.
fn publish_phones(hub: &ProjectionHub, phones: &Phones, gone: Option<Ulid>) {
    if let Some(id) = gone {
        hub.remove(Projection::Phones, &id.to_string());
    }
    let mut keys = Vec::with_capacity(phones.devices.len());
    for device in &phones.devices {
        if let Ok(json) = serde_json::to_value(phone_value(device)) {
            hub.set(Projection::Phones, &device.row.device, json);
        }
        keys.push(device.row.device.clone());
    }
    hub.set_order(Projection::Phones, &keys);
}

fn publish_pairing(hub: &ProjectionHub, phones: &Phones) {
    if let Ok(json) = serde_json::to_value(phones.value()) {
        hub.set(Projection::PhonePairing, SINGLETON_KEY, json);
    }
}

/// Publishes the phones and the offer, after the start of the core.
pub(crate) fn publish_all(parts: &mut Parts<'_>) {
    publish_phones(parts.projections, &parts.state.phones, None);
    publish_pairing(parts.projections, &parts.state.phones);
}

// --- codes -----------------------------------------------------------------------------------

fn random<const N: usize>() -> Result<[u8; N], RpcError> {
    let mut bytes = [0u8; N];
    getrandom::fill(&mut bytes).map_err(|error| RpcError::internal(error.to_string()))?;
    Ok(bytes)
}

/// The BLAKE3 hash of a token, as hex: the key of a device in the store.
pub(crate) fn token_hash(token: &str) -> String {
    blake3::hash(token.as_bytes()).to_hex().to_string()
}

/// 32 random bytes in base64url without padding: the code of the QR code.
fn long_code() -> Result<String, RpcError> {
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(random::<32>()?))
}

/// Eight random letters of [`SHORT_ALPHABET`]: 40 bits. 256 is a multiple of 32, so each letter
/// is as likely as any other.
fn short_code() -> Result<String, RpcError> {
    Ok(random::<8>()?
        .iter()
        .map(|byte| char::from(SHORT_ALPHABET[usize::from(byte % 32)]))
        .collect())
}

/// `ABCD-EFGH`.
fn show_short(short: &str) -> String {
    let (head, tail) = short.split_at(short.len() / 2);
    format!("{head}-{tail}")
}

/// "The code is good for 10 minutes and for one phone.", for the time of an offer.
fn ttl_note(ttl: Duration) -> String {
    let seconds = ttl.as_secs();
    let time = match seconds {
        60 => "1 minute".to_owned(),
        s if s >= 60 && s % 60 == 0 => format!("{} minutes", s / 60),
        1 => "1 second".to_owned(),
        s => format!("{s} seconds"),
    };
    format!("The code is good for {time} and for one phone.")
}

/// The short code that a person typed, as the core keeps it: capitals, no dash and no space, and
/// the letters that look like digits as those digits. `None` when it is not a short code.
fn typed_short(text: &str) -> Option<String> {
    let mut out = String::with_capacity(8);
    for c in text.chars() {
        let c = match c.to_ascii_uppercase() {
            '-' | ' ' => continue,
            'O' => '0',
            'I' | 'L' => '1',
            other => other,
        };
        if !SHORT_ALPHABET.contains(&u8::try_from(c).ok()?) {
            return None;
        }
        out.push(c);
    }
    (out.len() == 8).then_some(out)
}

/// Compares two texts in a time that does not depend on where they differ.
fn same(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}

impl Offer {
    fn matches(&self, code: &str) -> bool {
        let long = same(code, &self.long);
        let short = typed_short(code).is_some_and(|typed| same(&typed, &self.short));
        long | short
    }

    /// Ends the offer and forgets its codes.
    fn end(&mut self, state: PairingState) {
        self.state = state;
        self.long.clear();
        self.short.clear();
        self.url.clear();
        self.qr.clear();
    }
}

/// The QR code of `text` as an SVG `data:` URL.
fn qr_data_url(text: &str) -> Result<String, RpcError> {
    let code = qrcode::QrCode::new(text.as_bytes())
        .map_err(|error| RpcError::internal(format!("the QR code: {error}")))?;
    let svg = code
        .render::<qrcode::render::svg::Color<'_>>()
        .min_dimensions(256, 256)
        .quiet_zone(true)
        .build();
    let data = base64::engine::general_purpose::STANDARD.encode(svg.as_bytes());
    Ok(format!("data:image/svg+xml;base64,{data}"))
}

/// The name of a phone: no control characters, at most [`NAME_MAX`] characters, and "Phone" when
/// nothing is left.
pub(crate) fn clean_name(name: &str) -> String {
    let clean: String = name
        .chars()
        .filter(|c| !c.is_control())
        .take(NAME_MAX)
        .collect();
    let clean = clean.trim();
    if clean.is_empty() {
        "Phone".to_owned()
    } else {
        clean.to_owned()
    }
}

// --- methods ---------------------------------------------------------------------------------

pub(crate) fn handles(method: &str) -> bool {
    matches!(
        Method::from_name(method),
        Some(
            Method::PhonePairBegin
                | Method::PhonePairCancel
                | Method::PhoneRemove
                | Method::PhoneRename
                | Method::PhonePublish
                | Method::PhoneUnpublish
                | Method::PhonePushSubscribe
                | Method::PhonePushUnsubscribe
                | Method::PhonePushTest
                | Method::PhoneVisible
        )
    )
}

fn gone(error: impl std::fmt::Display) -> RpcError {
    RpcError::internal(error.to_string())
}

fn unknown(device: Ulid) -> RpcError {
    RpcError::new(ErrorCode::NotFound, format!("no phone {device}"))
}

/// The device of a caller of scope `phone`.
fn calling_phone(origin: &Origin) -> Result<Ulid, RpcError> {
    match (origin.scope, origin.device) {
        (Scope::Phone, Some(device)) => Ok(device),
        _ => Err(RpcError::new(
            ErrorCode::PermissionDenied,
            "only a phone calls this method, for itself",
        )),
    }
}

pub(crate) async fn run(inner: &Arc<Inner>, call: Call, origin: Origin) -> Result<Value, RpcError> {
    match call {
        Call::PhonePairBegin(_) => {
            let started = pair_begin(inner).await?;
            serde_json::to_value(started).map_err(gone)
        }
        Call::PhonePairCancel(_) => {
            inner
                .actor
                .update(|parts| {
                    parts.state.phones.offer = None;
                    publish_pairing(parts.projections, &parts.state.phones);
                })
                .await
                .map_err(gone)?;
            Ok(serde_json::json!({}))
        }
        // A phone has full access: a second phone can remove a lost one.
        Call::PhoneRemove(PhoneRemove { device }) => {
            remove(inner, device.0).await?;
            Ok(serde_json::json!({}))
        }
        Call::PhoneRename(PhoneRename { device, name }) => {
            rename(inner, device.0, &name).await?;
            Ok(serde_json::json!({}))
        }
        Call::PhonePublish(_) => {
            network(inner)?.publish().await?;
            serde_json::to_value(inner.phones.status()).map_err(gone)
        }
        Call::PhoneUnpublish(_) => {
            network(inner)?.unpublish().await?;
            serde_json::to_value(inner.phones.status()).map_err(gone)
        }
        Call::PhonePushSubscribe(PhonePushSubscribe {
            endpoint,
            p256dh,
            auth,
        }) => {
            let device = calling_phone(&origin)?;
            let subscription = PushSubscription {
                endpoint: endpoint.trim().to_owned(),
                p256dh: p256dh.trim().trim_end_matches('=').to_owned(),
                auth: auth.trim().trim_end_matches('=').to_owned(),
            };
            push_out(inner)?
                .check(&subscription)
                .map_err(RpcError::invalid_params)?;
            set_push(inner, device, Some(subscription)).await?;
            Ok(serde_json::json!({}))
        }
        Call::PhonePushUnsubscribe(_) => {
            let device = calling_phone(&origin)?;
            set_push(inner, device, None).await?;
            Ok(serde_json::json!({}))
        }
        Call::PhoneVisible(PhoneVisible { visible }) => {
            let device = calling_phone(&origin)?;
            let connection = origin.client;
            inner
                .actor
                .update(move |parts| {
                    if parts.state.phones.device(device).is_none() {
                        return Err(unknown(device));
                    }
                    parts.state.phones.set_visible(device, connection, visible);
                    Ok(())
                })
                .await
                .map_err(gone)??;
            Ok(serde_json::json!({}))
        }
        Call::PhonePushTest(PhonePushTest { device }) => {
            let outcome = deliver(
                inner,
                device.0,
                PushMessage::test(),
                PushUrgency::Normal,
                None,
            )
            .await?;
            match outcome {
                PushOutcome::Delivered => Ok(serde_json::json!({})),
                PushOutcome::Gone { status } => Err(RpcError::new(
                    ErrorCode::NotFound,
                    format!(
                        "The push service answered {status}: it no longer knows this phone. \
                         Turn on Notifications on the phone again."
                    ),
                )),
                PushOutcome::Failed { status, reason } => Err(match status {
                    Some(429) | Some(500..) | None => RpcError::new(
                        ErrorCode::Busy,
                        format!("The push did not go: {reason}. Try again later."),
                    ),
                    Some(_) => RpcError::internal(format!("The push did not go: {reason}.")),
                }),
            }
        }
        other => Err(RpcError::internal(format!("not a phone call: {other:?}"))),
    }
}

/// The sender of the push. A core without a listener in its process has none.
fn push_out(inner: &Inner) -> Result<Arc<dyn PushOut>, RpcError> {
    inner.phones.push_out().ok_or_else(|| {
        RpcError::new(
            ErrorCode::Unsupported,
            "No phone listener runs in the process of this core, so it sends no push.",
        )
    })
}

/// Sets or removes the push subscription of a phone, and puts the change in `phones`.
async fn set_push(
    inner: &Arc<Inner>,
    id: Ulid,
    subscription: Option<PushSubscription>,
) -> Result<(), RpcError> {
    inner
        .actor
        .update(move |mut parts| {
            let Some(device) = parts.state.phones.device(id).cloned() else {
                return Err(unknown(id));
            };
            let row: Option<PushRow> = subscription.map(PushRow::from);
            parts
                .commit(|tx| tx.set_phone_push(&device.row.device, row.as_ref()))
                .map_err(gone)?;
            if let Some(d) = parts.state.phones.devices.iter_mut().find(|d| d.id == id) {
                d.row.push = row;
            }
            publish_phones(parts.projections, &parts.state.phones, None);
            Ok(())
        })
        .await
        .map_err(gone)?
}

/// Sends one push to a paired phone. When the push service no longer knows the
/// subscription (404 or 410), the core removes it, and the row in `phones` shows push off. A phone
/// without a subscription is `not_found`.
pub(crate) async fn deliver(
    inner: &Arc<Inner>,
    device: Ulid,
    message: PushMessage,
    urgency: PushUrgency,
    topic: Option<String>,
) -> Result<PushOutcome, RpcError> {
    let out = push_out(inner)?;
    let found = inner
        .actor
        .update(move |parts| {
            parts
                .state
                .phones
                .device(device)
                .map(|d| (d.row.name.clone(), d.row.push.clone()))
        })
        .await
        .map_err(gone)?;
    let subscription = match found {
        None => return Err(unknown(device)),
        Some((name, None)) => {
            return Err(RpcError::new(
                ErrorCode::NotFound,
                format!("The phone {name} has no push subscription. Turn on Notifications on it."),
            ));
        }
        Some((_, Some(row))) => PushSubscription::from(row),
    };
    let endpoint = subscription.endpoint.clone();
    let outcome = out
        .send(PushJob {
            device,
            subscription,
            message,
            urgency,
            topic,
        })
        .await;
    if let PushOutcome::Gone { status } = outcome {
        tracing::info!(%device, status, "the push service no longer knows the phone");
        inner
            .actor
            .update(move |mut parts| drop_gone(&mut parts, device, &endpoint))
            .await
            .map_err(gone)??;
    }
    Ok(outcome)
}

/// Removes the subscription of a phone after a 404 or a 410 of its push service, and puts the
/// change in `phones`. A newer subscription that came during the push stays.
pub(crate) fn drop_gone(
    parts: &mut Parts<'_>,
    device: Ulid,
    endpoint: &str,
) -> Result<(), RpcError> {
    let Some(found) = parts.state.phones.device(device).cloned() else {
        return Ok(());
    };
    if found.row.push.as_ref().map(|p| p.endpoint.as_str()) != Some(endpoint) {
        return Ok(());
    }
    parts
        .commit(|tx| tx.set_phone_push(&found.row.device, None))
        .map_err(gone)?;
    if let Some(d) = parts
        .state
        .phones
        .devices
        .iter_mut()
        .find(|d| d.id == device)
    {
        d.row.push = None;
    }
    publish_phones(parts.projections, &parts.state.phones, None);
    Ok(())
}

/// A phone sent `agent.reply` to a run: the event `phone.replied`, for the metrics.
pub(crate) fn replied(parts: &mut Parts<'_>, device: Ulid, run: RunId) {
    let event = EventPayload::PhoneReplied(PhoneReplied {
        device: DeviceId(device),
        run,
    });
    if let Err(error) = parts.commit(|tx| tx.append(Some(&device.to_string()), &event)) {
        tracing::warn!(%device, %error, "the reply of the phone is not in the log");
    }
}

/// The network of the listener. A core without a listener in its process has none.
fn network(inner: &Inner) -> Result<Arc<dyn PhoneNetwork>, RpcError> {
    inner.phones.network().ok_or_else(|| {
        RpcError::new(
            ErrorCode::Unsupported,
            "No phone listener runs in the process of this core.",
        )
    })
}

async fn pair_begin(inner: &Arc<Inner>) -> Result<PhonePairStarted, RpcError> {
    // A person who fixed Tailscale a moment ago gets an offer at once.
    inner.phones.refresh().await;
    let status = inner.phones.status();
    // A hint says that a phone cannot reach the listener yet, so the URL of the QR code would
    // lead nowhere.
    let origin = match (status.listening, status.origin, status.hint) {
        (true, Some(origin), None) => origin,
        (_, _, hint) => {
            let reason = hint.unwrap_or_else(|| "The phone listener does not run.".to_owned());
            return Err(RpcError::new(ErrorCode::Unsupported, reason));
        }
    };
    let number = inner.phones.offers.fetch_add(1, Ordering::Relaxed) + 1;
    let long = long_code()?;
    let short = short_code()?;
    let url = format!("{}/pair?code={long}", origin.trim_end_matches('/'));
    let qr = qr_data_url(&url)?;
    let ttl = inner.phones.offer_ttl;
    let expires_at = now_ms() + i64::try_from(ttl.as_millis()).unwrap_or(i64::MAX);
    let started = PhonePairStarted {
        url: url.clone(),
        code: show_short(&short),
        expires_at: Timestamp(expires_at),
    };
    let offer = Offer {
        number,
        state: PairingState::Waiting,
        long,
        short,
        url,
        qr,
        expires_at,
        note: ttl_note(ttl),
        wrong: 0,
        device: None,
    };
    inner
        .actor
        .update(move |parts| {
            parts.state.phones.offer = Some(offer);
            publish_pairing(parts.projections, &parts.state.phones);
        })
        .await
        .map_err(gone)?;
    // The offer ends when its time runs out, if no phone took it and no new offer replaced it.
    let actor = inner.actor.clone();
    inner.runtime.spawn(async move {
        tokio::time::sleep(ttl).await;
        actor.update_detached(move |parts| {
            if let Some(offer) = parts.state.phones.offer.as_mut()
                && offer.number == number
                && offer.state == PairingState::Waiting
            {
                offer.end(PairingState::Expired);
                publish_pairing(parts.projections, &parts.state.phones);
            }
        });
    });
    Ok(started)
}

async fn remove(inner: &Arc<Inner>, id: Ulid) -> Result<(), RpcError> {
    inner
        .actor
        .update(move |mut parts| {
            let Some(device) = parts.state.phones.device(id).cloned() else {
                return Err(unknown(id));
            };
            let event = EventPayload::PhoneRemoved(PhoneEvent {
                device: DeviceId(id),
                name: device.row.name.clone(),
            });
            parts
                .commit(|tx| {
                    tx.delete_phone(&device.row.device)?;
                    tx.append(Some(&device.row.device), &event)
                })
                .map_err(gone)?;
            parts.state.phones.devices.retain(|d| d.id != id);
            parts
                .state
                .phones
                .visible
                .retain(|(device, _)| *device != id);
            publish_phones(parts.projections, &parts.state.phones, Some(id));
            crate::clients::remove_windows_of(&mut parts, id);
            Ok(())
        })
        .await
        .map_err(gone)??;
    // No receiver means no listener runs: nothing to close.
    let _ = inner.phones.revoked.send(id);
    Ok(())
}

async fn rename(inner: &Arc<Inner>, id: Ulid, name: &str) -> Result<(), RpcError> {
    if name.trim().is_empty() || name.chars().count() > NAME_MAX {
        return Err(RpcError::invalid_params(format!(
            "a name has 1 to {NAME_MAX} characters"
        )));
    }
    let name = clean_name(name);
    inner
        .actor
        .update(move |mut parts| {
            let Some(device) = parts.state.phones.device(id).cloned() else {
                return Err(unknown(id));
            };
            let event = EventPayload::PhoneRenamed(PhoneEvent {
                device: DeviceId(id),
                name: name.clone(),
            });
            parts
                .commit(|tx| {
                    tx.rename_phone(&device.row.device, &name)?;
                    tx.append(Some(&device.row.device), &event)
                })
                .map_err(gone)?;
            if let Some(device) = parts.state.phones.devices.iter_mut().find(|d| d.id == id) {
                device.row.name = name;
            }
            publish_phones(parts.projections, &parts.state.phones, None);
            Ok(())
        })
        .await
        .map_err(gone)?
}

// --- the side of the listener ----------------------------------------------------------------

/// A phone that a pairing made. The listener puts the token in the cookie of the phone and keeps
/// it nowhere else.
#[derive(Clone, Debug)]
pub struct PairedPhone {
    pub device: Ulid,
    pub name: String,
    pub token: String,
}

/// Why a pairing failed. The listener answers each one the same way, so that a guess learns
/// nothing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PairRefusal {
    /// No offer waits for a phone.
    NoOffer,
    /// The time of the offer ran out, or it had too many wrong codes.
    Expired,
    /// The code is not the code of the offer.
    Wrong,
    /// The store did not keep the device.
    Store(String),
}

/// The device of a cookie.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PhoneIdentity {
    pub device: Ulid,
    pub name: String,
}

/// What the check of a code decided.
enum Verdict {
    Take,
    Refuse(PairRefusal),
}

impl CoreHandle {
    /// Pairs a phone with the code of the offer: the long code of the QR code or the short code.
    /// On success the offer is `paired`, and the result has the token for the cookie.
    pub async fn phone_pair(&self, code: &str, name: &str) -> Result<PairedPhone, PairRefusal> {
        let code = code.trim().to_owned();
        let name = clean_name(name);
        let token: String = random::<32>()
            .map_err(|error| PairRefusal::Store(error.message))?
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        let hash = token_hash(&token);
        let id = crate::ids::new_ulid().map_err(|error| PairRefusal::Store(error.message))?;
        let kept = name.clone();
        self.inner()
            .actor
            .update(move |mut parts| {
                let now = now_ms();
                let verdict = match parts.state.phones.offer.as_mut() {
                    None => Verdict::Refuse(PairRefusal::NoOffer),
                    Some(offer) if offer.state != PairingState::Waiting => {
                        Verdict::Refuse(PairRefusal::NoOffer)
                    }
                    Some(offer) if now >= offer.expires_at => {
                        offer.end(PairingState::Expired);
                        Verdict::Refuse(PairRefusal::Expired)
                    }
                    Some(offer) if !offer.matches(&code) => {
                        offer.wrong += 1;
                        if offer.wrong >= WRONG_CODES {
                            offer.end(PairingState::Expired);
                        }
                        Verdict::Refuse(PairRefusal::Wrong)
                    }
                    Some(_) => Verdict::Take,
                };
                if let Verdict::Refuse(refusal) = verdict {
                    publish_pairing(parts.projections, &parts.state.phones);
                    return Err(refusal);
                }
                let row = PhoneRow {
                    device: id.to_string(),
                    name,
                    token_hash: hash,
                    paired_at: now,
                    seen_at: None,
                    push: None,
                };
                let event = EventPayload::PhonePaired(PhoneEvent {
                    device: DeviceId(id),
                    name: row.name.clone(),
                });
                parts
                    .commit(|tx| {
                        tx.put_phone(&row)?;
                        tx.append(Some(&row.device), &event)
                    })
                    .map_err(|error| PairRefusal::Store(error.to_string()))?;
                if let Some(offer) = parts.state.phones.offer.as_mut() {
                    offer.end(PairingState::Paired);
                    offer.device = Some(id);
                }
                parts.state.phones.devices.insert(0, Device { id, row });
                publish_phones(parts.projections, &parts.state.phones, None);
                publish_pairing(parts.projections, &parts.state.phones);
                Ok(())
            })
            .await
            .map_err(|error| PairRefusal::Store(error.to_string()))??;
        Ok(PairedPhone {
            device: id,
            name: kept,
            token,
        })
    }

    /// The paired phone whose cookie has this token, or `None`.
    pub async fn phone_of_token(&self, token: &str) -> Option<PhoneIdentity> {
        let hash = token_hash(token);
        self.inner()
            .actor
            .update(move |parts| {
                parts
                    .state
                    .phones
                    .devices
                    .iter()
                    .find(|device| same(&device.row.token_hash, &hash))
                    .map(|device| PhoneIdentity {
                        device: device.id,
                        name: device.row.name.clone(),
                    })
            })
            .await
            .ok()
            .flatten()
    }

    /// Notes a connection of a phone: the time of its last connection in `phones`.
    pub async fn phone_seen(&self, device: Ulid) {
        let _ = self
            .inner()
            .actor
            .update(move |mut parts| {
                let now = now_ms();
                let Some(found) = parts.state.phones.device(device).cloned() else {
                    return;
                };
                if parts
                    .commit(|tx| tx.see_phone(&found.row.device, now))
                    .is_err()
                {
                    return;
                }
                if let Some(d) = parts
                    .state
                    .phones
                    .devices
                    .iter_mut()
                    .find(|d| d.id == device)
                {
                    d.row.seen_at = Some(now);
                }
                publish_phones(parts.projections, &parts.state.phones, None);
            })
            .await;
    }

    /// Removes a paired phone, as `phone.remove` does, and closes its connections.
    pub async fn phone_unpair(&self, device: Ulid) -> Result<(), RpcError> {
        remove(self.inner(), device).await
    }

    /// The devices that `phone.remove` removes, from now on. The listener closes their
    /// connections.
    pub fn phone_revocations(&self) -> broadcast::Receiver<Ulid> {
        self.inner().phones.revoked.subscribe()
    }

    /// Where the listener of the phone listens (`phone_status`). The listener calls it when it
    /// starts, stops or fails.
    pub fn set_phone_status(&self, status: PhoneStatusValue) {
        let inner = self.inner();
        inner.phones.set_status(status, inner.projections.hub());
    }

    /// The status of the listener of the phone.
    pub fn phone_status(&self) -> PhoneStatusValue {
        self.inner().phones.status()
    }

    /// Gives the core the network of the listener of the phone: `phone.publish` and
    /// `phone.unpublish` go to it. A second network replaces the first.
    pub fn set_phone_network(&self, network: Arc<dyn PhoneNetwork>) {
        *self
            .inner()
            .phones
            .network
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(network);
    }

    /// Gives the core the sender of the push. A second sender replaces the first.
    pub fn set_push_out(&self, out: Arc<dyn PushOut>) {
        self.inner().phones.push_out.set(out);
    }

    /// Takes the sender back when the listener stops for good. A sender that replaced it stays.
    pub fn release_push_out(&self, out: &Arc<dyn PushOut>) {
        self.inner().phones.push_out.release(out);
    }

    /// Attaches a paired phone as a client of the core while one of its connections is open:
    /// its window is present, and the focus of the window counts for gate mode.
    /// Dropping the attachment detaches it.
    pub fn attach_phone(&self, device: Ulid, name: &str) -> crate::host::ClientAttachment {
        crate::clients::attach_phone(
            &self.inner().host,
            &self.inner().actor,
            crate::host::ClientInfo {
                device,
                name: name.to_owned(),
            },
        )
    }

    /// A connection of a phone ended: its app is no longer visible. `connection` is
    /// the number that the listener gave the connection in [`crate::Origin::phone_on`].
    pub fn phone_left(&self, device: Ulid, connection: u64) {
        self.inner().actor.update_detached(move |parts| {
            parts.state.phones.set_visible(device, connection, false);
        });
    }

    /// The directory of the files of the phones: `<data>/phone`. The key of the push is
    /// in it.
    pub fn phone_dir(&self) -> std::path::PathBuf {
        self.inner().phones.dir.clone()
    }

    /// Takes the network back when the listener stops for good. A network that replaced it stays.
    pub fn release_phone_network(&self, network: &Arc<dyn PhoneNetwork>) {
        let mut current = self
            .inner()
            .phones
            .network
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let ours = current
            .as_ref()
            .is_some_and(|c| std::ptr::addr_eq(Arc::as_ptr(c), Arc::as_ptr(network)));
        if ours {
            *current = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_short_code_has_eight_letters_of_the_alphabet_and_shows_with_a_dash() {
        let code = short_code().expect("a code");
        assert_eq!(code.len(), 8);
        assert!(code.bytes().all(|b| SHORT_ALPHABET.contains(&b)), "{code}");
        assert_eq!(show_short("ABCDEFGH"), "ABCD-EFGH");
    }

    #[test]
    fn a_typed_short_code_takes_small_letters_dashes_and_letters_like_digits() {
        assert_eq!(typed_short("abcd-efgh").as_deref(), Some("ABCDEFGH"));
        assert_eq!(typed_short("O1IL 2345").as_deref(), Some("01112345"));
        assert_eq!(typed_short("ABCDEFG"), None, "seven letters");
        assert_eq!(typed_short("ABCDEFGU"), None, "U is not in the alphabet");
        assert_eq!(typed_short("ABCD€FGH"), None);
    }

    #[test]
    fn an_offer_takes_its_long_code_or_its_short_code() {
        let offer = Offer {
            number: 1,
            state: PairingState::Waiting,
            long: "long-code-of-the-qr".to_owned(),
            short: "ABCDEFGH".to_owned(),
            url: String::new(),
            qr: String::new(),
            expires_at: 0,
            note: String::new(),
            wrong: 0,
            device: None,
        };
        assert!(offer.matches("long-code-of-the-qr"));
        assert!(offer.matches("abcd-efgh"));
        assert!(!offer.matches("long-code-of-the-qR"));
        assert!(!offer.matches("ABCDEFGJ"));
        assert!(!offer.matches(""));
    }

    #[test]
    fn a_long_code_is_base64url_of_32_bytes() {
        let code = long_code().expect("a code");
        assert_eq!(code.len(), 43);
        assert!(
            code.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        );
        assert_ne!(code, long_code().expect("a code"));
    }

    #[test]
    fn the_qr_code_is_an_svg_data_url() {
        let url = qr_data_url("https://studio.example.ts.net:8443/pair?code=abc").expect("a QR");
        assert!(url.starts_with("data:image/svg+xml;base64,"), "{url}");
    }

    #[test]
    fn the_note_of_an_offer_says_its_time() {
        assert_eq!(
            ttl_note(Duration::from_secs(600)),
            "The code is good for 10 minutes and for one phone."
        );
        assert_eq!(
            ttl_note(Duration::from_secs(60)),
            "The code is good for 1 minute and for one phone."
        );
        assert_eq!(
            ttl_note(Duration::from_secs(90)),
            "The code is good for 90 seconds and for one phone."
        );
    }

    #[test]
    fn a_name_loses_control_characters_and_has_a_limit() {
        assert_eq!(clean_name("  iPhone\u{7}  "), "iPhone");
        assert_eq!(clean_name("\n"), "Phone");
        assert_eq!(clean_name(&"x".repeat(100)).chars().count(), NAME_MAX);
    }

    #[test]
    fn a_push_has_the_harness_and_the_work_in_its_title_and_only_the_reason_in_its_body() {
        let run = Ulid::from_string("01M4F1W7EM2GN3EV6NHXZJEGGA").expect("a run");
        let push = PushMessage::notice(
            "Claude Code",
            Some("fix the\nlogin   bug"),
            AttentionReason::TurnEnded,
            run,
            3,
        );
        assert_eq!(push.title, "Claude Code · fix the login bug");
        assert_eq!(push.body, "Finished its turn");
        assert_eq!(push.tag, run.to_string());
        assert_eq!(push.url, format!("#/run/{run}"));
        assert_eq!(push.badge, Some(3));
        let json = serde_json::to_value(&push).expect("JSON");
        let keys: Vec<&str> = json
            .as_object()
            .expect("an object")
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(keys, ["badge", "body", "tag", "title", "url"]);

        let long = "x".repeat(200);
        let push = PushMessage::notice("Codex", Some(&long), AttentionReason::Question, run, 1);
        assert_eq!(
            push.title.chars().count(),
            "Codex · ".chars().count() + WORK_MAX
        );
        assert!(push.title.ends_with('…'), "{}", push.title);
        let push = PushMessage::notice("Codex", Some("  "), AttentionReason::Error, run, 1);
        assert_eq!(push.title, "Codex");
    }

    #[test]
    fn each_reason_has_a_short_text_and_an_urgency() {
        let all = [
            (
                AttentionReason::TurnEnded,
                "Finished its turn",
                PushUrgency::Normal,
            ),
            (
                AttentionReason::Question,
                "Asks a question",
                PushUrgency::High,
            ),
            (
                AttentionReason::Approval,
                "Waits for an approval",
                PushUrgency::High,
            ),
            (
                AttentionReason::PlanReview,
                "Waits for a plan review",
                PushUrgency::High,
            ),
            (
                AttentionReason::Error,
                "Stopped with an error",
                PushUrgency::Normal,
            ),
            (
                AttentionReason::RateLimited,
                "Hit a rate limit",
                PushUrgency::Normal,
            ),
        ];
        for (reason, text, urgency) in all {
            assert_eq!(reason_text(reason), text);
            assert_eq!(reason_urgency(reason), urgency);
        }
    }

    #[test]
    fn a_phone_follows_the_projections_of_a_window_and_not_the_codes() {
        let window = WindowId::new(Ulid::from(7u128));
        let other = WindowId::new(Ulid::from(8u128));
        for name in [
            "attention".to_owned(),
            "agents".to_owned(),
            "windows".to_owned(),
            "settings".to_owned(),
            "jobs".to_owned(),
            "health".to_owned(),
            "pane_text:01M4F1W7EM2GN3EV6NHXZJEGGA".to_owned(),
            "timeline:01M4F1W7EM2GN3EV6NHXZJEGGA".to_owned(),
            format!("agents_for:{window}"),
            format!("changes_for:{window}"),
            format!("changes_head:{window}"),
        ] {
            assert!(phone_may_follow(&name, window), "{name}");
        }
        for name in [
            "phone_pairing".to_owned(),
            "nonsense".to_owned(),
            format!("agents_for:{other}"),
            format!("changes_for:{other}"),
            format!("changes_head:{other}"),
        ] {
            assert!(!phone_may_follow(&name, window), "{name}");
        }
    }
}
