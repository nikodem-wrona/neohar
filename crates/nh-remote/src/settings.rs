//! The projection `settings` of a window of a remote machine.
//!
//! The sections `ui`, `theme` and `keys` belong to the client: the font must be installed where
//! the window paints, and the keys are the habits of the person. A window of a remote machine
//! shows one `settings` projection that the host merges: those sections from the local core, the
//! other sections from the machine. A [`SettingsFeed`] holds the two halves for one machine and
//! sends a snapshot and then a patch for each change of the merge. Its versions are its own: they
//! grow with each change of the merged value, whichever side made it.

use std::{
    collections::BTreeMap,
    sync::{Mutex, MutexGuard, PoisonError},
};

use async_trait::async_trait;
use nh_core::{MessageSink, RpcError};
use nh_proto::api::ids::WindowId;
use serde_json::{Value, json};

use crate::Link;

/// The sections that the client owns.
pub const CLIENT_SECTIONS: [&str; 3] = ["ui", "theme", "keys"];

/// The two cores that make the merged projection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    /// The core of this Mac.
    Local,
    /// The core of the machine of the window.
    Machine,
}

/// Whether a section belongs to the client.
pub fn is_client_section(section: &str) -> bool {
    CLIENT_SECTIONS.contains(&section)
}

/// The side that owns a section.
pub fn side_of_section(section: &str) -> Side {
    if is_client_section(section) {
        Side::Local
    } else {
        Side::Machine
    }
}

/// The side that owns a key of `config.set`, for example `ui.font_size`. The section is the part
/// before the first dot.
pub fn side_of_key(key: &str) -> Side {
    side_of_section(key.split_once('.').map_or(key, |(section, _)| section))
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// One half: the last snapshot of a core, with the patches applied.
#[derive(Default)]
struct Half {
    seen: bool,
    items: BTreeMap<String, Value>,
    order: Vec<String>,
}

impl Half {
    /// Applies a message of the projection. Returns false for `resync`: the core could not send
    /// the patches, and the caller subscribes again.
    fn apply(&mut self, message: &Value) -> bool {
        match message["type"].as_str() {
            Some("snapshot") => {
                self.items = message["data"]["items"]
                    .as_object()
                    .into_iter()
                    .flatten()
                    .map(|(key, value)| (key.clone(), value.clone()))
                    .collect();
                self.order = keys(&message["data"]["order"]);
                self.seen = true;
                true
            }
            Some("patch") => {
                for op in message["ops"].as_array().into_iter().flatten() {
                    let key = op["key"].as_str().unwrap_or_default().to_owned();
                    match op["op"].as_str() {
                        Some("set") => {
                            self.items.insert(key, op["value"].clone());
                        }
                        Some("remove") => {
                            self.items.remove(&key);
                        }
                        Some("order") => self.order = keys(&op["keys"]),
                        _ => {}
                    }
                }
                true
            }
            _ => false,
        }
    }
}

fn keys(order: &Value) -> Vec<String> {
    order
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|key| key.as_str().map(str::to_owned))
        .collect()
}

/// What the page sees.
#[derive(Clone, PartialEq, Default)]
struct Merged {
    items: BTreeMap<String, Value>,
    order: Vec<String>,
}

struct Inner {
    local: Half,
    machine: Half,
    version: u64,
    /// The merged value, once both halves have a snapshot.
    shown: Option<Merged>,
    subscribers: Vec<(String, Box<dyn MessageSink>)>,
    /// Subscribers that wait for the first merge.
    waiting: Vec<(String, Box<dyn MessageSink>)>,
    next: u64,
}

/// The merged `settings` of a window of a remote machine, and the windows that watch it.
pub struct SettingsFeed {
    machine: String,
    inner: Mutex<Inner>,
}

impl SettingsFeed {
    pub fn new(machine: &str) -> Self {
        Self {
            machine: machine.to_owned(),
            inner: Mutex::new(Inner {
                local: Half::default(),
                machine: Half::default(),
                version: 0,
                shown: None,
                subscribers: Vec::new(),
                waiting: Vec::new(),
                next: 0,
            }),
        }
    }

    /// The version of the merged value. It is 0 until both halves have a snapshot.
    pub fn version(&self) -> u64 {
        lock(&self.inner).version
    }

    /// Starts a subscription and returns the id that `projection.unsubscribe` takes. The sink gets
    /// a snapshot as soon as there is one. A `since` that is the current version gets no snapshot,
    /// as the core does. Any other `since` gets a snapshot: the feed keeps no patches.
    pub fn subscribe(&self, since: Option<u64>, sink: Box<dyn MessageSink>) -> String {
        let mut inner = lock(&self.inner);
        inner.next += 1;
        let id = format!("settings#{}-{}", self.machine, inner.next);
        match &inner.shown {
            None => inner.waiting.push((id.clone(), sink)),
            Some(shown) => {
                let current = since == Some(inner.version);
                if current || sink.send(snapshot(inner.version, shown)) {
                    inner.subscribers.push((id.clone(), sink));
                }
            }
        }
        id
    }

    /// Ends a subscription. Returns whether it existed.
    pub fn unsubscribe(&self, id: &str) -> bool {
        let mut inner = lock(&self.inner);
        let before = inner.subscribers.len() + inner.waiting.len();
        inner.subscribers.retain(|(known, _)| known != id);
        inner.waiting.retain(|(known, _)| known != id);
        inner.subscribers.len() + inner.waiting.len() != before
    }

    /// Applies a message of one core. Returns false for `resync`: the caller subscribes to that
    /// core again, and the feed keeps its last value until the new snapshot comes.
    pub fn apply(&self, side: Side, message: &Value) -> bool {
        let mut inner = lock(&self.inner);
        let half = match side {
            Side::Local => &mut inner.local,
            Side::Machine => &mut inner.machine,
        };
        if !half.apply(message) {
            return false;
        }
        if !(inner.local.seen && inner.machine.seen) {
            return true;
        }
        let merged = merge(&inner.local, &inner.machine);
        match inner.shown.clone() {
            None => {
                inner.version += 1;
                let message = snapshot(inner.version, &merged);
                inner.shown = Some(merged);
                let waiting = std::mem::take(&mut inner.waiting);
                for (id, sink) in waiting {
                    if sink.send(message.clone()) {
                        inner.subscribers.push((id, sink));
                    }
                }
            }
            Some(old) => {
                let ops = diff(&old, &merged);
                if ops.is_empty() {
                    return true;
                }
                inner.version += 1;
                let patch = json!({"type": "patch", "version": inner.version, "ops": ops});
                inner.shown = Some(merged);
                inner
                    .subscribers
                    .retain(|(_, sink)| sink.send(patch.clone()));
            }
        }
        true
    }
}

/// The sections of the machine in its order, with the client sections from the local core. A
/// client section that the machine does not list follows the others, in the order of the local
/// core.
fn merge(local: &Half, machine: &Half) -> Merged {
    let mut merged = Merged::default();
    let put = |merged: &mut Merged, section: &str| {
        let half = if is_client_section(section) {
            local
        } else {
            machine
        };
        if let Some(value) = half.items.get(section)
            && !merged.items.contains_key(section)
        {
            merged.items.insert(section.to_owned(), value.clone());
            merged.order.push(section.to_owned());
        }
    };
    for section in &machine.order {
        put(&mut merged, section);
    }
    for section in &local.order {
        if is_client_section(section) {
            put(&mut merged, section);
        }
    }
    merged
}

fn snapshot(version: u64, merged: &Merged) -> Value {
    json!({
        "type": "snapshot",
        "version": version,
        "data": {"items": merged.items, "order": merged.order},
    })
}

fn diff(old: &Merged, new: &Merged) -> Vec<Value> {
    let mut ops = Vec::new();
    for key in old.items.keys().filter(|key| !new.items.contains_key(*key)) {
        ops.push(json!({"op": "remove", "key": key}));
    }
    for (key, value) in &new.items {
        if old.items.get(key) != Some(value) {
            ops.push(json!({"op": "set", "key": key, "value": value}));
        }
    }
    if old.order != new.order {
        ops.push(json!({"op": "order", "keys": new.order}));
    }
    ops
}

/// Joins the answers of `config.get` of the two cores into one, the same way. A call with a
/// section goes to the side that owns it and needs no merge.
pub fn merge_config_values(local: &Value, machine: &Value) -> Value {
    let mut sections = serde_json::Map::new();
    for (side, source) in [(Side::Machine, machine), (Side::Local, local)] {
        for (name, value) in source["sections"].as_object().into_iter().flatten() {
            if side_of_section(name) == side {
                sections.insert(name.clone(), value.clone());
            }
        }
    }
    json!({ "sections": sections })
}

/// What the routing of `config.*` needs of a core: a call. Every [`Link`] is one, and the core in
/// the process of the app is one through the backend of the app.
#[async_trait]
pub trait Calls: Sync {
    async fn call(
        &self,
        window: Option<WindowId>,
        method: &str,
        params: Value,
    ) -> Result<Value, RpcError>;
}

#[async_trait]
impl<T: Link + ?Sized> Calls for T {
    async fn call(
        &self,
        window: Option<WindowId>,
        method: &str,
        params: Value,
    ) -> Result<Value, RpcError> {
        Link::call(self, window, method, params).await
    }
}

/// `config.get`, `config.set` and `config.reload` of a window of a remote machine. A key
/// of `ui`, `theme` or `keys` goes to the core of this Mac, and any other key to the machine.
/// `config.reload` goes to both, and `config.get` without a section joins the two answers.
/// Returns `None` for a method that this does not route.
pub async fn route_config(
    local: &dyn Calls,
    machine: &dyn Calls,
    window: Option<WindowId>,
    method: &str,
    params: &Value,
) -> Option<Result<Value, RpcError>> {
    let pick = |side| match side {
        Side::Local => local,
        Side::Machine => machine,
    };
    Some(match method {
        "config.set" => {
            let key = params["key"].as_str().unwrap_or_default();
            pick(side_of_key(key))
                .call(window, method, params.clone())
                .await
        }
        "config.get" => match params["section"].as_str() {
            Some(section) => {
                pick(side_of_section(section))
                    .call(window, method, params.clone())
                    .await
            }
            None => {
                let (here, there) = tokio::join!(
                    local.call(None, method, params.clone()),
                    machine.call(window, method, params.clone()),
                );
                match (here, there) {
                    (Ok(here), Ok(there)) => Ok(merge_config_values(&here, &there)),
                    (Err(error), _) | (_, Err(error)) => Err(error),
                }
            }
        },
        "config.reload" => {
            let (here, there) = tokio::join!(
                local.call(None, method, params.clone()),
                machine.call(window, method, params.clone()),
            );
            match (here, there) {
                (Ok(mut here), Ok(there)) => {
                    let mut errors = here["errors"].as_array().cloned().unwrap_or_default();
                    errors.extend(there["errors"].as_array().cloned().unwrap_or_default());
                    here["errors"] = Value::Array(errors);
                    Ok(here)
                }
                (Err(error), _) | (_, Err(error)) => Err(error),
            }
        }
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    struct Seen(Arc<Mutex<Vec<Value>>>);

    impl MessageSink for Seen {
        fn send(&self, message: Value) -> bool {
            lock(&self.0).push(message);
            true
        }
    }

    fn watch(feed: &SettingsFeed, since: Option<u64>) -> (String, Arc<Mutex<Vec<Value>>>) {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let id = feed.subscribe(since, Box::new(Seen(Arc::clone(&seen))));
        (id, seen)
    }

    fn section(value: &str) -> Value {
        json!({"entries": {"x": {"value": value, "source": "file"}}})
    }

    fn snap(sections: &[(&str, &str)]) -> Value {
        let items: serde_json::Map<String, Value> = sections
            .iter()
            .map(|(name, value)| ((*name).to_owned(), section(value)))
            .collect();
        let order: Vec<&str> = sections.iter().map(|(name, _)| *name).collect();
        json!({"type": "snapshot", "version": 1, "data": {"items": items, "order": order}})
    }

    fn set(key: &str, value: &str) -> Value {
        json!({"type": "patch", "version": 2, "ops": [{"op": "set", "key": key, "value": section(value)}]})
    }

    #[test]
    fn the_client_sections_come_from_the_local_core_and_the_rest_from_the_machine() {
        let feed = SettingsFeed::new("studio");
        let (_, seen) = watch(&feed, None);
        assert!(feed.apply(
            Side::Machine,
            &snap(&[("approvals", "m"), ("ui", "m"), ("keys", "m")])
        ));
        // Nothing yet: the local half is missing.
        assert!(lock(&seen).is_empty());
        assert_eq!(feed.version(), 0);
        assert!(feed.apply(
            Side::Local,
            &snap(&[("ui", "l"), ("theme", "l"), ("keys", "l")])
        ));
        let seen = lock(&seen);
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0]["type"], "snapshot");
        assert_eq!(seen[0]["version"], 1);
        let items = &seen[0]["data"]["items"];
        assert_eq!(items["approvals"], section("m"));
        assert_eq!(items["ui"], section("l"));
        assert_eq!(items["keys"], section("l"));
        // The machine does not list `theme`, so it follows the others.
        assert_eq!(items["theme"], section("l"));
        assert_eq!(
            seen[0]["data"]["order"],
            json!(["approvals", "ui", "keys", "theme"])
        );
    }

    #[test]
    fn a_patch_of_either_side_is_a_patch_of_the_merge_and_the_versions_only_grow() {
        let feed = SettingsFeed::new("studio");
        let (_, seen) = watch(&feed, None);
        feed.apply(Side::Machine, &snap(&[("approvals", "m"), ("ui", "m")]));
        feed.apply(Side::Local, &snap(&[("ui", "l")]));
        feed.apply(Side::Local, &set("ui", "l2"));
        feed.apply(Side::Machine, &set("approvals", "m2"));
        // A section that the machine owns and the local core sends is not shown, and neither
        // is a client section of the machine.
        feed.apply(Side::Local, &set("approvals", "x"));
        feed.apply(Side::Machine, &set("ui", "x"));
        let seen = lock(&seen);
        let versions: Vec<u64> = seen.iter().filter_map(|m| m["version"].as_u64()).collect();
        assert_eq!(versions, [1, 2, 3]);
        assert_eq!(
            seen[1]["ops"],
            json!([{"op": "set", "key": "ui", "value": section("l2")}])
        );
        assert_eq!(
            seen[2]["ops"],
            json!([{"op": "set", "key": "approvals", "value": section("m2")}])
        );
        assert_eq!(feed.version(), 3);
    }

    #[test]
    fn a_resync_of_one_side_is_reported_and_the_new_snapshot_makes_a_patch() {
        let feed = SettingsFeed::new("studio");
        let (_, seen) = watch(&feed, None);
        feed.apply(Side::Machine, &snap(&[("approvals", "m")]));
        feed.apply(Side::Local, &snap(&[("ui", "l")]));
        assert!(!feed.apply(Side::Machine, &json!({"type": "resync"})));
        assert_eq!(feed.version(), 1);
        // The snapshot after the resync brings a section that went and one that came.
        feed.apply(Side::Machine, &snap(&[("agents", "m")]));
        let seen = lock(&seen);
        assert_eq!(seen.len(), 2);
        let ops = seen[1]["ops"].as_array().unwrap();
        assert!(ops.contains(&json!({"op": "remove", "key": "approvals"})));
        assert!(ops.contains(&json!({"op": "set", "key": "agents", "value": section("m")})));
        assert!(ops.contains(&json!({"op": "order", "keys": ["agents", "ui"]})));
    }

    #[test]
    fn a_window_that_comes_later_gets_a_snapshot_and_since_the_current_version_gets_none() {
        let feed = SettingsFeed::new("studio");
        feed.apply(Side::Machine, &snap(&[("approvals", "m")]));
        feed.apply(Side::Local, &snap(&[("ui", "l")]));
        feed.apply(Side::Local, &set("ui", "l2"));
        let (late, seen) = watch(&feed, None);
        assert_eq!(lock(&seen)[0]["version"], 2);
        let (_, same) = watch(&feed, Some(2));
        assert!(lock(&same).is_empty());
        let (_, behind) = watch(&feed, Some(1));
        assert_eq!(lock(&behind)[0]["type"], "snapshot");
        feed.apply(Side::Machine, &set("approvals", "m2"));
        assert_eq!(lock(&same).len(), 1);
        assert_eq!(lock(&seen).len(), 2);
        assert!(feed.unsubscribe(&late));
        assert!(!feed.unsubscribe(&late));
        feed.apply(Side::Machine, &set("approvals", "m3"));
        assert_eq!(lock(&seen).len(), 2);
    }

    #[test]
    fn a_key_goes_to_the_side_that_owns_its_section() {
        assert_eq!(side_of_key("ui.font_size"), Side::Local);
        assert_eq!(side_of_key("theme.accent"), Side::Local);
        assert_eq!(side_of_key("keys.palette"), Side::Local);
        assert_eq!(side_of_key("approvals.claude.mode"), Side::Machine);
        assert_eq!(side_of_key("ui"), Side::Local);
        assert_eq!(side_of_key("uix.a"), Side::Machine);
    }

    #[test]
    fn config_values_of_the_two_cores_join_by_the_owner_of_each_section() {
        let local = json!({"sections": {"ui": section("l"), "approvals": section("l")}});
        let machine = json!({"sections": {"ui": section("m"), "approvals": section("m")}});
        let joined = merge_config_values(&local, &machine);
        assert_eq!(joined["sections"]["ui"], section("l"));
        assert_eq!(joined["sections"]["approvals"], section("m"));
    }
}
