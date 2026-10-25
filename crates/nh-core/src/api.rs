//! The API socket: newline-delimited JSON-RPC 2.0 on `<runtime>/<session>/api.sock`.
//!
//! The socket has mode 0600 and the server checks the UID of every peer. A line may have at most
//! [`MAX_LINE`] bytes; this fits the 4 MiB payload of the shim with its envelope.
//!
//! **Requests.** A line is one JSON object with `jsonrpc: "2.0"`, a `method`, `params` (an object)
//! and an `id`. A request without an `id` is a notification of the client and gets no reply. A
//! batch (a JSON array) is not supported. A request may have the member `pane_token`. A request of
//! an `adapter.*` method may carry the token in `params.token` as well, which is what the shim of
//! a hook sends.
//!
//! **Scope.** A connection is a client of the CLI. A request with the token of a pane is a request
//! of pane scope, and a token that no pane has is `permission_denied`.
//! [`crate::authorize`] decides which scope may call which method. A client of the CLI that runs
//! inside a pane of this core may not answer interactions or weaken the approvals
//! ([`crate::in_pane`]).
//!
//! **Replies.** A method that is not `adapter.*` runs on its own task, so a slow call does not
//! hold the connection: replies come in the order in which the calls end. A connection runs at
//! most [`MAX_IN_FLIGHT`] calls at the same time, and more get `busy`. The `adapter.*` methods run
//! in the order of the lines, because the hook shim needs its reply before the push that follows
//! it. The errors have this shape: the integer of JSON-RPC, and `data.code` with the string that
//! clients match on.
//!
//! | Method | Parameters | Result |
//! |---|---|---|
//! | `adapter.signal` | `{token, harness, event, payload?, truncated?}` | `{}` |
//! | `adapter.gate` | `{token, harness, event, payload}` | `{wait: true, interaction, lease}`; later the notification `adapter.gate_result` |
//! | `adapter.gate_resume` | `{interaction}` | `{wait: true, …}`, `{decision: true, output, idempotency_key, …}` or `{no_decision: true, reason}` |
//! | `adapter.delivery_ack` | `{interaction, idempotency_key, applied}` | `{}` |
//!
//! `adapter.gate_result` is a notification from the core: `{interaction, output, idempotency_key}`
//! when a client decided, or `{interaction, no_decision: true, reason}`.

use std::{
    collections::VecDeque,
    io,
    os::unix::{
        fs::{FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt},
        net::{UnixListener as StdListener, UnixStream as StdStream},
    },
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, OnceLock, Weak,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use nh_proto::{
    api::{common::Harness, envelope::ErrorCode},
    paths::Env,
};
use serde::Deserialize;
use serde_json::{Map, Value, json};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::{UnixListener, UnixStream},
    sync::Semaphore,
};

use crate::{
    actor::{Origin, Scope},
    authorize::authorize,
    error::{CoreError, RpcError},
    interaction::{Gates, gate::GateStart},
    pane::lock,
    runtime::{CoreHandle, Inner},
    subscriptions::{self, Table},
    tokens::PaneTokens,
};

/// The longest request line. The shim reads at most 4 MiB of payload; the rest is the envelope.
pub const MAX_LINE: usize = 5 * 1024 * 1024;

/// How many calls of one connection run at the same time.
pub const MAX_IN_FLIGHT: usize = 64;

/// How many lines for the client wait in the queue of a connection. A task that has a line for a
/// client that reads slowly waits for room.
const OUT_QUEUE: usize = 512;

/// How many received signals the core keeps for [`crate::CoreHandle::recent_signals`].
const KEEP_SIGNALS: usize = 64;

/// What the core remembers of one `adapter.signal`. The payload is not kept.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignalRecord {
    pub harness: String,
    pub event: String,
    /// The length of the request line in bytes.
    pub bytes: usize,
    /// The shim cut the payload at 4 MiB.
    pub truncated: bool,
}

/// What the connections share.
#[derive(Default)]
pub(crate) struct ApiState {
    signals: Mutex<VecDeque<SignalRecord>>,
    /// The connections of the shims that hold a hook. Without it, the gate methods answer
    /// `gate_unavailable`.
    pub gates: Option<Arc<Gates>>,
    signals_total: AtomicU64,
    gates_total: AtomicU64,
    next_conn: AtomicU64,
    /// The tokens of the panes. A request with one has pane scope.
    pub tokens: Arc<PaneTokens>,
    /// The core that runs the calls. It is set once the core exists, because the core holds this
    /// state.
    core: OnceLock<Weak<Inner>>,
}

impl ApiState {
    pub fn with_gates(gates: Arc<Gates>, tokens: Arc<PaneTokens>) -> Self {
        Self {
            gates: Some(gates),
            tokens,
            ..Self::default()
        }
    }

    /// Tells the state which core runs its calls. Only the first call counts.
    pub fn bind_core(&self, core: Weak<Inner>) {
        let _ = self.core.set(core);
    }

    fn core(&self) -> Option<Arc<Inner>> {
        self.core.get().and_then(Weak::upgrade)
    }

    /// How many `adapter.signal` and how many `adapter.gate` requests arrived.
    pub fn totals(&self) -> (u64, u64) {
        (
            self.signals_total.load(Ordering::Relaxed),
            self.gates_total.load(Ordering::Relaxed),
        )
    }

    fn new_connection(&self) -> u64 {
        self.next_conn.fetch_add(1, Ordering::Relaxed) + 1
    }

    fn connection_closed(&self, conn: u64) {
        if let Some(gates) = &self.gates {
            gates.connection_closed(conn);
        }
    }

    pub fn recent_signals(&self) -> Vec<SignalRecord> {
        lock(&self.signals).iter().cloned().collect()
    }

    fn record(&self, record: SignalRecord) {
        let mut signals = lock(&self.signals);
        if signals.len() == KEEP_SIGNALS {
            signals.pop_front();
        }
        signals.push_back(record);
    }
}

/// A bound API socket. It removes its file when it drops, unless another core has replaced it.
pub(crate) struct ApiSocket {
    listener: Option<StdListener>,
    path: PathBuf,
    inode: u64,
    /// The lock of the session (`api.lock` next to the socket). While a core holds it, no other
    /// core of the session binds, also when two start at the same moment. The system
    /// releases it when the process ends, also by a crash.
    _lock: std::fs::File,
}

impl ApiSocket {
    /// Binds `api.sock` of `session`. It creates the runtime directory (mode 0700) when it is
    /// missing.
    ///
    /// A file at the path that no one listens on is a leftover of a core that ended,
    /// and the new socket replaces it. A socket that answers belongs to a running core, so this
    /// function refuses to start a second one for the session. Two cores that start at the same
    /// moment, for example from two bridges, race for the lock of the session first: the loser
    /// gets [`CoreError::ApiSocketInUse`] and touches no file.
    pub fn bind(paths: &Env, session: &str) -> Result<Self, CoreError> {
        let path = paths.api_socket(session)?;
        let dir = paths.runtime_dir(session)?;
        let io = |path: &Path| {
            let path = path.to_owned();
            move |source| CoreError::Io { path, source }
        };
        {
            use std::os::unix::fs::DirBuilderExt;
            std::fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(&dir)
                .map_err(io(&dir))?;
        }
        let lock_path = dir.join("api.lock");
        let lock = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .mode(0o600)
            .open(&lock_path)
            .map_err(io(&lock_path))?;
        match rustix::fs::flock(&lock, rustix::fs::FlockOperation::NonBlockingLockExclusive) {
            Ok(()) => {}
            Err(rustix::io::Errno::WOULDBLOCK) => return Err(CoreError::ApiSocketInUse(path)),
            Err(errno) => return Err(io(&lock_path)(errno.into())),
        }
        remove_stale(&path)?;
        let listener = StdListener::bind(&path).map_err(|source| {
            if source.kind() == io::ErrorKind::AddrInUse {
                CoreError::ApiSocketInUse(path.clone())
            } else {
                io(&path)(source)
            }
        })?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
            .map_err(io(&path))?;
        listener.set_nonblocking(true).map_err(io(&path))?;
        let inode = std::fs::symlink_metadata(&path).map_err(io(&path))?.ino();
        Ok(Self {
            listener: Some(listener),
            path,
            inode,
            _lock: lock,
        })
    }

    /// The path of the socket file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Takes the listener to serve it. Call it inside the runtime of the core.
    pub fn take_listener(&mut self) -> io::Result<UnixListener> {
        let listener = self
            .listener
            .take()
            .ok_or_else(|| io::Error::other("the listener is served already"))?;
        UnixListener::from_std(listener)
    }
}

impl Drop for ApiSocket {
    fn drop(&mut self) {
        let ours = std::fs::symlink_metadata(&self.path).is_ok_and(|m| m.ino() == self.inode);
        if ours {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

/// Removes a socket file that no core listens on. Refuses a live socket and a file that is no
/// socket.
fn remove_stale(path: &Path) -> Result<(), CoreError> {
    let error = |source| CoreError::Io {
        path: path.to_owned(),
        source,
    };
    match std::fs::symlink_metadata(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(error(e)),
        Ok(meta) if !meta.file_type().is_socket() => Err(error(io::Error::other(
            "a file that is not a socket is in the way",
        ))),
        Ok(_) => match StdStream::connect(path) {
            Ok(_) => Err(CoreError::ApiSocketInUse(path.to_owned())),
            Err(e) if e.kind() == io::ErrorKind::ConnectionRefused => {
                std::fs::remove_file(path).map_err(error)
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(error(e)),
        },
    }
}

/// Accepts connections until the runtime ends. `uid` is the only UID that may connect.
pub(crate) async fn serve(
    listener: UnixListener,
    uid: u32,
    state: Arc<ApiState>,
    stop: Arc<tokio::sync::Notify>,
) {
    loop {
        let accepted = tokio::select! {
            // The listener closes here, before the connections do (`Core::shutdown`).
            () = stop.notified() => return,
            accepted = listener.accept() => accepted,
        };
        match accepted {
            Ok((stream, _)) => {
                let state = Arc::clone(&state);
                tokio::spawn(async move {
                    let peer = stream.peer_cred();
                    if peer.as_ref().is_ok_and(|cred| cred.uid() == uid) {
                        connection(stream, state).await;
                    } else if peer.is_err() {
                        // A probe that connects and leaves at once, for example a second launch
                        // of the app that looks for a running core.
                        tracing::debug!("a client left api.sock before its user was read");
                    } else {
                        tracing::warn!("a client of another user connected to api.sock");
                    }
                });
            }
            Err(error) => {
                // Out of descriptors, for example. A pause keeps this from spinning.
                tracing::warn!(%error, "accept on api.sock failed");
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }
    }
}

/// What a task that reads the socket tells the connection.
enum ReadItem {
    Line(Vec<u8>),
    /// The client sent a line over the limit.
    TooLong,
}

/// Reads request lines from the socket and sends them on. A line that is cut by the end of the
/// connection is dropped. The task ends when the connection ends.
async fn read_lines(
    read: tokio::net::unix::OwnedReadHalf,
    tx: tokio::sync::mpsc::UnboundedSender<ReadItem>,
) {
    let mut reader = BufReader::new(read);
    let mut line = Vec::new();
    loop {
        line.clear();
        let limit = MAX_LINE as u64 + 1;
        match (&mut reader).take(limit).read_until(b'\n', &mut line).await {
            Ok(0) | Err(_) => return,
            Ok(_) => {}
        }
        let complete = line.last() == Some(&b'\n');
        if !complete && line.len() > MAX_LINE {
            let _ = tx.send(ReadItem::TooLong);
            return;
        }
        if !complete {
            // The client closed in the middle of a line.
            return;
        }
        if tx.send(ReadItem::Line(std::mem::take(&mut line))).is_err() {
            return;
        }
    }
}

/// What a request handler knows about the connection that it serves.
pub(crate) struct Conn {
    id: u64,
    pid: Option<i32>,
    /// The lines that the core pushes to the client after a reply.
    out: Option<tokio::sync::mpsc::Sender<Vec<u8>>>,
    /// The projections and the events that the client follows.
    subs: Arc<Table>,
    /// Set once the client was found inside a pane of this core ([`crate::in_pane`]). A client
    /// that is not inside is asked again at each call that needs it: the core can still be on its
    /// way up, and the panes of the core change.
    in_pane: OnceLock<bool>,
    /// The parents and the session of the client. The core reads them once, at the first call
    /// that needs them.
    lineage: OnceLock<crate::in_pane::Lineage>,
}

impl Conn {
    /// A connection with no way to push lines. The unit tests use it.
    #[cfg(test)]
    fn none() -> Self {
        Self {
            id: 0,
            pid: None,
            out: None,
            subs: Arc::default(),
            in_pane: OnceLock::new(),
            lineage: OnceLock::new(),
        }
    }

    /// Whether the client runs inside a pane of the core. A client whose process the system does
    /// not tell is not: this check is a second line, and the CLI must work without it. Only a
    /// "yes" is kept. Before the core is up, or before it found the holders of its panes again, a
    /// "no" can be wrong, so the core asks again at the next call.
    fn in_pane(&self, state: &ApiState) -> bool {
        if let Some(known) = self.in_pane.get() {
            return *known;
        }
        let (Some(pid), Some(core)) = (self.pid, state.core()) else {
            return false;
        };
        let lineage = self
            .lineage
            .get_or_init(|| crate::in_pane::Lineage::of(pid));
        let inside = lineage.in_panes(&crate::in_pane::Panes::of(&core));
        if inside {
            let _ = self.in_pane.set(true);
        }
        inside
    }

    /// Passes the outcomes of a gate to the client as `adapter.gate_result` lines.
    fn forward(&self, start: &mut GateStart) {
        let (Some(mut rx), Some(out)) = (start.rx.take(), self.out.clone()) else {
            return;
        };
        tokio::spawn(async move {
            while let Some(outcome) = rx.recv().await {
                if out.send(outcome.line()).await.is_err() {
                    return;
                }
            }
        });
    }
}

/// One client. It sends request lines and reads the replies. The core can also push lines to it
/// (`adapter.gate_result`).
async fn connection(stream: UnixStream, state: Arc<ApiState>) {
    let conn_id = state.new_connection();
    let pid = stream.peer_cred().ok().and_then(|cred| cred.pid());
    let (read, mut write) = stream.into_split();
    let (in_tx, mut in_rx) = tokio::sync::mpsc::unbounded_channel();
    tokio::spawn(read_lines(read, in_tx));
    let (out_tx, mut out_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(OUT_QUEUE);
    let conn = Conn {
        id: conn_id,
        pid,
        out: Some(out_tx.clone()),
        subs: Arc::default(),
        in_pane: OnceLock::new(),
        lineage: OnceLock::new(),
    };
    let in_flight = Arc::new(Semaphore::new(MAX_IN_FLIGHT));
    loop {
        tokio::select! {
            item = in_rx.recv() => match item {
                None => break,
                Some(ReadItem::TooLong) => {
                    let reply = error_reply(
                        &Value::Null,
                        &RpcError::new(ErrorCode::TooLarge, "the request line is too long"),
                        -32600,
                    );
                    let _ = write.write_all(&reply).await;
                    break;
                }
                Some(ReadItem::Line(line)) => match route(&line, &state, &conn) {
                    Routed::Reply(reply) => {
                        if let Some(reply) = reply
                            && write.write_all(&reply).await.is_err()
                        {
                            break;
                        }
                    }
                    Routed::Call(call) => {
                        spawn_call(call, &state, &in_flight, &out_tx, &conn.subs);
                    }
                    Routed::Gate(gate) => {
                        let reply = gate_call(&gate, &state, &conn).await;
                        if let Some(reply) = reply
                            && write.write_all(&reply).await.is_err()
                        {
                            break;
                        }
                    }
                },
            },
            Some(bytes) = out_rx.recv() => {
                if write.write_all(&bytes).await.is_err() {
                    break;
                }
            }
        }
    }
    if let Some(core) = state.core() {
        conn.subs.close(&core);
    }
    state.connection_closed(conn_id);
}

/// A request that passed the checks of the envelope.
struct Request {
    /// `None` for a notification of the client.
    id: Option<Value>,
    method: String,
    params: Value,
    pane_token: Option<String>,
}

fn invalid_request(message: &str) -> RpcError {
    RpcError::new(ErrorCode::InvalidRequest, message)
}

/// Reads a request line. The error is the reply line to send.
fn parse_request(line: &[u8]) -> Result<Request, Vec<u8>> {
    let fail = |id: &Value, error: RpcError| error_reply(id, &error, error.code.rpc_code());
    let value: Value = serde_json::from_slice(line).map_err(|e| {
        fail(
            &Value::Null,
            RpcError::new(ErrorCode::ParseError, e.to_string()),
        )
    })?;
    let Value::Object(mut object) = value else {
        return Err(fail(
            &Value::Null,
            invalid_request("a request is a JSON object (a batch is not supported)"),
        ));
    };
    let id = object.remove("id");
    if let Some(id) = &id
        && !(id.is_number() || id.is_string() || id.is_null())
    {
        return Err(fail(
            &Value::Null,
            invalid_request("`id` must be a number, a string or null"),
        ));
    }
    let reply_id = id.clone().unwrap_or(Value::Null);
    if object.get("jsonrpc") != Some(&json!("2.0")) {
        return Err(fail(
            &reply_id,
            invalid_request("`jsonrpc` must be \"2.0\""),
        ));
    }
    let Some(Value::String(method)) = object.remove("method") else {
        return Err(fail(
            &reply_id,
            invalid_request("`method` must be a string"),
        ));
    };
    let pane_token = match object.remove("pane_token") {
        None | Some(Value::Null) => None,
        Some(Value::String(token)) => Some(token),
        Some(_) => {
            return Err(fail(
                &reply_id,
                invalid_request("`pane_token` must be a string"),
            ));
        }
    };
    let params = object.remove("params").unwrap_or(Value::Null);
    Ok(Request {
        id,
        method,
        params,
        pane_token,
    })
}

/// The scope of a request: pane scope with a valid token, else the scope of a client of the CLI.
fn origin_of(state: &ApiState, conn: &Conn, request: &Request) -> Result<Origin, RpcError> {
    let in_params = request
        .method
        .starts_with("adapter.")
        .then(|| request.params.get("token").and_then(Value::as_str))
        .flatten();
    let Some(token) = request.pane_token.as_deref().or(in_params) else {
        return Ok(Origin::cli(conn.id));
    };
    match state.tokens.resolve(token) {
        Some(pane) => Ok(Origin::pane(conn.id, pane)),
        None => Err(RpcError::new(
            ErrorCode::PermissionDenied,
            "no pane has this token",
        )),
    }
}

/// What to do with a line.
enum Routed {
    /// The reply is ready, or there is none to send.
    Reply(Option<Vec<u8>>),
    /// A call for the core. It runs on its own task.
    Call(PendingCall),
    /// A gate method of the hook shim. It waits for the core, and the connection reads no other
    /// line before the reply, so the lines of a shim keep their order.
    Gate(PendingGate),
}

struct PendingGate {
    id: Option<Value>,
    origin: Origin,
    method: String,
    params: Value,
}

struct PendingCall {
    id: Option<Value>,
    origin: Origin,
    method: String,
    params: Value,
    /// The client runs inside a pane, and the call types into this target. The call runs only
    /// when the target waits for no answer ([`crate::in_pane::waits_for_answer`]).
    input: Option<crate::in_pane::Target>,
}

fn answer(id: &Value, result: Result<Value, RpcError>) -> Vec<u8> {
    match result {
        Ok(value) => reply(id, &value),
        Err(error) => error_reply(id, &error, error.code.rpc_code()),
    }
}

/// Reads a line, finds the scope of the caller, and either answers at once (the `adapter.*`
/// methods and every error) or hands the call on.
fn route(line: &[u8], state: &ApiState, conn: &Conn) -> Routed {
    let request = match parse_request(line) {
        Ok(request) => request,
        Err(reply) => return Routed::Reply(Some(reply)),
    };
    let origin = match origin_of(state, conn, &request) {
        Ok(origin) => origin,
        Err(error) => {
            return Routed::Reply(request.id.map(|id| answer(&id, Err(error))));
        }
    };
    // A program in a pane may not answer the approvals or weaken them with the powers of the CLI.
    if matches!(origin.scope, Scope::Cli | Scope::Dev)
        && let Some(why) = crate::in_pane::refusal(&request.method, &request.params)
        && conn.in_pane(state)
    {
        tracing::warn!(method = %request.method, "a client inside a pane was refused");
        let error = RpcError::new(ErrorCode::PermissionDenied, why);
        return Routed::Reply(request.id.map(|id| answer(&id, Err(error))));
    }
    if request.method.starts_with("adapter.") {
        if let Err(error) = authorize(&request.method, &origin) {
            return Routed::Reply(request.id.map(|id| answer(&id, Err(error))));
        }
        if matches!(
            request.method.as_str(),
            "adapter.gate" | "adapter.gate_resume" | "adapter.delivery_ack"
        ) {
            return Routed::Gate(PendingGate {
                id: request.id,
                origin,
                method: request.method,
                params: request.params,
            });
        }
        let outcome = adapter(&request.method, &request.params, line.len(), state);
        return Routed::Reply(request.id.map(|id| answer(&id, outcome)));
    }
    // A program in a pane may not type into a pane that waits for an answer: a key answers the
    // dialog of the harness. The check needs the state actor, so the call does it.
    let input = if matches!(origin.scope, Scope::Cli | Scope::Dev) {
        crate::in_pane::input_target(&request.method, &request.params)
            .filter(|_| conn.in_pane(state))
    } else {
        None
    };
    Routed::Call(PendingCall {
        id: request.id,
        origin,
        method: request.method,
        params: request.params,
        input,
    })
}

/// Runs a call of the core on a task of its own and sends the reply to the connection.
fn spawn_call(
    call: PendingCall,
    state: &Arc<ApiState>,
    in_flight: &Arc<Semaphore>,
    out: &tokio::sync::mpsc::Sender<Vec<u8>>,
    subs: &Arc<Table>,
) {
    let PendingCall {
        id,
        origin,
        method,
        params,
        input,
    } = call;
    // An early refusal goes through the queue like every other reply, on a task, because the queue
    // can be full.
    let refuse = |error: RpcError| {
        if let Some(id) = &id {
            let (out, line) = (out.clone(), answer(id, Err(error)));
            tokio::spawn(async move {
                let _ = out.send(line).await;
            });
        }
    };
    let Ok(permit) = Arc::clone(in_flight).try_acquire_owned() else {
        refuse(RpcError::new(
            ErrorCode::Busy,
            format!("this connection has {MAX_IN_FLIGHT} calls in flight"),
        ));
        return;
    };
    let Some(core) = state.core() else {
        refuse(RpcError::internal("the core is not ready"));
        return;
    };
    let (out, subs) = (out.clone(), Arc::clone(subs));
    tokio::spawn(async move {
        if subscriptions::is_subscription_method(&method) {
            // These need the connection, so the state actor does not run them.
            let result = match authorize(&method, &origin) {
                Ok(_) => subscriptions::call(&core, &subs, &out, &method, params).await,
                Err(error) => Err(error),
            };
            match result {
                Ok((value, start)) => {
                    if let Some(id) = &id {
                        let _ = out.send(reply(id, &value)).await;
                    }
                    // The notifications start after the reply is in the queue.
                    if let Some(start) = start {
                        start();
                    }
                }
                Err(error) => {
                    if let Some(id) = &id {
                        let _ = out.send(answer(id, Err(error))).await;
                    }
                }
            }
        } else {
            let allowed = match input {
                Some(target) => {
                    let checked = crate::in_pane::waits_for_answer(&core, &method, target).await;
                    if checked.is_err() {
                        tracing::warn!(%method, "input from inside a pane to a waiting pane was refused");
                    }
                    checked
                }
                None => Ok(()),
            };
            let result = match allowed {
                Ok(()) => {
                    CoreHandle::from_inner(core)
                        .call_as(origin, &method, params)
                        .await
                }
                Err(error) => Err(error),
            };
            if let Some(id) = id {
                let _ = out.send(answer(&id, result)).await;
            }
        }
        drop(permit);
    });
}

/// The parameters of `adapter.signal`.
#[derive(Deserialize)]
struct SignalParams {
    token: String,
    harness: String,
    event: String,
    #[serde(default)]
    truncated: bool,
}

fn params_of<T: for<'de> Deserialize<'de>>(params: &Value) -> Result<T, RpcError> {
    let params = if params.is_null() {
        Value::Object(Map::new())
    } else {
        params.clone()
    };
    T::deserialize(params).map_err(|e| RpcError::invalid_params(e.to_string()))
}

/// The `adapter.*` methods of the hook shim. They run in the order of the lines.
fn adapter(
    method: &str,
    params: &Value,
    bytes: usize,
    state: &ApiState,
) -> Result<Value, RpcError> {
    match method {
        "adapter.signal" => {
            let signal: SignalParams = params_of(params)?;
            if signal.token.is_empty() || signal.harness.is_empty() || signal.event.is_empty() {
                return Err(RpcError::invalid_params(
                    "token, harness and event must not be empty",
                ));
            }
            // The token and the payload are not logged: both can hold secrets.
            tracing::info!(
                harness = %signal.harness,
                event = %signal.event,
                bytes,
                truncated = signal.truncated,
                "adapter.signal"
            );
            state.signals_total.fetch_add(1, Ordering::Relaxed);
            // The adapter of the run in the pane that owns the token reads the event. The token
            // names one pane, so a token of another pane never reaches this run.
            if let (Some(core), Some(pane), Ok(harness)) = (
                state.core(),
                state.tokens.resolve(&signal.token),
                serde_json::from_value::<Harness>(Value::String(signal.harness.clone())),
            ) {
                core.agents.signal(
                    pane,
                    harness,
                    signal.event.clone(),
                    params.get("payload").cloned().unwrap_or(Value::Null),
                );
            }
            state.record(SignalRecord {
                harness: signal.harness,
                event: signal.event,
                bytes,
                truncated: signal.truncated,
            });
            Ok(json!({}))
        }
        other => Err(RpcError::new(
            ErrorCode::Unsupported,
            format!("{other} has no handler yet"),
        )),
    }
}

/// The parameters of `adapter.gate`.
#[derive(Deserialize)]
struct GateParams {
    harness: String,
    event: String,
    #[serde(default)]
    payload: Value,
}

/// The parameters of `adapter.gate_resume`.
#[derive(Deserialize)]
struct ResumeParams {
    interaction: String,
}

/// The parameters of `adapter.delivery_ack`.
#[derive(Deserialize)]
struct AckParams {
    interaction: String,
    idempotency_key: String,
    #[serde(default)]
    applied: bool,
}

fn interaction_id(text: &str) -> Result<nh_proto::api::ids::InteractionId, RpcError> {
    text.parse()
        .map_err(|_| RpcError::new(ErrorCode::NotFound, "no such interaction"))
}

/// The gate methods of the hook shim. The reply line goes back to the shim.
async fn gate_call(call: &PendingGate, state: &ApiState, conn: &Conn) -> Option<Vec<u8>> {
    let outcome = gate_result(call, state, conn).await;
    call.id.as_ref().map(|id| answer(id, outcome))
}

async fn gate_result(call: &PendingGate, state: &ApiState, conn: &Conn) -> Result<Value, RpcError> {
    let gates = state.gates.as_ref().ok_or_else(|| {
        RpcError::new(ErrorCode::GateUnavailable, "this core does not hold hooks")
    })?;
    match call.method.as_str() {
        "adapter.gate" => {
            let gate: GateParams = params_of(&call.params)?;
            if gate.harness.is_empty() || gate.event.is_empty() {
                return Err(RpcError::invalid_params(
                    "harness and event must not be empty",
                ));
            }
            let harness = serde_json::from_value(Value::String(gate.harness))
                .map_err(|_| RpcError::invalid_params("unknown harness"))?;
            let pane = call.origin.pane.ok_or_else(|| {
                RpcError::new(ErrorCode::PermissionDenied, "a hook needs a pane token")
            })?;
            state.gates_total.fetch_add(1, Ordering::Relaxed);
            let mut start = gates
                .open(pane, harness, gate.event, gate.payload, conn.id, conn.pid)
                .await?;
            conn.forward(&mut start);
            Ok(start.result)
        }
        // The token of a pane reaches only the interactions of that pane.
        "adapter.gate_resume" => {
            let resume: ResumeParams = params_of(&call.params)?;
            let id = interaction_id(&resume.interaction)?;
            let pane = call.origin.pane.ok_or_else(|| {
                RpcError::new(ErrorCode::PermissionDenied, "a hook needs a pane token")
            })?;
            let mut start = gates.resume(id, pane, conn.id, conn.pid).await?;
            conn.forward(&mut start);
            Ok(start.result)
        }
        "adapter.delivery_ack" => {
            let ack: AckParams = params_of(&call.params)?;
            let id = interaction_id(&ack.interaction)?;
            let pane = call.origin.pane.ok_or_else(|| {
                RpcError::new(ErrorCode::PermissionDenied, "a hook needs a pane token")
            })?;
            gates.ack(id, pane, ack.idempotency_key, ack.applied).await
        }
        other => Err(RpcError::new(
            ErrorCode::Unsupported,
            format!("{other} has no handler yet"),
        )),
    }
}

fn reply(id: &Value, result: &Value) -> Vec<u8> {
    let mut line = serde_json::to_vec(&json!({"jsonrpc": "2.0", "id": id, "result": result}))
        .unwrap_or_default();
    line.push(b'\n');
    line
}

/// An error reply: `code` is a JSON-RPC integer, and `data.code` is the name that clients
/// match on.
fn error_reply(id: &Value, error: &RpcError, code: i32) -> Vec<u8> {
    let mut data = json!({"code": error.code});
    if !error.details.is_null() {
        data["details"] = error.details.clone();
    }
    let mut line = serde_json::to_vec(&json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {"code": code, "message": error.message, "data": data},
    }))
    .unwrap_or_default();
    line.push(b'\n');
    line
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tokio::net::UnixStream;

    use super::*;

    fn text(reply: &[u8]) -> Value {
        assert_eq!(reply.last(), Some(&b'\n'), "a reply is one line");
        serde_json::from_slice(reply).expect("a JSON reply")
    }

    /// A state in which the pane with the number 1 has the token `t`.
    fn state_with_pane() -> ApiState {
        let state = ApiState::default();
        state.tokens.register(ulid::Ulid::from(1u128), "t");
        state
    }

    /// Handles a line that the server answers at once (an `adapter.*` method or an error).
    fn handle(line: &[u8], state: &ApiState) -> Option<Vec<u8>> {
        match route(line, state, &Conn::none()) {
            Routed::Reply(reply) => reply,
            Routed::Call(call) => panic!("{} is a call for the core", call.method),
            Routed::Gate(gate) => panic!("{} waits for the core", gate.method),
        }
    }

    #[test]
    fn a_client_inside_a_pane_may_not_answer_or_weaken_the_approvals() {
        let state = ApiState::default();
        let inside = Conn::none();
        inside.in_pane.set(true).expect("not read yet");
        let route_line = |line: &[u8], conn: &Conn| match route(line, &state, conn) {
            Routed::Reply(reply) => Err(text(&reply.expect("a reply"))),
            Routed::Call(call) => Ok(call.method),
            Routed::Gate(gate) => Ok(gate.method),
        };
        for line in [
            br#"{"jsonrpc":"2.0","id":1,"method":"interaction.answer","params":{"interaction":"i1","decision":{"kind":"approval","behavior":"allow"},"via":"policy"}}"#.as_slice(),
            br#"{"jsonrpc":"2.0","id":1,"method":"policy.add","params":{}}"#,
            br#"{"jsonrpc":"2.0","id":1,"method":"config.set","params":{"key":"approvals.mode","value":"observe"}}"#,
            br#"{"jsonrpc":"2.0","id":1,"method":"config.set","params":{"key":"phone.enabled","value":true}}"#,
        ] {
            let reply = route_line(line, &inside).expect_err("refused");
            assert_eq!(reply["error"]["data"]["code"], "permission_denied", "{reply}");
            let message = reply["error"]["message"].as_str().unwrap_or("");
            assert!(message.contains("outside a neohar pane"), "{message}");
        }
        // Harmless calls go on, inside a pane too.
        for line in [
            br#"{"jsonrpc":"2.0","id":1,"method":"config.set","params":{"key":"ui.font_size","value":14}}"#.as_slice(),
            br#"{"jsonrpc":"2.0","id":1,"method":"integration.install","params":{}}"#,
            br#"{"jsonrpc":"2.0","id":1,"method":"interaction.list","params":{}}"#,
        ] {
            assert!(route_line(line, &inside).is_ok());
        }
        // Outside a pane, the CLI may answer.
        let outside = Conn::none();
        let line = br#"{"jsonrpc":"2.0","id":1,"method":"interaction.answer","params":{}}"#;
        assert_eq!(
            route_line(line, &outside).expect("a call"),
            "interaction.answer"
        );
    }

    #[test]
    fn input_from_inside_a_pane_is_checked_against_its_target() {
        let state = ApiState::default();
        let line = br#"{"jsonrpc":"2.0","id":1,"method":"pane.send_bytes","params":{"pane":"w1:p2","input_id":1,"bytes":"MQ0="}}"#;
        let input_of = |conn: &Conn| match route(line, &state, conn) {
            Routed::Call(call) => call.input,
            _ => panic!("a call for the core"),
        };
        let inside = Conn::none();
        inside.in_pane.set(true).expect("not read yet");
        assert!(
            matches!(input_of(&inside), Some(crate::in_pane::Target::Pane(_))),
            "the call checks its target first"
        );
        assert_eq!(input_of(&Conn::none()), None, "outside a pane, no check");
    }

    #[test]
    fn a_no_is_not_kept_before_the_core_is_up() {
        // The core is not up yet: the answer is "no" for now, and the core asks again later.
        let state = ApiState::default();
        let mut conn = Conn::none();
        conn.pid = Some(i32::try_from(std::process::id()).expect("a pid"));
        assert!(!conn.in_pane(&state));
        assert_eq!(conn.in_pane.get(), None, "a no is not kept");
        assert!(conn.lineage.get().is_none(), "nothing was read yet");
    }

    #[test]
    fn a_signal_is_acknowledged_and_remembered() {
        let state = state_with_pane();
        let line = br#"{"jsonrpc":"2.0","id":7,"method":"adapter.signal","params":{"token":"t","harness":"claude","event":"Stop","payload":{"a":1}}}"#;
        let reply = text(&handle(line, &state).expect("a reply"));
        assert_eq!(reply, json!({"jsonrpc": "2.0", "id": 7, "result": {}}));
        assert_eq!(
            state.recent_signals(),
            [SignalRecord {
                harness: "claude".into(),
                event: "Stop".into(),
                bytes: line.len(),
                truncated: false,
            }]
        );
    }

    #[test]
    fn a_request_without_an_id_gets_no_reply() {
        let state = state_with_pane();
        let line = br#"{"jsonrpc":"2.0","method":"adapter.signal","params":{"token":"t","harness":"h","event":"e"}}"#;
        assert!(handle(line, &state).is_none());
        assert_eq!(state.recent_signals().len(), 1, "it still counts");
    }

    #[test]
    fn errors_use_the_json_rpc_codes_and_a_string_code() {
        let state = state_with_pane();
        let code = |line: &[u8]| {
            let reply = text(&handle(line, &state).expect("a reply"));
            (
                reply["error"]["code"].clone(),
                reply["error"]["data"]["code"].clone(),
                reply["id"].clone(),
            )
        };
        assert_eq!(
            code(b"{not json}\n"),
            (json!(-32700), json!("parse_error"), Value::Null)
        );
        assert_eq!(
            code(br#"{"id":1,"method":"adapter.signal"}"#),
            (json!(-32600), json!("invalid_request"), json!(1))
        );
        assert_eq!(
            code(br#"{"jsonrpc":"2.0","id":2,"method":"adapter.nope"}"#),
            (json!(-32601), json!("method_not_found"), json!(2))
        );
        assert_eq!(
            code(br#"{"jsonrpc":"2.0","id":3,"method":"adapter.signal","params":{"token":"t"}}"#),
            (json!(-32602), json!("invalid_params"), json!(3))
        );
        assert_eq!(
            code(br#"{"jsonrpc":"2.0","id":4,"method":"adapter.signal","params":{"token":"t","harness":"","event":"e"}}"#),
            (json!(-32602), json!("invalid_params"), json!(4))
        );
        assert!(state.recent_signals().is_empty(), "no bad request is kept");
    }

    #[test]
    fn a_line_that_is_no_request_is_invalid_request_with_no_id() {
        let state = state_with_pane();
        for line in [
            &br#"[{"jsonrpc":"2.0","id":1,"method":"adapter.signal"}]"#[..],
            br#""text""#,
            br#"42"#,
            br#"null"#,
            br#"{"jsonrpc":"2.0","id":{"a":1},"method":"adapter.signal"}"#,
        ] {
            let reply = text(&handle(line, &state).expect("a reply"));
            assert_eq!(reply["error"]["code"], json!(-32600), "{reply}");
            assert_eq!(reply["id"], Value::Null, "{reply}");
        }
        // The id is known when the rest is wrong.
        for line in [
            &br#"{"jsonrpc":"1.0","id":"a","method":"adapter.signal"}"#[..],
            br#"{"jsonrpc":"2.0","id":"a","method":5}"#,
            br#"{"jsonrpc":"2.0","id":"a"}"#,
            br#"{"jsonrpc":"2.0","id":"a","method":"adapter.signal","pane_token":5}"#,
        ] {
            let reply = text(&handle(line, &state).expect("a reply"));
            assert_eq!(reply["error"]["code"], json!(-32600), "{reply}");
            assert_eq!(reply["id"], json!("a"), "{reply}");
        }
    }

    #[test]
    fn an_adapter_method_needs_the_token_of_a_pane() {
        let state = state_with_pane();
        let denied = |params: Value| {
            let line = json!({"jsonrpc":"2.0","id":1,"method":"adapter.signal","params":params});
            let reply = text(&handle(line.to_string().as_bytes(), &state).expect("a reply"));
            (
                reply["error"]["code"].clone(),
                reply["error"]["data"]["code"].clone(),
            )
        };
        let nobody = (json!(-32000), json!("permission_denied"));
        // No token: the caller is a client of the CLI, and the CLI has no adapter methods.
        assert_eq!(denied(json!({"harness":"h","event":"e"})), nobody);
        // A token that no pane has, an empty one, and one of the wrong type.
        assert_eq!(
            denied(json!({"token":"other","harness":"h","event":"e"})),
            nobody
        );
        assert_eq!(
            denied(json!({"token":"","harness":"h","event":"e"})),
            nobody
        );
        assert!(state.recent_signals().is_empty());
        // The token can come in the envelope as well.
        let line = br#"{"jsonrpc":"2.0","id":2,"pane_token":"t","method":"adapter.signal","params":{"token":"t","harness":"h","event":"e"}}"#;
        assert_eq!(
            text(&handle(line, &state).expect("a reply"))["result"],
            json!({})
        );
        let line = br#"{"jsonrpc":"2.0","id":3,"pane_token":"bad","method":"adapter.signal","params":{"token":"t","harness":"h","event":"e"}}"#;
        assert_eq!(
            text(&handle(line, &state).expect("a reply"))["error"]["data"]["code"],
            json!("permission_denied"),
            "the envelope wins over the params"
        );
    }

    #[test]
    fn a_request_for_the_core_is_handed_on_with_the_scope_of_the_caller() {
        let state = state_with_pane();
        let conn = Conn::none();
        let routed = |line: &[u8]| route(line, &state, &conn);
        let Routed::Call(call) = routed(br#"{"jsonrpc":"2.0","id":9,"method":"session.info"}"#)
        else {
            panic!("a call");
        };
        assert_eq!(call.method, "session.info");
        assert_eq!(call.origin, Origin::cli(0));
        assert_eq!(call.id, Some(json!(9)));
        assert_eq!(call.params, Value::Null);
        // With the token of a pane in the envelope the scope is the scope of a pane.
        let Routed::Call(call) =
            routed(br#"{"jsonrpc":"2.0","id":10,"pane_token":"t","method":"interaction.answer","params":{}}"#)
        else {
            panic!("a call");
        };
        assert_eq!(call.origin, Origin::pane(0, ulid::Ulid::from(1u128)));
        // A notification has no id.
        let Routed::Call(call) = routed(br#"{"jsonrpc":"2.0","method":"session.info"}"#) else {
            panic!("a call");
        };
        assert_eq!(call.id, None);
        // A bad token is refused before the call is made.
        let Routed::Reply(Some(reply)) =
            routed(br#"{"jsonrpc":"2.0","id":11,"pane_token":"bad","method":"session.info"}"#)
        else {
            panic!("a reply");
        };
        assert_eq!(
            text(&reply)["error"]["data"]["code"],
            json!("permission_denied")
        );
    }

    #[test]
    fn an_error_reply_has_the_integer_of_jsonrpc_and_the_string_code() {
        for (code, name, int) in [
            (ErrorCode::ParseError, "parse_error", -32700),
            (ErrorCode::MethodNotFound, "method_not_found", -32601),
            (ErrorCode::Internal, "internal", -32603),
            (ErrorCode::NotFound, "not_found", -32000),
            (ErrorCode::PaneBusy, "pane_busy", -32000),
        ] {
            let error = RpcError::new(code, "x");
            let reply = text(&error_reply(&json!(1), &error, error.code.rpc_code()));
            assert_eq!(reply["error"]["code"], json!(int), "{name}");
            assert_eq!(reply["error"]["data"]["code"], json!(name));
        }
    }

    #[test]
    fn the_core_keeps_only_the_last_signals() {
        let state = ApiState::default();
        for n in 0..KEEP_SIGNALS + 5 {
            state.record(SignalRecord {
                harness: "h".into(),
                event: n.to_string(),
                bytes: 0,
                truncated: false,
            });
        }
        let kept = state.recent_signals();
        assert_eq!(kept.len(), KEEP_SIGNALS);
        assert_eq!(kept[0].event, "5");
    }

    /// A short directory: socket paths are at most 103 bytes on macOS.
    fn temp_env() -> (Env, PathBuf) {
        static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let root = std::env::temp_dir().join(format!(
            "nha{}-{}",
            std::process::id(),
            N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let mut env = Env::from_process();
        env.runtime_override = Some(root.clone());
        (env, root)
    }

    #[test]
    fn bind_makes_a_private_socket_and_removes_it_on_drop() {
        let (env, root) = temp_env();
        env.ensure_runtime_root().expect("a root");
        let socket = ApiSocket::bind(&env, "t").expect("a socket");
        let path = socket.path().to_owned();
        let mode = |p: &Path| std::fs::metadata(p).expect("metadata").permissions().mode() & 0o777;
        assert_eq!(mode(&path), 0o600);
        assert_eq!(mode(path.parent().expect("a directory")), 0o700);
        drop(socket);
        assert!(!path.exists(), "a clean end leaves no socket behind");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn bind_replaces_a_leftover_socket_and_refuses_a_live_one() {
        let (env, root) = temp_env();
        env.ensure_runtime_root().expect("a root");
        let path = env.api_socket("t").expect("a path");
        std::fs::create_dir_all(path.parent().expect("a directory")).expect("a directory");
        // A core that was killed leaves its file, and nobody listens.
        drop(StdListener::bind(&path).expect("a leftover"));
        assert!(path.exists(), "the file stays after the listener closed");
        let second = ApiSocket::bind(&env, "t").expect("it replaces the leftover");
        assert!(StdStream::connect(&path).is_ok(), "the new socket listens");
        // The socket is live now, so another core must not take it.
        assert!(matches!(
            ApiSocket::bind(&env, "t"),
            Err(CoreError::ApiSocketInUse(_))
        ));
        assert!(path.exists(), "the refused bind left the live socket alone");
        drop(second);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_core_that_holds_the_lock_of_the_session_keeps_a_second_one_out() {
        // Two cores that start at the same moment: the first one holds the lock and
        // has not bound yet. The second one must not touch the socket path.
        let (env, root) = temp_env();
        env.ensure_runtime_root().expect("a root");
        let first = ApiSocket::bind(&env, "t").expect("the first core binds");
        let path = first.path().to_owned();
        std::fs::remove_file(&path).expect("the socket file goes");
        assert!(matches!(
            ApiSocket::bind(&env, "t"),
            Err(CoreError::ApiSocketInUse(_))
        ));
        assert!(!path.exists(), "the refused core bound nothing");
        drop(first);
        assert!(ApiSocket::bind(&env, "t").is_ok(), "the lock is free again");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn bind_does_not_delete_a_file_that_is_no_socket() {
        let (env, root) = temp_env();
        env.ensure_runtime_root().expect("a root");
        let path = env.api_socket("t").expect("a path");
        std::fs::create_dir_all(path.parent().expect("a directory")).expect("a directory");
        std::fs::write(&path, b"mine").expect("a file");
        assert!(matches!(
            ApiSocket::bind(&env, "t"),
            Err(CoreError::Io { .. })
        ));
        assert_eq!(std::fs::read(&path).expect("still there"), b"mine");
        let _ = std::fs::remove_dir_all(root);
    }

    async fn serve_in(env: &Env) -> (ApiSocket, Arc<ApiState>) {
        let mut socket = ApiSocket::bind(env, "t").expect("a socket");
        let state = Arc::new(state_with_pane());
        let listener = socket.take_listener().expect("a listener");
        tokio::spawn(serve(
            listener,
            env.uid,
            Arc::clone(&state),
            Arc::new(tokio::sync::Notify::new()),
        ));
        (socket, state)
    }

    #[tokio::test]
    async fn one_connection_can_send_many_requests() {
        let (env, root) = temp_env();
        env.ensure_runtime_root().expect("a root");
        let (socket, state) = serve_in(&env).await;
        let stream = UnixStream::connect(socket.path()).await.expect("connect");
        let (read, mut write) = stream.into_split();
        let mut read = BufReader::new(read);
        for id in 1..=3 {
            let request = json!({"jsonrpc":"2.0","id":id,"method":"adapter.signal",
                "params":{"token":"t","harness":"claude","event":format!("e{id}")}});
            write
                .write_all(format!("{request}\n").as_bytes())
                .await
                .expect("write");
            let mut line = String::new();
            read.read_line(&mut line).await.expect("a reply");
            assert_eq!(text(line.as_bytes())["id"], json!(id));
        }
        assert_eq!(state.recent_signals().len(), 3);
        drop(socket);
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn a_line_over_the_limit_gets_an_error_and_the_connection_closes() {
        let (env, root) = temp_env();
        env.ensure_runtime_root().expect("a root");
        let (socket, _state) = serve_in(&env).await;
        let stream = UnixStream::connect(socket.path()).await.expect("connect");
        let (read, mut write) = stream.into_split();
        let writer = tokio::spawn(async move {
            // No newline: the server must stop reading at the limit.
            let chunk = vec![b'x'; 1 << 20];
            for _ in 0..=(MAX_LINE >> 20) {
                if write.write_all(&chunk).await.is_err() {
                    return;
                }
            }
            let _ = write.shutdown().await;
        });
        let mut read = BufReader::new(read);
        let mut line = String::new();
        read.read_line(&mut line).await.expect("a reply");
        assert_eq!(
            text(line.as_bytes())["error"]["data"]["code"],
            json!("too_large")
        );
        writer.abort();
        drop(socket);
        let _ = std::fs::remove_dir_all(root);
    }
}
