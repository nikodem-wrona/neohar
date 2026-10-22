//! Subscriptions on the API socket.
//!
//! A client of the socket has no `Channel`. It calls `projection.subscribe` or `events.subscribe`
//! and gets a subscription id. The messages arrive on the same connection as notifications:
//! `projection.message` and `events.event`. A connection has at most [`MAX_SUBSCRIPTIONS`]
//! subscriptions, and the core ends them all when the connection ends.
//!
//! **The order.** The reply to a subscribe call goes out first. The notifications start after it,
//! so a client knows the id of a message when it sees the message.
//!
//! **Projections.** The hub sends the snapshot and then the patches to a sink that must not block.
//! The sink here is a queue of [`PROJECTION_QUEUE`] messages, and a task moves the messages from
//! the queue to the connection. A client that reads too slowly fills the queue. Then the hub drops
//! the sink, the task sends what the queue holds and `{type: "resync"}`, and the client subscribes
//! again.
//!
//! **Events.** The event log in the database is the source. The broadcast channel of the actor
//! only says that there is something new. A subscription task:
//!
//! 1. subscribes to the broadcast channel,
//! 2. reads the end of the log,
//! 3. reads the events after the cursor of the client from the database, page by page, and sends
//!    them,
//! 4. then takes the events from the broadcast channel and sends them.
//!
//! Step 1 comes before step 2, so no event falls between the two. The task counts the position in
//! the whole log, also for events that the filter drops. If a live event does not follow the
//! position (the channel lost events because the task was slow), the task reads the missing events
//! from the database. So a client sees every event that matches its filter, once, in order. A
//! cursor that the log cannot continue gets `events.truncated`, and the subscription ends.

use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex, MutexGuard, PoisonError,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

use nh_proto::api::{
    events::{Cursor, Event, EventFilter},
    projections::Projection,
};
use nh_store::{EventsRead, StoreError};
use serde_json::{Value, json};
use tokio::{
    sync::{broadcast, mpsc},
    task::AbortHandle,
};

use crate::{
    ErrorCode,
    error::RpcError,
    projection::{MessageSink, SubscriptionId},
    runtime::Inner,
};

/// How many subscriptions one connection may have.
pub const MAX_SUBSCRIPTIONS: usize = 64;

/// How many messages of one projection subscription wait for the connection.
pub const PROJECTION_QUEUE: usize = 1024;

/// How many events one read of the database returns.
const PAGE: usize = 500;

/// The methods that this module serves. They need the connection, so the server does not send
/// them to the state actor.
pub(crate) fn is_subscription_method(method: &str) -> bool {
    matches!(
        method,
        "projection.subscribe"
            | "projection.unsubscribe"
            | "events.subscribe"
            | "events.unsubscribe"
    )
}

/// What the core does after the reply to a subscribe call is on its way to the client.
pub(crate) type Start = Box<dyn FnOnce() + Send>;

enum Entry {
    Projection(Projection, SubscriptionId),
    Events(AbortHandle),
}

/// The subscriptions of one connection.
#[derive(Default)]
pub(crate) struct Table {
    next: AtomicU64,
    entries: Mutex<HashMap<String, Entry>>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Table {
    fn new_id(&self, prefix: &str) -> String {
        format!("{prefix}-{}", self.next.fetch_add(1, Ordering::Relaxed) + 1)
    }

    fn len(&self) -> usize {
        lock(&self.entries).len()
    }

    fn insert(&self, id: String, entry: Entry) {
        lock(&self.entries).insert(id, entry);
    }

    fn take(&self, id: &str) -> Option<Entry> {
        lock(&self.entries).remove(id)
    }

    /// Ends every subscription. The connection calls it when it ends.
    pub fn close(&self, inner: &Inner) {
        let entries: Vec<Entry> = lock(&self.entries)
            .drain()
            .map(|(_, entry)| entry)
            .collect();
        for entry in entries {
            end(inner, entry);
        }
    }
}

fn end(inner: &Inner, entry: Entry) {
    match entry {
        Entry::Projection(projection, id) => {
            inner.projections.hub().unsubscribe(projection, id);
        }
        Entry::Events(task) => task.abort(),
    }
}

/// A line of the socket: a notification of the core.
pub(crate) fn notification(method: &str, params: &Value) -> Vec<u8> {
    let mut line = serde_json::to_vec(&json!({
        "jsonrpc": "2.0",
        "method": method,
        "params": params,
    }))
    .unwrap_or_default();
    line.push(b'\n');
    line
}

fn parse<T: for<'de> serde::Deserialize<'de>>(params: Value) -> Result<T, RpcError> {
    let params = if params.is_null() { json!({}) } else { params };
    serde_json::from_value(params).map_err(|e| RpcError::invalid_params(e.to_string()))
}

fn too_many() -> RpcError {
    RpcError::new(
        ErrorCode::Busy,
        format!("a connection has at most {MAX_SUBSCRIPTIONS} subscriptions"),
    )
}

/// Runs one of the four methods. Returns the result, and the code that starts the notifications.
pub(crate) async fn call(
    inner: &Arc<Inner>,
    table: &Arc<Table>,
    out: &mpsc::Sender<Vec<u8>>,
    method: &str,
    params: Value,
) -> Result<(Value, Option<Start>), RpcError> {
    match method {
        "projection.subscribe" => {
            let params: nh_proto::api::calls::system::ProjectionSubscribe = parse(params)?;
            projection_subscribe(inner, table, out, params)
        }
        "projection.unsubscribe" => {
            let params: nh_proto::api::calls::system::ProjectionUnsubscribe = parse(params)?;
            unsubscribe(inner, table, &params.subscription, "p-")
        }
        "events.subscribe" => {
            let params: nh_proto::api::calls::system::EventsSubscribe = parse(params)?;
            events_subscribe(inner, table, out, params).await
        }
        "events.unsubscribe" => {
            let params: nh_proto::api::calls::system::EventsUnsubscribe = parse(params)?;
            unsubscribe(inner, table, &params.subscription, "e-")
        }
        other => Err(RpcError::method_not_found(other)),
    }
}

fn unsubscribe(
    inner: &Inner,
    table: &Table,
    id: &str,
    prefix: &str,
) -> Result<(Value, Option<Start>), RpcError> {
    if !id.starts_with(prefix) {
        return Err(RpcError::new(
            ErrorCode::NotFound,
            format!("no subscription {id:?}"),
        ));
    }
    let entry = table
        .take(id)
        .ok_or_else(|| RpcError::new(ErrorCode::NotFound, format!("no subscription {id:?}")))?;
    end(inner, entry);
    Ok((json!({}), None))
}

/// A sink that puts the messages in a queue. When the queue is full, the hub drops the sink.
struct QueueSink {
    queue: mpsc::Sender<Value>,
    overflow: Arc<AtomicBool>,
}

impl MessageSink for QueueSink {
    fn send(&self, message: Value) -> bool {
        match self.queue.try_send(message) {
            Ok(()) => true,
            Err(mpsc::error::TrySendError::Full(_)) => {
                self.overflow.store(true, Ordering::SeqCst);
                false
            }
            Err(mpsc::error::TrySendError::Closed(_)) => false,
        }
    }
}

fn projection_subscribe(
    inner: &Arc<Inner>,
    table: &Arc<Table>,
    out: &mpsc::Sender<Vec<u8>>,
    params: nh_proto::api::calls::system::ProjectionSubscribe,
) -> Result<(Value, Option<Start>), RpcError> {
    if table.len() >= MAX_SUBSCRIPTIONS {
        return Err(too_many());
    }
    let (queue, messages) = mpsc::channel(PROJECTION_QUEUE);
    let overflow = Arc::new(AtomicBool::new(false));
    let sink = QueueSink {
        queue,
        overflow: Arc::clone(&overflow),
    };
    let id = table.new_id("p");
    let (hub_id, started) =
        inner
            .projections
            .hub()
            .subscribe(params.name, params.since, Box::new(sink));
    // `resync` leaves no subscription behind: the hub sent the message and dropped the sink.
    if started != crate::projection::Start::Resync {
        table.insert(id.clone(), Entry::Projection(params.name, hub_id));
    }
    let out = out.clone();
    let subscription = id.clone();
    let start: Start = Box::new(move || {
        tokio::spawn(forward_projection(subscription, messages, overflow, out));
    });
    Ok((json!({ "subscription": id }), Some(start)))
}

/// Moves the messages of a subscription to the connection.
async fn forward_projection(
    subscription: String,
    mut messages: mpsc::Receiver<Value>,
    overflow: Arc<AtomicBool>,
    out: mpsc::Sender<Vec<u8>>,
) {
    while let Some(message) = messages.recv().await {
        let line = notification(
            "projection.message",
            &json!({ "subscription": subscription, "message": message }),
        );
        if out.send(line).await.is_err() {
            return;
        }
    }
    // The queue is closed and empty: the hub dropped the sink, or the client unsubscribed.
    if overflow.load(Ordering::SeqCst) {
        let line = notification(
            "projection.message",
            &json!({ "subscription": subscription, "message": { "type": "resync" } }),
        );
        let _ = out.send(line).await;
    }
}

/// Whether an event matches a filter: the same rule as the filter of the database.
pub(crate) fn matches(filter: &EventFilter, event: &Event) -> bool {
    let kind_ok = filter.kinds.is_empty()
        || filter
            .kinds
            .iter()
            .any(|wanted| match wanted.strip_suffix('*') {
                Some(prefix) => event.kind.starts_with(prefix),
                None => event.kind == *wanted,
            });
    let subject_ok = filter
        .subject
        .as_ref()
        .is_none_or(|subject| event.subject.as_ref() == Some(subject));
    kind_ok && subject_ok
}

async fn events_subscribe(
    inner: &Arc<Inner>,
    table: &Arc<Table>,
    out: &mpsc::Sender<Vec<u8>>,
    params: nh_proto::api::calls::system::EventsSubscribe,
) -> Result<(Value, Option<Start>), RpcError> {
    if table.len() >= MAX_SUBSCRIPTIONS {
        return Err(too_many());
    }
    // The channel first, then the end of the log: an event that is committed in between is in
    // both, and the position tells the task to skip it the second time.
    let live = inner.events.subscribe();
    let end = inner
        .actor
        .store(|store| store.cursor_now())
        .await
        .map_err(|e| RpcError::internal(e.to_string()))?;
    let start_at = params.cursor.unwrap_or_else(|| end.clone());
    let id = table.new_id("e");
    let task = Task {
        inner: Arc::clone(inner),
        subscription: id.clone(),
        filter: params.filter,
        position: start_at,
        out: out.clone(),
    };
    let reply = json!({ "subscription": id, "cursor": end });
    // The task waits for the signal that the reply is on its way. The entry is in the table now,
    // so an `events.unsubscribe` that comes at once finds it.
    let (go, wait) = tokio::sync::oneshot::channel::<()>();
    let handle = tokio::spawn(async move {
        if wait.await.is_ok() {
            task.run(live).await;
        }
    });
    table.insert(id, Entry::Events(handle.abort_handle()));
    let start: Start = Box::new(move || {
        let _ = go.send(());
    });
    Ok((reply, Some(start)))
}

/// Why a subscription task stops.
enum Stop {
    /// The client went away, or the core is ending.
    Closed,
    /// The log cannot continue the cursor. The client has been told.
    Truncated,
}

struct Task {
    inner: Arc<Inner>,
    subscription: String,
    filter: EventFilter,
    /// The identity of the log, and the position of the last event that the task has seen. The
    /// position counts all events, also those that the filter drops.
    position: Cursor,
    out: mpsc::Sender<Vec<u8>>,
}

impl Task {
    async fn run(mut self, mut live: broadcast::Receiver<Event>) {
        if self.catch_up().await.is_err() {
            return;
        }
        loop {
            match live.recv().await {
                Ok(event) => {
                    if event.seq <= self.position.seq {
                        // The task read this event from the database already.
                        continue;
                    }
                    if event.seq == self.position.seq + 1 {
                        self.position.seq = event.seq;
                        if matches(&self.filter, &event) && self.send_event(&event).await.is_err() {
                            return;
                        }
                    } else if self.catch_up().await.is_err() {
                        // The channel lost events. The database has them.
                        return;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    if self.catch_up().await.is_err() {
                        return;
                    }
                }
                Err(broadcast::error::RecvError::Closed) => return,
            }
        }
    }

    async fn send_event(&self, event: &Event) -> Result<(), Stop> {
        let line = notification(
            "events.event",
            &json!({ "subscription": self.subscription, "event": event }),
        );
        self.out.send(line).await.map_err(|_| Stop::Closed)
    }

    /// Reads from the database every event after the position, and sends the ones that match.
    async fn catch_up(&mut self) -> Result<(), Stop> {
        loop {
            let (position, filter) = (self.position.clone(), self.filter.clone());
            // One job on the actor thread: no event is appended between the two reads.
            let read = self
                .inner
                .actor
                .store(move |store| -> Result<_, StoreError> {
                    let read = store.events_since(&position, &filter, PAGE)?;
                    let end = store.cursor_now()?;
                    Ok((read, end))
                })
                .await;
            let (read, end) = match read {
                Ok(pair) => pair,
                Err(error) => {
                    tracing::warn!(%error, "an events subscription ended: the log cannot be read");
                    return Err(Stop::Closed);
                }
            };
            match read {
                EventsRead::Truncated { cursor, .. } => {
                    let line = notification(
                        "events.truncated",
                        &json!({ "subscription": self.subscription, "cursor": cursor }),
                    );
                    let _ = self.out.send(line).await;
                    return Err(Stop::Truncated);
                }
                EventsRead::Events {
                    events,
                    cursor,
                    more,
                } => {
                    for event in &events {
                        self.send_event(event).await?;
                    }
                    if more {
                        self.position.seq = cursor.seq;
                    } else {
                        self.position.seq = end.seq;
                        return Ok(());
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
