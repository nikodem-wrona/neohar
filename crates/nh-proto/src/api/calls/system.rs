//! Params and results of `events.*`, `projection.*`, `config.*`, `policy.*`, `integration.*`,
//! `palette.*`, `rich.*`, `timeline.*` and `jobs.*`.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::api::{
    common::{Harness, wire},
    events::{Cursor, Event, EventFilter},
    ids::{JobId, Ref, RunId, WindowId},
    model::{Capabilities, PaletteCommand, SettingsSection, TimelineItem},
    projections::{Projection, ProjectionMessage},
    time::Timestamp,
};

wire! {
    // --- events ---

    /// `events.subscribe {cursor, filter}`. Events arrive as `events.event`
    /// notifications. A client whose cursor is older than the retention, or from another
    /// `log_epoch`, gets `events.truncated` and takes a fresh snapshot.
    pub struct EventsSubscribe {
        /// Where to start. Without it the subscription starts at the end of the log.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub cursor: Option<Cursor>,
        #[serde(default)]
        pub filter: EventFilter,
    }

    pub struct EventsSubscribed {
        /// The id of the subscription, as it is in the notifications.
        pub subscription: String,
        /// The cursor of the end of the log at the moment of the call.
        pub cursor: Cursor,
    }

    pub struct EventsUnsubscribe {
        pub subscription: String,
    }

    /// The params of the notification `events.event`.
    pub struct EventNotification {
        pub subscription: String,
        pub event: Event,
    }

    /// The params of the notification `events.truncated`.
    pub struct EventsTruncated {
        pub subscription: String,
        /// The cursor to start from after a fresh snapshot.
        pub cursor: Cursor,
    }

    // --- projections ---

    /// `projection.subscribe`. Messages arrive as `projection.message` notifications.
    pub struct ProjectionSubscribe {
        pub name: Projection,
        /// The version the client has. The core sends only patches if it still retains them.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub since: Option<u64>,
    }

    pub struct ProjectionSubscribed {
        pub subscription: String,
    }

    pub struct ProjectionUnsubscribe {
        pub subscription: String,
    }

    /// The params of the notification `projection.message`.
    pub struct ProjectionNotification {
        pub subscription: String,
        pub message: ProjectionMessage,
    }

    /// `projection.versions {names}`: the version of each projection named. A client
    /// compares them with the versions of its subscriptions, because a subscription can stop
    /// without a gap in its versions: a channel of the host can lose one message and hold the
    /// ones after it, and the core drops a sink whose send fails.
    pub struct ProjectionVersions {
        pub names: Vec<Projection>,
    }

    pub struct ProjectionVersionsResult {
        /// The version by name. A projection that the core has not made is left out.
        pub versions: BTreeMap<String, u64>,
    }

    // --- config ---

    pub struct ConfigGet {
        /// One section, for example `approvals`. Without it all sections.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub section: Option<String>,
    }

    pub struct ConfigValues {
        pub sections: BTreeMap<String, SettingsSection>,
    }

    /// Sets a value in `config.toml`. The file keeps its comments and its order. A `null` removes
    /// the key, so the default applies again.
    pub struct ConfigSet {
        /// The path, for example `approvals.mode`.
        pub key: String,
        pub value: Value,
    }

    pub struct ConfigReload {}

    pub struct ConfigReloaded {
        /// Errors of the files. The old values stay in force.
        pub errors: Vec<String>,
    }

    // --- policy ---

    #[derive(Copy, Eq, Hash)]
    #[serde(rename_all = "snake_case")]
    pub enum PolicyDecision {
        Allow,
        Deny,
    }

    /// A rule.
    pub struct PolicyRule {
        /// Made by the core if absent.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub id: Option<String>,
        pub decision: PolicyDecision,
        /// Default: any harness.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pub harness: Vec<Harness>,
        /// The exact name of the tool, or `*`.
        pub tool: String,
        /// For `Bash`: a prefix glob on the normalized command. `*` only at the end.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub command: Option<String>,
        /// For file tools: globs relative to the workspace root.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pub paths: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub workspace: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub reason: Option<String>,
    }

    #[derive(Copy, Eq, Hash)]
    #[serde(rename_all = "snake_case")]
    pub enum PolicyScope {
        /// From the config file.
        Config,
        /// From "Allow always" in a card.
        Session,
    }

    pub struct PolicyRuleValue {
        pub id: String,
        pub rule: PolicyRule,
        pub scope: PolicyScope,
        pub created_at: Timestamp,
    }

    pub struct PolicyList {}

    pub struct PolicyListResult {
        pub rules: Vec<PolicyRuleValue>,
    }

    pub struct PolicyAdd {
        pub rule: PolicyRule,
    }

    pub struct PolicyAdded {
        pub id: String,
    }

    pub struct PolicyRemove {
        pub id: String,
    }

    pub struct PolicySuggest {}

    /// A rule that the user could add: approved at least 5 times, never denied, in 30 days.
    pub struct PolicySuggestion {
        pub rule: PolicyRule,
        /// The rule as a `[[policy.rule]]` block, ready to paste into `config.toml`.
        pub toml: String,
        pub fingerprint: String,
        pub approvals: u32,
        pub last_at: Timestamp,
    }

    pub struct PolicySuggestResult {
        pub suggestions: Vec<PolicySuggestion>,
    }

    // --- integrations ---

    /// Which integration. `All` means both harnesses.
    #[derive(Copy, Eq, Hash)]
    #[serde(rename_all = "snake_case")]
    pub enum IntegrationTarget {
        Claude,
        Codex,
        All,
    }

    #[derive(Copy, Eq, Hash, Default)]
    #[serde(rename_all = "snake_case")]
    pub enum IntegrationScope {
        #[default]
        User,
        Project,
    }

    pub struct IntegrationList {}

    pub struct IntegrationInfo {
        pub harness: Harness,
        pub installed: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub version_seen: Option<String>,
        /// The transports in use: `hooks`, `transcript`, `screen`, `process`.
        pub transports: Vec<String>,
    }

    pub struct IntegrationListResult {
        pub integrations: Vec<IntegrationInfo>,
    }

    pub struct IntegrationInstall {
        pub target: IntegrationTarget,
        #[serde(default)]
        pub dry_run: bool,
        #[serde(default)]
        pub scope: IntegrationScope,
        /// The repository for the scope `project`. The CLI gives its working directory.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub project_root: Option<String>,
    }

    #[derive(Copy, Eq, Hash)]
    #[serde(rename_all = "snake_case")]
    pub enum FileAction {
        Created,
        Modified,
        Removed,
        Unchanged,
    }

    pub struct FileChange {
        pub path: String,
        pub action: FileAction,
        /// The backup that the installer wrote before the first change.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub backup: Option<String>,
    }

    pub struct IntegrationChanges {
        pub changes: Vec<FileChange>,
        /// What the user must do, for example `/hooks` in Codex.
        pub notes: Vec<String>,
    }

    pub struct IntegrationStatusParams {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub target: Option<IntegrationTarget>,
    }

    pub struct IntegrationStatusEntry {
        pub harness: Harness,
        pub installed: bool,
        pub files: Vec<String>,
        /// Codex only: every hook is trusted.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub hooks_trusted: Option<bool>,
        pub version_validated: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub last_signal_at: Option<Timestamp>,
    }

    pub struct IntegrationStatusResult {
        pub status: Vec<IntegrationStatusEntry>,
    }

    pub struct IntegrationUninstall {
        pub target: IntegrationTarget,
        #[serde(default)]
        pub scope: IntegrationScope,
        /// The repository for the scope `project`. The CLI gives its working directory.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub project_root: Option<String>,
    }

    /// Runs the round trip of `nh integration doctor`. It returns a job.
    pub struct IntegrationDoctor {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub target: Option<IntegrationTarget>,
    }

    pub struct IntegrationCapabilitiesParams {}

    #[derive(Copy, Eq, Hash)]
    #[serde(rename_all = "snake_case")]
    pub enum LaunchMode {
        Tui,
        Headless,
    }

    /// One row of the capability matrix.
    pub struct CapabilityRow {
        pub harness: Harness,
        pub version: String,
        pub mode: LaunchMode,
        pub capabilities: Capabilities,
    }

    pub struct IntegrationCapabilitiesResult {
        pub rows: Vec<CapabilityRow>,
    }

    // --- doctor and debug bundle ---

    pub struct DoctorRun {}

    /// How a check of the doctor came out.
    #[derive(Copy, Eq, Hash)]
    #[serde(rename_all = "snake_case")]
    pub enum CheckStatus {
        Ok,
        /// A fact that needs no action.
        Info,
        /// Something works, with a limit or a risk.
        Warn,
        /// Something does not work.
        Fail,
    }

    pub struct DoctorCheck {
        /// A stable name, for example `runtime_dir`.
        pub id: String,
        pub title: String,
        pub status: CheckStatus,
        pub detail: String,
        /// What the person can do about it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub hint: Option<String>,
    }

    /// `doctor.run`: the checks of `nh doctor`. The same facts feed `health`.
    pub struct DoctorReport {
        pub checks: Vec<DoctorCheck>,
    }

    /// `debug.bundle`: writes a zip for a bug report. Everything in it has passed the redaction.
    pub struct DebugBundle {
        /// The file to write. The CLI always gives one. Without it the core writes
        /// `neohar-debug-<time>.zip` in the directory of the logs.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub out: Option<String>,
        /// Add the text of the panes. Without it the bundle has no scrollback.
        #[serde(default)]
        pub include_scrollback: bool,
    }

    pub struct DebugBundled {
        pub path: String,
        pub bytes: u64,
        pub files: u32,
    }

    // --- palette, rich, timeline, jobs ---

    pub struct PaletteQuery {
        pub query: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub limit: Option<u32>,
        /// The window that asks. A later version ranks the commands of that window first.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub window: Option<WindowId>,
    }

    pub struct PaletteMatch {
        pub command: PaletteCommand,
        pub score: f32,
        /// The matched ranges of the title, in UTF-16 code units.
        pub ranges: Vec<(u32, u32)>,
    }

    pub struct PaletteResults {
        pub items: Vec<PaletteMatch>,
    }

    pub struct PaletteExecute {
        /// The id of the command (`PaletteCommand.id`).
        pub command: String,
        #[serde(default, skip_serializing_if = "Value::is_null")]
        pub args: Value,
        /// The window that runs the command. A command on the tab or the pane in focus needs it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub window: Option<WindowId>,
    }

    /// What `palette.execute` did. Some commands belong to the page and not to the core, for
    /// example the inbox. The core then names the action, and the page does it.
    pub struct PaletteExecuted {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub ui: Option<String>,
    }

    pub struct RichDiff {
        pub a: String,
        pub b: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub lang: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub path: Option<String>,
    }

    pub struct RichMarkdown {
        pub text: String,
    }

    pub struct RichHighlight {
        pub text: String,
        pub lang: String,
    }

    /// Older items of `timeline:<run>`. They are history, not live state.
    pub struct TimelinePageParams {
        pub run: Ref<RunId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub before_item: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub limit: Option<u32>,
    }

    pub struct TimelinePageResult {
        pub items: Vec<TimelineItem>,
        pub more: bool,
    }

    /// The full text of a `prompt` item or an `assistant` item of `timeline:<run>`. An
    /// item of the projection has a preview only.
    pub struct TimelineItemGet {
        pub run: Ref<RunId>,
        pub item: String,
    }

    /// The text as sanitized HTML. `truncated` is true when the text was longer than 64 KiB.
    pub struct TimelineItemText {
        pub html: String,
        pub truncated: bool,
    }

    pub struct JobsCancel {
        pub job: JobId,
    }
}
