//! The values of the projections, and the shapes that the methods return for them.
//!
//! The core builds these in Rust. The UI draws them and never derives one from another.

use std::collections::BTreeMap;

use serde_json::Value;

use super::{
    common::{Axis, Bounds, Harness, Rect, RendererKind, Risk, Source, Urgency, Via, wire},
    envelope::CallError,
    ids::{
        DeviceId, GroupId, InteractionId, JobId, NotificationId, PaneId, RunId, TabId, WindowId,
        WorkspaceId,
    },
    rich::{Command, Diff, DiffStats, Markdown},
    time::Timestamp,
};

// --- windows, tree and panes ---------------------------------------------------------------

wire! {
    /// `windows`: one app window. Restored on launch.
    pub struct WindowValue {
        pub id: WindowId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub bounds: Option<Bounds>,
        /// The panes that the window draws. The render pacer keys on this.
        pub visible_panes: Vec<PaneId>,
        /// The pane that has keyboard focus in the window.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub focused_pane: Option<PaneId>,
        /// The tab that the window shows.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub tab: Option<TabId>,
        /// The workspace of that tab. The tab bar shows its tabs.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub workspace: Option<WorkspaceId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub refresh_hz: Option<f32>,
        /// The OS focus of the window, as the host reports it.
        pub focused: bool,
        pub visible: bool,
        /// The interaction that the window must show now (`interaction.reveal`). The window
        /// acts when `seq` changes.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub reveal: Option<Reveal>,
        /// The device id of the client that owns the window. A client opens a native
        /// window only for its own rows. A row with none waits for the first client that attaches.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub client: Option<DeviceId>,
        /// The single view of a phone: the window draws only its focused pane,
        /// or the first pane of its tab. The core keeps it in memory only.
        #[serde(default)]
        pub single: bool,
    }

    /// A request to a window: show the inbox, and focus the card of this interaction.
    #[derive(Copy, Eq)]
    pub struct Reveal {
        pub interaction: InteractionId,
        /// Grows with each request, so that the same interaction can be asked for twice.
        pub seq: u64,
    }

    /// The group of a workspace in the sidebar.
    pub struct GroupRef {
        pub id: GroupId,
        pub name: String,
    }

    /// `tree`: a workspace with its tabs.
    pub struct WorkspaceValue {
        pub id: WorkspaceId,
        /// The short handle, for example `w3`.
        pub handle: String,
        pub name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub root: Option<String>,
        /// The label of the machine. The local machine is `local`.
        pub machine: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub group: Option<GroupRef>,
        pub tabs: Vec<TabValue>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub selected_tab: Option<TabId>,
    }

    pub struct TabValue {
        pub id: TabId,
        /// The short handle, for example `w3:t2`.
        pub handle: String,
        pub name: String,
        pub layout: LayoutNode,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pub floating: Vec<FloatingPane>,
        /// The pane that fills the tab, if one is zoomed.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub zoomed: Option<PaneId>,
    }

    /// The layout of a tab: panes in nested splits. The UI renders it with CSS.
    #[serde(tag = "type", rename_all = "snake_case")]
    pub enum LayoutNode {
        Pane { pane: PaneId },
        Split {
            axis: Axis,
            /// One share for each child. They add up to 1.
            ratios: Vec<f32>,
            children: Vec<LayoutNode>,
        },
    }

    pub struct FloatingPane {
        pub pane: PaneId,
        pub rect: Rect,
    }

    #[serde(tag = "state", rename_all = "snake_case")]
    pub enum PaneLiveness {
        Starting,
        Alive,
        Exited {
            #[serde(default, skip_serializing_if = "Option::is_none")]
            code: Option<i32>,
        },
    }

    /// The mouse protocol that the program in the pane asked for.
    #[derive(Copy, Eq, Hash)]
    #[serde(rename_all = "snake_case")]
    pub enum MouseMode {
        None,
        X10,
        Normal,
        ButtonEvent,
        AnyEvent,
    }

    /// An OSC 52 clipboard write that waits for an answer.
    pub struct ClipboardRequest {
        pub bytes: u64,
        pub preview: String,
        /// The pane has a remembered Allow, and the last input came from a phone: the write waits
        /// only for the tap on Copy, which the clipboard of a phone needs.
        #[serde(default)]
        pub allowed: bool,
    }

    /// An OSC 52 clipboard write that no client got: the client that sent the last
    /// input to the pane is not connected.
    #[derive(Copy)]
    pub struct ClipboardDropped {
        pub bytes: u64,
        pub at: Timestamp,
    }

    /// `panes`: one pane.
    pub struct PaneValue {
        pub id: PaneId,
        /// The short handle, for example `w3:p5`.
        pub handle: String,
        /// The number that frames carry for this pane.
        pub render_id: u32,
        pub workspace: WorkspaceId,
        pub tab: TabId,
        pub title: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub cwd: Option<String>,
        /// The foreground process.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub fg_process: Option<String>,
        pub cols: u16,
        pub rows: u16,
        pub liveness: PaneLiveness,
        /// The pane shows the restored screen and the holder has not caught up.
        #[serde(default)]
        pub restoring: bool,
        #[serde(default)]
        pub alt_screen: bool,
        pub mouse_mode: MouseMode,
        #[serde(default)]
        pub bracketed_paste: bool,
        /// Inputs that were in flight when something went away (`pane.input_unconfirmed`).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pub input_unconfirmed: Vec<u64>,
        /// The window whose size the PTY follows.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub geometry_owner: Option<WindowId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub clipboard_request: Option<ClipboardRequest>,
        /// The last OSC 52 write that no client got. The next write that a client gets clears it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub clipboard_dropped: Option<ClipboardDropped>,
        /// The run of an agent in the pane.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub run: Option<RunId>,
    }
}

// --- agents ---------------------------------------------------------------------------------

wire! {
    /// Where a facet came from.
    #[derive(Copy)]
    pub struct Provenance {
        pub source: Source,
        /// From 0 to 1.
        pub confidence: f32,
        pub at: Timestamp,
    }

    #[serde(tag = "state", rename_all = "snake_case")]
    pub enum Liveness {
        Starting,
        Alive,
        Exited {
            #[serde(default, skip_serializing_if = "Option::is_none")]
            code: Option<i32>,
        },
    }

    #[serde(tag = "state", rename_all = "snake_case")]
    pub enum Execution {
        Starting,
        Working,
        Idle,
        Error,
        RateLimited {
            #[serde(default, skip_serializing_if = "Option::is_none")]
            resets_at: Option<Timestamp>,
        },
        Exited,
        Unknown,
    }

    #[serde(tag = "state", rename_all = "snake_case")]
    pub enum AdapterHealth {
        Healthy,
        Degraded { reason: String },
        Disconnected,
        UnvalidatedVersion,
    }

    /// The read state of the run for one client. The core keeps it for each subscription.
    #[derive(Copy, Eq, Hash)]
    #[serde(rename_all = "snake_case")]
    pub enum ReadState {
        Seen,
        Unseen,
    }

    pub struct LivenessFacet {
        pub value: Liveness,
        pub provenance: Provenance,
    }

    pub struct ExecutionFacet {
        pub value: Execution,
        pub provenance: Provenance,
    }

    pub struct HealthFacet {
        pub value: AdapterHealth,
        pub provenance: Provenance,
    }

    /// The five facets of a run.
    pub struct Facets {
        pub liveness: LivenessFacet,
        pub execution: ExecutionFacet,
        /// The ids of the open interactions.
        pub pending: Vec<InteractionId>,
        pub adapter_health: HealthFacet,
        pub read_state: ReadState,
    }

    /// What neohar can do with the run. The UI offers only what is listed.
    pub struct Capabilities {
        pub observe: bool,
        pub gate: bool,
        /// The kinds of interaction that have a native channel.
        pub answer_native: Vec<InteractionKind>,
        pub answer_keystroke: bool,
        pub reconcile: bool,
        pub resume: bool,
        pub steer: bool,
        pub survive_disconnect: bool,
    }

    /// Token use and cost. The cost is the figure of the harness when it reports one.
    #[derive(Default)]
    pub struct Usage {
        pub input_tokens: u64,
        pub output_tokens: u64,
        pub cache_read_tokens: u64,
        pub cache_write_tokens: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub cost_usd: Option<f64>,
    }

    pub struct RateLimitWindow {
        pub name: String,
        /// From 0 to 100.
        pub used_percent: f32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub resets_at: Option<Timestamp>,
    }

    /// How sure the core is about the pane of a run.
    #[serde(tag = "state", rename_all = "snake_case")]
    pub enum Binding {
        /// Deterministic: neohar started the run, a signal carried the token of the pane, or the
        /// user linked it.
        Bound,
        /// Correlation only. It can drive display, and never an answer.
        Suggested { confidence: f32 },
        Unlinked,
    }

    /// `agents`: one run of Claude Code or Codex.
    pub struct AgentValue {
        pub id: RunId,
        /// The unique name, if the run has one. A call can use it instead of the id.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub name: Option<String>,
        pub harness: Harness,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub harness_version: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub pane: Option<PaneId>,
        pub binding: Binding,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub session_id: Option<String>,
        pub facets: Facets,
        pub capabilities: Capabilities,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub model: Option<String>,
        pub usage: Usage,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub rate_limit: Option<RateLimitWindow>,
        /// The run bypasses the permission prompts of the harness.
        #[serde(default)]
        pub yolo: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub isolation: Option<String>,
        /// A resume handle exists (`claude --resume`, `codex resume`).
        #[serde(default)]
        pub resumable: bool,
        pub started_at: Timestamp,
    }

    #[derive(Copy, Eq, Hash)]
    #[serde(rename_all = "snake_case")]
    pub enum AttentionReason {
        Approval,
        Question,
        PlanReview,
        Error,
        RateLimited,
        /// The turn of the run ended while its pane had no focus, and nobody looked at the pane or
        /// typed into it since.
        TurnEnded,
    }

    /// `attention`: a run that needs the user, in the order of urgency. The number of
    /// entries is the badge count.
    pub struct AttentionValue {
        pub run: RunId,
        pub urgency: Urgency,
        pub reasons: Vec<AttentionReason>,
        pub interactions: Vec<InteractionId>,
        pub since: Timestamp,
    }
}

// --- interactions ---------------------------------------------------------------------------

wire! {
    #[derive(Copy, Eq, Hash)]
    #[serde(rename_all = "snake_case")]
    pub enum InteractionKind {
        Approval,
        Question,
        PlanReview,
        Notice,
    }

    /// The state of the delivery record, plus how an interaction can end otherwise.
    #[derive(Copy, Eq, Hash)]
    #[serde(rename_all = "snake_case")]
    pub enum InteractionState {
        Open,
        DecisionRecorded,
        Delivering,
        Delivered,
        DeliveryUnknown,
        Failed,
        Superseded,
        ResolvedElsewhere,
        Dismissed,
        Cancelled,
    }

    /// Whether the core holds the hook of the harness.
    #[derive(Copy, Eq, Hash)]
    #[serde(rename_all = "snake_case")]
    pub enum GateMode {
        Gate,
        Observe,
    }

    /// How an answer can reach the harness.
    #[derive(Copy, Eq, Hash)]
    #[serde(rename_all = "snake_case")]
    pub enum AnswerChannel {
        Native,
        Keystroke,
    }

    pub struct DeliveryInfo {
        /// The revision of the decision.
        pub rev: u32,
        /// How many times the core tried to deliver.
        pub attempts: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub via: Option<Via>,
        /// Who decided: a client name, `policy`, or the id of a window.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub decided_by: Option<String>,
        /// Why the delivery failed, when the state is `failed`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub fail_reason: Option<String>,
    }

    pub struct ApprovalPayload {
        /// The tool: `Bash`, `Edit`, `exec_command`, an MCP tool.
        pub tool: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub command: Option<Command>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub cwd: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pub paths: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub diff: Option<Diff>,
        /// The input of a tool that has no special card, cut at a cap.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub input: Option<Value>,
        /// The last lines of the pane, for the context strip.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pub peek: Vec<String>,
        /// A rule of the policy matches this request, but the core cannot answer it,
        /// for example "The rule `pnpm-test` would allow this."
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub policy_note: Option<String>,
    }

    pub struct QuestionOption {
        pub label: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub description: Option<String>,
    }

    pub struct Question {
        pub question: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub header: Option<String>,
        pub options: Vec<QuestionOption>,
        #[serde(default)]
        pub multi_select: bool,
    }

    pub struct QuestionPayload {
        pub questions: Vec<Question>,
        /// The last message of the agent, as context.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub context: Option<String>,
    }

    pub struct PlanPayload {
        pub plan: Markdown,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub plan_path: Option<String>,
    }

    pub struct NoticePayload {
        pub text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub resets_at: Option<Timestamp>,
    }

    #[serde(tag = "kind", rename_all = "snake_case")]
    pub enum InteractionPayload {
        Approval(Box<ApprovalPayload>),
        Question(QuestionPayload),
        PlanReview(PlanPayload),
        Notice(NoticePayload),
    }

    /// `interactions`: an approval, a question, a plan review or a notice. The core renders the
    /// payload once, when the interaction opens.
    pub struct InteractionValue {
        pub id: InteractionId,
        /// The short handle, for example `i42`.
        pub handle: String,
        pub kind: InteractionKind,
        pub state: InteractionState,
        pub mode: GateMode,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub run: Option<RunId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub pane: Option<PaneId>,
        pub title: String,
        pub risk: Risk,
        pub opened_at: Timestamp,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub deadline: Option<Timestamp>,
        pub payload: InteractionPayload,
        pub delivery: DeliveryInfo,
        /// A client can answer it now. A run that is only suggested is not answerable.
        pub answerable: bool,
        /// The channels that this kind has for this run.
        pub channels: Vec<AnswerChannel>,
    }
}

// --- timeline, changes, notifications and the rest ----------------------------------------

wire! {
    #[derive(Copy, Eq, Hash)]
    #[serde(rename_all = "snake_case")]
    pub enum ToolStatus {
        Running,
        Ok,
        Failed,
        Denied,
    }

    #[serde(tag = "type", rename_all = "snake_case")]
    pub enum TimelineBody {
        Prompt { preview: String },
        Assistant { preview: String },
        Tool {
            tool: String,
            status: ToolStatus,
            summary: String,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            duration_ms: Option<u64>,
        },
        Diff { diff: Diff },
        Output {
            text: String,
            #[serde(default)]
            truncated: bool,
        },
        Subagent { agent_id: String, agent_type: String, finished: bool },
        Compaction {},
    }

    /// `timeline:<run>`: a turn or an item of a turn.
    pub struct TimelineItem {
        pub id: String,
        pub at: Timestamp,
        pub turn: u32,
        pub body: TimelineBody,
    }

    /// `changes` of `changes_head:<window>`: the totals of the changes of a worktree.
    pub struct ChangeTotals {
        /// The changed files, also those after the first 1,000.
        pub files: u32,
        /// The worktree has more than 1,000 changed files, and `changes_for:<window>` has the
        /// first 1,000.
        #[serde(default)]
        pub truncated: bool,
        /// Why the last count failed, for example "the worktree is gone". The other values and
        /// the rows are those of the count before.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub error: Option<String>,
    }

    /// What happened to a file since `HEAD`. A change of type counts as `modified`.
    #[derive(Copy, Eq)]
    #[serde(rename_all = "snake_case")]
    pub enum ChangeStatus {
        Modified,
        Added,
        Deleted,
        Renamed,
        /// A file that git does not track and does not ignore.
        Untracked,
    }

    /// `changes_for:<window>`: one changed file. The key is `path`.
    pub struct ChangeRow {
        /// The path from the root of the worktree.
        pub path: String,
        /// The folder of the path with a `/` at its end, or empty for a file at the root.
        pub dir: String,
        /// The last part of the path.
        pub name: String,
        /// The path in `HEAD` of a renamed file.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub old_path: Option<String>,
        pub status: ChangeStatus,
        pub add: u32,
        pub del: u32,
        /// Git shows no lines for the file.
        #[serde(default)]
        pub binary: bool,
        /// Changes when the file in the worktree changes, so that an open diff loads again. It is
        /// 0 for a deleted file, and less than 2^53 so that JavaScript holds it exactly.
        pub stamp: u64,
    }

    /// `changes_head:<window>`: the head of the Changes tab of a window. The window names a
    /// folder with `changes.follow`, and the changes are those of the git worktree that holds
    /// it, from `HEAD`: what is not committed. The core makes the row when it knows the worktree.
    pub struct ChangesHead {
        /// The folder that the window named.
        pub folder: String,
        /// The root of the git worktree that holds the folder, or `None` when the folder is not
        /// in a git worktree.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub root: Option<String>,
        /// The branch of `HEAD`, or `None` for a detached `HEAD`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub branch: Option<String>,
        /// The totals of the last count, or `None` before the first count.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub changes: Option<ChangeTotals>,
        /// The lines of all changed files.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub diff_stats: Option<DiffStats>,
    }

    /// The login state of a harness on this machine. `unknown` means that the probe
    /// failed, timed out, or gave an output that no rule of the manifest matches.
    #[derive(Copy, Eq, Hash)]
    #[serde(rename_all = "snake_case")]
    pub enum LoginState {
        LoggedIn,
        LoggedOut,
        Unknown,
    }

    /// What gave the login state: the probe of the manifest, or a run that showed it.
    #[derive(Copy, Eq, Hash)]
    #[serde(rename_all = "snake_case")]
    pub enum LoginSource {
        Probe,
        Run,
    }

    /// How a harness is logged in, when its probe tells it. The core keeps no account name,
    /// address or token.
    #[derive(Copy, Eq, Hash)]
    #[serde(rename_all = "snake_case")]
    pub enum LoginMethod {
        /// A subscription account: Claude.ai for Claude Code, ChatGPT for Codex.
        Subscription,
        ApiKey,
        /// A token that the person gave the harness, for example `CLAUDE_CODE_OAUTH_TOKEN`.
        Token,
    }

    /// `harnesses`: a harness that a manifest describes, as the core of this machine found it.
    /// The key is the id of the manifest.
    pub struct HarnessValue {
        pub id: String,
        pub name: String,
        /// The program of `[launch] argv` is on the `PATH` of the login environment.
        pub installed: bool,
        /// The program that the core found.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub path: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub version: Option<String>,
        /// The version is in a range of `[[capabilities]]` of the manifest.
        pub validated: bool,
        pub login: LoginState,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub login_source: Option<LoginSource>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub login_method: Option<LoginMethod>,
        /// A start of an agent with this harness can go on: the harness is installed and not
        /// logged out.
        pub available: bool,
        /// The time of the last check.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub checked_at: Option<Timestamp>,
        /// What the person can do: `[auth] login_hint` of the manifest, or why the program was not
        /// found.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub hint: Option<String>,
    }

    /// `notifications`.
    pub struct NotificationValue {
        pub id: NotificationId,
        pub at: Timestamp,
        pub text: String,
        pub urgency: Urgency,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub interaction: Option<InteractionId>,
        pub seen: bool,
    }

    #[derive(Copy, Eq, Hash)]
    #[serde(rename_all = "snake_case")]
    pub enum MachineState {
        Local,
        Connecting,
        Connected,
        Disconnected,
    }

    /// The login state of one harness on a machine, as the row of that machine says it.
    pub struct MachineHarness {
        /// The id of the manifest: `claude` or `codex`.
        pub id: String,
        pub installed: bool,
        pub login: LoginState,
    }

    /// A port that the client forwards to a machine.
    #[derive(Copy, Eq, Hash)]
    pub struct PortForward {
        pub remote_port: u16,
        pub local_port: u16,
    }

    /// What the client should do about the version of the core of a machine.
    #[derive(Copy, Eq, Hash)]
    #[serde(rename_all = "snake_case")]
    pub enum MachineUpdate {
        /// The core is older than the app. The link works, and the app offers an update.
        UpdateAvailable,
        /// The core is newer than the app within the major version. The link works, and the app
        /// asks the person to update the app.
        UpdateApp,
        /// The core is of another major version of the API. The link does not work.
        VersionMismatch,
        /// The machine has no `nh`, or a bridge that does not know `link/1`. The app offers the
        /// install.
        InstallNeeded,
    }

    /// `machines`: the machine of the person and one row for each `[machines.<label>]`.
    /// The key is the label. The core orders the rows: `local`, then the order of `config.toml`.
    pub struct MachineValue {
        pub label: String,
        pub state: MachineState,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub latency_ms: Option<f32>,
        /// The version of the core of the machine, from `Welcome`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub version: Option<String>,
        /// Why the link is not connected: the reason of the last disconnect or the last failure.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub reason: Option<String>,
        /// The ports forwarded to the machine.
        #[serde(default)]
        pub forwards: Vec<PortForward>,
        /// The summary of the harnesses of the machine, copied from its `harnesses` projection.
        #[serde(default)]
        pub harnesses: Vec<MachineHarness>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub update: Option<MachineUpdate>,
    }

    /// `limits`: the rate limits of an account.
    pub struct LimitValue {
        pub account: String,
        pub harness: Harness,
        pub windows: Vec<RateLimitWindow>,
        pub usage_today: Usage,
    }

    #[derive(Copy, Eq, Hash)]
    #[serde(rename_all = "snake_case")]
    pub enum JobStatus {
        Running,
        Done,
        Failed,
        Cancelled,
    }

    /// `jobs`: the progress of a long command.
    pub struct JobValue {
        pub id: JobId,
        pub title: String,
        pub status: JobStatus,
        /// From 0 to 1, when the job knows.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub progress: Option<f32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub message: Option<String>,
        /// What the job made, when it is done (for example `{"run": "…"}`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub result: Option<Value>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub error: Option<CallError>,
        pub started_at: Timestamp,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub finished_at: Option<Timestamp>,
    }

    pub struct PaletteCommand {
        pub id: String,
        pub title: String,
        pub category: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub shortcut: Option<String>,
        pub enabled: bool,
        /// Why a command is disabled, for the hint.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub disabled_reason: Option<String>,
    }

    /// `palette`: the command list with the enablement of the moment. It has one key, `_`.
    pub struct PaletteValue {
        pub commands: Vec<PaletteCommand>,
    }

    /// Where a setting got its value.
    #[derive(Copy, Eq, Hash)]
    #[serde(rename_all = "snake_case")]
    pub enum SettingSource {
        Default,
        File,
        Ui,
    }

    pub struct SettingEntry {
        pub value: Value,
        pub source: SettingSource,
    }

    /// `settings`: one section of the effective config, with provenance.
    pub struct SettingsSection {
        pub entries: BTreeMap<String, SettingEntry>,
    }

    #[serde(tag = "state", rename_all = "snake_case")]
    pub enum StorageHealth {
        Ok,
        /// The store is read-only or full (`degraded_storage`).
        Degraded { reason: String },
    }

    pub struct UnvalidatedHarness {
        pub harness: Harness,
        pub version: String,
    }

    pub struct DoctorWarning {
        pub id: String,
        pub text: String,
        /// What the user can do, for example a command.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub hint: Option<String>,
    }

    /// `link`: the link of a window to its machine. The host of the client serves it,
    /// to each window. It has one key, `_`. For a window of the local core the state is `local`.
    pub struct LinkValue {
        /// The label of the machine.
        pub machine: String,
        pub state: MachineState,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub rtt_ms: Option<f32>,
        /// When the state last changed.
        pub changed_at: Timestamp,
        /// Why the link is not connected.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub reason: Option<String>,
        /// What the status bar shows: `local`, the label of a link that is up, or the label with
        /// the state for a link that is not. The host makes it, and the page prints it.
        pub text: String,
    }

    /// `health`: what the user needs to know about the core itself. It has one key, `_`.
    pub struct HealthValue {
        pub storage: StorageHealth,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub renderer: Option<RendererKind>,
        pub holder_protocols: Vec<u16>,
        pub unvalidated_harnesses: Vec<UnvalidatedHarness>,
        pub warnings: Vec<DoctorWarning>,
    }
}

// --- phones ---------------------------------------------------------------------------------

wire! {
    /// `phones`: a paired phone. The key is the device id. The core orders the
    /// rows, the phone paired last first.
    pub struct PhoneValue {
        pub device: DeviceId,
        /// The name that the phone gave when it paired, or that `phone.rename` set.
        pub name: String,
        pub paired_at: Timestamp,
        /// The time of the last connection of the phone, or `null` before the first one.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub seen_at: Option<Timestamp>,
        /// Whether the phone has a push subscription.
        #[serde(default)]
        pub push: bool,
    }

    /// The state of the offer of a pairing.
    #[derive(Copy, Eq, Hash)]
    #[serde(rename_all = "snake_case")]
    pub enum PairingState {
        /// No offer.
        None,
        /// An offer waits for a phone.
        Waiting,
        /// A phone paired with the offer.
        Paired,
        /// The offer ended: its time ran out, or five codes were wrong.
        Expired,
    }

    /// `phone_pairing`: the offer of a pairing. It has one key, `_`. A phone cannot
    /// follow it, because it holds the codes.
    pub struct PhonePairingValue {
        pub state: PairingState,
        /// The URL of the QR code: the public origin, `/pair?code=` and the long code.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub url: Option<String>,
        /// The short code that a person types: 8 Crockford base32 characters, as `ABCD-EFGH`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub code: Option<String>,
        /// The QR code of `url` as an SVG `data:` URL. The page shows it in an image.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub qr: Option<String>,
        /// When the offer ends.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub expires_at: Option<Timestamp>,
        /// The time that the offer is good for, as a person reads it, for example "The code is
        /// good for 10 minutes and for one phone.". The page shows it as it is.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub note: Option<String>,
        /// The phone that paired with the offer.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub device: Option<DeviceId>,
    }

    /// `phone_status`: the listener of the phone. It has one key, `_`.
    pub struct PhoneStatusValue {
        /// Whether the listener serves its port.
        pub listening: bool,
        /// The port of `127.0.0.1`, when the listener has one.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub port: Option<u16>,
        /// The HTTPS origin that a phone uses, for example `https://studio.example.ts.net:8443`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub origin: Option<String>,
        /// What the person can do when the listener cannot serve a phone. `phone.pair_begin`
        /// refuses while there is a hint.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub hint: Option<String>,
        /// One line about the listener, for the settings window and the doctor, for example
        /// "Published with Tailscale at https://studio.example.ts.net:8443.".
        #[serde(default)]
        pub summary: String,
        /// Tailscale. `null` when the listener does not use it: the listener is
        /// off, `phone.url` names the origin, or the run is a development run.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub tailscale: Option<TailscaleStatus>,
        /// The public VAPID key of this core: the uncompressed P-256 point, base64url.
        /// The phone app subscribes to push with it. `null` while the listener does not run.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub vapid_key: Option<String>,
    }

    /// The state of the daemon of Tailscale.
    #[derive(Copy, Eq, Hash)]
    #[serde(rename_all = "snake_case")]
    pub enum TailscaleState {
        /// No `tailscale` on the `PATH` of the login environment and none in
        /// `/Applications/Tailscale.app`.
        Missing,
        /// The CLI gave no answer that the core understands, for example because the daemon
        /// does not run.
        Unreachable,
        /// The daemon starts.
        Starting,
        /// The daemon is not connected to the tailnet.
        Stopped,
        /// The daemon needs a login, or the admin of the tailnet must approve the machine.
        NeedsLogin,
        /// The daemon is connected to the tailnet.
        Running,
    }

    /// What `tailscale status --json` and `tailscale serve status --json` say about the listener.
    #[derive(Eq)]
    pub struct TailscaleStatus {
        pub state: TailscaleState,
        /// The name of this machine in the tailnet, without the dot at the end, for example
        /// `studio.example.ts.net`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub name: Option<String>,
        /// Whether the tailnet gives an HTTPS certificate for the name (`CertDomains`).
        #[serde(default)]
        pub https: bool,
        /// The HTTPS port of the mapping: `phone.https_port`.
        pub https_port: u16,
        /// Whether `tailscale serve` maps `https_port` to the listener.
        #[serde(default)]
        pub published: bool,
        /// Whether a Funnel is on `https_port`. neohar never turns one on.
        #[serde(default)]
        pub funnel: bool,
    }
}
