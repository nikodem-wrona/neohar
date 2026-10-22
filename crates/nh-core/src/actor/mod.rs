//! The state actor and the command bus.
//!
//! One OS thread owns the [`Store`] and the in-memory [`State`]. Everything that changes state
//! goes through it, one command at a time. A command is processed like this:
//!
//! 1. If the command needs the store and the store is degraded, the answer is `degraded_storage`,
//!    and nothing else happens.
//! 2. The handler of the method runs inside one transaction ([`Store::tx`]). It reads the state,
//!    writes rows, appends events, and returns a [`Mutation`] and the result.
//! 3. If the handler fails, the transaction rolls back: no row, no event, and the state, which
//!    the handler could only read, is as it was.
//! 4. If the commit works, the mutation changes the state and the projections, the events go
//!    out, and the reply goes to the caller.
//!
//! Code that is not a command and still needs the store (the pane tasks, the delivery of
//! answers) runs a closure on the actor thread with [`ActorHandle::store`]. It waits for its turn,
//! like a command.
//!
//! A handler that panics does not take the actor down. The caller gets `internal`, and the
//! transaction rolls back.

pub(crate) mod config;
pub(crate) mod domain;
pub(crate) mod jobs;
pub(crate) mod layout;
mod palette;
pub(crate) mod session;
pub(crate) mod state;

use std::{
    cell::Cell,
    collections::HashMap,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::Arc,
    thread::JoinHandle,
};

use nh_proto::api::{
    events::Event,
    ids::WindowId,
    methods::{Call, Method},
    model::StorageHealth,
};
use nh_store::{Store, StoreError};
use serde_json::{Value, json};
use tokio::sync::{broadcast, mpsc, oneshot};

pub(crate) use domain::{Ctx, Handled, Handler, HandlerTable, Mutation};
pub(crate) use state::State;

use crate::{
    ErrorCode, error::CoreError, error::RpcError, health::Health, persist::now_ms,
    projection::ProjectionHub,
};

/// Who sent a command, and from which side.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Origin {
    /// A number for the connection or the window.
    pub client: u64,
    pub scope: Scope,
    /// The pane that the token of the request belongs to. Only a caller of pane scope has one.
    pub pane: Option<ulid::Ulid>,
    /// The window of the app that sent the request. The host knows it for each webview, and the
    /// geometry lease follows it. A caller of the API socket has none.
    pub window: Option<WindowId>,
    /// The device id of the client whose window sent the request: the app that hosts
    /// the core, or a client over a link. A caller of the API socket has none.
    pub device: Option<ulid::Ulid>,
}

/// The side of a caller.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    Ui,
    Cli,
    Pane,
    Dev,
    /// A paired phone over the listener of the phone (`nh-web`). It may call only the methods
    /// that `Method::phone_may_call` allows.
    Phone,
}

impl Origin {
    pub const fn ui() -> Self {
        Self {
            client: 0,
            scope: Scope::Ui,
            pane: None,
            window: None,
            device: None,
        }
    }

    /// The same origin, for a request that came from this window of the app.
    pub const fn from_window(mut self, window: WindowId) -> Self {
        self.window = Some(window);
        self
    }

    /// The same origin, for a request from a window of this client device.
    pub const fn from_device(mut self, device: ulid::Ulid) -> Self {
        self.device = Some(device);
        self
    }

    /// A client of the API socket with no pane token.
    pub const fn cli(client: u64) -> Self {
        Self {
            client,
            scope: Scope::Cli,
            pane: None,
            window: None,
            device: None,
        }
    }

    /// A paired phone. The device is its id in `phone_devices`.
    pub const fn phone(device: ulid::Ulid) -> Self {
        Self::phone_on(device, 0)
    }

    /// A connection of a paired phone. The listener numbers its connections, and `phone.visible`
    /// holds for one connection.
    pub const fn phone_on(device: ulid::Ulid, connection: u64) -> Self {
        Self {
            client: connection,
            scope: Scope::Phone,
            pane: None,
            window: None,
            device: Some(device),
        }
    }

    /// A client of the API socket with the token of a pane.
    pub const fn pane(client: u64, pane: ulid::Ulid) -> Self {
        Self {
            client,
            scope: Scope::Pane,
            pane: Some(pane),
            window: None,
            device: None,
        }
    }
}

/// The code that sends the answer of a store job to its caller.
type Finish = Box<dyn FnOnce() + Send>;

/// A job for the store. It runs on the actor thread and returns the code that sends its answer.
/// The actor runs that code after it has told `health` about the store, so a caller that gets the
/// answer sees a `health` that is up to date.
type StoreJob = Box<dyn FnOnce(&mut Store) -> Finish + Send>;

/// What an update job can reach on the actor thread.
pub(crate) struct Parts<'a> {
    pub store: &'a mut Store,
    pub state: &'a mut State,
    pub projections: &'a ProjectionHub,
    pub events: &'a broadcast::Sender<Event>,
}

impl Parts<'_> {
    /// Runs `f` in one transaction, like a command, and sends the events that it appended after the
    /// commit.
    pub fn commit<R>(
        &mut self,
        f: impl FnOnce(&mut nh_store::Tx<'_>) -> Result<R, StoreError>,
    ) -> Result<R, StoreError> {
        let (result, events) = self.store.tx(now_ms(), f)?;
        for event in events {
            let _ = self.events.send(event);
        }
        Ok(result)
    }
}

/// A job that changes the state from outside a command. The pane runtime uses it to say that a
/// holder runs, that a child ended, and that a title changed.
type UpdateJob = Box<dyn FnOnce(Parts<'_>) -> Finish + Send>;

enum Envelope {
    Update(UpdateJob),
    Command {
        call: Box<Call>,
        origin: Origin,
        reply: oneshot::Sender<Result<Value, RpcError>>,
    },
    Store(StoreJob),
    /// The time to try a write when the store is degraded.
    Tick,
    Stop(std::sync::mpsc::Sender<()>),
}

thread_local! {
    /// True on the thread of the actor. A closure that waits for the actor from there would wait
    /// for itself.
    static ON_ACTOR: Cell<bool> = const { Cell::new(false) };
}

/// How the rest of the core talks to the actor. Cloning is cheap.
#[derive(Clone)]
pub(crate) struct ActorHandle {
    tx: mpsc::UnboundedSender<Envelope>,
    methods: Arc<Vec<Method>>,
}

impl ActorHandle {
    /// Whether the actor has a handler for `method`.
    pub fn handles(&self, method: &str) -> bool {
        Method::from_name(method).is_some_and(|m| self.methods.contains(&m))
    }

    /// Sends a command and waits for the result.
    pub async fn call(&self, call: Call, origin: Origin) -> Result<Value, RpcError> {
        let (reply, answer) = oneshot::channel();
        self.tx
            .send(Envelope::Command {
                call: Box::new(call),
                origin,
                reply,
            })
            .map_err(|_| RpcError::internal("the state actor ended"))?;
        answer
            .await
            .map_err(|_| RpcError::internal("the state actor ended"))?
    }

    /// Runs `f` on the actor thread, with the store, and waits in the calling thread. Do not call
    /// it from a handler: the handler runs on that thread.
    #[cfg_attr(
        not(test),
        allow(
            dead_code,
            reason = "a thread that is not a task can wait for the store; the tests use it"
        )
    )]
    pub fn store_blocking<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Store) -> Result<T, StoreError> + Send + 'static,
    ) -> Result<T, CoreError> {
        debug_assert!(
            !ON_ACTOR.with(Cell::get),
            "a store job from the actor thread waits for itself"
        );
        let (reply, answer) = std::sync::mpsc::channel();
        self.tx
            .send(Envelope::Store(Box::new(move |store| {
                let result = f(store);
                Box::new(move || {
                    let _ = reply.send(result);
                })
            })))
            .map_err(|_| actor_gone())?;
        Ok(answer.recv().map_err(|_| actor_gone())??)
    }

    /// Like [`ActorHandle::store_blocking`], and the task waits without holding a thread.
    pub async fn store<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Store) -> Result<T, StoreError> + Send + 'static,
    ) -> Result<T, CoreError> {
        let (reply, answer) = oneshot::channel();
        self.tx
            .send(Envelope::Store(Box::new(move |store| {
                let result = f(store);
                Box::new(move || {
                    let _ = reply.send(result);
                })
            })))
            .map_err(|_| actor_gone())?;
        Ok(answer.await.map_err(|_| actor_gone())??)
    }

    /// Runs `f` on the actor thread with the state, and waits for its answer.
    pub async fn update<T: Send + 'static>(
        &self,
        f: impl FnOnce(Parts<'_>) -> T + Send + 'static,
    ) -> Result<T, CoreError> {
        let (reply, answer) = oneshot::channel();
        self.tx
            .send(Envelope::Update(Box::new(move |parts| {
                let result = f(parts);
                Box::new(move || {
                    let _ = reply.send(result);
                })
            })))
            .map_err(|_| actor_gone())?;
        answer.await.map_err(|_| actor_gone())
    }

    /// Like [`ActorHandle::update`], and the caller does not wait. A failed send is lost: the
    /// actor has ended then.
    pub fn update_detached(&self, f: impl FnOnce(Parts<'_>) + Send + 'static) {
        let _ = self.tx.send(Envelope::Update(Box::new(move |parts| {
            f(parts);
            Box::new(|| {})
        })));
    }

    /// Tells the actor that a second has passed. It probes the store when it is degraded.
    pub fn tick(&self) {
        let _ = self.tx.send(Envelope::Tick);
    }
}

fn actor_gone() -> CoreError {
    CoreError::Runtime(std::io::Error::other("the state actor ended"))
}

/// The thread of the actor.
pub(crate) struct ActorThread {
    handle: ActorHandle,
    join: Option<JoinHandle<()>>,
}

impl ActorThread {
    pub fn handle(&self) -> ActorHandle {
        self.handle.clone()
    }

    /// Ends the actor after the commands before this call, and waits until the store is closed.
    pub fn stop(&mut self) {
        let Some(join) = self.join.take() else { return };
        let (done, wait) = std::sync::mpsc::channel();
        if self.handle.tx.send(Envelope::Stop(done)).is_ok() {
            let _ = wait.recv();
        }
        let _ = join.join();
    }
}

impl Drop for ActorThread {
    fn drop(&mut self) {
        // The thread ends when its channel closes. A caller that wants to wait calls `stop`.
        drop(self.join.take());
    }
}

/// What the actor needs to start.
pub(crate) struct ActorConfig {
    pub store: Store,
    pub state: State,
    pub handlers: HandlerTable,
    pub projections: Arc<ProjectionHub>,
    pub events: broadcast::Sender<Event>,
    pub health: Arc<Health>,
}

/// The handlers of the product.
pub(crate) fn default_handlers() -> HandlerTable {
    let mut table = HandlerTable::new();
    session::handlers(&mut table);
    config::handlers(&mut table);
    jobs::handlers(&mut table);
    layout::handlers(&mut table);
    palette::handlers(&mut table);
    crate::interaction::calls::handlers(&mut table);
    crate::policy::calls::handlers(&mut table);
    table
}

/// Starts the actor thread.
pub(crate) fn spawn(config: ActorConfig) -> std::io::Result<ActorThread> {
    let (tx, rx) = mpsc::unbounded_channel();
    let mut methods: Vec<Method> = config.handlers.keys().copied().collect();
    methods.sort_by_key(|m| m.name());
    let handle = ActorHandle {
        tx,
        methods: Arc::new(methods),
    };
    let actor = Actor {
        store: config.store,
        state: config.state,
        handlers: config.handlers,
        projections: config.projections,
        events: config.events,
        health: config.health,
        last_degraded: None,
    };
    let join = std::thread::Builder::new()
        .name("nh-state".to_owned())
        .spawn(move || actor.run(rx))?;
    Ok(ActorThread {
        handle,
        join: Some(join),
    })
}

struct Actor {
    store: Store,
    state: State,
    handlers: HashMap<Method, Handler>,
    projections: Arc<ProjectionHub>,
    events: broadcast::Sender<Event>,
    health: Arc<Health>,
    /// The reason that `health` shows for the store, so that only a change is reported.
    last_degraded: Option<String>,
}

impl Actor {
    /// Tells `health` when the store becomes degraded or works again.
    fn sync_health(&mut self) {
        let now = self.store.degraded_reason();
        if now != self.last_degraded {
            self.health.set_storage(match &now {
                Some(reason) => StorageHealth::Degraded {
                    reason: reason.clone(),
                },
                None => StorageHealth::Ok,
            });
            self.last_degraded = now;
            // The commands that write are disabled while the store is degraded.
            self.state.publish_palette(&self.projections);
        }
    }

    fn run(mut self, mut rx: mpsc::UnboundedReceiver<Envelope>) {
        ON_ACTOR.with(|flag| flag.set(true));
        while let Some(envelope) = rx.blocking_recv() {
            match envelope {
                Envelope::Command {
                    call,
                    origin,
                    reply,
                } => {
                    let result = catch_unwind(AssertUnwindSafe(|| self.process(&call, &origin)))
                        .unwrap_or_else(|_| {
                            Err(RpcError::internal(format!(
                                "{} panicked; nothing was changed",
                                call.method().name()
                            )))
                        });
                    self.sync_health();
                    let _ = reply.send(result);
                }
                Envelope::Update(job) => {
                    let finish = catch_unwind(AssertUnwindSafe(|| {
                        job(Parts {
                            store: &mut self.store,
                            state: &mut self.state,
                            projections: &self.projections,
                            events: &self.events,
                        })
                    }));
                    self.sync_health();
                    if let Ok(finish) = finish {
                        finish();
                    }
                }
                Envelope::Store(job) => {
                    // A job that panics drops its reply channel, and the caller sees an error.
                    let finish = catch_unwind(AssertUnwindSafe(|| job(&mut self.store)));
                    self.sync_health();
                    if let Ok(finish) = finish {
                        finish();
                    }
                }
                Envelope::Tick => {
                    let _ = self.store.maybe_probe(now_ms());
                    self.sync_health();
                }
                Envelope::Stop(done) => {
                    drop(self);
                    let _ = done.send(());
                    return;
                }
            }
        }
    }

    fn process(&mut self, call: &Call, origin: &Origin) -> Result<Value, RpcError> {
        let method = call.method();
        let now = now_ms();
        if method.needs_persistence() {
            // A degraded store gets a chance to come back before the command fails.
            let _ = self.store.maybe_probe(now);
            if let Some(reason) = self.store.degraded_reason() {
                return Err(degraded(&reason));
            }
        }
        let Some(handler) = self.handlers.get(&method).copied() else {
            return Err(RpcError::new(
                ErrorCode::Unsupported,
                format!("{} has no handler yet", method.name()),
            ));
        };

        let mut rejected: Option<RpcError> = None;
        let ctx = Ctx {
            state: &self.state,
            origin,
            now_ms: now,
        };
        let run = |tx: &mut nh_store::Tx<'_>| match handler(&ctx, tx, call) {
            Ok(handled) => Ok(handled),
            Err(error) => {
                // The error rolls the transaction back. The caller gets the error itself.
                rejected = Some(error);
                Err(StoreError::BadValue("rejected".to_owned()))
            }
        };
        // A command that needs no persistence reads in a transaction that works while the store
        // is degraded.
        let committed = if method.needs_persistence() {
            self.store.tx(now, run)
        } else {
            self.store
                .tx_read(now, run)
                .map(|handled| (handled, Vec::new()))
        };
        match committed {
            Ok((handled, events)) => {
                let Handled { result, mutation } = handled;
                if let Some(mutation) = mutation {
                    mutation(&mut self.state, &self.projections);
                }
                for event in events {
                    // Nobody may listen. The log has the event either way.
                    let _ = self.events.send(event);
                }
                Ok(result)
            }
            Err(StoreError::Degraded { reason }) => Err(degraded(&reason)),
            Err(error) => Err(rejected.unwrap_or_else(|| store_failure(&error))),
        }
    }
}

fn degraded(reason: &str) -> RpcError {
    let mut error = RpcError::new(
        ErrorCode::DegradedStorage,
        format!("neohar cannot save changes now: {reason}"),
    );
    error.details = json!({ "reason": reason });
    error
}

fn store_failure(error: &StoreError) -> RpcError {
    RpcError::new(ErrorCode::Internal, format!("the database failed: {error}"))
}

#[cfg(test)]
mod tests;
