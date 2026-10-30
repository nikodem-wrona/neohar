//! Params and results of `workflow.*`: the workflows of the person and their executions.
//!
//! A workflow is a graph of nodes in a TOML file in `<config>/workflows/`. A manual trigger
//! starts it. An agent node gives its prompt to Claude Code or Codex, and the last message of the
//! turn of the agent is the report of the node. A Report node writes `report.md` with links to
//! the reports of its inputs. A node starts when each of its inputs is done.
//!
//! The core is the only writer of the files. `workflow.save` takes the revision that it changes,
//! and answers `conflict` when the file changed since then.

use crate::api::{
    common::{Harness, wire},
    ids::{ExecutionId, Ref, RunId, WindowId},
    model::{ExecutionValue, WorkflowValue},
    rich::Markdown,
};

wire! {
    // --- the graph ---

    /// The kind of a node.
    #[derive(Copy, Eq, Hash)]
    #[serde(rename_all = "snake_case")]
    pub enum WorkflowNodeKind {
        /// The start of the workflow. Each workflow has exactly one.
        Manual,
        /// Claude Code or Codex does the prompt of the node and gives a report.
        Agent,
        /// Writes `report.md` from the reports of its inputs.
        Report,
    }

    /// A node of a workflow.
    pub struct WorkflowNode {
        /// The id: `[a-z][a-z0-9_]*`, unique in the workflow. A reference names it, for example
        /// `{{code.report}}`.
        pub id: String,
        pub kind: WorkflowNodeKind,
        /// The title on the card. Without it, the card shows the id.
        #[serde(default)]
        pub title: String,
        /// Agent: the harness.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub harness: Option<Harness>,
        /// Agent: the model, as the harness names it. Without it, the harness uses its own.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub model: Option<String>,
        /// Agent: the effort level. Without it, the harness uses its own.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub effort: Option<String>,
        /// Agent: the agent can edit files. Without it, the agent is read-only.
        #[serde(default)]
        pub can_edit: bool,
        /// Agent: the time limit in minutes. Without it, `workflows.time_limit_min`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub time_limit_min: Option<u32>,
        /// Agent: the prompt, with references such as `{{start.base}}`.
        #[serde(default)]
        pub prompt: String,
        /// Report: the harness that writes a summary at the top of `report.md`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub summary_by: Option<Harness>,
        /// Report: the first part of the prompt of the summary. Without it, the core uses its own.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub summary_prompt: Option<String>,
        /// The ids of the nodes that connect to this node.
        #[serde(default)]
        pub inputs: Vec<String>,
        /// The position on the canvas, in canvas units.
        pub x: i32,
        pub y: i32,
    }

    /// The graph of a workflow.
    pub struct WorkflowGraph {
        /// The name, for example "Review".
        pub name: String,
        pub nodes: Vec<WorkflowNode>,
    }

    /// An error in a workflow. A workflow with errors can be saved, but not executed.
    pub struct WorkflowProblem {
        /// The node with the error, if the error is in one node.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub node: Option<String>,
        pub text: String,
    }

    /// A workflow and its errors: the result of `workflow.show` and of each change.
    pub struct WorkflowShown {
        /// The name of the file without `.toml`.
        pub workflow: String,
        /// The revision of the file. `workflow.save` takes it.
        pub revision: String,
        pub graph: WorkflowGraph,
        pub problems: Vec<WorkflowProblem>,
        /// `workflow.undo` has a version to go back to.
        #[serde(default)]
        pub can_undo: bool,
        /// `workflow.redo` has a version to go forward to.
        #[serde(default)]
        pub can_redo: bool,
        /// The absolute path of the file.
        pub path: String,
    }

    /// `workflow.list`: the workflows, as the `workflows` projection has them.
    pub struct WorkflowList {
        pub workflows: Vec<WorkflowValue>,
    }

    /// `workflow.show`, `workflow.delete`, `workflow.duplicate`, `workflow.undo`, `workflow.redo`:
    /// one workflow.
    pub struct WorkflowRef {
        /// The name of the file without `.toml`.
        pub workflow: String,
    }

    /// `workflow.create`: a new workflow file. The name of the file comes from `name`.
    pub struct WorkflowCreate {
        pub name: String,
        /// A preset workflow of `workflow.presets`, for example `review`. Without it, the workflow
        /// has only the trigger.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub preset: Option<String>,
    }

    /// `workflow.save`: writes the graph of a workflow.
    pub struct WorkflowSave {
        pub workflow: String,
        /// The revision that the change starts from. An older one answers `conflict`.
        pub revision: String,
        pub graph: WorkflowGraph,
    }

    /// `workflow.rename`: gives a workflow a new name, and its file the name that comes from it.
    pub struct WorkflowRename {
        pub workflow: String,
        pub name: String,
    }

    /// `workflow.layout`: gives each node a new position, one column for each depth, and saves.
    pub struct WorkflowLayout {
        pub workflow: String,
        pub revision: String,
    }

    /// A node of the node picker.
    pub struct WorkflowNodePreset {
        pub id: String,
        /// The text of the picker, for example "Security review".
        pub label: String,
        /// The node without `id`, `inputs` and position.
        pub node: WorkflowNode,
    }

    /// A workflow that `workflow.create` can start from.
    pub struct WorkflowPreset {
        pub id: String,
        pub label: String,
    }

    /// `workflow.presets`: the items of the node picker and the preset workflows.
    pub struct WorkflowPresets {
        pub nodes: Vec<WorkflowNodePreset>,
        pub workflows: Vec<WorkflowPreset>,
    }

    // --- executions ---

    /// `workflow.facts`: what the dialog of an execution shows for a folder.
    pub struct WorkflowFactsGet {
        /// An absolute path.
        pub folder: String,
    }

    /// The facts of a folder for an execution.
    pub struct WorkflowFacts {
        pub folder: String,
        /// The root of the git worktree that holds the folder.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub repo: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub branch: Option<String>,
        /// The default base: the default branch of `origin` (`origin/HEAD`), else `main` or `master`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub base: Option<String>,
    }

    /// `workflow.execute`: starts an execution.
    pub struct WorkflowExecute {
        pub workflow: String,
        /// The absolute path of the folder in which the agents work.
        pub folder: String,
        /// The base of the changes. Without it, the default of `workflow.facts`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub base: Option<String>,
        /// What the person built and why. `{{start.notes}}` gives it to a prompt.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub notes: Option<String>,
        /// Execute only this node, with the reports of the last execution in this folder as inputs.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub node: Option<String>,
    }

    /// The result of `workflow.execute`.
    pub struct WorkflowExecuted {
        pub execution: ExecutionId,
        /// The name of the execution, for example "Review 3".
        pub label: String,
    }

    /// `workflow.notes_from`: the prompts of an agent run, as one text for the notes.
    pub struct WorkflowNotesFrom {
        pub agent: Ref<RunId>,
    }

    /// The result of `workflow.notes_from`.
    pub struct WorkflowNotes {
        pub text: String,
    }

    /// `workflow.executions`: the executions, the newest first.
    pub struct WorkflowExecutions {
        /// Only the executions in this folder.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub folder: Option<String>,
    }

    /// The result of `workflow.executions`.
    pub struct WorkflowExecutionList {
        pub executions: Vec<ExecutionValue>,
    }

    /// `workflow.execution`, `workflow.cancel`, `workflow.copy_to_folder`: one execution.
    pub struct WorkflowExecutionRef {
        pub execution: ExecutionId,
    }

    /// The result of `workflow.execution`: the execution and the graph that it runs.
    pub struct WorkflowExecutionShown {
        pub execution: ExecutionValue,
        pub graph: WorkflowGraph,
    }

    /// `workflow.retry`, `workflow.skip`: one node of an execution.
    pub struct WorkflowNodeRef {
        pub execution: ExecutionId,
        pub node: String,
    }

    /// `workflow.report`: a report of an execution.
    pub struct WorkflowReportGet {
        pub execution: ExecutionId,
        /// The node. Without it, `report.md`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub node: Option<String>,
    }

    /// The result of `workflow.report`.
    pub struct WorkflowReport {
        pub path: String,
        pub markdown: Markdown,
    }

    /// `workflow.continue`: starts the agent of a node again with its resume handle, in a new
    /// tab that the window shows.
    pub struct WorkflowContinue {
        pub execution: ExecutionId,
        pub node: String,
        /// The window that selects the new tab.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub window: Option<WindowId>,
    }

    /// `workflow.reveal`: the tab of the agent of a node shows in the tab bar, and the window
    /// selects it.
    pub struct WorkflowReveal {
        pub execution: ExecutionId,
        pub node: String,
        pub window: WindowId,
    }

    /// The result of `workflow.copy_to_folder`.
    pub struct WorkflowCopied {
        /// The absolute path of the copy.
        pub path: String,
    }
}
