//! The core of a recording: an in-process `nh-core` that serves the API socket, and the controls
//! that the steps `core = { op = "…" }` use (`scenario.rs`).
//!
//! A recording with a core is a delivery case. The hook shim of the harness
//! talks to this core, not to the small socket of the recorder. The steps answer, release, end
//! and start the core while the harness runs, and they note what happened in `core/events.jsonl`.
//! `core/interactions.json` has the interactions at the end.
//!
//! The functions here do not wait. The session of the recording loops with its own reads between
//! two looks at the core, so that the screen of the harness stays current.

use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use nh_core::{Core, CoreConfig, CoreHandle, RpcError, Tuning};
use nh_proto::paths::{Env, Platform};
use nh_record::holder::Scratch;
use rustix::process::{Pid, Signal, kill_process};
use serde_json::{Value, json};
use tokio::runtime::Handle;

use crate::{
    GoldenError,
    scenario::{CoreOp, CoreSettings},
};

/// The name of the session of the core. The socket is `<run>/golden/api.sock`.
const SESSION: &str = "golden";

/// The result of one answer of a race: `Ok` or the error code.
pub type Answer = Result<Value, String>;

pub struct CoreCtl {
    config: CoreConfig,
    core: Option<Core>,
    handle: CoreHandle,
    runtime: Handle,
    begin: Instant,
    events: Vec<Value>,
    /// The requests of the cores that ended, `(signals, gates)`.
    ended_totals: (u64, u64),
    /// The window that looks at the pane. A core that starts again opens it again.
    window: Option<String>,
}

impl CoreCtl {
    /// Starts the core over the directories of the scratch tree.
    pub fn start(
        scratch: &Scratch,
        nh: &Path,
        runtime: Handle,
        settings: &CoreSettings,
    ) -> Result<Self, GoldenError> {
        let paths = Env {
            platform: Platform::current(),
            uid: rustix::process::getuid().as_raw(),
            home: Some(scratch.home.clone()),
            config_override: Some(scratch.config.clone()),
            data_override: Some(scratch.data.clone()),
            runtime_override: Some(scratch.run.clone()),
            xdg_config_home: None,
            xdg_data_home: None,
            xdg_runtime_dir: None,
        };
        // The pane of the recording is a pane of the layout with no holder: the harness runs under
        // the recorder, and the core only needs the pane to own the token of the hook.
        let mut tuning = Tuning {
            start_holders: false,
            ..Tuning::default()
        };
        if let Some(ms) = settings.gate_timeout_ms {
            tuning.gate_timeout = Duration::from_millis(ms);
        }
        let config = CoreConfig {
            session: SESSION.to_owned(),
            paths,
            nh_path: nh.to_owned(),
            base_env: Vec::new(),
            tuning,
            host: Some(nh_core::InProcessHost::noop()),
            pane_tokens: Vec::new(),
        };
        let mut config = config;
        let core = Core::start(config.clone())
            .map_err(|e| GoldenError::Setup(format!("the core does not start: {e}")))?;
        let handle = core.handle();
        // The recording runs one pane. Its token is the one that the recipe puts in the
        // environment of the harness, and a core that starts again keeps it.
        let made = runtime
            .block_on(handle.rpc("workspace.create", json!({ "name": "Golden" })))
            .map_err(|e| GoldenError::Setup(format!("the pane of the recording: {e}")))?;
        let pane: ulid::Ulid = made["pane"]
            .as_str()
            .and_then(|id| id.parse::<nh_proto::api::ids::PaneId>().ok())
            .map(nh_proto::api::ids::PaneId::ulid)
            .ok_or_else(|| GoldenError::Setup("the pane of the recording has no id".into()))?;
        config.pane_tokens = vec![(pane, crate::recipe::PANE_TOKEN.to_owned())];
        handle
            .pane_tokens()
            .register(pane, crate::recipe::PANE_TOKEN);
        Ok(Self {
            config,
            core: Some(core),
            handle,
            runtime,
            begin: Instant::now(),
            events: Vec::new(),
            ended_totals: (0, 0),
            window: None,
        })
    }

    /// The socket of the API: what `NEOHAR_SOCKET` names.
    pub fn socket(&self) -> Result<PathBuf, GoldenError> {
        self.config
            .paths
            .api_socket(SESSION)
            .map_err(|e| GoldenError::Setup(format!("the API socket path: {e}")))
    }

    /// Calls a method of the core and waits for the result.
    fn rpc(&self, method: &str, params: Value) -> Result<Value, RpcError> {
        self.runtime.block_on(self.handle.rpc(method, params))
    }

    /// The interactions, oldest first.
    pub fn interactions(&self) -> Vec<Value> {
        self.rpc("debug.interactions", json!({}))
            .ok()
            .and_then(|v| v["interactions"].as_array().cloned())
            .unwrap_or_default()
    }

    /// The interaction that an op names: `index`, or the last one.
    pub fn target(&self, op: &CoreOp) -> Option<Value> {
        let all = self.interactions();
        match op.index {
            Some(index) => all.into_iter().nth(index),
            None => all.into_iter().next_back(),
        }
    }

    /// Whether an interaction has the properties that the op asks for.
    pub fn fits(interaction: &Value, op: &CoreOp) -> bool {
        op.state
            .as_deref()
            .is_none_or(|s| interaction["state"] == s)
            && op.mode.as_deref().is_none_or(|m| interaction["mode"] == m)
            && op.lease.is_none_or(|l| interaction["lease"] == l)
            && op.acked.is_none_or(|a| interaction["acked"] == a)
    }

    /// The interaction that fits, or `None`.
    pub fn find(&self, op: &CoreOp) -> Option<Value> {
        self.target(op).filter(|i| Self::fits(i, op))
    }

    fn id(&self, op: &CoreOp) -> Result<String, GoldenError> {
        self.target(op)
            .and_then(|i| i["id"].as_str().map(str::to_owned))
            .ok_or_else(|| GoldenError::Step(format!("core {}: there is no interaction", op.op)))
    }

    /// Notes an event in `core/events.jsonl`.
    pub fn note(&mut self, op: &str, detail: Value) {
        self.events.push(json!({
            "t": self.begin.elapsed().as_micros() as f64 / 1_000_000.0,
            "op": op,
            "detail": detail,
        }));
    }

    fn decision(op: &CoreOp, behavior: &str) -> Value {
        let mut decision = json!({ "kind": "approval", "behavior": behavior });
        if let Some(message) = &op.message {
            decision["message"] = json!(message);
        }
        if op.interrupt {
            decision["stop"] = json!(true);
        }
        decision
    }

    /// `debug.answer` on the interaction of the op.
    pub fn answer(&mut self, op: &CoreOp) -> Result<Value, GoldenError> {
        let id = self.id(op)?;
        let behavior = op.decision.as_deref().unwrap_or("allow");
        let result = self.rpc(
            "debug.answer",
            json!({
                "interaction": id,
                "decision": Self::decision(op, behavior),
                "via": "cli",
                "by": op.by.as_deref().unwrap_or("golden"),
                "hold": op.hold,
            }),
        );
        self.note_result("answer", &result);
        match result {
            Ok(v) => Ok(v),
            Err(e) if op.may_fail => Ok(json!({ "error": e.code, "details": e.details })),
            Err(e) => Err(GoldenError::Step(format!("core answer: {e}"))),
        }
    }

    /// Two answers at the same time. Returns `(first, second)`, each `Ok` or the error code.
    pub fn race(&mut self, op: &CoreOp) -> Result<(Answer, Answer), GoldenError> {
        let id = self.id(op)?;
        let call = |behavior: &str, by: &str| {
            self.rpc(
                "debug.answer",
                json!({"interaction": id, "decision": {"kind": "approval", "behavior": behavior}, "via": "cli", "by": by}),
            )
            .map_err(|e| e.code.to_string())
        };
        let (first, second) = (
            op.first.clone().unwrap_or_default(),
            op.second.clone().unwrap_or_default(),
        );
        let (a, b) = std::thread::scope(|scope| {
            let a = scope.spawn(|| call(&first, "first"));
            let b = scope.spawn(|| call(&second, "second"));
            (a.join(), b.join())
        });
        let (Ok(a), Ok(b)) = (a, b) else {
            return Err(GoldenError::Step("a thread of the race panicked".into()));
        };
        self.note(
            "race",
            json!({
                "first": {"decision": first, "result": a.as_ref().map_or_else(|c| json!({"error": c}), |_| json!("ok"))},
                "second": {"decision": second, "result": b.as_ref().map_or_else(|c| json!({"error": c}), |_| json!("ok"))},
            }),
        );
        Ok((a, b))
    }

    /// `debug.deliver`.
    pub fn deliver(&mut self, op: &CoreOp) -> Result<(), GoldenError> {
        let id = self.id(op)?;
        let result = self.rpc("debug.deliver", json!({"interaction": id}));
        self.note_result("deliver", &result);
        result
            .map(|_| ())
            .map_err(|e| GoldenError::Step(format!("core deliver: {e}")))
    }

    /// `window.focus_report`: a window of the core looks at the pane of the recording, or the app
    /// goes to the background. The focus counts after the debounce of the core.
    pub fn focus(&mut self, op: &CoreOp) -> Result<(), GoldenError> {
        let window = match self.window.clone() {
            Some(window) => window,
            None => {
                let opened = self
                    .rpc("window.open", json!({}))
                    .map_err(|e| GoldenError::Step(format!("core focus: {e}")))?;
                let window = opened["window"]
                    .as_str()
                    .ok_or_else(|| GoldenError::Step("core focus: no window id".into()))?
                    .to_owned();
                self.window = Some(window.clone());
                window
            }
        };
        let pane = self
            .config
            .pane_tokens
            .first()
            .map(|(pane, _)| nh_proto::api::ids::PaneId::new(*pane).to_string())
            .ok_or_else(|| GoldenError::Step("core focus: the recording has no pane".into()))?;
        let focused = op.focused.unwrap_or(true);
        let result = self.rpc(
            "window.focus_report",
            json!({
                "window": window,
                "focused": focused,
                "visible": true,
                "active_pane": pane,
            }),
        );
        self.note_result("focus", &result);
        result
            .map(|_| ())
            .map_err(|e| GoldenError::Step(format!("core focus: {e}")))
    }

    /// `debug.release`.
    pub fn release(&mut self, op: &CoreOp) -> Result<(), GoldenError> {
        let id = self.id(op)?;
        let result = self.rpc(
            "debug.release",
            json!({"interaction": id, "reason": op.reason.as_deref().unwrap_or("released")}),
        );
        self.note_result("release", &result);
        result
            .map(|_| ())
            .map_err(|e| GoldenError::Step(format!("core release: {e}")))
    }

    /// Ends the core as a crash does: the runtime stops, and the socket is gone.
    pub fn kill(&mut self) -> Result<(), GoldenError> {
        let Some(core) = self.core.take() else {
            return Err(GoldenError::Step("the core is down already".into()));
        };
        let (signals, gates) = self.handle.api_totals();
        self.ended_totals = (self.ended_totals.0 + signals, self.ended_totals.1 + gates);
        self.note("kill", json!({"signals": signals, "gates": gates}));
        self.window = None;
        core.shutdown(Duration::from_secs(5));
        Ok(())
    }

    /// Starts a new core over the same directories.
    pub fn restart(&mut self) -> Result<(), GoldenError> {
        if self.core.is_some() {
            return Err(GoldenError::Step("the core runs already".into()));
        }
        let core = Core::start(self.config.clone())
            .map_err(|e| GoldenError::Step(format!("the core does not restart: {e}")))?;
        self.handle = core.handle();
        self.core = Some(core);
        let restored = self.interactions().len();
        self.note("start", json!({"interactions": restored}));
        Ok(())
    }

    /// Kills the process of the shim that holds the interaction.
    pub fn kill_shim(&mut self, op: &CoreOp) -> Result<(), GoldenError> {
        let target = self
            .target(op)
            .ok_or_else(|| GoldenError::Step("core kill_shim: there is no interaction".into()))?;
        let pid = target["shim_pid"]
            .as_i64()
            .and_then(|p| i32::try_from(p).ok())
            .and_then(Pid::from_raw)
            .ok_or_else(|| {
                GoldenError::Step("core kill_shim: the core does not know the pid".into())
            })?;
        kill_process(pid, Signal::KILL)
            .map_err(|e| GoldenError::Step(format!("core kill_shim: {e}")))?;
        self.note("kill_shim", json!({"interaction": target["id"]}));
        Ok(())
    }

    fn note_result(&mut self, op: &str, result: &Result<Value, RpcError>) {
        let detail = match result {
            Ok(v) => json!({"result": v}),
            Err(e) => json!({"error": e.code, "details": e.details}),
        };
        self.note(op, detail);
    }

    /// The requests that the cores of the recording got, `(signals, gates)`.
    pub fn totals(&self) -> (u64, u64) {
        let (signals, gates) = self.handle.api_totals();
        (self.ended_totals.0 + signals, self.ended_totals.1 + gates)
    }

    /// `core/events.jsonl`.
    pub fn events_text(&self) -> String {
        self.events.iter().map(|e| format!("{e}\n")).collect()
    }

    /// `core/interactions.json`.
    pub fn interactions_text(&self) -> String {
        serde_json::to_string_pretty(&self.interactions()).unwrap_or_default() + "\n"
    }
}
