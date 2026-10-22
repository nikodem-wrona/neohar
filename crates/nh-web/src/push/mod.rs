//! The push to the phones: the sender that the listener gives the core.
//!
//! The core decides when a push goes and what it says. The sender checks the endpoint against the
//! allowlist, encrypts the message (RFC 8291), signs a JWT with the VAPID key (RFC 8292) and posts
//! the push. A 201 is done. A 404 or a 410 is the end of the subscription. A 429, a 5xx or no
//! answer gets three tries, with a pause that doubles from 5 s.
//!
//! The VAPID key and the TLS client come at their first use, never at the start of `nh`: the same
//! binary is the hook shim, and its start time has a budget.

pub mod allow;
pub mod encrypt;
mod send;
pub mod vapid;

use std::{
    path::PathBuf,
    sync::{Arc, Mutex, OnceLock, PoisonError},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use async_trait::async_trait;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use nh_core::{
    config::ConfigStore,
    phones::{PushJob, PushOut, PushOutcome, PushSubscription, PushUrgency},
};
use rustls::ClientConfig;

use self::{encrypt::Receiver, vapid::VapidKey};

/// The tries of a push that the service could not take.
const TRIES: u32 = 3;

/// The first pause between two tries. Each pause doubles.
const PAUSE: Duration = Duration::from_secs(5);

/// How long the push service keeps a push for a phone that is off (`TTL`).
const TTL_SECS: u32 = 3600;

/// The `sub` of the JWT when `phone.contact` is empty: an address that reaches nobody
/// **[verify]**.
pub const NO_CONTACT: &str = "mailto:push@neohar.invalid";

/// The sender of the push.
pub(crate) struct Pusher {
    /// `<data>/phone`.
    dir: PathBuf,
    config: Arc<ConfigStore>,
    key: Mutex<Option<Arc<VapidKey>>>,
    tls: OnceLock<Result<Arc<ClientConfig>, String>>,
    /// The test origin of a development build (`allow`).
    test_origin: Mutex<Option<String>>,
    pause: Mutex<Duration>,
}

impl Pusher {
    pub(crate) fn new(dir: PathBuf, config: Arc<ConfigStore>) -> Self {
        let test_origin = if cfg!(debug_assertions) {
            std::env::var("NEOHAR_PUSH_TEST_ORIGIN").ok()
        } else {
            None
        };
        Self {
            dir,
            config,
            key: Mutex::new(None),
            tls: OnceLock::new(),
            test_origin: Mutex::new(test_origin),
            pause: Mutex::new(PAUSE),
        }
    }

    /// The VAPID key: the one of the file, or a new one when there is no file.
    pub(crate) fn key(&self) -> Result<Arc<VapidKey>, String> {
        let mut key = self.key.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(key) = key.as_ref() {
            return Ok(Arc::clone(key));
        }
        let loaded = Arc::new(VapidKey::load_or_create(&self.dir).map_err(|error| {
            tracing::warn!(%error, "the VAPID key of the push");
            error.to_string()
        })?);
        *key = Some(Arc::clone(&loaded));
        Ok(loaded)
    }

    /// The test origin and the first pause of a development build.
    pub(crate) fn set_test(&self, origin: &str, pause: Duration) {
        if cfg!(debug_assertions) {
            *self
                .test_origin
                .lock()
                .unwrap_or_else(PoisonError::into_inner) = Some(origin.to_owned());
            *self.pause.lock().unwrap_or_else(PoisonError::into_inner) = pause;
        }
    }

    fn test_origin(&self) -> Option<String> {
        self.test_origin
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn tls(&self) -> Result<Arc<ClientConfig>, String> {
        self.tls.get_or_init(send::tls_config).clone()
    }

    /// `phone.contact` as a `mailto:` or an `https:` address, else [`NO_CONTACT`].
    fn contact(&self) -> String {
        let contact = self.config.config().phone.contact.trim().to_owned();
        if contact.starts_with("mailto:") || contact.starts_with("https://") {
            contact
        } else if contact.contains('@') && !contact.contains(char::is_whitespace) {
            format!("mailto:{contact}")
        } else {
            NO_CONTACT.to_owned()
        }
    }

    /// One try.
    async fn post_once(
        &self,
        target: &allow::Target,
        body: &[u8],
        job: &PushJob,
    ) -> Result<send::Answer, String> {
        let key = self.key()?;
        let authorization = key.authorization(&target.origin(), &self.contact(), unix_now());
        let tls = if target.tls { Some(self.tls()?) } else { None };
        send::post(
            send::Post {
                target,
                authorization,
                body: body.to_vec(),
                ttl: TTL_SECS,
                urgency: match job.urgency {
                    PushUrgency::Normal => "normal",
                    PushUrgency::High => "high",
                },
                topic: job.topic.as_deref(),
            },
            tls,
        )
        .await
    }
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// The bytes of a key of a subscription: base64url, with or without padding.
fn key_bytes(name: &str, text: &str) -> Result<Vec<u8>, String> {
    URL_SAFE_NO_PAD
        .decode(text.trim().trim_end_matches('='))
        .map_err(|_| format!("{name} is not base64url"))
}

fn receiver(subscription: &PushSubscription) -> Result<Receiver, String> {
    Receiver::new(
        &key_bytes("p256dh", &subscription.p256dh)?,
        &key_bytes("auth", &subscription.auth)?,
    )
}

#[async_trait]
impl PushOut for Pusher {
    fn check(&self, subscription: &PushSubscription) -> Result<(), String> {
        allow::check(&subscription.endpoint, self.test_origin().as_deref())?;
        receiver(subscription).map(|_| ())
    }

    async fn send(&self, job: PushJob) -> PushOutcome {
        let failed = |reason: String| PushOutcome::Failed {
            status: None,
            reason,
        };
        let target = match allow::check(&job.subscription.endpoint, self.test_origin().as_deref()) {
            Ok(target) => target,
            Err(reason) => return failed(reason),
        };
        let body = match receiver(&job.subscription).and_then(|receiver| {
            serde_json::to_vec(&job.message)
                .map_err(|error| error.to_string())
                .and_then(|message| encrypt::encrypt(&receiver, &message))
        }) {
            Ok(body) => body,
            Err(reason) => return failed(reason),
        };
        let mut pause = *self.pause.lock().unwrap_or_else(PoisonError::into_inner);
        let mut last = failed("no try".to_owned());
        for attempt in 1..=TRIES {
            match self.post_once(&target, &body, &job).await {
                Ok(answer) if (200..300).contains(&answer.status) => {
                    tracing::debug!(device = %job.device, host = %target.host, "the push went");
                    return PushOutcome::Delivered;
                }
                Ok(answer) if matches!(answer.status, 404 | 410) => {
                    return PushOutcome::Gone {
                        status: answer.status,
                    };
                }
                Ok(answer) if answer.status == 429 || answer.status >= 500 => {
                    tracing::info!(
                        device = %job.device,
                        host = %target.host,
                        status = answer.status,
                        attempt,
                        answer = %answer.text,
                        "the push service could not take the push"
                    );
                    last = PushOutcome::Failed {
                        status: Some(answer.status),
                        reason: format!("{} answered {}", target.host, answer.status),
                    };
                }
                Ok(answer) => {
                    // The answer of the service, never the JWT or the keys.
                    tracing::warn!(
                        device = %job.device,
                        host = %target.host,
                        status = answer.status,
                        answer = %answer.text,
                        "the push service refused the push"
                    );
                    return PushOutcome::Failed {
                        status: Some(answer.status),
                        reason: refusal(&target.host, answer.status, &answer.text),
                    };
                }
                Err(reason) => {
                    tracing::info!(
                        device = %job.device,
                        host = %target.host,
                        attempt,
                        %reason,
                        "the push did not reach the push service"
                    );
                    last = failed(reason);
                }
            }
            if attempt < TRIES {
                tokio::time::sleep(pause).await;
                pause *= 2;
            }
        }
        last
    }
}

/// The reason of a refusal, for the person.
fn refusal(host: &str, status: u16, text: &str) -> String {
    if text.is_empty() {
        format!("{host} answered {status}")
    } else {
        format!("{host} answered {status}: {text}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_of_a_subscription_takes_padding_or_none() {
        assert_eq!(
            key_bytes("auth", "BTBZMqHH6r4Tts7J_aSIgg")
                .expect("b64")
                .len(),
            16
        );
        assert_eq!(
            key_bytes("auth", "BTBZMqHH6r4Tts7J_aSIgg==")
                .expect("b64")
                .len(),
            16
        );
        assert!(key_bytes("auth", "BTBZ*").is_err());
    }
}
