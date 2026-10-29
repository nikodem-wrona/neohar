//! Params and results of `agent.*`, `interaction.*` and `changes.*`.

use crate::api::{
    common::{Harness, SplitDirection, wire},
    ids::{InteractionId, PaneId, Ref, RunId, WindowId, WorkspaceId},
    model::{AgentValue, InteractionState, InteractionValue, LoginState},
};

wire! {
    // --- agents ---

    pub struct AgentList {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub pane: Option<Ref<PaneId>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub workspace: Option<Ref<WorkspaceId>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub harness: Option<Harness>,
    }

    pub struct AgentListResult {
        pub agents: Vec<AgentValue>,
    }

    pub struct AgentShow {
        pub agent: Ref<RunId>,
    }

    pub struct AgentShown {
        pub agent: AgentValue,
    }

    /// Where an agent starts.
    #[serde(tag = "type", rename_all = "snake_case")]
    pub enum Placement {
        /// A new tab of the workspace.
        NewTab { workspace: Ref<WorkspaceId> },
        /// A new pane beside an existing one.
        Split {
            pane: Ref<PaneId>,
            direction: SplitDirection,
        },
        /// An existing pane whose shell is idle. The core types the command there.
        Pane { pane: Ref<PaneId> },
    }

    /// `agent.start` returns a job. The run appears in the `agents` projection.
    pub struct AgentStart {
        pub harness: Harness,
        pub placement: Placement,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub cwd: Option<String>,
        /// A unique name for the run. A call can use it instead of the id.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub name: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub prompt: Option<String>,
        /// The model, as the harness names it (`opus`, `gpt-5.5`). Without it, the harness uses
        /// the model of its own settings.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub model: Option<String>,
        /// The effort level (`high`). Without it, the harness uses the level of its own settings.
        /// A manifest without `effort_arg` refuses it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub effort: Option<String>,
        /// Extra arguments for the harness.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pub args: Vec<String>,
    }

    /// The models and the effort levels that `agent.start` can choose.
    pub struct AgentChoices {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub harness: Option<Harness>,
    }

    pub struct ModelChoice {
        /// What goes on the command line.
        pub id: String,
        pub label: String,
        /// The effort levels of this model. Empty: the levels of the harness.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pub efforts: Vec<String>,
    }

    pub struct HarnessChoices {
        pub harness: Harness,
        /// The models, in the order of the menu. The default of the harness is not in the list:
        /// a launch without a model gets it.
        pub models: Vec<ModelChoice>,
        /// The effort levels for the default model and for a model without its own levels. Empty:
        /// the harness takes no effort level.
        pub efforts: Vec<String>,
        /// The program of the harness is on the `PATH` of this machine.
        pub installed: bool,
        pub login: LoginState,
        /// A start with this harness can go on: it is installed and not logged out.
        pub available: bool,
        /// What the person can do when the harness is not available.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub hint: Option<String>,
    }

    pub struct AgentChoicesResult {
        pub harnesses: Vec<HarnessChoices>,
    }

    /// Finds the harnesses again and asks each one for its login state now. The call
    /// returns when the probes are done, and the result is in the projection `harnesses`.
    pub struct HarnessRefresh {
        /// The id of one manifest, for example `codex`. Without it, every harness.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub harness: Option<String>,
    }

    /// Asks the agent to end. With `force` the core ends the process.
    pub struct AgentStop {
        pub agent: Ref<RunId>,
        #[serde(default)]
        pub force: bool,
    }

    pub struct AgentInterrupt {
        pub agent: Ref<RunId>,
    }

    /// Types a prompt into a bound run (never into a suggested one).
    pub struct AgentPrompt {
        pub agent: Ref<RunId>,
        pub text: String,
        /// Press Enter after the text.
        #[serde(default = "crate::api::common::yes")]
        pub submit: bool,
    }

    /// The next prompt for a run that is `idle` and has no open interaction. The
    /// core writes the text as a bracketed paste and a carriage return, at most once for an
    /// `input_id`, and takes no geometry lease.
    pub struct AgentReply {
        pub run: Ref<RunId>,
        pub input_id: u64,
        pub text: String,
    }

    /// The skills and the custom commands that the harness of a run can start in the folder of
    /// the pane. With `query`, only the names that have it, ignoring case, and those that start
    /// with it first.
    pub struct AgentSkills {
        pub run: Ref<RunId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub query: Option<String>,
    }

    #[derive(Copy, Eq, Hash)]
    #[serde(rename_all = "snake_case")]
    pub enum SkillSource {
        /// The configuration directory of the harness, or `~/.agents/skills`.
        User,
        /// The folder of the pane or a parent of it, up to the root of its repository.
        Project,
        /// An enabled plugin of Claude Code.
        Plugin,
    }

    pub struct Skill {
        pub name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub description: Option<String>,
        pub source: SkillSource,
        /// The text that starts the skill at the start of a prompt, for example `/review ` for
        /// Claude Code and `$review ` for Codex.
        pub insert: String,
    }

    pub struct AgentSkillsResult {
        pub skills: Vec<Skill>,
    }

    /// Starts the harness again with the resume handle of the run. It returns a job.
    pub struct AgentResume {
        pub agent: Ref<RunId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub placement: Option<Placement>,
    }

    #[derive(Copy, Eq, Hash)]
    #[serde(rename_all = "snake_case")]
    pub enum WaitFor {
        /// Execution is `idle`.
        Idle,
        /// The process ended.
        Exited,
        /// The run has an open interaction, or it is in `error` or `rate_limited`.
        Attention,
    }

    pub struct AgentWait {
        pub agent: Ref<RunId>,
        pub until: WaitFor,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub timeout_ms: Option<u64>,
    }

    pub struct AgentWaitResult {
        /// `false` when the time ran out.
        pub reached: bool,
        pub agent: AgentValue,
    }

    /// Binds a run to a pane by the user's decision.
    pub struct AgentLink {
        pub pane: Ref<PaneId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub harness: Option<Harness>,
        /// The thread of a Codex run on the shared daemon.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub thread: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub session_id: Option<String>,
    }

    pub struct AgentLinked {
        pub run: RunId,
    }

    pub struct AgentUnlink {
        pub agent: Ref<RunId>,
    }

    /// Puts a run at another place on the board of every window. The order stays across a restart
    /// of the core, and a new run in the pane of a run that ended takes its place.
    pub struct AgentMove {
        pub agent: Ref<RunId>,
        /// The place among the runs, counted from 0. A place after the last run is the end.
        pub index: u32,
    }

    // --- interactions ---

    pub struct InteractionList {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub run: Option<Ref<RunId>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub pane: Option<Ref<PaneId>>,
        /// Only the open ones. It is the default.
        #[serde(default = "crate::api::common::yes")]
        pub open_only: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub limit: Option<u32>,
    }

    pub struct InteractionListResult {
        pub interactions: Vec<InteractionValue>,
    }

    pub struct InteractionShow {
        pub interaction: Ref<InteractionId>,
    }

    pub struct InteractionShown {
        pub interaction: InteractionValue,
    }

    #[derive(Copy, Eq, Hash)]
    #[serde(rename_all = "snake_case")]
    pub enum Behavior {
        Allow,
        Deny,
    }

    pub struct ApprovalDecision {
        pub behavior: Behavior,
        /// "Allow always": a rule for the session.
        #[serde(default)]
        pub always: bool,
        /// With `always`: write the rule to the settings of the harness. It needs
        /// `approvals.claude.persist_always`.
        #[serde(default)]
        pub persist: bool,
        /// The text for a deny. The model sees it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub message: Option<String>,
        /// "Deny and stop": the turn ends.
        #[serde(default)]
        pub stop: bool,
    }

    pub struct QuestionAnswer {
        /// The text of the question.
        pub question: String,
        /// The labels of the chosen options.
        pub selected: Vec<String>,
        /// Free text, when the user typed an answer.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub text: Option<String>,
    }

    pub struct QuestionDecision {
        pub answers: Vec<QuestionAnswer>,
    }

    pub struct PlanDecision {
        pub approve: bool,
        /// With `approve: false`, the feedback becomes the deny message.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub feedback: Option<String>,
    }

    #[serde(tag = "kind", rename_all = "snake_case")]
    pub enum Decision {
        Approval(ApprovalDecision),
        Question(QuestionDecision),
        PlanReview(PlanDecision),
    }

    /// `via` goes into the delivery record. A second answer gets `already_answered`.
    pub struct InteractionAnswer {
        pub interaction: Ref<InteractionId>,
        pub decision: Decision,
        pub via: crate::api::common::Via,
    }

    pub struct InteractionAnswered {
        /// The revision of the decision.
        pub rev: u32,
        pub state: InteractionState,
    }

    /// Closes an interaction without an answer (a notice, or a card the user does not want).
    pub struct InteractionDismiss {
        pub interaction: Ref<InteractionId>,
    }

    /// The most urgent open interaction that a client can answer.
    pub struct InteractionNext {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub after: Option<Ref<InteractionId>>,
    }

    pub struct InteractionNextResult {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub interaction: Option<InteractionId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub handle: Option<String>,
    }

    /// Brings an interaction to the person: the app comes to the front, and the window
    /// shows the inbox with the card focused. It does not answer and does not focus the pane.
    /// Without `interaction`, it takes the result of `interaction.next`.
    pub struct InteractionReveal {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub interaction: Option<Ref<InteractionId>>,
        /// The window that shows the card. The default is the window with the OS focus, else the
        /// first one.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub window: Option<WindowId>,
    }

    pub struct InteractionRevealed {
        /// The interaction that the window shows, or none when nothing waits.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub interaction: Option<InteractionId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub window: Option<WindowId>,
    }

    // --- changes ---

    /// `changes.follow`: the Changes tab of a window shows the changes of the git worktree that
    /// holds `path`. The core fills `changes_head:<window>` and `changes_for:<window>`
    /// while the window follows one of them, and counts again when the worktree changes.
    pub struct ChangesFollow {
        pub window: WindowId,
        /// An absolute path: the folder of the Files tab of the window.
        pub path: String,
    }

    /// `changes.diff` gives the diff of one path of the rows of `changes_for:<window>`.
    pub struct ChangesDiff {
        pub window: WindowId,
        pub path: String,
    }
}
