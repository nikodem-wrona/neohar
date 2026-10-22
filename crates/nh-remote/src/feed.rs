//! The value of the projection `link`.
//!
//! The host of the client serves `link` to each window, and no core does. A [`LinkFeed`] holds the
//! value for one machine and sends a snapshot and then a patch for each change to the windows that
//! watch it. The app has one for each machine. The dev WebSocket of the tests has one for the
//! link to its daemon.

use std::sync::{Mutex, MutexGuard, PoisonError};

use nh_core::MessageSink;
use nh_proto::api::{
    model::{LinkValue, MachineState},
    time::Timestamp,
};
use serde_json::json;

/// The label of the machine of this Mac.
const LOCAL: &str = "local";

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The value of `link` for a machine, and the windows that watch it.
pub struct LinkFeed {
    machine: String,
    inner: Mutex<FeedInner>,
}

struct FeedInner {
    version: u64,
    value: LinkValue,
    subscribers: Vec<(String, Box<dyn MessageSink>)>,
    next: u64,
}

/// What the state of a link says to the person.
fn text_of(machine: &str, state: MachineState) -> String {
    match state {
        MachineState::Local => LOCAL.to_owned(),
        MachineState::Connected => machine.to_owned(),
        MachineState::Connecting => format!("{machine} · connecting"),
        MachineState::Disconnected => format!("{machine} · disconnected"),
    }
}

fn now() -> Timestamp {
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(0));
    Timestamp(millis)
}

impl LinkFeed {
    pub fn new(machine: &str) -> Self {
        let state = if machine == LOCAL {
            MachineState::Local
        } else {
            MachineState::Disconnected
        };
        Self {
            machine: machine.to_owned(),
            inner: Mutex::new(FeedInner {
                version: 1,
                value: LinkValue {
                    machine: machine.to_owned(),
                    state,
                    rtt_ms: None,
                    changed_at: now(),
                    reason: None,
                    text: text_of(machine, state),
                },
                subscribers: Vec::new(),
                next: 0,
            }),
        }
    }

    pub fn version(&self) -> u64 {
        lock(&self.inner).version
    }

    /// Starts a subscription: the sink gets a snapshot now and a patch for each change. Returns
    /// the id that `projection.unsubscribe` takes.
    pub fn subscribe(&self, sink: Box<dyn MessageSink>) -> String {
        let mut inner = lock(&self.inner);
        inner.next += 1;
        let id = format!("link#{}-{}", self.machine, inner.next);
        let snapshot = json!({
            "type": "snapshot",
            "version": inner.version,
            "data": {"items": {"_": inner.value}, "order": ["_"]},
        });
        if sink.send(snapshot) {
            inner.subscribers.push((id.clone(), sink));
        }
        id
    }

    pub fn unsubscribe(&self, id: &str) -> bool {
        let mut inner = lock(&self.inner);
        let before = inner.subscribers.len();
        inner.subscribers.retain(|(known, _)| known != id);
        inner.subscribers.len() != before
    }

    /// The state of the link changed, or its round-trip time. A value that equals the one before
    /// changes nothing. `changed_at` moves only when the state or the reason moves.
    pub fn update(&self, state: MachineState, rtt_ms: Option<f32>, reason: Option<String>) {
        let mut inner = lock(&self.inner);
        let old = &inner.value;
        if old.state == state && old.rtt_ms == rtt_ms && old.reason == reason {
            return;
        }
        let changed_at = if old.state == state && old.reason == reason {
            old.changed_at
        } else {
            now()
        };
        inner.value = LinkValue {
            machine: self.machine.clone(),
            state,
            rtt_ms,
            changed_at,
            reason,
            text: text_of(&self.machine, state),
        };
        inner.version += 1;
        let patch = json!({
            "type": "patch",
            "version": inner.version,
            "ops": [{"op": "set", "key": "_", "value": inner.value}],
        });
        inner
            .subscribers
            .retain(|(_, sink)| sink.send(patch.clone()));
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use super::*;
    use serde_json::Value;

    struct Count(Arc<AtomicUsize>, Arc<Mutex<Vec<Value>>>);

    impl MessageSink for Count {
        fn send(&self, message: Value) -> bool {
            self.0.fetch_add(1, Ordering::SeqCst);
            lock(&self.1).push(message);
            true
        }
    }

    #[test]
    fn the_link_of_a_window_has_a_snapshot_and_a_patch_for_each_change() {
        let feed = LinkFeed::new("studio");
        let (count, seen) = (
            Arc::new(AtomicUsize::new(0)),
            Arc::new(Mutex::new(Vec::new())),
        );
        let id = feed.subscribe(Box::new(Count(Arc::clone(&count), Arc::clone(&seen))));
        assert!(id.starts_with("link#studio-"));
        {
            let seen = lock(&seen);
            assert_eq!(seen[0]["type"], "snapshot");
            assert_eq!(seen[0]["data"]["items"]["_"]["machine"], "studio");
            assert_eq!(seen[0]["data"]["items"]["_"]["state"], "disconnected");
            assert_eq!(
                seen[0]["data"]["items"]["_"]["text"],
                "studio · disconnected"
            );
        }
        let v0 = feed.version();
        feed.update(MachineState::Connected, Some(14.2), None);
        assert_eq!(feed.version(), v0 + 1);
        {
            let seen = lock(&seen);
            assert_eq!(seen[1]["type"], "patch");
            assert_eq!(seen[1]["ops"][0]["value"]["text"], "studio");
            assert_eq!(seen[1]["ops"][0]["value"]["rtt_ms"], 14.2_f32);
        }
        // The same value is no change. A new round-trip time keeps `changed_at`.
        feed.update(MachineState::Connected, Some(14.2), None);
        assert_eq!(feed.version(), v0 + 1);
        let at = lock(&feed.inner).value.changed_at;
        feed.update(MachineState::Connected, Some(20.0), None);
        assert_eq!(lock(&feed.inner).value.changed_at, at);
        feed.update(
            MachineState::Disconnected,
            None,
            Some("the link ended".to_owned()),
        );
        {
            let seen = lock(&seen);
            let last = &seen[seen.len() - 1]["ops"][0]["value"];
            assert_eq!(last["reason"], "the link ended");
            assert_eq!(last["text"], "studio · disconnected");
        }
        assert!(feed.unsubscribe(&id));
        let before = count.load(Ordering::SeqCst);
        feed.update(MachineState::Connecting, None, None);
        assert_eq!(
            count.load(Ordering::SeqCst),
            before,
            "an ended subscription gets nothing"
        );
        assert!(!feed.unsubscribe(&id));
    }

    #[test]
    fn the_link_of_local_says_local() {
        let feed = LinkFeed::new(LOCAL);
        let value = lock(&feed.inner).value.clone();
        assert_eq!(value.state, MachineState::Local);
        assert_eq!(value.text, "local");
    }
}
