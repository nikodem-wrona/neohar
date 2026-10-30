//! The executor: it runs the executions of workflows.
//!
//! One task takes the messages in order: the signals of the agent runs, the end of the start of
//! an agent, the calls of the API and a tick. Each change of an execution is one transaction of
//! its rows and its events, then a row of `workflow_executions`. The executor reacts to changes and
//! never waits in a loop for an agent.
//!
//! The agent of a node runs in a hidden tab. The last message of its turn is its report. A turn
//! without a verdict line gets one reminder. When the node is done, the executor closes the hidden
//! tab, which stops the agent; the run keeps its resume handle for "Continue in a tab". A tab that
//! the person opened stays.
//!
//! When the app quits, the agents go on in their holders, and their `Stop` hooks find no core.
//! At the next start, the executor reads the transcript of each run that works and takes the turns
//! that ended.

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError},
    time::Duration,
};

use nh_proto::api::{
    calls::{
        agent::{AgentResume, Placement},
        workflow::{
            WorkflowExecute, WorkflowExecuted, WorkflowGraph, WorkflowNode, WorkflowNodeKind,
        },
        workspace::TabSelect,
    },
    common::{Harness, JobStarted, Urgency},
    envelope::ErrorCode,
    events::{EventPayload, WorkflowExecutionEvent, WorkflowNodeEvent},
    ids::{ExecutionId, JobId, PaneId, Ref, RunId, TabId, WindowId, WorkspaceId},
    methods::Call,
    model::{
        Execution, ExecutionNodeValue, ExecutionState, ExecutionValue, JobStatus, NodeState,
        Verdict,
    },
    projections::Projection,
};
use nh_store::{ExecutionRow, NodeRow};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use tokio::sync::{mpsc, oneshot};

use super::{
    Signal,
    facts::{Facts, FactsReader, last_turn},
    file, graph,
    library::Library,
    text::{self, Footer, Input, ReportHead, ReportRow},
};
use crate::{
    actor::{ActorHandle, Origin},
    agents::{Control, HiddenStart},
    config::ConfigStore,
    error::RpcError,
    host::{Host, HostNotification},
    ids::new_ulid,
    jobs::Jobs,
    persist::now_ms,
    projection::ProjectionHub,
};

/// The tick: time limits and time texts.
const TICK: Duration = Duration::from_secs(20);
/// How often the waiter of a start looks at its job.
const JOB_POLL: Duration = Duration::from_millis(200);
/// The longest report that the core reads back for `report.md`.
const REPORT_CAP: u64 = 4 * 1024 * 1024;
/// The name of the folder of the executions in the data folder.
pub(crate) const DIR_NAME: &str = "workflows";

/// A call of the API that changes an execution.
pub(crate) enum Request {
    Execute(WorkflowExecute, Facts),
    Retry(ExecutionId, String),
    Skip(ExecutionId, String),
    Cancel(ExecutionId),
}

enum Message {
    Signal(Signal),
    Started {
        execution: ExecutionId,
        node: String,
        result: Result<(RunId, PaneId), String>,
    },
    Request(Request, oneshot::Sender<Result<Value, RpcError>>),
    /// Start the nodes of an execution that can start.
    Advance(ExecutionId),
    Tick,
}

/// One execution in memory.
#[derive(Clone)]
pub(crate) struct Exec {
    pub id: ExecutionId,
    pub row: ExecutionRow,
    pub graph: WorkflowGraph,
    /// In the order of the graph.
    pub nodes: Vec<NodeRow>,
    /// The panes of the agents that run.
    pub panes: HashMap<String, PaneId>,
}

impl Exec {
    fn node(&self, id: &str) -> Option<&NodeRow> {
        self.nodes.iter().find(|n| n.node == id)
    }

    fn node_mut(&mut self, id: &str) -> Option<&mut NodeRow> {
        self.nodes.iter_mut().find(|n| n.node == id)
    }

    fn spec(&self, id: &str) -> Option<&WorkflowNode> {
        self.graph.nodes.iter().find(|n| n.id == id)
    }

    fn state(&self) -> ExecutionState {
        from_text(&self.row.state).unwrap_or(ExecutionState::Running)
    }

    fn label(&self) -> String {
        format!("{} {}", self.row.name, self.row.number)
    }
}

fn to_text<T: Serialize>(value: &T) -> String {
    match serde_json::to_value(value) {
        Ok(Value::String(text)) => text,
        _ => String::new(),
    }
}

fn from_text<T: DeserializeOwned>(text: &str) -> Option<T> {
    serde_json::from_value(Value::String(text.to_owned())).ok()
}

fn node_state(row: &NodeRow) -> NodeState {
    from_text(&row.state).unwrap_or(NodeState::Waiting)
}

fn is_final(state: NodeState) -> bool {
    matches!(
        state,
        NodeState::Done | NodeState::Failed | NodeState::Skipped | NodeState::Cancelled
    )
}

fn harness_title(harness: Harness) -> &'static str {
    match harness {
        Harness::Claude => "Claude Code",
        Harness::Codex => "Codex",
    }
}

/// "Codex · gpt-5.5 · high" for an agent node.
fn agent_label(node: &WorkflowNode) -> String {
    let Some(harness) = node.harness else {
        return String::new();
    };
    let mut parts = vec![harness_title(harness).to_owned()];
    parts.extend(node.model.clone());
    parts.extend(node.effort.clone());
    parts.join(" · ")
}

fn not_found(what: &str) -> RpcError {
    RpcError::new(ErrorCode::NotFound, what.to_owned())
}

/// What the executor reads of a run.
struct RunLook {
    live: bool,
    execution: Execution,
    pending: bool,
    pane: Option<PaneId>,
    harness: Harness,
    transcript: Option<String>,
}

/// The executor of the workflows.
pub(crate) struct Executor {
    handle: tokio::runtime::Handle,
    actor: OnceLock<ActorHandle>,
    control: Arc<Control>,
    jobs: Arc<Jobs>,
    config: Arc<ConfigStore>,
    hub: Arc<ProjectionHub>,
    library: Arc<Library>,
    pub(crate) facts: FactsReader,
    root: PathBuf,
    book: Mutex<Vec<Exec>>,
    queue: mpsc::UnboundedSender<Message>,
    receiver: Mutex<Option<mpsc::UnboundedReceiver<Message>>>,
    /// The turns of runs that ended before the executor knew the run of their node.
    early: Mutex<HashMap<RunId, Option<String>>>,
}

impl Executor {
    #[expect(
        clippy::too_many_arguments,
        reason = "the services that the executor uses"
    )]
    pub(crate) fn new(
        handle: tokio::runtime::Handle,
        control: Arc<Control>,
        jobs: Arc<Jobs>,
        config: Arc<ConfigStore>,
        hub: Arc<ProjectionHub>,
        library: Arc<Library>,
        facts: FactsReader,
        data_dir: &Path,
    ) -> Arc<Self> {
        let (queue, receiver) = mpsc::unbounded_channel();
        Arc::new(Self {
            handle,
            actor: OnceLock::new(),
            control,
            jobs,
            config,
            hub,
            library,
            facts,
            root: data_dir.join(DIR_NAME).join("executions"),
            book: Mutex::new(Vec::new()),
            queue,
            receiver: Mutex::new(Some(receiver)),
            early: Mutex::new(HashMap::new()),
        })
    }

    fn book(&self) -> MutexGuard<'_, Vec<Exec>> {
        self.book.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn actor(&self) -> Result<&ActorHandle, RpcError> {
        self.actor
            .get()
            .ok_or_else(|| RpcError::internal("the executor has no actor yet"))
    }

    /// Binds the actor, loads the executions, and starts the task of the executor. The runs send
    /// their signals to `signals`.
    pub(crate) fn start(
        self: &Arc<Self>,
        actor: ActorHandle,
        mut signals: mpsc::UnboundedReceiver<Signal>,
    ) {
        let _ = self.actor.set(actor);
        let Some(mut receiver) = self
            .receiver
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
        else {
            return;
        };
        let this = Arc::clone(self);
        self.handle.spawn(async move {
            this.load().await;
            let mut tick = tokio::time::interval(TICK);
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                let message = tokio::select! {
                    Some(signal) = signals.recv() => Message::Signal(signal),
                    Some(message) = receiver.recv() => message,
                    _ = tick.tick() => Message::Tick,
                    else => return,
                };
                this.handle_message(message).await;
            }
        });
    }

    /// Sends a call to the task and waits for its answer.
    pub(crate) async fn request(&self, request: Request) -> Result<Value, RpcError> {
        let (sender, receiver) = oneshot::channel();
        self.queue
            .send(Message::Request(request, sender))
            .map_err(|_| RpcError::internal("the executor of the workflows has stopped"))?;
        receiver
            .await
            .map_err(|_| RpcError::internal("the executor of the workflows has stopped"))?
    }

    async fn handle_message(self: &Arc<Self>, message: Message) {
        match message {
            Message::Signal(Signal::TurnEnded { run, message }) => {
                self.turn_ended(run, message).await;
            }
            Message::Signal(Signal::Changed { run }) => self.run_changed(run).await,
            Message::Started {
                execution,
                node,
                result,
            } => self.started(execution, &node, result).await,
            Message::Request(request, answer) => {
                let result = match request {
                    Request::Execute(params, facts) => self.execute(params, facts).await,
                    Request::Retry(execution, node) => self.retry(execution, &node).await,
                    Request::Skip(execution, node) => self.skip(execution, &node).await,
                    Request::Cancel(execution) => self.cancel(execution).await,
                };
                let _ = answer.send(result);
            }
            Message::Advance(id) => self.advance(id).await,
            Message::Tick => self.tick().await,
        }
    }

    // --- reads ----------------------------------------------------------------------------------

    /// The executions, the newest first, in one folder or in all.
    pub(crate) fn values(&self, folder: Option<&str>) -> Vec<ExecutionValue> {
        let now = now_ms();
        self.book()
            .iter()
            .rev()
            .filter(|exec| folder.is_none_or(|f| exec.row.folder == f))
            .map(|exec| value(exec, now))
            .collect()
    }

    /// One execution and its graph.
    pub(crate) fn execution(&self, id: ExecutionId) -> Option<(ExecutionValue, WorkflowGraph)> {
        let book = self.book();
        let exec = book.iter().find(|e| e.id == id)?;
        Some((value(exec, now_ms()), exec.graph.clone()))
    }

    /// The path of a report: of a node, or of the last Report node.
    pub(crate) fn report_path(
        &self,
        id: ExecutionId,
        node: Option<&str>,
    ) -> Result<String, RpcError> {
        let book = self.book();
        let exec = book
            .iter()
            .find(|e| e.id == id)
            .ok_or_else(|| not_found("no such execution"))?;
        match node {
            Some(node) => exec
                .node(node)
                .ok_or_else(|| not_found("no such node"))?
                .report
                .clone()
                .ok_or_else(|| not_found("the node has no report")),
            None => value(exec, now_ms())
                .report
                .ok_or_else(|| not_found("the execution has no report.md")),
        }
    }

    /// The folder of the files of an execution.
    pub(crate) fn dir(&self, id: ExecutionId) -> Option<(String, String)> {
        let book = self.book();
        let exec = book.iter().find(|e| e.id == id)?;
        Some((exec.row.dir.clone(), exec.row.folder.clone()))
    }

    /// The run of a node, its execution workspace and whether the node has ended.
    pub(crate) fn node_run(
        &self,
        id: ExecutionId,
        node: &str,
    ) -> Result<(RunId, Option<WorkspaceId>, Option<PaneId>), RpcError> {
        let book = self.book();
        let exec = book
            .iter()
            .find(|e| e.id == id)
            .ok_or_else(|| not_found("no such execution"))?;
        let row = exec.node(node).ok_or_else(|| not_found("no such node"))?;
        let run = row
            .run
            .as_deref()
            .and_then(|r| r.parse().ok())
            .ok_or_else(|| not_found("the node has no agent"))?;
        let workspace = exec.row.workspace.as_deref().and_then(|w| w.parse().ok());
        Ok((run, workspace, exec.panes.get(node).copied()))
    }

    // --- the projection and the store -----------------------------------------------------------

    fn publish(&self, exec: &Exec) {
        if let Ok(json) = serde_json::to_value(value(exec, now_ms())) {
            self.hub
                .set(Projection::WorkflowExecutions, &exec.id.to_string(), json);
        }
    }

    fn publish_order(&self) {
        let keys: Vec<String> = self.book().iter().rev().map(|e| e.id.to_string()).collect();
        self.hub.set_order(Projection::WorkflowExecutions, &keys);
    }

    /// Writes an execution and its nodes, with events, then publishes its row.
    async fn store(&self, exec: &Exec, events: Vec<EventPayload>) {
        let row = exec.row.clone();
        let nodes = exec.nodes.clone();
        let subject = exec.id.to_string();
        if let Ok(actor) = self.actor() {
            let stored = actor
                .update(move |mut parts| {
                    parts.commit(|tx| {
                        tx.put_execution(&row)?;
                        for node in &nodes {
                            tx.put_execution_node(node)?;
                        }
                        for payload in &events {
                            tx.append(Some(&subject), payload)?;
                        }
                        Ok(())
                    })
                })
                .await;
            match stored {
                Ok(Ok(())) => {}
                Ok(Err(error)) => tracing::warn!(%error, "an execution is not stored"),
                Err(error) => tracing::warn!(%error, "an execution is not stored"),
            }
        }
        self.publish(exec);
    }

    /// Changes an execution in the book and returns a copy of it.
    fn change<R>(&self, id: ExecutionId, f: impl FnOnce(&mut Exec) -> R) -> Option<(Exec, R)> {
        let mut book = self.book();
        let exec = book.iter_mut().find(|e| e.id == id)?;
        let result = f(exec);
        Some((exec.clone(), result))
    }

    fn node_event(exec: &Exec, node: &str) -> EventPayload {
        let row = exec.node(node);
        EventPayload::WorkflowNodeChanged(WorkflowNodeEvent {
            execution: exec.id,
            node: node.to_owned(),
            state: row.map_or(NodeState::Waiting, node_state),
            run: row
                .and_then(|r| r.run.as_deref())
                .and_then(|r| r.parse().ok()),
            reason: row.and_then(|r| r.reason.clone()),
        })
    }

    /// Sets the state of a node, stores it with its event, and publishes the execution.
    async fn set_node(
        &self,
        id: ExecutionId,
        node: &str,
        f: impl FnOnce(&mut NodeRow),
    ) -> Option<Exec> {
        let (exec, ()) = self.change(id, |exec| {
            if let Some(row) = exec.node_mut(node) {
                f(row);
            }
        })?;
        let event = Self::node_event(&exec, node);
        self.store(&exec, vec![event]).await;
        Some(exec)
    }

    // --- runs -----------------------------------------------------------------------------------

    async fn look(&self, run: RunId) -> Option<RunLook> {
        let actor = self.actor().ok()?;
        actor
            .update(move |parts| {
                let run = parts.state.runs.by_id.get(&run)?;
                Some(RunLook {
                    live: run.live(),
                    execution: run.arbiter.execution().value.clone(),
                    pending: !run.arbiter.pending().is_empty(),
                    pane: run.pane,
                    harness: run.harness,
                    transcript: run.transcript_path.clone(),
                })
            })
            .await
            .ok()
            .flatten()
    }

    /// Marks runs as agents of workflow nodes, after a start of the core.
    async fn mark(&self, runs: Vec<RunId>) {
        if runs.is_empty() {
            return;
        }
        if let Ok(actor) = self.actor() {
            let _ = actor
                .update(move |mut parts| {
                    for run in runs {
                        if let Some(found) = parts.state.runs.by_id.get_mut(&run) {
                            found.workflow = true;
                            crate::agents::runs::publish(&mut parts, run);
                        }
                    }
                })
                .await;
        }
    }

    /// Closes the hidden tab of a pane, which stops its agent. A tab that the person opened stays.
    async fn close_hidden(&self, pane: Option<PaneId>) {
        let (Some(pane), Ok(actor)) = (pane, self.actor()) else {
            return;
        };
        let _ = actor
            .update(move |mut parts| {
                let Some((_, tab)) = parts.state.layout.tab_of_pane(pane) else {
                    return;
                };
                if !parts.state.layout.tab_hidden(tab) {
                    return;
                }
                if let Err(error) = crate::actor::layout::run_in(&mut parts, |layout, _| {
                    layout.close_tab(&Ref::<TabId>::id(tab), true)
                }) {
                    tracing::warn!(%error, "the hidden tab of a workflow node is not closed");
                }
            })
            .await;
    }

    /// The pane of the run of a node, from the book or from the run.
    async fn pane_of(&self, exec: &Exec, node: &str) -> Option<PaneId> {
        if let Some(pane) = exec.panes.get(node) {
            return Some(*pane);
        }
        let run: RunId = exec.node(node)?.run.as_deref()?.parse().ok()?;
        self.look(run).await?.pane
    }

    /// The workspace for the hidden tabs: the one with a pane in the folder, else the one with
    /// the folder as its root, else the first. Without a workspace, a new one.
    async fn workspace_for(&self, folder: &str) -> Result<WorkspaceId, RpcError> {
        let actor = self.actor()?;
        let folder = folder.to_owned();
        let found = actor
            .update({
                let folder = folder.clone();
                move |parts| {
                    let layout = &parts.state.layout;
                    let inside =
                        |path: &str| path == folder || path.starts_with(&format!("{folder}/"));
                    layout
                        .panes
                        .values()
                        .find(|pane| pane.cwd.as_deref().is_some_and(inside))
                        .map(|pane| pane.workspace)
                        .or_else(|| {
                            layout
                                .workspaces
                                .iter()
                                .find(|w| w.root.as_deref() == Some(folder.as_str()))
                                .map(|w| w.id)
                        })
                        .or_else(|| layout.workspaces.first().map(|w| w.id))
                }
            })
            .await
            .map_err(|e| RpcError::internal(e.to_string()))?;
        if let Some(workspace) = found {
            return Ok(workspace);
        }
        let call = Call::parse(
            "workspace.create",
            json!({ "name": "Workflows", "root": folder }),
        )
        .map_err(|e| RpcError::invalid_params(e.to_string()))?;
        let created = actor.call(call, Origin::ui()).await?;
        created["workspace"]
            .as_str()
            .and_then(|w| w.parse().ok())
            .ok_or_else(|| RpcError::internal("the layout gave no workspace"))
    }

    // --- execute --------------------------------------------------------------------------------

    async fn execute(
        self: &Arc<Self>,
        params: WorkflowExecute,
        facts: Facts,
    ) -> Result<Value, RpcError> {
        let (_, full) = self.library.graph(&params.workflow)?;
        let problems = graph::problems(&full);
        if let Some(first) = problems.first() {
            return Err(RpcError::new(
                ErrorCode::InvalidParams,
                format!("The workflow has errors. {}", first.text),
            ));
        }
        let folder = params.folder.trim_end_matches('/').to_owned();
        if !Path::new(&folder).is_absolute() || !Path::new(&folder).is_dir() {
            return Err(RpcError::invalid_params(format!(
                "{folder} is not a folder"
            )));
        }
        let now = now_ms();
        let trigger = graph::trigger(&full)
            .map(|t| t.id.clone())
            .ok_or_else(|| RpcError::invalid_params("the workflow has no trigger"))?;
        // "Execute this node": the trigger, the inputs with their reports of before, and the node.
        let mut pinned: HashMap<String, NodeRow> = HashMap::new();
        let graph = match &params.node {
            None => full.clone(),
            Some(only) => {
                let node = full
                    .nodes
                    .iter()
                    .find(|n| &n.id == only)
                    .ok_or_else(|| not_found("no such node"))?;
                if node.kind == WorkflowNodeKind::Manual {
                    return Err(RpcError::invalid_params(
                        "execute the workflow to start the trigger",
                    ));
                }
                let inputs: Vec<&String> = node.inputs.iter().filter(|i| **i != trigger).collect();
                {
                    let book = self.book();
                    for input in &inputs {
                        let found = book.iter().rev().find_map(|exec| {
                            (exec.row.workflow == params.workflow && exec.row.folder == folder)
                                .then(|| exec.node(input))
                                .flatten()
                                .filter(|row| {
                                    node_state(row) == NodeState::Done && row.report.is_some()
                                })
                                .map(|row| (row.clone(), exec.label()))
                        });
                        let Some((row, label)) = found else {
                            return Err(RpcError::new(
                                ErrorCode::NotFound,
                                format!(
                                    "The input \"{input}\" has no report in an execution in this folder. Execute the workflow first."
                                ),
                            ));
                        };
                        pinned.insert(
                            (*input).clone(),
                            NodeRow {
                                reason: Some(format!("From {label}")),
                                ..row
                            },
                        );
                    }
                }
                let keep: Vec<&str> = std::iter::once(trigger.as_str())
                    .chain(inputs.iter().map(|i| i.as_str()))
                    .chain(std::iter::once(only.as_str()))
                    .collect();
                WorkflowGraph {
                    name: full.name.clone(),
                    nodes: full
                        .nodes
                        .iter()
                        .filter(|n| keep.contains(&n.id.as_str()))
                        .map(|n| {
                            let mut n = n.clone();
                            if n.id != *only {
                                n.inputs.retain(|i| keep.contains(&i.as_str()));
                            }
                            n
                        })
                        .collect(),
                }
            }
        };
        let id = ExecutionId::new(new_ulid()?);
        let number = self
            .book()
            .iter()
            .filter(|e| e.row.workflow == params.workflow)
            .map(|e| e.row.number)
            .max()
            .unwrap_or(0)
            + 1;
        let dir = self
            .root
            .join(format!("{}-{}-{number}", text::stamp(now), params.workflow));
        fs::create_dir_all(&dir).map_err(|e| RpcError::internal(e.to_string()))?;
        let graph_text = file::render(&graph);
        fs::write(dir.join("graph.toml"), &graph_text)
            .map_err(|e| RpcError::internal(e.to_string()))?;
        let workspace = self.workspace_for(&folder).await.ok();
        let row = ExecutionRow {
            id: id.to_string(),
            workflow: params.workflow.clone(),
            name: graph.name.clone(),
            number,
            folder: folder.clone(),
            branch: facts.branch.clone(),
            base: params
                .base
                .clone()
                .map(|b| b.trim().to_owned())
                .filter(|b| !b.is_empty())
                .or(facts.base.clone()),
            head: facts.head.clone(),
            tree: facts.tree.clone(),
            notes: params.notes.clone().filter(|n| !n.trim().is_empty()),
            graph: graph_text,
            only_node: params.node.clone(),
            workspace: workspace.map(|w| w.to_string()),
            dir: dir.to_string_lossy().into_owned(),
            state: to_text(&ExecutionState::Running),
            tree_changed: false,
            started_at: now,
            ended_at: None,
        };
        let nodes = graph
            .nodes
            .iter()
            .map(|node| {
                if let Some(row) = pinned.remove(&node.id) {
                    return NodeRow {
                        execution: id.to_string(),
                        ..row
                    };
                }
                let trigger = node.kind == WorkflowNodeKind::Manual;
                NodeRow {
                    execution: id.to_string(),
                    node: node.id.clone(),
                    state: to_text(if trigger {
                        &NodeState::Done
                    } else {
                        &NodeState::Waiting
                    }),
                    reason: None,
                    run: None,
                    verdict: None,
                    report: None,
                    turns: 0,
                    started_at: trigger.then_some(now),
                    ended_at: trigger.then_some(now),
                }
            })
            .collect();
        let exec = Exec {
            id,
            row,
            graph,
            nodes,
            panes: HashMap::new(),
        };
        let label = exec.label();
        self.book().push(exec.clone());
        self.store(
            &exec,
            vec![EventPayload::WorkflowExecutionStarted(
                WorkflowExecutionEvent {
                    execution: id,
                    workflow: params.workflow.clone(),
                    state: ExecutionState::Running,
                },
            )],
        )
        .await;
        self.publish_order();
        self.advance(id).await;
        serde_json::to_value(WorkflowExecuted {
            execution: id,
            label,
        })
        .map_err(|e| RpcError::internal(e.to_string()))
    }

    // --- the order ------------------------------------------------------------------------------

    /// Starts each node that can start, and gives the execution its state.
    async fn advance(self: &Arc<Self>, id: ExecutionId) {
        let max = usize::try_from(self.config.config().workflows.max_agents.max(1)).unwrap_or(1);
        loop {
            let Some(exec) = self.book().iter().find(|e| e.id == id).cloned() else {
                return;
            };
            if exec.state() != ExecutionState::Running {
                return;
            }
            let working = exec
                .nodes
                .iter()
                .filter(|n| matches!(node_state(n), NodeState::Working | NodeState::NeedsYou))
                .count();
            let mut next: Option<(String, bool)> = None;
            let mut queued = Vec::new();
            for node_id in graph::start_order(&exec.graph) {
                let (Some(row), Some(spec)) = (exec.node(&node_id), exec.spec(&node_id)) else {
                    continue;
                };
                let state = node_state(row);
                if !matches!(state, NodeState::Waiting | NodeState::Queued) {
                    continue;
                }
                let ready = spec.inputs.iter().all(|input| {
                    exec.node(input).is_some_and(|r| {
                        matches!(node_state(r), NodeState::Done | NodeState::Skipped)
                    })
                });
                if !ready {
                    continue;
                }
                let needs_agent = spec.kind == WorkflowNodeKind::Agent
                    || (spec.kind == WorkflowNodeKind::Report && spec.summary_by.is_some());
                if !needs_agent {
                    next = Some((node_id, false));
                    break;
                }
                if working < max {
                    next = Some((node_id, true));
                    break;
                }
                if state == NodeState::Waiting {
                    queued.push(node_id);
                }
            }
            for node in queued {
                self.set_node(id, &node, |row| {
                    row.state = to_text(&NodeState::Queued);
                    row.reason = Some("Waits for a free agent.".to_owned());
                })
                .await;
            }
            match next {
                Some((node, true)) => self.start_agent(id, &node).await,
                Some((node, false)) => self.write_report(id, &node, None).await,
                None => break,
            }
        }
        self.settle(id).await;
    }

    /// The state of the execution after a change: done, paused or running.
    async fn settle(self: &Arc<Self>, id: ExecutionId) {
        let Some(exec) = self.book().iter().find(|e| e.id == id).cloned() else {
            return;
        };
        let current = exec.state();
        if matches!(current, ExecutionState::Done | ExecutionState::Cancelled) {
            return;
        }
        let states: Vec<NodeState> = exec.nodes.iter().map(node_state).collect();
        let next = if states
            .iter()
            .all(|s| matches!(s, NodeState::Done | NodeState::Skipped))
        {
            ExecutionState::Done
        } else if states.iter().any(|s| {
            matches!(
                s,
                NodeState::Working | NodeState::NeedsYou | NodeState::Queued
            )
        }) {
            ExecutionState::Running
        } else if states.contains(&NodeState::Failed) {
            ExecutionState::Paused
        } else {
            ExecutionState::Running
        };
        if next == current {
            return;
        }
        if next == ExecutionState::Done {
            self.end(id, ExecutionState::Done).await;
            return;
        }
        let Some((exec, ())) = self.change(id, |exec| exec.row.state = to_text(&next)) else {
            return;
        };
        self.store(&exec, Vec::new()).await;
        if next == ExecutionState::Paused {
            self.notify(&exec, "is paused: a node failed").await;
        }
    }

    /// Ends an execution: the state, the time, whether the tree changed, the notice and the
    /// retention.
    async fn end(self: &Arc<Self>, id: ExecutionId, state: ExecutionState) {
        let Some(folder) = self
            .book()
            .iter()
            .find(|e| e.id == id)
            .map(|e| (e.row.folder.clone(), e.row.tree.clone()))
        else {
            return;
        };
        let changed = match folder.1 {
            Some(before) => self
                .facts
                .facts(Path::new(&folder.0))
                .await
                .tree
                .is_some_and(|now| now != before),
            None => false,
        };
        let now = now_ms();
        let Some((exec, ())) = self.change(id, |exec| {
            exec.row.state = to_text(&state);
            exec.row.ended_at = Some(now);
            exec.row.tree_changed = changed;
        }) else {
            return;
        };
        self.store(
            &exec,
            vec![EventPayload::WorkflowExecutionEnded(
                WorkflowExecutionEvent {
                    execution: id,
                    workflow: exec.row.workflow.clone(),
                    state,
                },
            )],
        )
        .await;
        if state == ExecutionState::Done {
            let failed = exec
                .nodes
                .iter()
                .filter(|n| n.verdict.as_deref() == Some("fail"))
                .count();
            let warned = exec
                .nodes
                .iter()
                .filter(|n| n.verdict.as_deref() == Some("warn"))
                .count();
            let mut words = vec!["is done".to_owned()];
            if failed > 0 {
                words.push(format!("{failed} fail"));
            }
            if warned > 0 {
                words.push(format!("{warned} warn"));
            }
            self.notify(&exec, &words.join(" · ")).await;
        }
        self.prune().await;
    }

    /// A native notification when no window of the desk has the focus.
    async fn notify(&self, exec: &Exec, what: &str) {
        let Ok(actor) = self.actor() else {
            return;
        };
        let title = format!("{} {what}", exec.label());
        let body = exec.row.folder.clone();
        let _ = actor
            .update(move |parts| {
                if parts.state.focus.desk_focused() {
                    return;
                }
                if let Some(host) = parts.state.host.clone() {
                    host.notify(HostNotification {
                        title,
                        body,
                        urgency: Urgency::Normal,
                        interaction: None,
                        pane: None,
                        actions: Vec::new(),
                    });
                }
            })
            .await;
    }

    /// Removes the oldest executions that ended after `workflows.keep_executions`, with their
    /// files.
    async fn prune(&self) {
        let keep = usize::try_from(self.config.config().workflows.keep_executions).unwrap_or(0);
        let gone: Vec<Exec> = {
            let mut book = self.book();
            let ended = book
                .iter()
                .filter(|e| matches!(e.state(), ExecutionState::Done | ExecutionState::Cancelled))
                .count();
            let mut extra = ended.saturating_sub(keep);
            let mut gone = Vec::new();
            book.retain(|e| {
                let old = extra > 0
                    && matches!(e.state(), ExecutionState::Done | ExecutionState::Cancelled);
                if old {
                    extra -= 1;
                    gone.push(e.clone());
                }
                !old
            });
            gone
        };
        if gone.is_empty() {
            return;
        }
        let ids: Vec<String> = gone.iter().map(|e| e.row.id.clone()).collect();
        if let Ok(actor) = self.actor() {
            let _ = actor
                .update(move |mut parts| {
                    parts.commit(|tx| {
                        for id in &ids {
                            tx.delete_execution(id)?;
                        }
                        Ok(())
                    })
                })
                .await;
        }
        for exec in gone {
            self.hub
                .remove(Projection::WorkflowExecutions, &exec.id.to_string());
            // Only a folder of the executions goes, never a folder that a row names by mistake.
            let dir = PathBuf::from(&exec.row.dir);
            if dir.starts_with(&self.root) {
                let _ = fs::remove_dir_all(&dir);
            }
        }
        self.publish_order();
    }

    // --- agents ---------------------------------------------------------------------------------

    /// The inputs of a node for its footer, and the values of its references.
    fn inputs(exec: &Exec, spec: &WorkflowNode) -> Vec<Input> {
        spec.inputs
            .iter()
            .filter_map(|input| {
                let input_spec = exec.spec(input)?;
                if input_spec.kind == WorkflowNodeKind::Manual {
                    return None;
                }
                let row = exec.node(input)?;
                Some(Input {
                    title: graph::title(input_spec).to_owned(),
                    state: if node_state(row) == NodeState::Done {
                        "done"
                    } else {
                        "skipped"
                    },
                    verdict: row.verdict.as_deref().and_then(from_text),
                    report: row.report.clone(),
                })
            })
            .collect()
    }

    /// The prompt of a node with its references filled in, and the footer.
    fn prompt(exec: &Exec, spec: &WorkflowNode, summary: bool) -> String {
        let trigger = graph::trigger(&exec.graph).map(|t| t.id.clone());
        let row = &exec.row;
        let body = if summary {
            spec.summary_prompt
                .clone()
                .filter(|p| !p.trim().is_empty())
                .unwrap_or_else(|| text::SUMMARY_PROMPT.to_owned())
        } else {
            spec.prompt.clone()
        };
        let filled = graph::fill(&body, |reference| {
            if Some(&reference.node) == trigger.as_ref() {
                let value = match reference.field.as_str() {
                    "folder" => Some(row.folder.clone()),
                    "branch" => row.branch.clone(),
                    "base" => row.base.clone(),
                    "head" => row.head.clone(),
                    "notes" => row.notes.clone(),
                    _ => return None,
                };
                return Some(value.unwrap_or_else(|| format!("(no {})", reference.field)));
            }
            if reference.field == "report" {
                let found = exec.node(&reference.node)?;
                return Some(
                    found
                        .report
                        .clone()
                        .unwrap_or_else(|| "(no report: the node was skipped)".to_owned()),
                );
            }
            None
        });
        let inputs = Self::inputs(exec, spec);
        let footer = text::footer(&Footer {
            workflow: &exec.row.name,
            node: graph::title(spec),
            execution: exec.row.number,
            inputs: &inputs,
            can_edit: spec.can_edit && !summary,
            summary,
        });
        format!("{}\n\n{footer}", filled.trim_end())
    }

    async fn start_agent(self: &Arc<Self>, id: ExecutionId, node: &str) {
        let Some(exec) = self.book().iter().find(|e| e.id == id).cloned() else {
            return;
        };
        let Some(spec) = exec.spec(node).cloned() else {
            return;
        };
        let summary = spec.kind == WorkflowNodeKind::Report;
        let harness = if summary {
            spec.summary_by
        } else {
            spec.harness
        };
        let Some(harness) = harness else {
            self.fail_node(id, node, "The node has no harness.").await;
            return;
        };
        let now = now_ms();
        self.set_node(id, node, |row| {
            row.state = to_text(&NodeState::Working);
            row.reason = Some("Starting the agent.".to_owned());
            row.run = None;
            row.turns = 0;
            row.started_at = Some(now);
            row.ended_at = None;
        })
        .await;
        let workspace = match exec.row.workspace.as_deref().and_then(|w| w.parse().ok()) {
            Some(workspace) if self.workspace_exists(workspace).await => Ok(workspace),
            _ => self.workspace_for(&exec.row.folder).await,
        };
        let workspace = match workspace {
            Ok(workspace) => {
                let text = workspace.to_string();
                self.change(id, |exec| exec.row.workspace = Some(text));
                workspace
            }
            Err(error) => {
                self.fail_node(
                    id,
                    node,
                    &format!("No workspace for the agent: {}", error.message),
                )
                .await;
                return;
            }
        };
        let start = HiddenStart {
            harness,
            workspace,
            tab: format!("{} · {}", exec.label(), graph::title(&spec)),
            cwd: exec.row.folder.clone(),
            prompt: Self::prompt(&exec, &spec, summary),
            model: if summary { None } else { spec.model.clone() },
            effort: if summary { None } else { spec.effort.clone() },
            read_only: summary || !spec.can_edit,
            read_dirs: self.read_dirs(&exec),
        };
        match self.control.start_hidden(start).await {
            Ok(job) => self.wait_for_start(id, node.to_owned(), job),
            Err(error) => {
                self.fail_node(
                    id,
                    node,
                    &format!("The agent did not start: {}", error.message),
                )
                .await;
            }
        }
    }

    /// The folders of the reports that a node reads: this execution, and the executions that
    /// pinned inputs come from.
    fn read_dirs(&self, exec: &Exec) -> Vec<String> {
        let mut dirs = vec![exec.row.dir.clone()];
        for row in &exec.nodes {
            if let Some(parent) = row
                .report
                .as_deref()
                .and_then(|r| Path::new(r).parent())
                .map(|p| p.to_string_lossy().into_owned())
                && !dirs.contains(&parent)
            {
                dirs.push(parent);
            }
        }
        dirs
    }

    async fn workspace_exists(&self, workspace: WorkspaceId) -> bool {
        let Ok(actor) = self.actor() else {
            return false;
        };
        actor
            .update(move |parts| {
                parts
                    .state
                    .layout
                    .workspaces
                    .iter()
                    .any(|w| w.id == workspace)
            })
            .await
            .unwrap_or(false)
    }

    /// Waits for the job of a start in a task of its own, and sends its end to the executor.
    fn wait_for_start(self: &Arc<Self>, execution: ExecutionId, node: String, job: JobId) {
        let jobs = Arc::clone(&self.jobs);
        let queue = self.queue.clone();
        self.handle.spawn(async move {
            let result = loop {
                match jobs.get(job) {
                    Some(value) if value.status == JobStatus::Done => {
                        let result = value.result.unwrap_or(Value::Null);
                        let run = result["run"].as_str().and_then(|r| r.parse().ok());
                        let pane = result["pane"].as_str().and_then(|p| p.parse().ok());
                        break match (run, pane) {
                            (Some(run), Some(pane)) => Ok((run, pane)),
                            _ => Err("the start gave no agent".to_owned()),
                        };
                    }
                    Some(value)
                        if matches!(value.status, JobStatus::Failed | JobStatus::Cancelled) =>
                    {
                        break Err(value
                            .error
                            .map_or_else(|| "the start failed".to_owned(), |e| e.message));
                    }
                    None => break Err("the start is gone".to_owned()),
                    _ => tokio::time::sleep(JOB_POLL).await,
                }
            };
            let _ = queue.send(Message::Started {
                execution,
                node,
                result,
            });
        });
    }

    async fn started(
        self: &Arc<Self>,
        id: ExecutionId,
        node: &str,
        result: Result<(RunId, PaneId), String>,
    ) {
        let still = self
            .book()
            .iter()
            .find(|e| e.id == id)
            .and_then(|e| e.node(node).map(node_state))
            == Some(NodeState::Working);
        match result {
            Ok((run, pane)) if still => {
                self.change(id, |exec| {
                    exec.panes.insert(node.to_owned(), pane);
                });
                self.set_node(id, node, |row| {
                    row.run = Some(run.to_string());
                    row.reason = None;
                })
                .await;
                let early = self
                    .early
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .remove(&run);
                if let Some(message) = early {
                    self.turn_ended(run, message).await;
                }
            }
            // The node was cancelled while its agent started.
            Ok((_, pane)) => self.close_hidden(Some(pane)).await,
            Err(error) if still => {
                self.fail(id, node, &format!("The agent did not start: {error}"))
                    .await;
            }
            Err(_) => {}
        }
    }

    /// The execution and the node of a run.
    fn find_run(&self, run: RunId) -> Option<(ExecutionId, String)> {
        let text = run.to_string();
        self.book().iter().find_map(|exec| {
            exec.nodes
                .iter()
                .find(|n| n.run.as_deref() == Some(text.as_str()))
                .map(|n| (exec.id, n.node.clone()))
        })
    }

    async fn turn_ended(self: &Arc<Self>, run: RunId, message: Option<String>) {
        let Some((id, node)) = self.find_run(run) else {
            // The start has not told the executor its run yet.
            self.early
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .insert(run, message);
            return;
        };
        let Some(exec) = self.book().iter().find(|e| e.id == id).cloned() else {
            return;
        };
        let (Some(row), Some(spec)) = (exec.node(&node), exec.spec(&node)) else {
            return;
        };
        if !matches!(node_state(row), NodeState::Working | NodeState::NeedsYou) {
            return;
        }
        let turns = row.turns + 1;
        self.change(id, |exec| {
            if let Some(row) = exec.node_mut(&node) {
                row.turns = turns;
            }
        });
        let text = message.unwrap_or_default();
        if spec.kind == WorkflowNodeKind::Report {
            self.write_report(id, &node, Some(text)).await;
            return;
        }
        if text::verdict(&text).is_none() && turns == 1 {
            let input_id = u64::try_from(now_ms()).unwrap_or(0);
            let reminded = self
                .control
                .reply(Ref::<RunId>::id(run), input_id, text::REMINDER.to_owned())
                .await;
            if reminded.is_ok() {
                self.set_node(id, &node, |row| {
                    row.reason = Some("Reminded the agent to write its report.".to_owned());
                })
                .await;
                return;
            }
        }
        self.finish(id, &node, &text).await;
    }

    /// The report of an agent node is in: the file, the verdict, the end of the agent.
    async fn finish(self: &Arc<Self>, id: ExecutionId, node: &str, report: &str) {
        let Some(exec) = self.book().iter().find(|e| e.id == id).cloned() else {
            return;
        };
        let path = Path::new(&exec.row.dir).join(format!("{node}.md"));
        let body = if report.trim().is_empty() {
            "The agent gave no report.\n".to_owned()
        } else {
            format!("{}\n", report.trim_end())
        };
        if let Err(error) = fs::write(&path, body) {
            self.fail(id, node, &format!("The report is not written: {error}"))
                .await;
            return;
        }
        let verdict = text::verdict(report).unwrap_or(Verdict::Unknown);
        let now = now_ms();
        let path = path.to_string_lossy().into_owned();
        self.set_node(id, node, |row| {
            row.state = to_text(&NodeState::Done);
            row.reason = None;
            row.verdict = Some(to_text(&verdict));
            row.report = Some(path);
            row.ended_at = Some(now);
        })
        .await;
        let pane = self.pane_of(&exec, node).await;
        self.close_hidden(pane).await;
        self.change(id, |exec| exec.panes.remove(node));
        self.later(id);
    }

    /// Writes `<node>.md` of a Report node from the reports of its inputs, with the summary of its
    /// agent when it has one.
    async fn write_report(self: &Arc<Self>, id: ExecutionId, node: &str, summary: Option<String>) {
        let Some(exec) = self.book().iter().find(|e| e.id == id).cloned() else {
            return;
        };
        let Some(spec) = exec.spec(node).cloned() else {
            return;
        };
        let dir = PathBuf::from(&exec.row.dir);
        let mut verdict = None;
        let rows: Vec<ReportRow> = spec
            .inputs
            .iter()
            .filter_map(|input| {
                let input_spec = exec.spec(input)?;
                if input_spec.kind == WorkflowNodeKind::Manual {
                    return None;
                }
                let row = exec.node(input)?;
                let state = node_state(row);
                let input_verdict: Option<Verdict> = row.verdict.as_deref().and_then(from_text);
                if state == NodeState::Done {
                    verdict = text::worse(verdict, input_verdict);
                }
                let path = row.report.clone();
                let link = path.as_ref().map(|p| {
                    let p = Path::new(p);
                    if p.parent() == Some(dir.as_path()) {
                        p.file_name()
                            .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
                    } else {
                        p.to_string_lossy().into_owned()
                    }
                });
                let text = path.as_ref().and_then(|p| read_capped(Path::new(p)));
                Some(ReportRow {
                    title: graph::title(input_spec).to_owned(),
                    agent: agent_label(input_spec),
                    state: if state == NodeState::Done {
                        "done"
                    } else {
                        "skipped"
                    },
                    verdict: input_verdict,
                    time: match (row.started_at, row.ended_at) {
                        (Some(start), Some(end)) if state == NodeState::Done => {
                            Some(text::duration(end - start))
                        }
                        _ => None,
                    },
                    link,
                    text,
                })
            })
            .collect();
        let now = now_ms();
        let changed = match &exec.row.tree {
            Some(before) => self
                .facts
                .facts(Path::new(&exec.row.folder))
                .await
                .tree
                .is_some_and(|tree| &tree != before),
            None => false,
        };
        let label = exec.label();
        let markdown = text::report(
            &ReportHead {
                label: &label,
                folder: &exec.row.folder,
                branch: exec.row.branch.as_deref(),
                head: exec.row.head.as_deref(),
                base: exec.row.base.as_deref(),
                started_at: exec.row.started_at,
                now,
                tree_changed: changed,
            },
            summary.as_deref(),
            &rows,
        );
        let path = dir.join(format!("{node}.md"));
        if let Err(error) = fs::write(&path, markdown) {
            self.fail(id, node, &format!("The report is not written: {error}"))
                .await;
            return;
        }
        let path = path.to_string_lossy().into_owned();
        let started = exec.node(node).and_then(|r| r.started_at).unwrap_or(now);
        self.set_node(id, node, |row| {
            row.state = to_text(&NodeState::Done);
            row.reason = None;
            row.verdict = verdict.map(|v| to_text(&v));
            row.report = Some(path);
            row.started_at = Some(started);
            row.ended_at = Some(now);
        })
        .await;
        if summary.is_some() {
            let pane = self.pane_of(&exec, node).await;
            self.close_hidden(pane).await;
            self.change(id, |exec| exec.panes.remove(node));
            self.later(id);
        }
    }

    /// A node failed: its state and reason, the end of its agent, and the next nodes.
    async fn fail(self: &Arc<Self>, id: ExecutionId, node: &str, reason: &str) {
        self.fail_node(id, node, reason).await;
        self.later(id);
    }

    /// Starts the nodes that can start after this message. A message breaks the recursion of
    /// `advance`.
    fn later(&self, id: ExecutionId) {
        let _ = self.queue.send(Message::Advance(id));
    }

    async fn fail_node(self: &Arc<Self>, id: ExecutionId, node: &str, reason: &str) {
        let found = self.book().iter().find(|e| e.id == id).cloned();
        let pane = match found {
            Some(exec) => self.pane_of(&exec, node).await,
            None => None,
        };
        let now = now_ms();
        let reason = reason.to_owned();
        self.set_node(id, node, |row| {
            row.state = to_text(&NodeState::Failed);
            row.reason = Some(reason);
            row.ended_at = Some(now);
        })
        .await;
        self.close_hidden(pane).await;
        self.change(id, |exec| exec.panes.remove(node));
    }

    /// The state of a run of a node changed: an interaction, an error, the end of the process.
    async fn run_changed(self: &Arc<Self>, run: RunId) {
        let Some((id, node)) = self.find_run(run) else {
            return;
        };
        let state = self
            .book()
            .iter()
            .find(|e| e.id == id)
            .and_then(|e| e.node(&node).map(node_state));
        if !matches!(state, Some(NodeState::Working | NodeState::NeedsYou)) {
            return;
        }
        let Some(look) = self.look(run).await else {
            self.fail(id, &node, "The agent is gone.").await;
            return;
        };
        if !look.live {
            // A turn that ended just before the process may be in the transcript.
            if let Some(turn) = look
                .transcript
                .as_deref()
                .and_then(|path| last_turn(look.harness, Path::new(path)))
                .filter(|turn| turn.ended)
            {
                self.turn_ended(run, turn.message).await;
                return;
            }
            self.fail(id, &node, "The agent ended before its report.")
                .await;
            return;
        }
        match look.execution {
            Execution::Error => {
                self.fail(
                    id,
                    &node,
                    "The agent stopped with an error. Look at its pane, then retry.",
                )
                .await;
                return;
            }
            Execution::RateLimited { .. } => {
                self.fail(
                    id,
                    &node,
                    "The agent hit a rate limit. Retry when the limit is over.",
                )
                .await;
                return;
            }
            _ => {}
        }
        let next = if look.pending {
            NodeState::NeedsYou
        } else {
            NodeState::Working
        };
        if state != Some(next) {
            self.set_node(id, &node, |row| row.state = to_text(&next))
                .await;
        }
    }

    async fn tick(self: &Arc<Self>) {
        let now = now_ms();
        let default = self.config.config().workflows.time_limit_min;
        let running: Vec<Exec> = self
            .book()
            .iter()
            .filter(|e| e.state() == ExecutionState::Running)
            .cloned()
            .collect();
        for exec in running {
            for row in &exec.nodes {
                if !matches!(node_state(row), NodeState::Working | NodeState::NeedsYou) {
                    continue;
                }
                let limit = exec
                    .spec(&row.node)
                    .and_then(|s| s.time_limit_min)
                    .unwrap_or(default)
                    .max(1);
                if row
                    .started_at
                    .is_some_and(|start| now - start > i64::from(limit) * 60_000)
                {
                    self.fail(
                        exec.id,
                        &row.node,
                        &format!("The time limit of {limit} min ran out."),
                    )
                    .await;
                }
            }
            self.publish(&exec);
        }
    }

    // --- the calls of the person ----------------------------------------------------------------

    async fn retry(self: &Arc<Self>, id: ExecutionId, node: &str) -> Result<Value, RpcError> {
        let exec = self
            .book()
            .iter()
            .find(|e| e.id == id)
            .cloned()
            .ok_or_else(|| not_found("no such execution"))?;
        let row = exec.node(node).ok_or_else(|| not_found("no such node"))?;
        if !matches!(node_state(row), NodeState::Failed | NodeState::Cancelled) {
            return Err(RpcError::new(
                ErrorCode::Conflict,
                "only a node that failed can start again",
            ));
        }
        let pane = self.pane_of(&exec, node).await;
        self.close_hidden(pane).await;
        let Some((exec, ())) = self.change(id, |exec| {
            exec.row.state = to_text(&ExecutionState::Running);
            exec.row.ended_at = None;
            exec.panes.remove(node);
            for row in &mut exec.nodes {
                if row.node == node || node_state(row) == NodeState::Cancelled {
                    row.state = to_text(&NodeState::Waiting);
                    row.reason = None;
                    row.run = None;
                    row.turns = 0;
                    row.started_at = None;
                    row.ended_at = None;
                }
            }
        }) else {
            return Err(not_found("no such execution"));
        };
        let event = Self::node_event(&exec, node);
        self.store(&exec, vec![event]).await;
        self.advance(id).await;
        Ok(json!({}))
    }

    async fn skip(self: &Arc<Self>, id: ExecutionId, node: &str) -> Result<Value, RpcError> {
        let exec = self
            .book()
            .iter()
            .find(|e| e.id == id)
            .cloned()
            .ok_or_else(|| not_found("no such execution"))?;
        let row = exec.node(node).ok_or_else(|| not_found("no such node"))?;
        if !matches!(
            node_state(row),
            NodeState::Failed | NodeState::Waiting | NodeState::Queued
        ) || exec.state() == ExecutionState::Cancelled
        {
            return Err(RpcError::new(
                ErrorCode::Conflict,
                "only a node that failed or waits can be skipped",
            ));
        }
        let pane = self.pane_of(&exec, node).await;
        self.close_hidden(pane).await;
        self.change(id, |exec| {
            exec.row.state = to_text(&ExecutionState::Running);
            exec.panes.remove(node);
        });
        self.set_node(id, node, |row| {
            row.state = to_text(&NodeState::Skipped);
            row.reason = None;
        })
        .await;
        self.advance(id).await;
        Ok(json!({}))
    }

    async fn cancel(self: &Arc<Self>, id: ExecutionId) -> Result<Value, RpcError> {
        let exec = self
            .book()
            .iter()
            .find(|e| e.id == id)
            .cloned()
            .ok_or_else(|| not_found("no such execution"))?;
        if matches!(
            exec.state(),
            ExecutionState::Done | ExecutionState::Cancelled
        ) {
            return Ok(json!({}));
        }
        for row in &exec.nodes {
            if matches!(node_state(row), NodeState::Working | NodeState::NeedsYou) {
                let pane = self.pane_of(&exec, &row.node).await;
                self.close_hidden(pane).await;
            }
        }
        self.change(id, |exec| {
            exec.panes.clear();
            for row in &mut exec.nodes {
                if !is_final(node_state(row)) {
                    row.state = to_text(&NodeState::Cancelled);
                    row.reason = None;
                }
            }
        });
        self.end(id, ExecutionState::Cancelled).await;
        Ok(json!({}))
    }

    /// `workflow.continue`: the agent of a node starts again with its resume handle.
    pub(crate) async fn resume(
        &self,
        id: ExecutionId,
        node: &str,
        window: Option<WindowId>,
    ) -> Result<JobStarted, RpcError> {
        let (run, workspace, _) = self.node_run(id, node)?;
        let actor = self.actor()?;
        let shown = match window {
            Some(window) => actor
                .update(move |parts| {
                    let layout = &parts.state.layout;
                    let tab = layout.windows.iter().find(|w| w.id == window)?.tab?;
                    layout
                        .workspaces
                        .iter()
                        .find(|w| w.tabs.iter().any(|t| t.id == tab))
                        .map(|w| w.id)
                })
                .await
                .ok()
                .flatten(),
            None => None,
        };
        let Some(workspace) = shown.or(workspace) else {
            return Err(not_found("no workspace for the agent"));
        };
        let job = self
            .control
            .resume(AgentResume {
                agent: Ref::<RunId>::id(run),
                placement: Some(Placement::NewTab {
                    workspace: Ref::<WorkspaceId>::id(workspace),
                }),
            })
            .await?;
        Ok(JobStarted { job })
    }

    /// `workflow.reveal`: the hidden tab of the agent of a node shows, and the window selects it.
    pub(crate) async fn reveal(
        &self,
        id: ExecutionId,
        node: &str,
        window: WindowId,
    ) -> Result<(), RpcError> {
        let (run, _, pane) = self.node_run(id, node)?;
        let pane = match pane {
            Some(pane) => Some(pane),
            None => self.look(run).await.and_then(|look| look.pane),
        }
        .ok_or_else(|| not_found("the agent of the node has no pane"))?;
        let actor = self.actor()?;
        let tab = actor
            .update(move |parts| parts.state.layout.tab_of_pane(pane).map(|(_, tab)| tab))
            .await
            .map_err(|e| RpcError::internal(e.to_string()))?
            .ok_or_else(|| not_found("the tab of the agent is gone"))?;
        actor
            .call(
                Call::TabSelect(TabSelect {
                    tab: Ref::<TabId>::id(tab),
                    window: Some(window),
                }),
                Origin::ui(),
            )
            .await?;
        Ok(())
    }

    // --- the start of the core ------------------------------------------------------------------

    /// Loads the executions of the store, marks the runs of their nodes, and takes the turns that
    /// ended while the core was away.
    async fn load(self: &Arc<Self>) {
        let Ok(actor) = self.actor() else {
            return;
        };
        let rows = actor
            .update(|parts| parts.store.executions())
            .await
            .ok()
            .and_then(Result::ok)
            .unwrap_or_default();
        let mut book = Vec::new();
        for (row, nodes) in rows {
            let Ok(id) = row.id.parse::<ExecutionId>() else {
                continue;
            };
            let Ok(graph) = file::parse(&row.graph) else {
                tracing::warn!(execution = %row.id, "the graph of an execution is not read");
                continue;
            };
            book.push(Exec {
                id,
                row,
                graph,
                nodes,
                panes: HashMap::new(),
            });
        }
        *self.book() = book.clone();
        for exec in &book {
            self.publish(exec);
        }
        self.publish_order();
        let mut runs = Vec::new();
        for exec in book
            .iter()
            .filter(|e| e.state() == ExecutionState::Running || e.state() == ExecutionState::Paused)
        {
            for row in &exec.nodes {
                if matches!(node_state(row), NodeState::Working | NodeState::NeedsYou)
                    && let Some(run) = row.run.as_deref().and_then(|r| r.parse::<RunId>().ok())
                {
                    runs.push(run);
                }
            }
        }
        self.mark(runs.clone()).await;
        for exec in book.iter().filter(|e| e.state() == ExecutionState::Running) {
            for row in &exec.nodes {
                if !matches!(node_state(row), NodeState::Working | NodeState::NeedsYou) {
                    continue;
                }
                let Some(run) = row.run.as_deref().and_then(|r| r.parse::<RunId>().ok()) else {
                    // The agent did not start before the core went away.
                    self.set_node(exec.id, &row.node, |row| {
                        row.state = to_text(&NodeState::Waiting);
                        row.reason = None;
                    })
                    .await;
                    continue;
                };
                let Some(look) = self.look(run).await else {
                    self.fail(exec.id, &row.node, "The agent is gone.").await;
                    continue;
                };
                if let Some(pane) = look.pane {
                    self.change(exec.id, |e| {
                        e.panes.insert(row.node.clone(), pane);
                    });
                }
                let turn = look
                    .transcript
                    .as_deref()
                    .and_then(|path| last_turn(look.harness, Path::new(path)));
                match turn {
                    Some(turn) if turn.ended => self.turn_ended(run, turn.message).await,
                    _ if !look.live => {
                        self.fail(exec.id, &row.node, "The agent ended before its report.")
                            .await;
                    }
                    _ => {}
                }
            }
            self.advance(exec.id).await;
        }
        self.prune().await;
    }
}

fn read_capped(path: &Path) -> Option<String> {
    let size = fs::metadata(path).ok()?.len();
    if size > REPORT_CAP {
        return None;
    }
    fs::read_to_string(path).ok()
}

/// The row of `workflow_executions` for an execution.
fn value(exec: &Exec, now: i64) -> ExecutionValue {
    let state = exec.state();
    let nodes = exec
        .graph
        .nodes
        .iter()
        .filter_map(|spec| {
            let row = exec.node(&spec.id)?;
            let node_state = node_state(row);
            let time_text = match (row.started_at, row.ended_at) {
                _ if spec.kind == WorkflowNodeKind::Manual => None,
                (Some(start), Some(end)) => Some(text::duration(end - start)),
                (Some(start), None) if !is_final(node_state) => Some(text::duration(now - start)),
                _ => None,
            };
            Some(ExecutionNodeValue {
                node: spec.id.clone(),
                state: node_state,
                reason: row.reason.clone(),
                run: row.run.as_deref().and_then(|r| r.parse().ok()),
                pane: exec.panes.get(&spec.id).copied(),
                verdict: row.verdict.as_deref().and_then(from_text),
                report: row.report.clone(),
                time_text,
                resumable: row.run.is_some()
                    && is_final(node_state)
                    && spec.kind != WorkflowNodeKind::Manual,
            })
        })
        .collect::<Vec<_>>();
    // The report of the execution: the Report node that ended last.
    let report = exec
        .graph
        .nodes
        .iter()
        .filter(|spec| spec.kind == WorkflowNodeKind::Report)
        .filter_map(|spec| exec.node(&spec.id))
        .filter(|row| node_state(row) == NodeState::Done)
        .max_by_key(|row| row.ended_at)
        .and_then(|row| row.report.clone());
    let length = text::duration(exec.row.ended_at.unwrap_or(now) - exec.row.started_at);
    let time_text = match state {
        ExecutionState::Running => format!("running · {length}"),
        _ => format!(
            "{} · {length}",
            crate::git_view::ago(now, exec.row.started_at)
        ),
    };
    ExecutionValue {
        id: exec.id,
        workflow: exec.row.workflow.clone(),
        label: exec.label(),
        number: exec.row.number,
        folder: exec.row.folder.clone(),
        branch: exec.row.branch.clone(),
        base: exec.row.base.clone(),
        head: exec.row.head.clone(),
        state,
        nodes,
        only: exec.row.only_node.clone(),
        started_at: nh_proto::api::time::Timestamp(exec.row.started_at),
        ended_at: exec.row.ended_at.map(nh_proto::api::time::Timestamp),
        time_text,
        tree_changed: exec.row.tree_changed,
        dir: exec.row.dir.clone(),
        report,
    }
}
