//! `hermes.*`: the chats with Hermes Agent.
//!
//! The core runs `hermes acp` and speaks the Agent Client Protocol (ACP) to it ([`conn`]). That
//! process only shows sessions: the gateway daemon of Hermes owns them, so a turn continues when
//! the app quits, and the core resumes each session when it starts again. [`thread`] turns the
//! messages into the items of `hermes_chat:<chat>`, and [`worker`] owns the chats, the connection
//! and `hermes_chats`. `docs/plans/hermes-chat.md` has the plan and the results of the spike.

mod conn;
#[cfg(test)]
mod tests;
mod thread;
mod worker;

use std::{
    path::{Path, PathBuf},
    process::Stdio,
    sync::{Arc, Mutex, PoisonError},
    time::Duration,
};

use nh_proto::api::{
    calls::hermes::HermesStatus,
    envelope::ErrorCode,
    methods::{Call, Method},
};
use serde_json::Value;
use tokio::sync::{mpsc, oneshot};

use self::worker::{Reply, Request, Worker};
use crate::{
    actor::{ActorHandle, Origin},
    config::ConfigStore,
    error::RpcError,
    projection::ProjectionHub,
    runtime::Inner,
};

/// How long `hermes.check` waits for each run of the program.
const CHECK_TIMEOUT: Duration = Duration::from_secs(20);

/// The chats with Hermes: the queue to the worker.
pub(crate) struct Hermes {
    handle: tokio::runtime::Handle,
    hub: Arc<ProjectionHub>,
    config: Arc<ConfigStore>,
    env: Vec<(String, String)>,
    queue: Mutex<Option<mpsc::UnboundedSender<(Request, Reply)>>>,
}

impl Hermes {
    pub(crate) fn new(
        handle: tokio::runtime::Handle,
        hub: Arc<ProjectionHub>,
        config: Arc<ConfigStore>,
        env: Vec<(String, String)>,
    ) -> Arc<Self> {
        Arc::new(Self {
            handle,
            hub,
            config,
            env,
            queue: Mutex::new(None),
        })
    }

    /// Starts the worker. It loads the open chats and resumes their sessions.
    pub(crate) fn start(&self, actor: ActorHandle) {
        let (queue, requests) = mpsc::unbounded_channel();
        let (incoming, incoming_rx) = mpsc::unbounded_channel();
        let worker = Worker::new(
            Arc::clone(&self.hub),
            actor,
            Arc::clone(&self.config),
            self.env.clone(),
            incoming,
        );
        self.handle.spawn(worker.run(requests, incoming_rx));
        *self.queue.lock().unwrap_or_else(PoisonError::into_inner) = Some(queue);
    }

    async fn request(&self, request: Request) -> Result<Value, RpcError> {
        let (reply, answer) = oneshot::channel();
        let queue = self
            .queue
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
            .ok_or_else(|| RpcError::internal("the chats with Hermes have not started"))?;
        queue
            .send((request, reply))
            .map_err(|_| RpcError::internal("the chats with Hermes have stopped"))?;
        answer
            .await
            .map_err(|_| RpcError::internal("the chats with Hermes have stopped"))?
    }

    /// `hermes.check`: runs the program with `--version` and `--check`, and reads
    /// `approvals.mode` of the config of Hermes.
    pub(crate) async fn check(&self) -> HermesStatus {
        let config = self.config.config();
        let hermes = &config.hermes;
        let version = run_program(&hermes.command, &hermes.args, "--version", &self.env).await;
        let Ok(version) = version else {
            return HermesStatus {
                found: false,
                version: None,
                ok: false,
                approvals_mode: None,
                message: Some(format!(
                    "neohar did not find `{}`. Install Hermes Agent, or set hermes.command.",
                    hermes.command
                )),
            };
        };
        let check = run_program(&hermes.command, &hermes.args, "--check", &self.env).await;
        let home = hermes_home(&self.env);
        let approvals_mode = tokio::task::spawn_blocking(move || {
            std::fs::read_to_string(home.join("config.yaml"))
                .ok()
                .map(|text| approvals_mode(&text).unwrap_or_else(|| "smart".to_owned()))
        })
        .await
        .ok()
        .flatten();
        let (ok, message) = match check {
            Ok(_) => (true, None),
            Err(error) => (
                false,
                Some(format!(
                    "`{} {} --check` failed: {error}. Run `hermes model` to set a provider.",
                    hermes.command,
                    hermes.args.join(" ")
                )),
            ),
        };
        HermesStatus {
            found: true,
            version: Some(version.trim().to_owned()).filter(|v| !v.is_empty()),
            ok,
            approvals_mode,
            message,
        }
    }
}

/// Runs the program with one more argument, and gives the first line of its output.
async fn run_program(
    program: &str,
    args: &[String],
    extra: &str,
    env: &[(String, String)],
) -> Result<String, String> {
    let mut command = tokio::process::Command::new(program);
    command
        .args(args)
        .arg(extra)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if !env.is_empty() {
        command.env_clear().envs(env.iter().map(|(k, v)| (k, v)));
    }
    let output = tokio::time::timeout(CHECK_TIMEOUT, command.output())
        .await
        .map_err(|_| "no answer".to_owned())?
        .map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&output.stdout);
    let first = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    if output.status.success() {
        Ok(first.to_owned())
    } else {
        let error = String::from_utf8_lossy(&output.stderr);
        let line = error
            .lines()
            .rev()
            .find(|l| !l.trim().is_empty())
            .unwrap_or(first);
        Err(line.to_owned())
    }
}

/// `$HERMES_HOME`, or `~/.hermes`.
fn hermes_home(env: &[(String, String)]) -> PathBuf {
    let get = |key: &str| {
        env.iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
            .or_else(|| std::env::var(key).ok())
    };
    if let Some(home) = get("HERMES_HOME").filter(|h| !h.is_empty()) {
        return PathBuf::from(home);
    }
    get("HOME").map_or_else(
        || PathBuf::from(".hermes"),
        |home| Path::new(&home).join(".hermes"),
    )
}

/// The value of `mode:` in the top-level `approvals:` block of a YAML text. No YAML parser: the
/// block is short, and a wrong guess only changes a hint.
fn approvals_mode(text: &str) -> Option<String> {
    let mut inside = false;
    for line in text.lines() {
        let content = line.split('#').next().unwrap_or("").trim_end();
        if content.trim().is_empty() {
            continue;
        }
        let indented = line.starts_with(' ') || line.starts_with('\t');
        if !indented {
            inside = content == "approvals:";
            continue;
        }
        if inside && let Some(value) = content.trim().strip_prefix("mode:") {
            let value = value.trim().trim_matches(|c| c == '"' || c == '\'');
            return (!value.is_empty()).then(|| value.to_owned());
        }
    }
    None
}

/// Whether a method is a call of this module.
pub(crate) fn handles(method: &str) -> bool {
    matches!(
        Method::from_name(method),
        Some(
            Method::HermesCheck
                | Method::HermesStart
                | Method::HermesPrompt
                | Method::HermesCancel
                | Method::HermesAnswer
                | Method::HermesSet
                | Method::HermesRetry
                | Method::HermesClose
        )
    )
}

/// Runs a call that [`handles`] accepted.
pub(crate) async fn run(
    inner: &Arc<Inner>,
    call: Call,
    _origin: Origin,
) -> Result<Value, RpcError> {
    let hermes = &inner.hermes;
    match call {
        Call::HermesCheck(_) => serde_json::to_value(hermes.check().await)
            .map_err(|e| RpcError::internal(e.to_string())),
        Call::HermesStart(p) => hermes.request(Request::Start(p)).await,
        Call::HermesPrompt(p) => {
            hermes
                .request(Request::Prompt {
                    chat: p.chat,
                    text: p.text,
                    input_id: p.input_id,
                })
                .await
        }
        Call::HermesCancel(p) => hermes.request(Request::Cancel(p.chat)).await,
        Call::HermesAnswer(p) => {
            hermes
                .request(Request::Answer {
                    chat: p.chat,
                    request: p.request,
                    option: p.option,
                })
                .await
        }
        Call::HermesSet(p) => {
            hermes
                .request(Request::Set {
                    chat: p.chat,
                    model: p.model,
                    mode: p.mode,
                })
                .await
        }
        Call::HermesRetry(p) => hermes.request(Request::Retry(p.chat)).await,
        Call::HermesClose(p) => hermes.request(Request::Close(p.chat)).await,
        other => Err(RpcError::new(
            ErrorCode::Internal,
            format!("not a hermes call: {other:?}"),
        )),
    }
}
