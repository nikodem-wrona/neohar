//! The scenario file of a recording.
//!
//! One TOML file holds both halves of a recording: what the mock model says (`[[turn]]` and the
//! other keys of [`mock_llm::Scenario`]) and what the driver does to the harness (`[[step]]`).
//!
//! ```toml
//! harness = "claude"        # optional: the recording refuses another harness
//! after = "startup"         # optional: run the scenario of this name (the same directory) first,
//!                           # in the same scratch tree, and record only this one
//! mode = "pty"              # "pty" (the default: the harness in a terminal) or "pipe" (a program
//!                           # that talks JSON lines on its standard input and output)
//! cols = 120
//! rows = 40
//! args = ["--debug-file", "{scratch}/debug.log"]   # optional: arguments for the harness
//!
//! [settings.permissions]    # optional: merged into the settings of the harness
//! defaultMode = "default"   # (settings.json of Claude Code, config.toml of Codex)
//!
//! [[file]]                  # optional: a file that the recipe writes before the harness starts
//! path = "{scratch}/mcp.json"    # inside the scratch tree; {golden} stands for the nh-golden binary
//! content = "{}"
//!
//! hook_marker = "_neohar"   # optional: an extra key (set to true) in every hook object that the
//!                           # recipe writes, to see whether the harness accepts it
//!
//! [state]                  # optional: merged into .claude.json (Claude Code only)
//! hasResetAutoModeOptInForDefaultOffer = true
//!
//! [[hook_reply]]            # optional: what a hook prints, instead of nothing
//! event = "PermissionRequest"
//! reply = { hookSpecificOutput = { hookEventName = "PermissionRequest", decision = { behavior = "allow" } } }
//!
//! [[turn]]                  # the mock model
//! blocks = [{ type = "text", text = "Hello from the mock model." }]
//!
//! [[mask]]                  # text that changes from run to run, hidden when screens are compared
//! pattern = "done \\d+:\\d+ [AP]M"
//! with = "done <time>"
//!
//! [[step]]
//! wait_screen = "❯"
//! [[step]]
//! send = "say hello"
//! [[step]]
//! key = "enter"
//! [[step]]
//! wait_screen = "Hello from the mock model\\."
//! [[step]]
//! snapshot = "answered"     # wait until the output is quiet, then keep this screen for comparisons
//! ```
//!
//! A step has one action and, for the actions that wait, an optional `timeout_ms`:
//!
//! | Action | Meaning |
//! |---|---|
//! | `send = "text"` | Write the text to the harness, as typed. `\r` is Enter. |
//! | `key = "enter"` | Press a key: `enter`, `esc`, `tab`, `shift-tab`, `backspace`, `up`, `down`, `left`, `right`, `f1` to `f12`, `ctrl-c`, `ctrl-d`, `ctrl-o`, ... |
//! | `wait_screen = "regex"` | Wait until a row of the screen matches. |
//! | `wait_signal = "Event"` | Wait until a hook of this event arrives, after the last one that a step took. |
//! | `sleep_ms = 200` | Pause. Only for pacing typing, never to wait for something. |
//! | `expect_screen = "regex"` | Fail the recording if no row matches now. |
//! | `expect_signal = "Event"` | Fail if no hook of this event came since the last step that took one. |
//! | `expect_exit = true` | Wait until the harness exits. |
//! | `snapshot = "name"` | Wait until the output is quiet for 400 ms, then mark the cast. |
//! | `capture = "name"` | Keep the screen as it is now, without waiting for quiet output. For a screen that never rests (a spinner with a clock). |
//! | `processes = "name"` | Write the tree of processes under the harness to `processes/<name>.txt` (both modes). |
//!
//! These actions are for `mode = "pipe"`, and the others except `send`, `key`, `wait_screen`,
//! `expect_screen` and `snapshot` work in both modes:
//!
//! | Action | Meaning |
//! |---|---|
//! | `send_json = { ... }` | Write the value as one JSON line to the standard input. A string with `{{find:REGEX}}` gets the first match in the lines so far (group 1 if there is one). |
//! | `wait_line = "regex"` | Wait for a line of the standard output that matches, after the last one that a step took. |
//! | `respond = { ... }` | Answer the oldest `control_request` that nobody answered. The string `"$input"` in the value stands for `request.input`. |
//! | `close_stdin = true` | Close the standard input. |
//! | `answer_request = { method = "…", result = { … } }` | Answer the oldest request of the server with this `method` that nobody answered: `{"id": <its id>, "result": …}`. |
//! | `start_peer = "name"` | Start a peer that has `start = "step"`. |
//! | `core = { op = "…" }` | Control the core of the recording (see below). |
//!
//! **The core.** A scenario with a `[core]` table runs an in-process core (`nh-core`) as the API
//! socket, instead of the small socket of the recorder. The hook shim then holds a
//! `PermissionRequest` for the core, and the steps `core = { … }` act on the core.
//! This is how the delivery cases run against a live harness. The operations:
//!
//! | `op` | Fields | Meaning |
//! |---|---|---|
//! | `wait` | `state?`, `lease?`, `mode?`, `acked?`, `index?` | Wait until the interaction fits (`index` counts from 0; the last one by default). |
//! | `expect` | the same | Fail the recording unless the interaction fits now. |
//! | `answer` | `decision` (`allow` or `deny`), `message?`, `interrupt?`, `hold?`, `by?` | `debug.answer`. The error of the core is noted and does not fail the step if `may_fail = true`. |
//! | `race` | `first`, `second` | Two `debug.answer` at the same time with different decisions. |
//! | `deliver` | | `debug.deliver`: deliver a decision that `hold` kept back. |
//! | `release` | `reason?` | `debug.release`: the hook prints nothing and the harness shows its dialog. |
//! | `focus` | `focused?` | `window.focus_report`: the user looks at the pane (`focused = false`: the app is in the background). The core waits for the debounce of 500 ms before the focus counts. |
//! | `kill` | | End the core as a crash does. |
//! | `start` | | Start a new core over the same directories. |
//! | `kill_shim` | | Kill the process of the shim that holds the interaction. |
//!
//! `[core]` has `gate_timeout_ms` (the deadline of an interaction).
//!
//! In `mode = "pipe"`, a scenario can have more programs and connections than the harness. A
//! `[[peer]]` is a program of the same binary (`kind = "process"`, with `args` and an optional
//! `ready_path` to wait for) or a WebSocket connection to a Unix socket (`kind = "ws"`, with
//! `path`). A step with `peer = "name"` acts on that peer. A step without `peer` acts on the
//! harness. `writable = ["…"]` adds paths where the sandbox allows writes (`{uid}` is the user id).

use std::time::Duration;

use serde::Deserialize;

/// The default wait of a step. A harness takes seconds to start.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// How the harness runs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    /// In a terminal (a pty under a holder). The steps type keys and read the screen.
    #[default]
    Pty,
    /// With pipes. The steps write and read JSON lines.
    Pipe,
}

impl Mode {
    pub fn name(self) -> &'static str {
        match self {
            Self::Pty => "pty",
            Self::Pipe => "pipe",
        }
    }
}

/// One thing that the driver does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    SendJson(serde_json::Value),
    WaitLine(String),
    Respond(serde_json::Value),
    CloseStdin,
    Send(String),
    Key(String),
    WaitScreen(String),
    WaitSignal(String),
    Sleep(Duration),
    ExpectScreen(String),
    ExpectSignal(String),
    ExpectExit,
    Snapshot(String),
    Capture(String),
    /// Writes the process tree of the harness to `processes/<name>.txt`.
    Processes(String),
    StartPeer(String),
    Core(CoreOp),
    AnswerRequest {
        method: String,
        result: serde_json::Value,
    },
}

/// A step: an action and how long it may wait.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step {
    pub action: Action,
    pub timeout: Duration,
    /// The peer that the action is for. `None` is the harness.
    pub peer: Option<String>,
}

/// One operation on the core of a recording (`core = { op = "…" }`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoreOp {
    pub op: String,
    pub state: Option<String>,
    pub lease: Option<bool>,
    pub mode: Option<String>,
    pub acked: Option<bool>,
    pub index: Option<usize>,
    pub decision: Option<String>,
    pub message: Option<String>,
    #[serde(default)]
    pub interrupt: bool,
    #[serde(default)]
    pub hold: bool,
    #[serde(default)]
    pub may_fail: bool,
    pub by: Option<String>,
    pub first: Option<String>,
    pub second: Option<String>,
    pub reason: Option<String>,
    /// For `focus`: whether the window has the focus of the OS. The default is `true`.
    pub focused: Option<bool>,
}

/// The `[core]` table of a scenario.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoreSettings {
    /// How long the core holds a hook for an answer. The default of the core is 30 minutes.
    pub gate_timeout_ms: Option<u64>,
}

/// What a peer is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PeerKind {
    /// A program of the harness binary with its own arguments.
    Process {
        args: Vec<String>,
        /// A path to wait for after the start (a socket that the program makes).
        ready_path: Option<String>,
    },
    /// A WebSocket connection to a Unix socket. Each message is one JSON text.
    Ws { path: String },
}

/// A program or a connection next to the harness (pipe mode).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Peer {
    pub name: String,
    pub kind: PeerKind,
    /// Starts before the first step. Otherwise a `start_peer` step starts it.
    pub at_begin: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPeer {
    name: String,
    kind: String,
    #[serde(default)]
    args: Vec<String>,
    ready_path: Option<String>,
    path: Option<String>,
    start: Option<String>,
}

/// Text that differs between two runs of the same scenario.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mask {
    pub pattern: String,
    pub with: String,
}

/// A file that the recipe writes before the harness starts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileSpec {
    /// The path, with the placeholders of the scratch tree and `{golden}`.
    pub path: String,
    pub content: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawFile {
    path: String,
    content: String,
}

/// What the hook of one event prints on its standard output, after the shim ran.
#[derive(Clone, Debug, PartialEq)]
pub struct HookReply {
    pub event: String,
    pub reply: serde_json::Value,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawHookReply {
    event: String,
    reply: toml::Value,
}

/// A scenario file, read.
#[derive(Clone, Debug)]
pub struct Scenario {
    pub harness: Option<String>,
    /// The name of a scenario in the same directory that runs first, in the same scratch tree.
    /// Only this scenario is recorded. It uses what the first run left: for `claude --continue`,
    /// the transcript of the first session.
    pub after: Option<String>,
    pub mode: Mode,
    pub cols: u16,
    pub rows: u16,
    /// Arguments for the harness. `{scratch}`, `{home}`, `{tmp}` and `{work}` stand for the
    /// scratch directories.
    pub args: Vec<String>,
    /// Files for the recipe to write. `{golden}` in a path, in a content and in `args` stands for
    /// this binary, so that a scenario can start `nh-golden mcp-elicit` as an MCP server.
    pub files: Vec<FileSpec>,
    /// A key that the recipe adds, with the value `true`, to every hook object it writes.
    pub hook_marker: Option<String>,
    /// Settings that are merged into the files of the recipe.
    pub settings: toml::Table,
    /// State that is merged into the `.claude.json` of the seed. Codex has no such file.
    pub state: toml::Table,
    pub hook_replies: Vec<HookReply>,
    /// Programs and connections next to the harness (pipe mode).
    pub peers: Vec<Peer>,
    /// Runs an in-process core as the API socket (`[core]`).
    pub core: Option<CoreSettings>,
    /// Paths where the sandbox allows writes, besides the scratch tree. `{uid}` stands for the
    /// user id.
    pub writable: Vec<String>,
    pub steps: Vec<Step>,
    pub masks: Vec<Mask>,
    /// The model's half, as the mock reads it.
    pub mock: mock_llm::Scenario,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawStep {
    send: Option<String>,
    key: Option<String>,
    wait_screen: Option<String>,
    wait_signal: Option<String>,
    sleep_ms: Option<u64>,
    expect_screen: Option<String>,
    expect_signal: Option<String>,
    expect_exit: Option<bool>,
    snapshot: Option<String>,
    capture: Option<String>,
    processes: Option<String>,
    send_json: Option<toml::Value>,
    wait_line: Option<String>,
    respond: Option<toml::Value>,
    close_stdin: Option<bool>,
    start_peer: Option<String>,
    core: Option<CoreOp>,
    answer_request: Option<RawAnswer>,
    peer: Option<String>,
    timeout_ms: Option<u64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawAnswer {
    method: String,
    result: toml::Value,
}

/// Why a scenario does not load.
#[derive(Debug, thiserror::Error)]
pub enum ScenarioError {
    #[error("the scenario is not valid TOML: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("{0}")]
    Mock(#[from] mock_llm::ScenarioError),
    #[error("step {step}: {message}")]
    Step { step: usize, message: String },
    #[error("{0}")]
    Other(String),
}

fn step_error(step: usize, message: impl Into<String>) -> ScenarioError {
    ScenarioError::Step {
        step,
        message: message.into(),
    }
}

impl RawStep {
    fn into_step(self, index: usize) -> Result<Step, ScenarioError> {
        let mut actions = Vec::new();
        let mut take = |name: &str, action: Option<Action>| {
            if let Some(action) = action {
                actions.push((name.to_owned(), action));
            }
        };
        take("send", self.send.map(Action::Send));
        take("key", self.key.map(Action::Key));
        take("wait_screen", self.wait_screen.map(Action::WaitScreen));
        take("wait_signal", self.wait_signal.map(Action::WaitSignal));
        take(
            "sleep_ms",
            self.sleep_ms
                .map(|ms| Action::Sleep(Duration::from_millis(ms))),
        );
        take(
            "expect_screen",
            self.expect_screen.map(Action::ExpectScreen),
        );
        take(
            "expect_signal",
            self.expect_signal.map(Action::ExpectSignal),
        );
        take(
            "expect_exit",
            self.expect_exit
                .filter(|yes| *yes)
                .map(|_| Action::ExpectExit),
        );
        take("snapshot", self.snapshot.map(Action::Snapshot));
        take("capture", self.capture.map(Action::Capture));
        take("processes", self.processes.map(Action::Processes));
        let to_json = |value: toml::Value| {
            serde_json::to_value(&value).map_err(|e| step_error(index, e.to_string()))
        };
        take(
            "send_json",
            self.send_json
                .map(to_json)
                .transpose()?
                .map(Action::SendJson),
        );
        take("wait_line", self.wait_line.map(Action::WaitLine));
        take(
            "respond",
            self.respond.map(to_json).transpose()?.map(Action::Respond),
        );
        take(
            "close_stdin",
            self.close_stdin
                .filter(|yes| *yes)
                .map(|_| Action::CloseStdin),
        );
        take("start_peer", self.start_peer.map(Action::StartPeer));
        take("core", self.core.map(Action::Core));
        let answer = self
            .answer_request
            .map(|a| {
                serde_json::to_value(&a.result)
                    .map(|result| Action::AnswerRequest {
                        method: a.method,
                        result,
                    })
                    .map_err(|e| step_error(index, e.to_string()))
            })
            .transpose()?;
        take("answer_request", answer);
        let mut actions = actions.into_iter();
        let (Some((_, action)), None) = (actions.next(), actions.next()) else {
            return Err(step_error(index, "a step needs exactly one action"));
        };
        match &action {
            Action::Send(text)
            | Action::Key(text)
            | Action::WaitSignal(text)
            | Action::Snapshot(text)
            | Action::Capture(text)
            | Action::Processes(text)
                if text.is_empty() =>
            {
                return Err(step_error(index, "the text of an action is empty"));
            }
            Action::WaitScreen(pattern)
            | Action::ExpectScreen(pattern)
            | Action::WaitLine(pattern) => {
                regex::Regex::new(pattern)
                    .map_err(|e| step_error(index, format!("a bad regular expression: {e}")))?;
            }
            Action::Key(name) if crate::keys::key_bytes(name).is_none() => {
                return Err(step_error(index, format!("an unknown key {name:?}")));
            }
            _ => {}
        }
        Ok(Step {
            action,
            timeout: self
                .timeout_ms
                .map_or(DEFAULT_TIMEOUT, Duration::from_millis),
            peer: self.peer,
        })
    }
}

impl Scenario {
    /// Reads a scenario file.
    pub fn parse(text: &str) -> Result<Self, ScenarioError> {
        let mut table: toml::Table = text.parse()?;
        let harness = match table.remove("harness") {
            None => None,
            Some(toml::Value::String(name)) => Some(name),
            Some(_) => return Err(ScenarioError::Other("harness is not a string".into())),
        };
        let size = |table: &mut toml::Table, key: &str, default: u16| match table.remove(key) {
            None => Ok(default),
            Some(toml::Value::Integer(n)) => {
                u16::try_from(n).ok().filter(|n| *n >= 20).ok_or_else(|| {
                    ScenarioError::Other(format!("{key} is not a size from 20 to 65535"))
                })
            }
            Some(_) => Err(ScenarioError::Other(format!("{key} is not a number"))),
        };
        let args: Vec<String> = match table.remove("args") {
            None => Vec::new(),
            Some(value) => value
                .try_into()
                .map_err(|_| ScenarioError::Other("args is not a list of strings".into()))?,
        };
        let core: Option<CoreSettings> = match table.remove("core") {
            None => None,
            Some(value) => Some(value.try_into()?),
        };
        let writable: Vec<String> = match table.remove("writable") {
            None => Vec::new(),
            Some(value) => value
                .try_into()
                .map_err(|_| ScenarioError::Other("writable is not a list of strings".into()))?,
        };
        let raw_peers: Vec<RawPeer> = match table.remove("peer") {
            Some(value) => value.try_into()?,
            None => Vec::new(),
        };
        let mut peers: Vec<Peer> = Vec::new();
        for raw in raw_peers {
            let valid = !raw.name.is_empty()
                && raw.name != "main"
                && raw
                    .name
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_');
            if !valid || peers.iter().any(|p| p.name == raw.name) {
                return Err(ScenarioError::Other(format!(
                    "the peer name {:?} is not lower case letters, digits, - and _, or it is used twice, or it is main",
                    raw.name
                )));
            }
            let kind = match raw.kind.as_str() {
                "process" => PeerKind::Process {
                    args: raw.args,
                    ready_path: raw.ready_path,
                },
                "ws" => PeerKind::Ws {
                    path: raw.path.ok_or_else(|| {
                        ScenarioError::Other(format!("the ws peer {} needs a path", raw.name))
                    })?,
                },
                other => {
                    return Err(ScenarioError::Other(format!(
                        "the peer {} has the kind {other:?}, not process or ws",
                        raw.name
                    )));
                }
            };
            let at_begin = match raw.start.as_deref() {
                None | Some("begin") => true,
                Some("step") => false,
                Some(other) => {
                    return Err(ScenarioError::Other(format!(
                        "the peer {} has start = {other:?}, not begin or step",
                        raw.name
                    )));
                }
            };
            peers.push(Peer {
                name: raw.name,
                kind,
                at_begin,
            });
        }
        let raw_files: Vec<RawFile> = match table.remove("file") {
            Some(value) => value.try_into()?,
            None => Vec::new(),
        };
        let files: Vec<FileSpec> = raw_files
            .into_iter()
            .map(|f| FileSpec {
                path: f.path,
                content: f.content,
            })
            .collect();
        if let Some(bad) = files.iter().find(|f| f.path.is_empty()) {
            return Err(ScenarioError::Other(format!(
                "a file has an empty path (content: {:?})",
                bad.content
            )));
        }
        let after = match table.remove("after") {
            None => None,
            Some(toml::Value::String(name))
                if !name.is_empty()
                    && name.chars().all(|c| {
                        c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_'
                    }) =>
            {
                Some(name)
            }
            Some(_) => {
                return Err(ScenarioError::Other(
                    "after is not the name of a scenario (lower case letters, digits, - and _)"
                        .into(),
                ));
            }
        };
        let mode = match table.remove("mode") {
            None => Mode::Pty,
            Some(toml::Value::String(name)) if name == "pty" => Mode::Pty,
            Some(toml::Value::String(name)) if name == "pipe" => Mode::Pipe,
            Some(_) => {
                return Err(ScenarioError::Other(
                    "mode is not \"pty\" or \"pipe\"".into(),
                ));
            }
        };
        let hook_marker = match table.remove("hook_marker") {
            None => None,
            Some(toml::Value::String(key)) if !key.is_empty() => Some(key),
            Some(_) => {
                return Err(ScenarioError::Other(
                    "hook_marker is not a non-empty string".into(),
                ));
            }
        };
        let settings: toml::Table = match table.remove("settings") {
            None => toml::Table::new(),
            Some(toml::Value::Table(settings)) => settings,
            Some(_) => return Err(ScenarioError::Other("settings is not a table".into())),
        };
        let state: toml::Table = match table.remove("state") {
            None => toml::Table::new(),
            Some(toml::Value::Table(state)) => state,
            Some(_) => return Err(ScenarioError::Other("state is not a table".into())),
        };
        let raw_replies: Vec<RawHookReply> = match table.remove("hook_reply") {
            Some(value) => value.try_into()?,
            None => Vec::new(),
        };
        let mut hook_replies = Vec::new();
        for (index, raw) in raw_replies.into_iter().enumerate() {
            let valid =
                !raw.event.is_empty() && raw.event.chars().all(|c| c.is_ascii_alphanumeric());
            if !valid {
                return Err(ScenarioError::Other(format!(
                    "hook_reply {index}: the event {:?} is not a name like PermissionRequest",
                    raw.event
                )));
            }
            if hook_replies
                .iter()
                .any(|r: &HookReply| r.event == raw.event)
            {
                return Err(ScenarioError::Other(format!(
                    "hook_reply {index}: the event {} has a reply already",
                    raw.event
                )));
            }
            let reply = serde_json::to_value(&raw.reply)
                .map_err(|e| ScenarioError::Other(format!("hook_reply {index}: {e}")))?;
            hook_replies.push(HookReply {
                event: raw.event,
                reply,
            });
        }
        let cols = size(&mut table, "cols", 120)?;
        let rows = size(&mut table, "rows", 40)?;
        let raw_steps: Vec<RawStep> = match table.remove("step") {
            Some(value) => value.try_into()?,
            None => Vec::new(),
        };
        let masks: Vec<Mask> = match table.remove("mask") {
            Some(value) => value.try_into()?,
            None => Vec::new(),
        };
        for (index, mask) in masks.iter().enumerate() {
            regex::Regex::new(&mask.pattern)
                .map_err(|e| ScenarioError::Other(format!("mask {index}: {e}")))?;
        }
        let steps = raw_steps
            .into_iter()
            .enumerate()
            .map(|(index, raw)| raw.into_step(index))
            .collect::<Result<Vec<_>, _>>()?;
        for (index, step) in steps.iter().enumerate() {
            let pty_only = matches!(
                step.action,
                Action::Send(_)
                    | Action::Key(_)
                    | Action::WaitScreen(_)
                    | Action::ExpectScreen(_)
                    | Action::Snapshot(_)
                    | Action::Capture(_)
            );
            let pipe_only = matches!(
                step.action,
                Action::SendJson(_)
                    | Action::WaitLine(_)
                    | Action::Respond(_)
                    | Action::CloseStdin
                    | Action::StartPeer(_)
                    | Action::AnswerRequest { .. }
            );
            if mode == Mode::Pipe && pty_only {
                return Err(step_error(index, "this action needs mode = \"pty\""));
            }
            if mode == Mode::Pty && pipe_only {
                return Err(step_error(index, "this action needs mode = \"pipe\""));
            }
        }
        if mode == Mode::Pty && !peers.is_empty() {
            return Err(ScenarioError::Other("peers need mode = \"pipe\"".into()));
        }
        for (index, step) in steps.iter().enumerate() {
            if let Some(name) = &step.peer
                && !peers.iter().any(|p| &p.name == name)
            {
                return Err(step_error(
                    index,
                    format!("the peer {name:?} is not declared"),
                ));
            }
            if let Action::StartPeer(name) = &step.action
                && !peers.iter().any(|p| &p.name == name && !p.at_begin)
            {
                return Err(step_error(
                    index,
                    format!("start_peer needs a peer {name:?} with start = \"step\""),
                ));
            }
        }
        for (index, step) in steps.iter().enumerate() {
            let Action::Core(op) = &step.action else {
                continue;
            };
            if core.is_none() {
                return Err(step_error(index, "a core step needs a [core] table"));
            }
            if mode == Mode::Pipe {
                return Err(step_error(index, "a core step needs mode = \"pty\""));
            }
            const OPS: &[&str] = &[
                "wait",
                "expect",
                "answer",
                "race",
                "deliver",
                "release",
                "focus",
                "kill",
                "start",
                "kill_shim",
            ];
            if !OPS.contains(&op.op.as_str()) {
                return Err(step_error(
                    index,
                    format!("the core op {:?} is not known", op.op),
                ));
            }
            let needs = |ok: bool, what: &str| {
                if ok {
                    Ok(())
                } else {
                    Err(step_error(
                        index,
                        format!("the core op {} needs {what}", op.op),
                    ))
                }
            };
            match op.op.as_str() {
                "answer" => needs(
                    matches!(op.decision.as_deref(), Some("allow" | "deny")),
                    "decision = \"allow\" or \"deny\"",
                )?,
                "race" => needs(
                    matches!(op.first.as_deref(), Some("allow" | "deny"))
                        && matches!(op.second.as_deref(), Some("allow" | "deny")),
                    "first and second, each allow or deny",
                )?,
                "wait" | "expect" => needs(
                    op.state.is_some()
                        || op.lease.is_some()
                        || op.mode.is_some()
                        || op.acked.is_some(),
                    "at least one of state, lease, mode and acked",
                )?,
                _ => {}
            }
        }
        let mut process_names = std::collections::BTreeSet::new();
        for (index, step) in steps.iter().enumerate() {
            if let Action::Processes(name) = &step.action {
                let valid = name
                    .starts_with(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit())
                    && name.chars().all(|c| {
                        c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_'
                    });
                if !valid || !process_names.insert(name.clone()) {
                    return Err(step_error(
                        index,
                        format!(
                            "the name {name:?} of a process list is not lower case letters, digits, - and _, or it is used twice"
                        ),
                    ));
                }
            }
        }
        let mut names = std::collections::BTreeSet::new();
        for (index, step) in steps.iter().enumerate() {
            if let Action::Snapshot(name) | Action::Capture(name) = &step.action {
                let valid = name
                    .starts_with(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit())
                    && name.chars().all(|c| {
                        c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_'
                    });
                if !valid {
                    return Err(step_error(
                        index,
                        format!(
                            "the name {name:?} of a snapshot is not lower case letters, digits, - and _"
                        ),
                    ));
                }
                if name == "final" || !names.insert(name.clone()) {
                    return Err(step_error(
                        index,
                        format!(
                            "the name {name:?} of a snapshot is used twice, or is the reserved name final"
                        ),
                    ));
                }
            }
        }
        let mock = mock_llm::Scenario::parse(
            &toml::to_string(&table).map_err(|e| ScenarioError::Other(e.to_string()))?,
        )?;
        Ok(Self {
            harness,
            after,
            mode,
            cols,
            rows,
            args,
            files,
            hook_marker,
            settings,
            state,
            hook_replies,
            peers,
            core,
            writable,
            steps,
            masks,
            mock,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = r#"
harness = "claude"
after = "startup"
cols = 100
rows = 30
args = ["--debug-file", "{scratch}/d.log"]
hook_marker = "_neohar"
default_reply = "Fallback."

[[turn]]
blocks = [{ type = "text", text = "Hello." }]

[settings.permissions]
defaultMode = "default"

[[hook_reply]]
event = "PermissionRequest"
reply = { hookSpecificOutput = { hookEventName = "PermissionRequest", decision = { behavior = "allow" } } }

[[mask]]
pattern = "\\d+:\\d+"
with = "<time>"

[[step]]
wait_screen = "❯"
timeout_ms = 5000

[[step]]
send = "say hello"

[[step]]
key = "enter"

[[step]]
snapshot = "answered"
"#;

    #[test]
    fn a_scenario_has_a_model_half_and_a_driver_half() {
        let s = Scenario::parse(GOOD).expect("a good scenario");
        assert_eq!(
            (s.harness.as_deref(), s.cols, s.rows),
            (Some("claude"), 100, 30)
        );
        assert_eq!(s.args, ["--debug-file", "{scratch}/d.log"]);
        assert_eq!(s.hook_marker.as_deref(), Some("_neohar"));
        assert_eq!(s.after.as_deref(), Some("startup"));
        assert_eq!(
            s.settings["permissions"]["defaultMode"].as_str(),
            Some("default")
        );
        assert_eq!(s.hook_replies.len(), 1);
        assert_eq!(s.hook_replies[0].event, "PermissionRequest");
        assert_eq!(
            s.hook_replies[0].reply["hookSpecificOutput"]["decision"]["behavior"],
            "allow"
        );
        assert_eq!(s.mock.turns.len(), 1);
        assert_eq!(s.mock.default_reply, "Fallback.");
        assert_eq!(s.steps.len(), 4);
        assert_eq!(s.steps[0].timeout, Duration::from_secs(5));
        assert_eq!(s.steps[1].timeout, DEFAULT_TIMEOUT);
        assert_eq!(s.steps[3].action, Action::Snapshot("answered".into()));
        assert_eq!(s.masks[0].with, "<time>");
    }

    #[test]
    fn settings_keep_their_plain_keys_next_to_their_tables() {
        let s = Scenario::parse(
            "[settings]\nskipAutoPermissionPrompt = true\n\n[settings.permissions]\ndefaultMode = \"default\"\n",
        )
        .expect("a scenario");
        assert_eq!(s.settings["skipAutoPermissionPrompt"].as_bool(), Some(true));
        assert_eq!(
            s.settings["permissions"]["defaultMode"].as_str(),
            Some("default")
        );
    }

    #[test]
    fn a_pipe_scenario_takes_json_steps_and_refuses_screen_steps() {
        let s = Scenario::parse(
            "mode = \"pipe\"\n[[step]]\nsend_json = { type = \"user\" }\n[[step]]\nwait_line = \"result\"\n[[step]]\nrespond = { behavior = \"allow\", updatedInput = \"$input\" }\n[[step]]\nclose_stdin = true\n[[step]]\nexpect_exit = true",
        )
        .expect("a pipe scenario");
        assert_eq!(s.mode, Mode::Pipe);
        assert_eq!(s.steps.len(), 5);
        assert_eq!(
            s.steps[0].action,
            Action::SendJson(serde_json::json!({ "type": "user" }))
        );
        assert_eq!(
            s.steps[2].action,
            Action::Respond(serde_json::json!({ "behavior": "allow", "updatedInput": "$input" }))
        );
        let bad = |text: &str| {
            Scenario::parse(text)
                .expect_err("a bad scenario")
                .to_string()
        };
        assert!(bad("mode = \"pipe\"\n[[step]]\nkey = \"enter\"").contains("needs mode = \"pty\""));
        assert!(bad("[[step]]\nsend_json = {}").contains("needs mode = \"pipe\""));
        assert!(bad("mode = \"tty\"").contains("mode"));
        assert!(bad("after = \"../x\"").contains("after"));
        assert!(bad("mode = \"pipe\"\n[[step]]\nwait_line = \"(\"").contains("regular expression"));
    }

    #[test]
    fn peers_writable_paths_and_the_answer_step_load() {
        let s = Scenario::parse(
            "mode = \"pipe\"\nwritable = [\"/tmp/x-{uid}\"]\n\
             [[peer]]\nname = \"a\"\nkind = \"ws\"\npath = \"{scratch}/s\"\nstart = \"step\"\n\
             [[peer]]\nname = \"srv\"\nkind = \"process\"\nargs = [\"x\"]\nready_path = \"{scratch}/s\"\n\
             [[step]]\nstart_peer = \"a\"\n\
             [[step]]\npeer = \"a\"\nanswer_request = { method = \"m\", result = { decision = \"accept\" } }",
        )
        .expect("a scenario with peers");
        assert_eq!(s.writable, ["/tmp/x-{uid}"]);
        assert_eq!(s.peers.len(), 2);
        assert!(!s.peers[0].at_begin && s.peers[1].at_begin);
        assert_eq!(s.steps[0].action, Action::StartPeer("a".into()));
        assert_eq!(s.steps[1].peer.as_deref(), Some("a"));
        assert_eq!(
            s.steps[1].action,
            Action::AnswerRequest {
                method: "m".into(),
                result: serde_json::json!({ "decision": "accept" })
            }
        );
        let bad = |text: &str| {
            Scenario::parse(text)
                .expect_err("a bad scenario")
                .to_string()
        };
        assert!(
            bad("[[peer]]\nname = \"a\"\nkind = \"ws\"\npath = \"p\"").contains("peers need mode")
        );
        assert!(
            bad("mode = \"pipe\"\n[[step]]\npeer = \"zz\"\nsend_json = {}")
                .contains("not declared")
        );
        assert!(bad("mode = \"pipe\"\n[[step]]\nstart_peer = \"zz\"").contains("start_peer needs"));
        assert!(
            bad("mode = \"pipe\"\n[[peer]]\nname = \"main\"\nkind = \"ws\"\npath = \"p\"")
                .contains("main")
        );
        assert!(
            bad("mode = \"pipe\"\n[[peer]]\nname = \"a\"\nkind = \"ws\"").contains("needs a path")
        );
        assert!(
            bad("mode = \"pipe\"\n[[peer]]\nname = \"a\"\nkind = \"tcp\"")
                .contains("not process or ws")
        );
    }

    #[test]
    fn the_core_table_and_the_core_steps_load() {
        let s = Scenario::parse(
            "[core]\ngate_timeout_ms = 1500\n\
             [[step]]\ncore = { op = \"wait\", state = \"open\", lease = true }\n\
             [[step]]\ncore = { op = \"answer\", decision = \"allow\", hold = true }\n\
             [[step]]\ncore = { op = \"race\", first = \"allow\", second = \"deny\" }\n\
             [[step]]\ncore = { op = \"kill\" }",
        )
        .expect("a core scenario");
        assert_eq!(s.core.expect("core").gate_timeout_ms, Some(1500));
        let Action::Core(op) = &s.steps[0].action else {
            panic!("a core step");
        };
        assert_eq!(
            (op.op.as_str(), op.state.as_deref(), op.lease),
            ("wait", Some("open"), Some(true))
        );
        let Action::Core(op) = &s.steps[1].action else {
            panic!("a core step");
        };
        assert!(op.hold);
        let bad = |text: &str| {
            Scenario::parse(text)
                .expect_err("a bad scenario")
                .to_string()
        };
        assert!(bad("[[step]]\ncore = { op = \"kill\" }").contains("needs a [core] table"));
        assert!(bad("[core]\n[[step]]\ncore = { op = \"dance\" }").contains("not known"));
        assert!(bad("[core]\n[[step]]\ncore = { op = \"answer\" }").contains("decision"));
        assert!(bad("[core]\n[[step]]\ncore = { op = \"wait\" }").contains("at least one"));
        assert!(
            bad("[core]\n[[step]]\ncore = { op = \"race\", first = \"allow\" }")
                .contains("first and second")
        );
        assert!(bad("[core]\n[[step]]\ncore = { op = \"kill\", what = 1 }").contains("what"));
        assert!(
            bad("mode = \"pipe\"\n[core]\n[[step]]\ncore = { op = \"kill\" }")
                .contains("mode = \"pty\"")
        );
    }

    #[test]
    fn the_defaults_are_a_large_terminal_and_no_steps() {
        let s = Scenario::parse("").expect("an empty scenario");
        assert_eq!((s.cols, s.rows, s.steps.len()), (120, 40, 0));
    }

    #[test]
    fn mistakes_are_errors_that_name_the_step() {
        let bad = |text: &str| {
            Scenario::parse(text)
                .expect_err("a bad scenario")
                .to_string()
        };
        assert!(
            bad("[[step]]\nsend = \"a\"\nkey = \"enter\"")
                .contains("step 0: a step needs exactly one action")
        );
        assert!(bad("[[step]]\ntimeout_ms = 5").contains("exactly one action"));
        assert!(bad("[[step]]\nwait_screen = \"(\"").contains("regular expression"));
        assert!(bad("[[step]]\nkey = \"hyper\"").contains("unknown key"));
        assert!(bad("[[step]]\nsend = \"\"").contains("empty"));
        assert!(bad("[[step]]\nsend = \"a\"\nwhat = 1").contains("what"));
        assert!(bad("cols = 3").contains("from 20"));
        assert!(bad("args = [1]").contains("args"));
        assert!(bad("settings = 3").contains("settings"));
        assert!(bad("hook_marker = 3").contains("hook_marker"));
        assert!(bad("[[hook_reply]]\nevent = \"a b\"\nreply = {}").contains("hook_reply 0"));
        assert!(
            bad("[[hook_reply]]\nevent = \"Stop\"\nreply = {}\n[[hook_reply]]\nevent = \"Stop\"\nreply = {}")
                .contains("has a reply already")
        );
        assert!(bad("[[step]]\nsnapshot = \"Bad Name\"").contains("not lower case"));
        assert!(bad("[[step]]\nsnapshot = \"final\"").contains("reserved"));
        assert!(
            bad("[[step]]\nsnapshot = \"a\"\n[[step]]\nsnapshot = \"a\"").contains("used twice")
        );
        assert!(bad("[[mask]]\npattern = \"(\"\nwith = \"x\"").contains("mask 0"));
        assert!(
            bad("[[turn]]\nblocks = []").contains("no blocks"),
            "the model half is checked too"
        );
    }
}
