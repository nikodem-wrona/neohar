//! The task that owns the chats and the connection to `hermes acp`.
//!
//! One process serves every open chat: ACP names the session in each message. The process starts
//! when the first chat needs it and stops when the last chat closes. When it ends, the worker
//! starts it again after a pause and resumes each session; after [`MAX_FAILURES`] ends in
//! [`FAILURE_WINDOW`] it waits for a prompt or `hermes.retry`.

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use nh_proto::api::{
    calls::hermes::HermesStart,
    envelope::ErrorCode,
    events::{EventPayload, HermesChatEvent},
    ids::ChatId,
    model::{HermesAnswerState, HermesChatState, HermesChatValue, HermesChoice},
    projections::Projection,
    time::Timestamp,
};
use nh_store::HermesChatRow;
use serde_json::{Value, json};
use tokio::sync::{mpsc, oneshot};

use super::{
    conn::{Conn, Incoming, IncomingKind},
    thread::{AnswerError, Thread},
};
use crate::{
    actor::ActorHandle, config::ConfigStore, error::RpcError, ids::new_ulid, persist::now_ms,
    projection::ProjectionHub,
};

/// The tick of the worker: renders, deadlines, the guess of the end of a turn.
const TICK: Duration = Duration::from_millis(100);
/// A turn that the core found in a resume counts as ended when the agent wrote, a usage update
/// came, and nothing happened for this long.
const SETTLE: Duration = Duration::from_secs(3);
const MAX_FAILURES: usize = 3;
const FAILURE_WINDOW: Duration = Duration::from_secs(60);
/// The pauses before a new start, by the number of recent failures.
const BACKOFF: [Duration; 3] = [
    Duration::from_secs(1),
    Duration::from_secs(5),
    Duration::from_secs(15),
];

/// A call for the worker.
pub(crate) enum Request {
    Start(HermesStart),
    Prompt {
        chat: ChatId,
        text: String,
        input_id: u64,
    },
    Cancel(ChatId),
    Answer {
        chat: ChatId,
        request: String,
        option: String,
    },
    Set {
        chat: ChatId,
        model: Option<String>,
        mode: Option<String>,
    },
    Retry(ChatId),
    Close(ChatId),
}

pub(crate) type Reply = oneshot::Sender<Result<Value, RpcError>>;

/// What a request of the core waits for.
enum Pending {
    Initialize,
    Open(ChatId),
    Prompt(ChatId),
    Ignore,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Down,
    Initializing,
    Ready,
}

struct Chat {
    id: ChatId,
    cwd: String,
    session: Option<String>,
    title: Option<String>,
    started_at: i64,
    /// The session is open on the current connection.
    open: bool,
    /// The core waits for the answer of `session/new` or `session/resume`.
    opening: bool,
    prompts: u32,
    guessed: bool,
    agent_wrote: bool,
    settle_at: Option<Instant>,
    model: Option<String>,
    models: Vec<HermesChoice>,
    mode: Option<String>,
    modes: Vec<HermesChoice>,
    context: Option<(u64, u64)>,
    error: Option<String>,
    queued: Vec<String>,
    inputs: HashSet<u64>,
    /// The answers by request, to answer a request again that a new connection repeats.
    answered: HashMap<String, String>,
    thread: Thread,
}

impl Chat {
    fn new(id: ChatId, cwd: String, started_at: i64) -> Self {
        Self {
            id,
            cwd,
            session: None,
            title: None,
            started_at,
            open: false,
            opening: false,
            prompts: 0,
            guessed: false,
            agent_wrote: false,
            settle_at: None,
            model: None,
            models: Vec::new(),
            mode: None,
            modes: Vec::new(),
            context: None,
            error: None,
            queued: Vec::new(),
            inputs: HashSet::new(),
            answered: HashMap::new(),
            thread: Thread::default(),
        }
    }

    fn state(&self, phase: Phase) -> HermesChatState {
        if !self.open {
            // An error and no attempt that runs: a prompt or `hermes.retry` tries again.
            if self.error.is_some() && !self.opening && phase != Phase::Initializing {
                return HermesChatState::Disconnected;
            }
            return HermesChatState::Starting;
        }
        if self.thread.open_approvals() > 0 {
            HermesChatState::Waiting
        } else if self.prompts > 0 || self.guessed {
            HermesChatState::Working
        } else {
            HermesChatState::Idle
        }
    }

    fn working(&self) -> bool {
        self.prompts > 0 || self.guessed
    }

    fn row(&self) -> HermesChatRow {
        HermesChatRow {
            id: self.id.to_string(),
            cwd: self.cwd.clone(),
            session: self.session.clone(),
            title: self.title.clone(),
            started_at: self.started_at,
        }
    }
}

pub(crate) struct Worker {
    hub: Arc<ProjectionHub>,
    actor: ActorHandle,
    config: Arc<ConfigStore>,
    env: Vec<(String, String)>,
    chats: BTreeMap<ChatId, Chat>,
    phase: Phase,
    conn: Option<Conn>,
    generation: u64,
    incoming: mpsc::UnboundedSender<Incoming>,
    pending: HashMap<u64, Pending>,
    failures: Vec<Instant>,
    retry_at: Option<Instant>,
    /// The process ended too often: wait for a person.
    stopped: bool,
}

impl Worker {
    pub fn new(
        hub: Arc<ProjectionHub>,
        actor: ActorHandle,
        config: Arc<ConfigStore>,
        env: Vec<(String, String)>,
        incoming: mpsc::UnboundedSender<Incoming>,
    ) -> Self {
        Self {
            hub,
            actor,
            config,
            env,
            chats: BTreeMap::new(),
            phase: Phase::Down,
            conn: None,
            generation: 0,
            incoming,
            pending: HashMap::new(),
            failures: Vec::new(),
            retry_at: None,
            stopped: false,
        }
    }

    /// The loop of the worker. It ends when the core drops the queue.
    pub async fn run(
        mut self,
        mut requests: mpsc::UnboundedReceiver<(Request, Reply)>,
        mut incoming: mpsc::UnboundedReceiver<Incoming>,
    ) {
        self.load().await;
        let mut tick = tokio::time::interval(TICK);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                request = requests.recv() => {
                    let Some((request, reply)) = request else { return };
                    let result = self.request(request).await;
                    let _ = reply.send(result);
                }
                Some(message) = incoming.recv() => self.incoming(message).await,
                _ = tick.tick() => self.tick().await,
            }
            self.publish();
        }
    }

    async fn load(&mut self) {
        let rows = self
            .actor
            .update(|parts| parts.store.hermes_chats())
            .await
            .ok()
            .and_then(Result::ok)
            .unwrap_or_default();
        for row in rows {
            let Ok(id) = row.id.parse::<ChatId>() else {
                continue;
            };
            let mut chat = Chat::new(id, row.cwd, row.started_at);
            chat.session = row.session;
            chat.title = row.title;
            self.chats.insert(id, chat);
        }
        if !self.chats.is_empty() {
            self.connect().await;
        }
        self.publish_all();
    }

    // --- the calls ---------------------------------------------------------------------------

    async fn request(&mut self, request: Request) -> Result<Value, RpcError> {
        match request {
            Request::Start(start) => self.start(start).await,
            Request::Prompt {
                chat,
                text,
                input_id,
            } => self.prompt(chat, text, input_id).await,
            Request::Cancel(chat) => self.cancel(chat).await,
            Request::Answer {
                chat,
                request,
                option,
            } => self.answer(chat, &request, &option).await,
            Request::Set { chat, model, mode } => self.set(chat, model, mode).await,
            Request::Retry(chat) => {
                self.chat(chat)?;
                self.wake().await;
                Ok(json!({}))
            }
            Request::Close(chat) => self.close(chat).await,
        }
    }

    fn chat(&self, id: ChatId) -> Result<&Chat, RpcError> {
        self.chats
            .get(&id)
            .ok_or_else(|| RpcError::new(ErrorCode::NotFound, "no such chat"))
    }

    fn chat_mut(&mut self, id: ChatId) -> Result<&mut Chat, RpcError> {
        self.chats
            .get_mut(&id)
            .ok_or_else(|| RpcError::new(ErrorCode::NotFound, "no such chat"))
    }

    async fn start(&mut self, start: HermesStart) -> Result<Value, RpcError> {
        let cwd = PathBuf::from(&start.cwd);
        if !cwd.is_absolute() || !cwd.is_dir() {
            return Err(RpcError::invalid_params(format!(
                "{} is not the absolute path of a folder",
                start.cwd
            )));
        }
        let id = ChatId::new(new_ulid()?);
        let chat = Chat::new(id, start.cwd.clone(), now_ms());
        let row = chat.row();
        let event = EventPayload::HermesChatStarted(HermesChatEvent {
            chat: id,
            cwd: start.cwd.clone(),
        });
        self.store(row, Some(event)).await?;
        self.chats.insert(id, chat);
        if let Some(text) = start.prompt.filter(|t| !t.trim().is_empty())
            && let Some(chat) = self.chats.get_mut(&id)
        {
            chat.thread.prompt(&text, now_ms());
            chat.queued.push(text);
        }
        self.wake().await;
        if self.phase == Phase::Ready {
            self.open(id).await;
        }
        self.publish_all();
        Ok(json!({ "chat": id }))
    }

    async fn prompt(&mut self, id: ChatId, text: String, input_id: u64) -> Result<Value, RpcError> {
        if text.trim().is_empty() {
            return Err(RpcError::invalid_params("the prompt is empty"));
        }
        let chat = self.chat_mut(id)?;
        if !chat.inputs.insert(input_id) {
            return Ok(json!({}));
        }
        if chat.open {
            chat.thread.prompt(&text, now_ms());
            self.send_prompt(id, text).await;
        } else {
            // A resume starts the list again, so a session that is not open shows the prompt
            // only while it waits (see `opened`).
            if chat.session.is_none() || !chat.opening {
                chat.thread.prompt(&text, now_ms());
            }
            chat.queued.push(text);
            self.wake().await;
        }
        Ok(json!({}))
    }

    async fn send_prompt(&mut self, id: ChatId, text: String) {
        let Some(session) = self.chats.get(&id).and_then(|c| c.session.clone()) else {
            return;
        };
        let params = json!({
            "sessionId": session,
            "prompt": [{"type": "text", "text": text}],
        });
        match self.send_request("session/prompt", params).await {
            Some(rpc) => {
                self.pending.insert(rpc, Pending::Prompt(id));
                if let Some(chat) = self.chats.get_mut(&id) {
                    chat.prompts += 1;
                    chat.thread.touch_last();
                }
            }
            None => {
                if let Some(chat) = self.chats.get_mut(&id) {
                    chat.thread
                        .notice("The prompt did not reach Hermes.", now_ms());
                }
            }
        }
    }

    async fn cancel(&mut self, id: ChatId) -> Result<Value, RpcError> {
        let chat = self.chat_mut(id)?;
        let session = chat.session.clone();
        let ids = chat.thread.end_approvals(HermesAnswerState::Gone);
        chat.guessed = false;
        if let (Some(session), Some(conn)) = (session, self.conn.as_mut()) {
            for rpc in ids {
                let _ = conn
                    .respond(rpc, json!({"outcome": {"outcome": "cancelled"}}))
                    .await;
            }
            let _ = conn
                .notify("session/cancel", json!({"sessionId": session}))
                .await;
        }
        Ok(json!({}))
    }

    async fn answer(&mut self, id: ChatId, request: &str, option: &str) -> Result<Value, RpcError> {
        let chat = self.chat_mut(id)?;
        let (rpc, option) = chat
            .thread
            .answer(request, option)
            .map_err(|error| match error {
                AnswerError::NoSuchRequest => {
                    RpcError::new(ErrorCode::NotFound, "no such approval")
                }
                AnswerError::NoSuchOption => RpcError::invalid_params("no such option"),
                AnswerError::NotOpen(HermesAnswerState::Expired) => RpcError::new(
                    ErrorCode::Conflict,
                    "the deadline went by, and Hermes denied the request",
                ),
                AnswerError::NotOpen(_) => {
                    RpcError::new(ErrorCode::Conflict, "the approval is not open")
                }
            })?;
        chat.answered.insert(request.to_owned(), option.clone());
        if let Some(conn) = self.conn.as_mut() {
            let result = json!({"outcome": {"outcome": "selected", "optionId": option}});
            if let Err(error) = conn.respond(rpc, result).await {
                tracing::warn!(%error, "an answer did not reach hermes acp");
            }
        }
        Ok(json!({}))
    }

    async fn set(
        &mut self,
        id: ChatId,
        model: Option<String>,
        mode: Option<String>,
    ) -> Result<Value, RpcError> {
        let chat = self.chat(id)?;
        let session = chat
            .session
            .clone()
            .filter(|_| chat.open)
            .ok_or_else(|| RpcError::new(ErrorCode::Conflict, "the chat is not open yet"))?;
        if let Some(model) = &model
            && !chat.models.iter().any(|m| m.id == *model)
        {
            return Err(RpcError::invalid_params("no such model"));
        }
        if let Some(mode) = &mode
            && !chat.modes.iter().any(|m| m.id == *mode)
        {
            return Err(RpcError::invalid_params("no such mode"));
        }
        if let Some(model) = model {
            let params = json!({"sessionId": session, "modelId": model});
            if let Some(rpc) = self.send_request("session/set_model", params).await {
                self.pending.insert(rpc, Pending::Ignore);
            }
            if let Some(chat) = self.chats.get_mut(&id) {
                chat.model = Some(model);
            }
        }
        if let Some(mode) = mode {
            let params = json!({"sessionId": session, "modeId": mode});
            if let Some(rpc) = self.send_request("session/set_mode", params).await {
                self.pending.insert(rpc, Pending::Ignore);
            }
            if let Some(chat) = self.chats.get_mut(&id) {
                chat.mode = Some(mode);
            }
        }
        Ok(json!({}))
    }

    async fn close(&mut self, id: ChatId) -> Result<Value, RpcError> {
        let cwd = self.chat(id)?.cwd.clone();
        if self.chat(id)?.working() {
            let _ = self.cancel(id).await;
        }
        let row_id = id.to_string();
        let event = EventPayload::HermesChatClosed(HermesChatEvent { chat: id, cwd });
        self.actor
            .update(move |mut parts| {
                parts.commit(|tx| {
                    tx.delete_hermes_chat(&row_id)?;
                    tx.append(Some(&row_id), &event)?;
                    Ok(())
                })
            })
            .await
            .map_err(|e| RpcError::internal(e.to_string()))?
            .map_err(|e| RpcError::internal(e.to_string()))?;
        if let Some(chat) = self.chats.remove(&id) {
            for entry in chat.thread.entries() {
                self.hub.remove(Projection::HermesChat(id), &entry.key);
            }
        }
        self.hub.remove(Projection::HermesChats, &id.to_string());
        if self.chats.is_empty() {
            self.disconnect();
        }
        self.publish_order();
        Ok(json!({}))
    }

    async fn store(&self, row: HermesChatRow, event: Option<EventPayload>) -> Result<(), RpcError> {
        let subject = row.id.clone();
        self.actor
            .update(move |mut parts| {
                parts.commit(|tx| {
                    tx.put_hermes_chat(&row)?;
                    if let Some(event) = &event {
                        tx.append(Some(&subject), event)?;
                    }
                    Ok(())
                })
            })
            .await
            .map_err(|e| RpcError::internal(e.to_string()))?
            .map_err(|e| RpcError::internal(e.to_string()))
    }

    async fn store_detached(&self, id: ChatId) {
        if let Some(chat) = self.chats.get(&id)
            && let Err(error) = self.store(chat.row(), None).await
        {
            tracing::warn!(%error, "a chat with hermes is not stored");
        }
    }

    // --- the connection ---------------------------------------------------------------------

    /// Starts the process if a chat needs it, or opens the sessions that are not open.
    async fn wake(&mut self) {
        self.stopped = false;
        self.retry_at = None;
        match self.phase {
            Phase::Down if !self.chats.is_empty() => self.connect().await,
            Phase::Ready => {
                let ids: Vec<ChatId> = self.chats.keys().copied().collect();
                for id in ids {
                    self.open(id).await;
                }
            }
            _ => {}
        }
    }

    async fn connect(&mut self) {
        let config = self.config.config();
        let hermes = &config.hermes;
        self.generation += 1;
        let home = std::env::var_os("HOME").map_or_else(|| PathBuf::from("/"), PathBuf::from);
        let spawned = Conn::spawn(
            &hermes.command,
            &hermes.args,
            &self.env,
            &home,
            self.generation,
            self.incoming.clone(),
        );
        let mut conn = match spawned {
            Ok(conn) => conn,
            Err(error) => {
                let text = format!("{} does not start: {error}", hermes.command);
                self.failed(text);
                return;
            }
        };
        let params = json!({
            "protocolVersion": 1,
            "clientCapabilities": {
                "fs": {"readTextFile": false, "writeTextFile": false},
                "terminal": false,
            },
            "clientInfo": {"name": "neohar", "version": env!("CARGO_PKG_VERSION")},
        });
        match conn.request("initialize", params).await {
            Ok(rpc) => {
                self.pending.insert(rpc, Pending::Initialize);
                self.conn = Some(conn);
                self.phase = Phase::Initializing;
            }
            Err(error) => self.failed(format!("hermes acp does not answer: {error}")),
        }
    }

    fn disconnect(&mut self) {
        // The reader of the old process must not stop the next one.
        self.generation += 1;
        self.conn = None;
        self.phase = Phase::Down;
        self.pending.clear();
        for chat in self.chats.values_mut() {
            chat.open = false;
            chat.opening = false;
        }
    }

    /// The process ended or did not start.
    fn failed(&mut self, error: String) {
        self.disconnect();
        let now = Instant::now();
        self.failures
            .retain(|at| now.duration_since(*at) < FAILURE_WINDOW);
        self.failures.push(now);
        let count = self.failures.len();
        if count > MAX_FAILURES {
            self.stopped = true;
            self.retry_at = None;
        } else {
            self.retry_at = Some(now + BACKOFF[(count - 1).min(BACKOFF.len() - 1)]);
        }
        let at = now_ms();
        for chat in self.chats.values_mut() {
            let lost = chat.prompts > 0;
            chat.prompts = 0;
            chat.guessed = false;
            chat.thread.end_approvals(HermesAnswerState::Gone);
            if lost {
                chat.thread.notice(
                    "hermes acp stopped during the turn. The gateway of Hermes can still run it; \
                     the chat shows it when the session comes back.",
                    at,
                );
            }
            chat.error = Some(error.clone());
        }
    }

    async fn send_request(&mut self, method: &str, params: Value) -> Option<u64> {
        let conn = self.conn.as_mut()?;
        match conn.request(method, params).await {
            Ok(rpc) => Some(rpc),
            Err(error) => {
                tracing::warn!(%error, method, "a request did not reach hermes acp");
                None
            }
        }
    }

    /// Opens the session of a chat on the connection: new or resumed.
    async fn open(&mut self, id: ChatId) {
        let Some(chat) = self.chats.get_mut(&id) else {
            return;
        };
        if chat.open || chat.opening {
            return;
        }
        chat.opening = true;
        let (method, params) = match &chat.session {
            Some(session) => {
                // The resume sends the history again, before its answer.
                chat.thread.reset();
                (
                    "session/resume",
                    json!({"sessionId": session, "cwd": chat.cwd, "mcpServers": []}),
                )
            }
            None => ("session/new", json!({"cwd": chat.cwd, "mcpServers": []})),
        };
        match self.send_request(method, params).await {
            Some(rpc) => {
                self.pending.insert(rpc, Pending::Open(id));
            }
            None => {
                if let Some(chat) = self.chats.get_mut(&id) {
                    chat.opening = false;
                }
            }
        }
    }

    async fn incoming(&mut self, message: Incoming) {
        if message.generation != self.generation {
            return;
        }
        match message.kind {
            IncomingKind::Closed(last) => {
                let text = if last.is_empty() {
                    "hermes acp stopped".to_owned()
                } else {
                    format!("hermes acp stopped: {last}")
                };
                self.failed(text);
            }
            IncomingKind::Message(message) => {
                let method = message.get("method").and_then(Value::as_str);
                let id = message.get("id").cloned();
                match (method, id) {
                    (Some(method), Some(id)) => {
                        let method = method.to_owned();
                        self.agent_request(&method, id, &message).await;
                    }
                    (Some("session/update"), None) => self.update(&message),
                    (Some(_), None) => {}
                    (None, Some(id)) => self.response(&id, &message).await,
                    (None, None) => {}
                }
            }
        }
    }

    fn session_chat(&mut self, params: &Value) -> Option<&mut Chat> {
        let session = params.get("sessionId").and_then(Value::as_str)?;
        self.chats
            .values_mut()
            .find(|chat| chat.session.as_deref() == Some(session))
    }

    fn update(&mut self, message: &Value) {
        let params = message.get("params").cloned().unwrap_or(Value::Null);
        let Some(update) = params.get("update") else {
            return;
        };
        let Some(chat) = self.session_chat(&params) else {
            return;
        };
        let outcome = chat.thread.update(update, now_ms());
        if let Some((used, size)) = outcome.context {
            chat.context = Some((used, size));
        }
        if let Some(mode) = outcome.mode {
            chat.mode = Some(mode);
        }
        if chat.guessed {
            if outcome.agent_text {
                chat.agent_wrote = true;
            }
            if outcome.tool {
                chat.agent_wrote = false;
                chat.settle_at = None;
            } else if outcome.usage && chat.agent_wrote {
                chat.settle_at = Some(Instant::now() + SETTLE);
            } else if outcome.agent_text {
                chat.settle_at = None;
            }
        }
        if let Some(title) = outcome.title
            && chat.title.as_deref() != Some(title.as_str())
        {
            chat.title = Some(title);
            let id = chat.id;
            let row = chat.row();
            let actor = self.actor.clone();
            tokio::spawn(async move {
                let stored = actor
                    .update(move |mut parts| parts.commit(|tx| tx.put_hermes_chat(&row)))
                    .await;
                if !matches!(stored, Ok(Ok(()))) {
                    tracing::warn!(chat = %id, "the title of a chat is not stored");
                }
            });
        }
    }

    async fn agent_request(&mut self, method: &str, rpc: Value, message: &Value) {
        let params = message.get("params").cloned().unwrap_or(Value::Null);
        if method != "session/request_permission" {
            if let Some(conn) = self.conn.as_mut() {
                let _ = conn
                    .respond_error(rpc, -32601, &format!("neohar does not support {method}"))
                    .await;
            }
            return;
        }
        let timeout = i64::from(self.config.config().hermes.approval_timeout_s) * 1000;
        let now = now_ms();
        let Some(chat) = self.session_chat(&params) else {
            if let Some(conn) = self.conn.as_mut() {
                let _ = conn
                    .respond(rpc, json!({"outcome": {"outcome": "cancelled"}}))
                    .await;
            }
            return;
        };
        let request = chat.thread.approval(rpc, &params, now, now + timeout);
        // A request that a new connection repeats, after an answer of the person.
        if let Some(option) = chat.answered.get(&request).cloned()
            && let Ok((rpc, option)) = chat.thread.answer(&request, &option)
            && let Some(conn) = self.conn.as_mut()
        {
            let _ = conn
                .respond(
                    rpc,
                    json!({"outcome": {"outcome": "selected", "optionId": option}}),
                )
                .await;
        }
    }

    async fn response(&mut self, id: &Value, message: &Value) {
        let Some(pending) = id.as_u64().and_then(|id| self.pending.remove(&id)) else {
            return;
        };
        let error = message
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(Value::as_str)
            .map(str::to_owned);
        let result = message.get("result").cloned().unwrap_or(Value::Null);
        match pending {
            Pending::Ignore => {}
            Pending::Initialize => {
                if let Some(error) = error {
                    self.failed(format!("hermes acp refused to start: {error}"));
                    return;
                }
                self.phase = Phase::Ready;
                self.failures.clear();
                let ids: Vec<ChatId> = self.chats.keys().copied().collect();
                for id in ids {
                    self.open(id).await;
                }
            }
            Pending::Open(id) => self.opened(id, error, &result).await,
            Pending::Prompt(id) => {
                let Some(chat) = self.chats.get_mut(&id) else {
                    return;
                };
                chat.prompts = chat.prompts.saturating_sub(1);
                chat.guessed = false;
                chat.settle_at = None;
                let at = now_ms();
                if let Some(error) = error {
                    chat.thread.notice(&format!("Hermes: {error}"), at);
                } else if result.get("stopReason").and_then(Value::as_str) == Some("cancelled") {
                    chat.thread.notice("The turn was stopped.", at);
                }
                if chat.prompts == 0 {
                    chat.thread.end_turn();
                }
            }
        }
    }

    async fn opened(&mut self, id: ChatId, error: Option<String>, result: &Value) {
        let Some(chat) = self.chats.get_mut(&id) else {
            return;
        };
        chat.opening = false;
        if let Some(error) = error {
            chat.error = Some(format!("Hermes did not open the session: {error}"));
            chat.thread.notice(
                chat.error
                    .as_deref()
                    .unwrap_or("Hermes did not open the session"),
                now_ms(),
            );
            return;
        }
        let new_session = result
            .get("sessionId")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let resumed = chat.session.is_some();
        if let Some(session) = new_session {
            chat.session = Some(session);
        }
        chat.open = true;
        chat.error = None;
        if let Some(models) = result.get("models") {
            chat.models = choices(models.get("availableModels"), "modelId");
            chat.model = models
                .get("currentModelId")
                .and_then(Value::as_str)
                .map(str::to_owned);
        }
        if let Some(modes) = result.get("modes") {
            chat.modes = choices(modes.get("availableModes"), "id");
            chat.mode = modes
                .get("currentModeId")
                .and_then(Value::as_str)
                .map(str::to_owned);
        }
        if resumed && chat.prompts == 0 && chat.thread.ends_open() {
            chat.guessed = true;
            chat.agent_wrote = false;
        }
        let queued = std::mem::take(&mut chat.queued);
        if resumed {
            // The resume started the list again: the prompts that wait show after the history.
            for text in &queued {
                chat.thread.prompt(text, now_ms());
            }
        } else {
            self.store_detached(id).await;
        }
        for text in queued {
            self.send_prompt(id, text).await;
        }
    }

    async fn tick(&mut self) {
        let now = Instant::now();
        let at = now_ms();
        for chat in self.chats.values_mut() {
            chat.thread.expire(at);
            if chat.guessed && chat.settle_at.is_some_and(|settle| settle <= now) {
                chat.guessed = false;
                chat.settle_at = None;
                chat.thread.end_turn();
            }
        }
        if !self.stopped
            && !self.chats.is_empty()
            && self.phase == Phase::Down
            && self.retry_at.is_some_and(|retry| retry <= now)
        {
            self.retry_at = None;
            self.connect().await;
        }
    }

    // --- the projections ---------------------------------------------------------------------

    fn value(&self, chat: &Chat) -> HermesChatValue {
        let now = now_ms();
        HermesChatValue {
            id: chat.id,
            title: chat.title.clone().unwrap_or_else(|| folder_name(&chat.cwd)),
            cwd: chat.cwd.clone(),
            session: chat.session.clone(),
            state: chat.state(self.phase),
            guessed: chat.guessed,
            model: chat.model.clone(),
            models: chat.models.clone(),
            mode: chat.mode.clone(),
            modes: chat.modes.clone(),
            context_text: chat
                .context
                .map(|(used, size)| format!("{} of {} tokens", kilo(used), kilo(size))),
            pending: chat.thread.open_approvals(),
            error: if self.stopped {
                chat.error
                    .as_ref()
                    .map(|e| format!("{e}. neohar stopped trying; send a prompt or retry."))
            } else {
                chat.error.clone()
            },
            started_at: Timestamp(chat.started_at),
            time_text: crate::git_view::ago(now, chat.started_at),
        }
    }

    fn publish_all(&mut self) {
        let ids: Vec<ChatId> = self.chats.keys().copied().collect();
        for id in ids {
            self.publish_chat(id);
        }
        self.publish_order();
    }

    fn publish_chat(&self, id: ChatId) {
        let Some(chat) = self.chats.get(&id) else {
            return;
        };
        if let Ok(json) = serde_json::to_value(self.value(chat)) {
            self.hub.set(Projection::HermesChats, &id.to_string(), json);
        }
    }

    fn publish_order(&self) {
        let keys: Vec<String> = self.chats.keys().rev().map(ToString::to_string).collect();
        self.hub.set_order(Projection::HermesChats, &keys);
    }

    /// Publishes what changed. The hub compares the values, so a row that did not change makes
    /// no patch.
    fn publish(&mut self) {
        let ids: Vec<ChatId> = self.chats.keys().copied().collect();
        for id in ids {
            self.publish_chat(id);
            let Some(chat) = self.chats.get_mut(&id) else {
                continue;
            };
            if !chat.thread.has_changes() {
                continue;
            }
            let streaming = chat.working();
            let changes = chat.thread.take_changes(streaming);
            let projection = Projection::HermesChat(id);
            for key in &changes.removed {
                self.hub.remove(projection, key);
            }
            for item in changes.items {
                if let Ok(json) = serde_json::to_value(&item) {
                    self.hub.set(projection, &item.id, json);
                }
            }
            if let Some(order) = changes.order {
                self.hub.set_order(projection, &order);
            }
        }
    }
}

fn choices(list: Option<&Value>, id_key: &str) -> Vec<HermesChoice> {
    list.and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(|choice| {
                    let id = choice.get(id_key)?.as_str()?.to_owned();
                    let name = choice
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or(&id)
                        .to_owned();
                    Some(HermesChoice { id, name })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn folder_name(cwd: &str) -> String {
    std::path::Path::new(cwd)
        .file_name()
        .map_or_else(|| cwd.to_owned(), |n| n.to_string_lossy().into_owned())
}

/// 15234 → "15k".
fn kilo(tokens: u64) -> String {
    if tokens >= 1000 {
        format!("{}k", tokens / 1000)
    } else {
        tokens.to_string()
    }
}
