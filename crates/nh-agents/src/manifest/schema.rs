//! The types of a manifest. A manifest is read from TOML into these types. Every key
//! except `schema` and `id` is optional, so that a file of the user can give only what it changes.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The version of the manifest format that this crate reads.
pub const SCHEMA: u32 = 1;

/// The priority of a manifest that gives none. A user wrapper uses a higher one.
pub const DEFAULT_PRIORITY: i32 = 100;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Manifest {
    pub schema: u32,
    /// `[a-z][a-z0-9_-]{0,31}`.
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The id of a manifest to start from. This one gives only what it changes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extends: Option<String>,
    /// Below this version the capabilities are reduced to `observe`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default)]
    pub detect: Detect,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<VersionRule>,
    /// How to ask the harness for its login state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<Auth>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub launch: Option<Launch>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume: Option<Resume>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity: Option<Identity>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub integration: Option<Integration>,
    /// Only the ranges that the golden recordings cover.
    #[serde(default)]
    pub capabilities: Vec<CapabilityRange>,
    /// A harness event to an execution state, or to an interaction that opens: `working`, `idle`,
    /// `error`, `session_ended` or `open:<kind>`.
    #[serde(default)]
    pub states: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interactions: Option<Interactions>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub yolo: Option<Yolo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screen: Option<ScreenRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transcript: Option<Transcript>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui: Option<UiHints>,
}

/// How a run of the harness is found in a pane. A run is detected when ANY process in the
/// foreground tree of the pane matches ANY rule.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Detect {
    /// When two manifests match, the higher one wins.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<i32>,
    /// Names the shared daemon whose hooks do not run in the pane.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_loop: Option<String>,
    #[serde(default)]
    pub process: Vec<ProcessRule>,
}

/// A rule for one process. A process matches when every key that the rule has matches.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ProcessRule {
    #[serde(default)]
    pub exe_basename: Vec<String>,
    /// The program of a script: `node`, `bun`. It goes with `script_regex`.
    #[serde(default)]
    pub interpreter: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub script_regex: Option<String>,
    /// Matched against the canonical path of the executable, for a path in the Nix store.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exe_path_regex: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub argv_regex: Option<String>,
    /// The name that a process had before `exec -a` changed it.
    #[serde(default)]
    pub argv0_basename: Vec<String>,
    /// Variables that the process has, with these values.
    #[serde(default)]
    pub env: BTreeMap<String, String>,
}

/// How to learn the version of the harness. The command runs once for each executable and
/// modification time.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct VersionRule {
    pub command: Vec<String>,
    /// A regular expression with the group `v`.
    pub regex: String,
}

/// How to ask the harness for its login state. The command runs in the login
/// environment with no stdin and a limit of ten seconds. It must not start a login, call a model or
/// change a file of the harness. `logged_in` is tried first, then `logged_out`. When neither
/// matches, the state is `unknown`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Auth {
    /// The command line of the probe: `["claude", "auth", "status", "--json"]`.
    #[serde(default)]
    pub status: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logged_in: Option<AuthRule>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logged_out: Option<AuthRule>,
    /// The method of the login, when the probe gives it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<MethodRule>,
    /// What the person does to log in on this machine. The core shows it with the state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub login_hint: Option<String>,
}

/// A rule that reads the result of the probe. It matches when every key that it has matches, and
/// a rule needs at least one key.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AuthRule {
    /// The exit status of the command.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit: Option<i32>,
    /// A regular expression that matches in the text of both outputs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub regex: Option<String>,
    /// A JSON pointer (RFC 6901) into the standard output, read as JSON: `/loggedIn`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub json: Option<String>,
    /// The value that the field of `json` must have. Without it, the field must be there and
    /// not `null` or `false`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub equals: Option<serde_json::Value>,
}

/// How to read the method of the login. The text comes from the field of `json`, or from the
/// group `m` of `regex` (the whole match when the expression has no group `m`). `values` gives the
/// method for each text: `subscription`, `api_key` or `token`. A text that `values` does not have
/// gives no method.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MethodRule {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub json: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub regex: Option<String>,
    #[serde(default)]
    pub values: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Launch {
    #[serde(default)]
    pub argv: Vec<String>,
    /// The command line with a session id that neohar chooses. `{session_id}` is the id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub argv_with_session: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id_format: Option<String>,
    /// Where a prompt goes on the command line: `trailing`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_arg: Option<String>,
    /// The arguments that choose a model. `{model}` is the name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_arg: Option<Vec<String>>,
    /// The arguments that choose an effort level. `{effort}` is the level.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort_arg: Option<Vec<String>>,
    /// The arguments that keep the agent of a read-only workflow node from editing files.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub read_only_args: Vec<String>,
    /// The arguments that let the agent read one more folder, such as the folder of the reports
    /// of a workflow. `{add_dir}` is the folder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub add_dir_arg: Option<Vec<String>>,
    /// The models that the form of a task offers, in the order of the menu.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub models: Vec<ModelEntry>,
    /// The effort levels that the form of a task offers, from the lowest.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub efforts: Vec<String>,
    /// Where the list of models comes from, before `models`: `codex` is the model catalog of
    /// Codex. `models` stays the list when the catalog is not there.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_catalog: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ready: Option<Ready>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ModelEntry {
    /// The name on the command line.
    pub id: String,
    /// The name in the menu. Without it, the menu shows the id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Ready {
    /// The rule of the screen manifest that says the input box is there.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screen_rule: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Resume {
    /// The command line to resume a session. `{resume_id}` is the handle.
    #[serde(default)]
    pub argv: Vec<String>,
    /// The command line to continue the last session of the directory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub continue_argv: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub needs_cwd: Option<bool>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Identity {
    /// Where the session id comes from, in order: `hook:<Event>`, `preassigned`, `transcript_scan`.
    #[serde(default)]
    pub sources: Vec<String>,
    /// The transcript file of a session. `~` is the home directory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transcript_glob: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd_slug: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Integration {
    /// `hooks`, `transcript`, `screen`, `app_server`.
    #[serde(default)]
    pub transports: Vec<String>,
    /// `builtin:<id>`: the installer that neohar has.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub install: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub headless: Option<String>,
}

/// What the harness can do in a range of versions and a mode.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CapabilityRange {
    /// A range of versions, as `semver` writes it: `>=2.1.0, <2.2.0`.
    pub versions: String,
    /// `tui`.
    #[serde(default)]
    pub mode: String,
    #[serde(default)]
    pub observe: bool,
    #[serde(default)]
    pub gate: bool,
    /// The kinds of interaction that the harness answers itself: `approval`, `question`,
    /// `plan_review`.
    #[serde(default)]
    pub answer_native: Vec<String>,
    #[serde(default)]
    pub answer_keystroke: bool,
    #[serde(default)]
    pub resume: bool,
    #[serde(default)]
    pub survive_disconnect: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Interactions {
    #[serde(default)]
    pub question_tools: Vec<String>,
    #[serde(default)]
    pub plan_tools: Vec<String>,
}

/// How to tell from the command line that a run has no approval dialog.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Yolo {
    /// A flag that does it alone.
    #[serde(default)]
    pub flags: Vec<String>,
    /// Arguments that do it when they come one after the other: `["-a", "never"]`.
    #[serde(default)]
    pub sequences: Vec<Vec<String>>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ScreenRef {
    /// The id of the screen manifest.
    pub manifest: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Transcript {
    /// `claude_jsonl`, `codex_rollout`.
    pub format: String,
    #[serde(default)]
    pub usage: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct UiHints {
    #[serde(default)]
    pub interrupt_keys: Vec<String>,
    #[serde(default)]
    pub quit_keys: Vec<String>,
}

impl Manifest {
    /// The priority of the manifest.
    pub fn priority(&self) -> i32 {
        self.detect.priority.unwrap_or(DEFAULT_PRIORITY)
    }

    /// The JSON Schema of a manifest, for the editors of the users.
    pub fn json_schema() -> serde_json::Value {
        serde_json::to_value(schemars::schema_for!(Manifest)).unwrap_or_default()
    }
}
