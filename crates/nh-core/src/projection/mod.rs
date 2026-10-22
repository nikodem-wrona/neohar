//! The projection hub.
//!
//! A projection is a keyed map plus an order that the core keeps in memory. A client subscribes
//! by name and gets a **snapshot** once and **patches** after it. This module keeps the state of
//! each projection, makes the patches, and delivers them.
//!
//! - **Versions.** Each projection has a version. It grows by one for each **flush**, and a flush
//!   makes one patch with all the changes since the last one.
//! - **Coalescing.** The core changes a projection as often as it likes, and the hub merges the
//!   changes of one window (16 ms by default) into one patch, so that a storm of agent events
//!   cannot starve the renderer. Merging keeps the last change of a key, and the last order. A
//!   value that equals the value of the key is no change, and a part of the core can set all its
//!   rows again without making a patch of rows that did not change.
//! - **Snapshots** are published through an `ArcSwap`, so that a reader that is not a subscriber
//!   (the CLI, a test) reads the last flushed state without a lock. A new subscriber gets the
//!   current state: the hub flushes the projection before it makes the snapshot.
//! - **`since`.** The hub keeps the last 1,000 patches of each projection. A client that asks
//!   for the patches since a version gets them, or `resync` when the ring does not reach back
//!   that far. After `resync` the client subscribes again without `since`.
//!
//! Per-client projections (`agents` read state, `palette`, `pane_text:*`) compute their values
//! for each subscription.

use std::{
    collections::{HashMap, VecDeque},
    sync::{
        Arc, Mutex, MutexGuard, OnceLock, PoisonError,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use arc_swap::ArcSwap;
use nh_proto::api::projections::{
    PatchOp, Projection, ProjectionData, ProjectionMessage, SINGLETON_KEY,
};
use serde_json::Value;

/// How long the changes of a projection collect before they go out as one patch.
pub const COALESCE: Duration = Duration::from_millis(16);

/// How many patches the ring of a projection keeps for `since`.
pub const RING_PATCHES: usize = 1_000;

/// Where the messages of a subscription go. `send` returns false when the receiver is gone, and
/// the subscription ends. A sink must not block: the hub calls it while it holds a lock.
pub trait MessageSink: Send + Sync + 'static {
    fn send(&self, message: Value) -> bool;
}

/// The id of a subscription.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SubscriptionId(pub u64);

/// How a subscription began.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Start {
    /// The sink got the snapshot.
    Snapshot,
    /// The sink got the patches since `since`, and it may have got none.
    Patches(usize),
    /// The sink got `resync` and is not subscribed. The client subscribes again without `since`.
    Resync,
}

/// The flushed state of a projection. A reader gets it without a lock.
#[derive(Debug, Default)]
pub struct Published {
    pub version: u64,
    pub order: Vec<String>,
    pub items: HashMap<String, Value>,
}

impl Published {
    fn data(&self) -> ProjectionData {
        ProjectionData {
            items: self
                .items
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            order: self.order.clone(),
        }
    }
}

/// A change of one key that has not been flushed.
enum Pending {
    Set(Value),
    Remove,
}

struct Batch {
    /// The version that the patch makes.
    version: u64,
    ops: Vec<PatchOp>,
}

struct Subscriber {
    id: SubscriptionId,
    sink: Box<dyn MessageSink>,
}

#[derive(Default)]
struct Inner {
    order: Vec<String>,
    items: HashMap<String, Value>,
    /// The changes since the last flush, in the order in which keys were first touched.
    pending: Vec<(String, Pending)>,
    /// Where a key is in `pending`.
    pending_at: HashMap<String, usize>,
    order_changed: bool,
    version: u64,
    ring: VecDeque<Batch>,
    subscribers: Vec<Subscriber>,
}

/// One projection.
struct Cell {
    published: ArcSwap<Published>,
    inner: Mutex<Inner>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Cell {
    fn new() -> Self {
        Self {
            published: ArcSwap::from_pointee(Published::default()),
            inner: Mutex::new(Inner::default()),
        }
    }

    fn touch(inner: &mut Inner, key: &str, change: Pending) {
        match inner.pending_at.get(key) {
            Some(&at) => inner.pending[at].1 = change,
            None => {
                inner.pending_at.insert(key.to_owned(), inner.pending.len());
                inner.pending.push((key.to_owned(), change));
            }
        }
    }

    fn set(&self, key: &str, value: Value) {
        let mut inner = lock(&self.inner);
        // A value that is the same is no change, and it makes no op in the patch.
        if inner.items.get(key) == Some(&value) {
            return;
        }
        if inner.items.insert(key.to_owned(), value.clone()).is_none() {
            inner.order.push(key.to_owned());
            inner.order_changed = true;
        }
        Self::touch(&mut inner, key, Pending::Set(value));
    }

    fn remove(&self, key: &str) {
        let mut inner = lock(&self.inner);
        if inner.items.remove(key).is_some() {
            inner.order.retain(|k| k != key);
            inner.order_changed = true;
            Self::touch(&mut inner, key, Pending::Remove);
        }
    }

    /// Puts the keys that exist in the given order. A key that is not in the list keeps its
    /// place after the listed ones. A key that does not exist is ignored.
    fn set_order(&self, keys: &[String]) {
        let mut inner = lock(&self.inner);
        let mut order: Vec<String> = Vec::with_capacity(inner.order.len());
        for key in keys {
            if inner.items.contains_key(key) && !order.contains(key) {
                order.push(key.clone());
            }
        }
        for key in &inner.order {
            if !order.contains(key) {
                order.push(key.clone());
            }
        }
        if order != inner.order {
            inner.order = order;
            inner.order_changed = true;
        }
    }

    /// Makes one patch of the changes that wait, and delivers it. Returns whether there was one.
    fn flush(&self) -> bool {
        let mut inner = lock(&self.inner);
        self.flush_inner(&mut inner)
    }

    fn flush_inner(&self, inner: &mut Inner) -> bool {
        if inner.pending.is_empty() && !inner.order_changed {
            return false;
        }
        inner.pending_at.clear();
        let mut ops: Vec<PatchOp> = std::mem::take(&mut inner.pending)
            .into_iter()
            .map(|(key, change)| match change {
                Pending::Set(value) => PatchOp::Set { key, value },
                Pending::Remove => PatchOp::Remove { key },
            })
            .collect();
        if std::mem::take(&mut inner.order_changed) {
            ops.push(PatchOp::Order {
                keys: inner.order.clone(),
            });
        }
        inner.version += 1;
        let version = inner.version;
        self.published.store(Arc::new(Published {
            version,
            order: inner.order.clone(),
            items: inner.items.clone(),
        }));
        let message = patch_message(version, &ops);
        inner.ring.push_back(Batch { version, ops });
        while inner.ring.len() > RING_PATCHES {
            inner.ring.pop_front();
        }
        inner.subscribers.retain(|s| s.sink.send(message.clone()));
        true
    }

    fn subscribe(
        &self,
        id: SubscriptionId,
        since: Option<u64>,
        sink: Box<dyn MessageSink>,
    ) -> Start {
        let mut inner = lock(&self.inner);
        // A new subscriber sees the current state, not the state of the last window.
        self.flush_inner(&mut inner);
        let version = inner.version;
        let Some(since) = since else {
            let snapshot = self.published.load();
            if sink.send(snapshot_message(&snapshot)) {
                inner.subscribers.push(Subscriber { id, sink });
            }
            return Start::Snapshot;
        };
        // The ring reaches back to the patch after `oldest_base`. A client at `since` needs the
        // patch `since + 1`.
        let covered = since <= version
            && (since == version || inner.ring.front().is_some_and(|b| b.version <= since + 1));
        if !covered {
            sink.send(resync_message());
            return Start::Resync;
        }
        let missed: Vec<&Batch> = inner.ring.iter().filter(|b| b.version > since).collect();
        let count = missed.len();
        for batch in missed {
            if !sink.send(patch_message(batch.version, &batch.ops)) {
                return Start::Patches(count);
            }
        }
        inner.subscribers.push(Subscriber { id, sink });
        Start::Patches(count)
    }

    fn unsubscribe(&self, id: SubscriptionId) -> bool {
        let mut inner = lock(&self.inner);
        let before = inner.subscribers.len();
        inner.subscribers.retain(|s| s.id != id);
        inner.subscribers.len() != before
    }
}

fn to_value(message: &ProjectionMessage) -> Value {
    serde_json::to_value(message).unwrap_or(Value::Null)
}

fn snapshot_message(published: &Published) -> Value {
    to_value(&ProjectionMessage::Snapshot {
        version: published.version,
        data: published.data(),
    })
}

fn patch_message(version: u64, ops: &[PatchOp]) -> Value {
    to_value(&ProjectionMessage::Patch {
        version,
        ops: ops.to_vec(),
    })
}

fn resync_message() -> Value {
    to_value(&ProjectionMessage::Resync)
}

/// What the hub calls after each new subscription, with the projection.
pub type OnSubscribe = Box<dyn Fn(Projection) + Send + Sync>;

/// All the projections of the core.
#[derive(Default)]
pub struct ProjectionHub {
    cells: Mutex<HashMap<Projection, Arc<Cell>>>,
    next_subscription: AtomicU64,
    on_subscribe: OnceLock<OnSubscribe>,
}

impl ProjectionHub {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    fn cell(&self, projection: Projection) -> Arc<Cell> {
        Arc::clone(
            lock(&self.cells)
                .entry(projection)
                .or_insert_with(|| Arc::new(Cell::new())),
        )
    }

    /// Sets the value of a key. A key that is new goes to the end of the order.
    pub fn set(&self, projection: Projection, key: &str, value: Value) {
        self.cell(projection).set(key, value);
    }

    /// Sets the value of the only key of a singleton projection (`palette`, `health`).
    pub fn set_singleton(&self, projection: Projection, value: Value) {
        self.cell(projection).set(SINGLETON_KEY, value);
    }

    /// Removes a key. A key that does not exist is ignored.
    pub fn remove(&self, projection: Projection, key: &str) {
        self.cell(projection).remove(key);
    }

    /// Puts the keys in a given order.
    pub fn set_order(&self, projection: Projection, keys: &[String]) {
        self.cell(projection).set_order(keys);
    }

    /// Flushes the changes of every projection now. The ticker calls it every [`COALESCE`].
    /// Returns the number of projections that made a patch.
    pub fn flush(&self) -> usize {
        let cells: Vec<Arc<Cell>> = lock(&self.cells).values().cloned().collect();
        cells.iter().filter(|cell| cell.flush()).count()
    }

    /// The last flushed state of a projection, without a lock.
    pub fn snapshot(&self, projection: Projection) -> Arc<Published> {
        self.cell(projection).published.load_full()
    }

    /// The version of the last flush of a projection, or `None` for a projection that nothing has
    /// made. Unlike [`ProjectionHub::snapshot`], it does not make the projection.
    pub fn version(&self, projection: Projection) -> Option<u64> {
        let cell = lock(&self.cells).get(&projection).cloned();
        cell.map(|cell| cell.published.load().version)
    }

    /// How many sinks follow a projection. A projection that nobody made has none.
    pub fn subscriber_count(&self, projection: Projection) -> usize {
        let cell = lock(&self.cells).get(&projection).cloned();
        cell.map_or(0, |cell| lock(&cell.inner).subscribers.len())
    }

    /// Drops a projection with its rows and its ring. The subscribers get nothing more. A
    /// projection of one object (`pane_text:<pane>`) goes when the object goes.
    pub fn forget(&self, projection: Projection) {
        lock(&self.cells).remove(&projection);
    }

    /// Subscribes a sink. Without `since` the sink gets the snapshot. With `since` it gets the
    /// patches after that version, or `resync` (see [`Start`]). No patch is lost between the
    /// first message and the first live patch.
    pub fn subscribe(
        &self,
        projection: Projection,
        since: Option<u64>,
        sink: Box<dyn MessageSink>,
    ) -> (SubscriptionId, Start) {
        let id = SubscriptionId(self.next_subscription.fetch_add(1, Ordering::Relaxed) + 1);
        let start = self.cell(projection).subscribe(id, since, sink);
        if start != Start::Resync
            && let Some(hook) = self.on_subscribe.get()
        {
            hook(projection);
        }
        (id, start)
    }

    /// Sets the hook that learns of each new subscription, after the sink got its first message.
    /// A projection that the core fills only while a client follows it (`changes:<task>`)
    /// starts its work there. The hook must not block. The first hook stays.
    pub fn on_subscribe(&self, hook: OnSubscribe) {
        let _ = self.on_subscribe.set(hook);
    }

    /// Ends a subscription. Returns whether it existed.
    pub fn unsubscribe(&self, projection: Projection, id: SubscriptionId) -> bool {
        self.cell(projection).unsubscribe(id)
    }

    /// Flushes every [`COALESCE`] for ever. Run it as a task.
    pub async fn run_ticker(self: Arc<Self>) {
        let mut interval = tokio::time::interval(COALESCE);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            interval.tick().await;
            self.flush();
        }
    }

    /// Starts the task that flushes every [`COALESCE`]. It ends with the runtime.
    pub fn spawn_ticker(
        self: &Arc<Self>,
        runtime: &tokio::runtime::Handle,
    ) -> tokio::task::JoinHandle<()> {
        runtime.spawn(Arc::clone(self).run_ticker())
    }
}

#[cfg(test)]
mod tests;
