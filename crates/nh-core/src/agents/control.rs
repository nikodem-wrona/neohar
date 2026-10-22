//! What the core does to a run, from the API and from the palette.
//!
//! Two groups. **Typing** (`prompt`, `reply`, `interrupt`, `stop`) puts keys into the pane of a
//! bound run through the lock of the keystrokes, so a person who types at the same moment
//! wins. A reply goes only to a run whose turn ended, at most once for an id.
//! **Launching** (`start`, `resume`) opens a pane, types the command line of the manifest into its
//! shell, waits for the run and for the screen of the harness, and then sends the prompt. It is a
//! job: the run appears in `agents`, and the job tells how far it came.
//!
//! The command line goes through the shell of the pane, so the harness starts with the PATH and
//! the shims that the user's own shell has.

use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, Instant},
};

use nh_agents::{
    manifest::{
        Manifest,
        choices::valid_effort,
        command::{Choice, launch_argv, preassigns, quote, resume_argv, shell_line},
        schema::Launch,
    },
    run::authorizes,
    screen::{self, Detector},
};
use nh_proto::api::{
    calls::agent::{AgentResume, AgentStart, Placement},
    common::{Harness, SplitDirection},
    envelope::{CallError, ErrorCode},
    ids::{InteractionId, JobId, PaneId, Ref, RunId, WorkspaceId},
    methods::Call,
    model::Execution,
};
use serde_json::{Value, json};
use tokio::runtime::Handle;
use ulid::Ulid;

use super::{
    keys::logical_key,
    runs::{self, Expected},
};
use crate::{
    actor::{ActorHandle, Origin, Parts},
    discovery::Discovery,
    error::RpcError,
    ids::new_uuid,
    jobs::{JobHandle, Jobs},
    pane::{Liveness, Pane, lock},
};

/// How long the quit keys have to end the program, before the core ends it.
const QUIT_GRACE: Duration = Duration::from_secs(5);
/// How long a forced stop waits for the program to end, before the core kills it.
const FORCE_GRACE: Duration = Duration::from_secs(3);
/// The time between two keys of a sequence, so that the program reads each one.
const KEY_GAP: Duration = Duration::from_millis(60);
/// The time between the paste of a prompt and the Enter that sends it.
const PASTE_GAP: Duration = Duration::from_millis(80);
/// How often the launch looks at the run and at the screen.
const POLL: Duration = Duration::from_millis(100);
/// How long a new pane has to start its process and show a first screen.
const PANE_WAIT: Duration = Duration::from_secs(10);
/// How long the core waits for the first output of a new shell before it types.
const SHELL_WAIT: Duration = Duration::from_secs(5);
/// The longest text of `agent.reply`, in bytes.
pub(crate) const REPLY_MAX: usize = 16 * 1024;
/// How many replies the core remembers, so that a client that sends one again writes it once.
const REPLIES_KEPT: usize = 256;
/// How long the core remembers a reply. A client sends an input again for 30 s at most.
const REPLY_MEMORY: Duration = Duration::from_secs(300);

type PaneMap = Arc<Mutex<HashMap<Ulid, Arc<Pane>>>>;

/// The service. The state actor, the API and the palette all reach it.
pub(crate) struct Control {
    handle: Handle,
    jobs: Arc<Jobs>,
    panes: PaneMap,
    actor: OnceLock<ActorHandle>,
    /// The login state of the harnesses, which a launch checks first.
    discovery: OnceLock<Arc<Discovery>>,
    /// The replies that went out lately, by pane and input id, the oldest first.
    replies: Mutex<VecDeque<(PaneId, u64, Instant)>>,
}

/// What a call that types into a pane needs to know about the run.
pub(crate) struct Typing {
    run: RunId,
    pane: PaneId,
    interrupt_keys: Vec<String>,
    quit_keys: Vec<String>,
    /// The run waits for an answer: a key now would go to its dialog.
    waiting: bool,
}

fn busy() -> RpcError {
    RpcError::new(
        ErrorCode::Busy,
        "someone else types in this pane, or a verified answer runs; try again",
    )
}

fn not_found(reference: &Ref<RunId>) -> RpcError {
    RpcError::new(ErrorCode::NotFound, format!("no agent {reference}"))
}

fn gone(error: impl std::fmt::Display) -> RpcError {
    RpcError::internal(error.to_string())
}

/// The error of a job from the error of a call.
fn job_error(error: &RpcError) -> CallError {
    CallError {
        code: error.code,
        message: error.message.clone(),
        details: error.details.clone(),
    }
}

/// The run that a call names, when it may be typed into.
pub(crate) fn typing(parts: &Parts<'_>, reference: &Ref<RunId>) -> Result<Typing, RpcError> {
    let id = parts
        .state
        .runs
        .resolve(reference)
        .ok_or_else(|| not_found(reference))?;
    let run = parts
        .state
        .runs
        .by_id
        .get(&id)
        .ok_or_else(|| not_found(reference))?;
    if !run.live() {
        return Err(RpcError::new(
            ErrorCode::AgentGone,
            format!("agent {reference} has ended"),
        ));
    }
    if !authorizes(&run.binding) {
        return Err(RpcError::new(
            ErrorCode::NotBound,
            format!(
                "agent {reference} is not linked to its pane, so nothing is typed into it; link it first"
            ),
        ));
    }
    let pane = run.pane.ok_or_else(|| {
        RpcError::new(
            ErrorCode::PaneGone,
            format!("agent {reference} has no pane"),
        )
    })?;
    let manifests = runs::loaded_manifests(parts);
    let manifest: Option<&Manifest> = runs::manifest_of(&manifests, run);
    let hints = manifest.and_then(|m| m.ui.clone()).unwrap_or_default();
    Ok(Typing {
        run: id,
        pane,
        interrupt_keys: hints.interrupt_keys,
        quit_keys: hints.quit_keys,
        waiting: !parts.state.interactions.open_of_run(id).is_empty(),
    })
}

/// The pane for a reply to a run: the checks of [`typing`], and the run is `idle` with no
/// open interaction. Else the error is `not_idle`, with the execution and the open interactions.
fn reply_target(parts: &Parts<'_>, reference: &Ref<RunId>) -> Result<(RunId, PaneId), RpcError> {
    let target = typing(parts, reference)?;
    let execution = parts
        .state
        .runs
        .by_id
        .get(&target.run)
        .map(|run| run.arbiter.execution().value.clone())
        .ok_or_else(|| not_found(reference))?;
    let pending: Vec<InteractionId> = parts
        .state
        .interactions
        .open_of_run(target.run)
        .into_iter()
        .map(|interaction| interaction.id)
        .collect();
    if execution != Execution::Idle || !pending.is_empty() {
        let mut error = RpcError::new(
            ErrorCode::NotIdle,
            "the agent works or waits for an answer; reply when its turn has ended",
        );
        error.details = json!({ "execution": execution, "pending": pending });
        return Err(error);
    }
    Ok((target.run, target.pane))
}

/// Where a launch goes, with the references resolved.
#[derive(Clone, Debug)]
enum Where {
    NewTab(WorkspaceId),
    Split(PaneId, SplitDirection),
    Pane(PaneId),
    /// A resume without a place: the pane of the old run, if it is an idle shell, else a new tab.
    Reuse(PaneId, WorkspaceId),
}

/// Everything a launch needs, worked out on the actor.
struct Plan {
    title: String,
    harness: Harness,
    argv: Vec<String>,
    session: Option<String>,
    place: Where,
    cwd: Option<String>,
    name: Option<String>,
    task: Option<nh_proto::api::ids::TaskId>,
    prompt: Option<String>,
    model: Option<String>,
    /// The screen manifest and the rule that says the input box is there, and how long to wait.
    ready: Option<Ready>,
}

struct Ready {
    screen: String,
    rule: String,
    timeout: Duration,
}

fn harness_title(harness: Harness) -> &'static str {
    match harness {
        Harness::Claude => "Claude Code",
        Harness::Codex => "Codex",
    }
}

fn place_of(parts: &Parts<'_>, placement: &Placement) -> Result<Where, RpcError> {
    let layout = &parts.state.layout;
    let missing =
        |e: crate::layout::model::LayoutError| RpcError::new(ErrorCode::NotFound, e.to_string());
    Ok(match placement {
        Placement::NewTab { workspace } => {
            let at = layout.resolve_workspace(workspace).map_err(missing)?;
            Where::NewTab(layout.workspaces[at].id)
        }
        Placement::Split { pane, direction } => {
            Where::Split(layout.resolve_pane(pane).map_err(missing)?, *direction)
        }
        Placement::Pane { pane } => Where::Pane(layout.resolve_pane(pane).map_err(missing)?),
    })
}

fn name_free(parts: &Parts<'_>, name: Option<&String>) -> Result<(), RpcError> {
    if let Some(name) = name
        && parts
            .state
            .runs
            .by_id
            .values()
            .any(|run| run.live() && run.name.as_ref() == Some(name))
    {
        return Err(RpcError::new(
            ErrorCode::NameTaken,
            format!("a live agent is called {name}"),
        ));
    }
    Ok(())
}

fn ready_of(manifest: &Manifest) -> Option<Ready> {
    let ready = manifest.launch.as_ref()?.ready.as_ref()?;
    Some(Ready {
        screen: manifest.screen.as_ref()?.manifest.clone(),
        rule: ready.screen_rule.clone()?,
        timeout: Duration::from_millis(ready.timeout_ms.unwrap_or(30_000)),
    })
}

fn plan_start(parts: &Parts<'_>, p: &AgentStart) -> Result<Plan, RpcError> {
    let manifests = runs::loaded_manifests(parts);
    let id = nh_agents::harness::name(p.harness);
    let manifest = manifests
        .get(id)
        .ok_or_else(|| RpcError::new(ErrorCode::NotFound, format!("no manifest {id}")))?;
    let launch = manifest
        .launch
        .as_ref()
        .filter(|launch| !launch.argv.is_empty())
        .ok_or_else(|| {
            RpcError::new(
                ErrorCode::Unsupported,
                format!("the manifest {id} has no launch command"),
            )
        })?;
    name_free(parts, p.name.as_ref())?;
    check_choice(id, launch, p)?;
    let session = if preassigns(launch) {
        Some(new_uuid()?)
    } else {
        None
    };
    Ok(Plan {
        title: format!("Start {}", harness_title(p.harness)),
        harness: p.harness,
        argv: launch_argv(
            launch,
            session.as_deref(),
            Choice {
                model: p.model.as_deref(),
                effort: p.effort.as_deref(),
            },
            &p.args,
        ),
        session,
        place: place_of(parts, &p.placement)?,
        cwd: p.cwd.clone(),
        name: p.name.clone(),
        task: match &p.task {
            Some(reference) => Some(
                parts
                    .state
                    .tasks
                    .resolve(reference)
                    .ok_or_else(|| RpcError::new(ErrorCode::NotFound, "no such task"))?,
            ),
            None => None,
        },
        prompt: p.prompt.clone(),
        model: p.model.clone(),
        ready: ready_of(manifest),
    })
}

/// A model that is not blank and an effort level that a command line can carry. The
/// harness checks the names. `task.create` makes this check before it makes the worktree.
pub(crate) fn check_words(model: Option<&str>, effort: Option<&str>) -> Result<(), RpcError> {
    if model.is_some_and(|model| model.trim().is_empty()) {
        return Err(RpcError::invalid_params("the model is empty"));
    }
    if let Some(effort) = effort
        && !valid_effort(effort)
    {
        return Err(RpcError::invalid_params(format!(
            "the effort level {effort:?} is not letters, digits, - and _"
        )));
    }
    Ok(())
}

/// A model and an effort level that the launch can carry.
fn check_choice(id: &str, launch: &Launch, p: &AgentStart) -> Result<(), RpcError> {
    check_words(p.model.as_deref(), p.effort.as_deref())?;
    if p.effort.is_some() && launch.effort_arg.is_none() {
        return Err(RpcError::new(
            ErrorCode::Unsupported,
            format!("the manifest {id} has no effort_arg, so it takes no effort level"),
        ));
    }
    Ok(())
}

fn plan_resume(parts: &Parts<'_>, p: &AgentResume) -> Result<Plan, RpcError> {
    let id = parts
        .state
        .runs
        .resolve(&p.agent)
        .ok_or_else(|| not_found(&p.agent))?;
    let run = parts
        .state
        .runs
        .by_id
        .get(&id)
        .ok_or_else(|| not_found(&p.agent))?;
    if run.live() {
        return Err(RpcError::new(
            ErrorCode::AgentLive,
            format!("agent {} still runs", p.agent),
        ));
    }
    let session = run.sessions.current().ok_or_else(|| {
        RpcError::new(
            ErrorCode::Unsupported,
            "the run never told its session id, so it has no handle to resume",
        )
    })?;
    let manifests = runs::loaded_manifests(parts);
    let manifest = runs::manifest_of(&manifests, run)
        .ok_or_else(|| RpcError::new(ErrorCode::NotFound, "the manifest of the run is gone"))?;
    let resume = manifest.resume.as_ref().ok_or_else(|| {
        RpcError::new(
            ErrorCode::Unsupported,
            "the manifest of this harness cannot resume",
        )
    })?;
    let argv = resume_argv(resume, session).ok_or_else(|| {
        RpcError::new(
            ErrorCode::Unsupported,
            "the manifest has no resume command line",
        )
    })?;
    let layout = &parts.state.layout;
    let entry = run.pane.and_then(|pane| layout.panes.get(&pane));
    let place = match &p.placement {
        Some(placement) => place_of(parts, placement)?,
        None => match entry {
            Some(entry) => Where::Reuse(entry.id, entry.workspace),
            None => {
                return Err(RpcError::new(
                    ErrorCode::InvalidParams,
                    "the pane of the run is gone, so the call must say where",
                ));
            }
        },
    };
    // The program must start in the directory of the old session (`needs_cwd`).
    let cwd = if resume.needs_cwd.unwrap_or(false) {
        entry.and_then(|entry| {
            entry.cwd.clone().or_else(|| {
                layout
                    .workspace(entry.workspace)
                    .and_then(|workspace| workspace.root.clone())
            })
        })
    } else {
        None
    };
    let name = run
        .name
        .clone()
        .filter(|name| name_free(parts, Some(name)).is_ok());
    Ok(Plan {
        title: format!("Resume {}", harness_title(run.harness)),
        harness: run.harness,
        argv,
        session: Some(session.to_owned()),
        place,
        cwd,
        name,
        task: run.task,
        prompt: None,
        model: run.model.clone(),
        ready: ready_of(manifest),
    })
}

impl Control {
    pub(crate) fn new(handle: Handle, jobs: Arc<Jobs>, panes: PaneMap) -> Arc<Self> {
        Arc::new(Self {
            handle,
            jobs,
            panes,
            actor: OnceLock::new(),
            discovery: OnceLock::new(),
            replies: Mutex::new(VecDeque::new()),
        })
    }

    /// Gives the service the discovery of the harnesses.
    pub(crate) fn set_discovery(&self, discovery: Arc<Discovery>) {
        let _ = self.discovery.set(discovery);
    }

    /// Fails with `harness_unavailable` when the harness is not installed, or when a new probe
    /// finds it logged out. A core without the discovery checks nothing.
    pub(crate) async fn ensure_harness(&self, harness: Harness) -> Result<(), RpcError> {
        match self.discovery.get() {
            Some(discovery) => discovery.ensure(harness).await,
            None => Ok(()),
        }
    }

    /// Gives the service the actor that it asks.
    pub(crate) fn bind(&self, actor: ActorHandle) {
        let _ = self.actor.set(actor);
    }

    fn actor(&self) -> Result<&ActorHandle, RpcError> {
        self.actor
            .get()
            .ok_or_else(|| RpcError::internal("the core is not up yet"))
    }

    fn pane(&self, pane: PaneId) -> Result<Arc<Pane>, RpcError> {
        lock(&self.panes).get(&pane.ulid()).cloned().ok_or_else(|| {
            RpcError::new(ErrorCode::PaneGone, format!("pane {pane} has no process"))
        })
    }

    // --- typing --------------------------------------------------------------------------------

    /// Types a list of keys, one after the other.
    async fn press(pane: &Arc<Pane>, names: &[String]) -> Result<(), RpcError> {
        let events: Vec<_> = names
            .iter()
            .map(|name| {
                logical_key(name).ok_or_else(|| {
                    RpcError::new(
                        ErrorCode::ManifestError,
                        format!("the manifest names a bad key: {name}"),
                    )
                })
            })
            .collect::<Result<_, _>>()?;
        let guard = pane.lock_keystrokes().ok_or_else(busy)?;
        for (at, event) in events.iter().enumerate() {
            if at > 0 {
                tokio::time::sleep(KEY_GAP).await;
            }
            if guard.interrupted() {
                return Err(busy());
            }
            guard.send_event(event)?;
        }
        Ok(())
    }

    /// Pastes text and presses Enter. The guard is the right to type.
    async fn paste_and_send(pane: &Arc<Pane>, text: &str, submit: bool) -> Result<(), RpcError> {
        let guard = pane.lock_keystrokes().ok_or_else(busy)?;
        guard.paste(text)?;
        if submit {
            tokio::time::sleep(PASTE_GAP).await;
            if guard.interrupted() {
                return Err(busy());
            }
            let enter = logical_key("enter").ok_or_else(|| RpcError::internal("no enter key"))?;
            guard.send_event(&enter)?;
        }
        Ok(())
    }

    async fn typing_target(&self, reference: Ref<RunId>) -> Result<Typing, RpcError> {
        self.actor()?
            .update(move |parts| typing(&parts, &reference))
            .await
            .map_err(gone)?
    }

    pub(crate) async fn prompt(
        &self,
        reference: Ref<RunId>,
        text: String,
        submit: bool,
    ) -> Result<(), RpcError> {
        let target = self.typing_target(reference).await?;
        if target.waiting {
            return Err(RpcError::new(
                ErrorCode::Busy,
                "the agent waits for an answer; answer it first",
            ));
        }
        let pane = self.pane(target.pane)?;
        Self::paste_and_send(&pane, &text, submit).await
    }

    /// `agent.reply`: the next prompt for a run whose turn ended. The text goes as
    /// a bracketed paste with the `input_id` of the client, and the Enter after it. An id that went
    /// out lately writes nothing again. The call takes no geometry lease: no window sends it. The
    /// answer is the run, or `None` when the id went out lately and nothing was written.
    pub(crate) async fn reply(
        &self,
        reference: Ref<RunId>,
        input_id: u64,
        text: String,
    ) -> Result<Option<RunId>, RpcError> {
        if text.is_empty() || text.len() > REPLY_MAX {
            return Err(RpcError::invalid_params(format!(
                "a reply has 1 to {REPLY_MAX} bytes"
            )));
        }
        let (run, pane_id) = self
            .actor()?
            .update(move |parts| reply_target(&parts, &reference))
            .await
            .map_err(gone)??;
        if self.replied(pane_id, input_id) {
            return Ok(None);
        }
        let pane = self.pane(pane_id)?;
        let guard = pane.lock_keystrokes().ok_or_else(busy)?;
        guard.paste_as(input_id, &text)?;
        // The text is in the pane. From here, the same id never writes it again.
        self.remember_reply(pane_id, input_id);
        tokio::time::sleep(PASTE_GAP).await;
        if guard.interrupted() {
            return Err(busy());
        }
        let enter = logical_key("enter").ok_or_else(|| RpcError::internal("no enter key"))?;
        guard.send_event(&enter)?;
        drop(guard);
        self.actor()?
            .update_detached(move |mut parts| super::turn::seen(&mut parts, pane_id));
        Ok(Some(run))
    }

    /// Whether a reply with this id went to the pane lately.
    fn replied(&self, pane: PaneId, input_id: u64) -> bool {
        let mut replies = lock(&self.replies);
        let now = Instant::now();
        while replies
            .front()
            .is_some_and(|(_, _, at)| now.duration_since(*at) > REPLY_MEMORY)
        {
            replies.pop_front();
        }
        replies
            .iter()
            .any(|(p, id, _)| *p == pane && *id == input_id)
    }

    fn remember_reply(&self, pane: PaneId, input_id: u64) {
        let mut replies = lock(&self.replies);
        if replies.len() >= REPLIES_KEPT {
            replies.pop_front();
        }
        replies.push_back((pane, input_id, Instant::now()));
    }

    pub(crate) async fn interrupt(&self, reference: Ref<RunId>) -> Result<(), RpcError> {
        let target = self.typing_target(reference).await?;
        if target.interrupt_keys.is_empty() {
            return Err(RpcError::new(
                ErrorCode::Unsupported,
                "the manifest of this harness names no interrupt keys",
            ));
        }
        let pane = self.pane(target.pane)?;
        Self::press(&pane, &target.interrupt_keys).await
    }

    async fn alive(&self, run: RunId) -> bool {
        match self.actor() {
            Ok(actor) => actor
                .update(move |parts| {
                    parts
                        .state
                        .runs
                        .by_id
                        .get(&run)
                        .is_some_and(runs::Run::live)
                })
                .await
                .unwrap_or(false),
            Err(_) => false,
        }
    }

    pub(crate) async fn stop(
        self: &Arc<Self>,
        reference: Ref<RunId>,
        force: bool,
    ) -> Result<(), RpcError> {
        let target = self.typing_target(reference).await?;
        let pane = self.pane(target.pane)?;
        if force {
            pane.terminate();
        } else if target.quit_keys.is_empty() {
            return Err(RpcError::new(
                ErrorCode::Unsupported,
                "the manifest of this harness names no quit keys; stop it with force",
            ));
        } else {
            Self::press(&pane, &target.quit_keys).await?;
        }
        // The keys ask the program to end. If it is still there after the grace time, the core
        // ends it: `Term` first, and then `Kill` for a forced stop.
        let (this, run) = (Arc::clone(self), target.run);
        let grace = if force { FORCE_GRACE } else { QUIT_GRACE };
        self.handle.spawn(async move {
            tokio::time::sleep(grace).await;
            if this.alive(run).await {
                if force {
                    pane.kill();
                } else {
                    pane.terminate();
                }
            }
        });
        Ok(())
    }

    /// The user stops everything: each bound agent first gets the quit keys of its
    /// manifest, so that a harness that quits by itself saves its session. Waits at most `wait`
    /// for the agents to end. What stays is for the signals of the caller.
    pub(crate) async fn quit_agents(self: &Arc<Self>, wait: Duration) {
        let Ok(actor) = self.actor() else {
            return;
        };
        let live: Vec<RunId> = actor
            .update(|parts| {
                parts
                    .state
                    .runs
                    .by_id
                    .values()
                    .filter(|run| run.live() && run.pane.is_some() && authorizes(&run.binding))
                    .map(|run| run.id)
                    .collect()
            })
            .await
            .unwrap_or_default();
        if live.is_empty() {
            return;
        }
        let mut asked = tokio::task::JoinSet::new();
        for run in &live {
            let (this, run) = (Arc::clone(self), *run);
            asked.spawn(async move { this.stop(Ref::<RunId>::id(run), false).await });
        }
        while let Some(done) = asked.join_next().await {
            if let Ok(Err(error)) = done {
                tracing::debug!(%error, "an agent did not get the quit keys");
            }
        }
        let deadline = Instant::now() + wait;
        while Instant::now() < deadline {
            let mut any = false;
            for run in &live {
                any |= self.alive(*run).await;
            }
            if !any {
                return;
            }
            tokio::time::sleep(POLL).await;
        }
    }

    // --- launching -----------------------------------------------------------------------------

    /// `agent.start`: the job that opens a pane and starts the harness there.
    pub(crate) async fn start(self: &Arc<Self>, p: AgentStart) -> Result<JobId, RpcError> {
        let plan = self
            .actor()?
            .update(move |parts| plan_start(&parts, &p))
            .await
            .map_err(gone)??;
        self.ensure_harness(plan.harness).await?;
        self.launch(plan)
    }

    /// `agent.resume`: the job that starts the harness again with the handle of the session.
    pub(crate) async fn resume(self: &Arc<Self>, p: AgentResume) -> Result<JobId, RpcError> {
        let plan = self
            .actor()?
            .update(move |parts| plan_resume(&parts, &p))
            .await
            .map_err(gone)??;
        self.ensure_harness(plan.harness).await?;
        self.launch(plan)
    }

    /// A command of the palette starts an agent beside a pane. The palette has no way to show an
    /// error, so a call that fails gives a job that failed.
    pub(crate) fn start_beside(self: &Arc<Self>, harness: Harness, pane: PaneId) {
        let this = Arc::clone(self);
        let request = AgentStart {
            harness,
            placement: Placement::Split {
                pane: Ref::<PaneId>::id(pane),
                direction: SplitDirection::Right,
            },
            cwd: None,
            name: None,
            prompt: None,
            model: None,
            effort: None,
            args: Vec::new(),
            task: None,
        };
        self.handle.spawn(async move {
            if let Err(error) = this.start(request).await {
                this.failed_job(&format!("Start {}", harness_title(harness)), &error);
            }
        });
    }

    /// A command of the palette resumes the agents that the notice of the restart offered.
    pub(crate) async fn resume_offered(self: &Arc<Self>) -> Result<(), RpcError> {
        let offered = self
            .actor()?
            .update(|mut parts| super::reboot::take_offered(&mut parts))
            .await
            .map_err(gone)?;
        if offered.is_empty() {
            return Err(RpcError::new(
                ErrorCode::NotFound,
                "no agent waits to be resumed",
            ));
        }
        for run in offered {
            self.resume(AgentResume {
                agent: Ref::<RunId>::id(run),
                placement: None,
            })
            .await?;
        }
        Ok(())
    }

    /// A command of the palette resumes the agent that ran in a pane.
    pub(crate) fn resume_in(self: &Arc<Self>, pane: PaneId) {
        let this = Arc::clone(self);
        self.handle.spawn(async move {
            let found = match this.actor() {
                Ok(actor) => actor
                    .update(move |parts| {
                        parts
                            .state
                            .runs
                            .by_id
                            .values()
                            .filter(|run| run.pane == Some(pane) && !run.live())
                            .filter(|run| run.sessions.current().is_some())
                            .max_by_key(|run| run.started_at.0)
                            .map(|run| run.id)
                    })
                    .await
                    .ok()
                    .flatten(),
                Err(_) => None,
            };
            let result = match found {
                Some(id) => {
                    this.resume(AgentResume {
                        agent: Ref::<RunId>::id(id),
                        placement: None,
                    })
                    .await
                }
                None => Err(RpcError::new(
                    ErrorCode::NotFound,
                    "no agent that ended in this pane can be resumed",
                )),
            };
            if let Err(error) = result {
                this.failed_job("Resume an agent", &error);
            }
        });
    }

    /// Runs work for a command of the palette, and shows its error as a job that failed.
    pub(crate) fn background(
        self: &Arc<Self>,
        title: &'static str,
        work: impl std::future::Future<Output = Result<(), RpcError>> + Send + 'static,
    ) {
        let this = Arc::clone(self);
        self.handle.spawn(async move {
            if let Err(error) = work.await {
                this.failed_job(title, &error);
            }
        });
    }

    /// The user links the live run of a pane (`agent.link` without a harness).
    pub(crate) async fn link_pane(&self, pane: PaneId) -> Result<(), RpcError> {
        self.actor()?
            .update(move |mut parts| runs::link(&mut parts, pane, None, None).map(|_| ()))
            .await
            .map_err(gone)?
    }

    /// Shows an error of a command of the palette as a job that failed.
    fn failed_job(&self, title: &str, error: &RpcError) {
        if let Ok(job) = self.jobs.start(title) {
            job.fail(job_error(error));
        }
    }

    fn launch(self: &Arc<Self>, plan: Plan) -> Result<JobId, RpcError> {
        let job = self.jobs.start(plan.title.clone())?;
        let id = job.id();
        let this = Arc::clone(self);
        self.handle.spawn(async move {
            match this.run_launch(&job, plan).await {
                Ok(done) => job.done(done),
                Err(error) => job.fail(job_error(&error)),
            }
        });
        Ok(id)
    }

    /// Opens the pane, or takes the one that the call named.
    async fn place(&self, plan: &Plan) -> Result<PaneId, RpcError> {
        let actor = self.actor()?;
        let spec = json!({ "argv": [], "cwd": plan.cwd });
        let command = if plan.cwd.is_some() { Some(spec) } else { None };
        let created = |value: &Value, key: &str| -> Result<PaneId, RpcError> {
            value[key]
                .as_str()
                .and_then(|text| text.parse::<Ulid>().ok())
                .map(PaneId::new)
                .ok_or_else(|| RpcError::internal("the layout gave no pane"))
        };
        let call = |method: &str, params: Value| {
            Call::parse(method, params).map_err(|e| RpcError::invalid_params(e.to_string()))
        };
        match plan.place.clone() {
            Where::NewTab(workspace) => {
                let call = call(
                    "tab.create",
                    json!({ "workspace": workspace.to_string(), "command": command }),
                )?;
                created(&actor.call(call, Origin::ui()).await?, "pane")
            }
            Where::Split(pane, direction) => {
                let call = call(
                    "pane.split",
                    json!({
                        "pane": pane.to_string(),
                        "direction": direction,
                        "command": command,
                    }),
                )?;
                created(&actor.call(call, Origin::ui()).await?, "pane")
            }
            Where::Pane(pane) => {
                self.require_idle_shell(pane).await?;
                Ok(pane)
            }
            Where::Reuse(pane, workspace) => {
                if self.require_idle_shell(pane).await.is_ok() {
                    return Ok(pane);
                }
                let call = call(
                    "tab.create",
                    json!({ "workspace": workspace.to_string(), "command": command }),
                )?;
                created(&actor.call(call, Origin::ui()).await?, "pane")
            }
        }
    }

    /// The pane must be a shell that waits: nothing runs in it, and no agent is in it. A pane
    /// that the layout made a moment ago gets time to start its process.
    async fn require_idle_shell(&self, pane: PaneId) -> Result<(), RpcError> {
        let deadline = Instant::now() + PANE_WAIT;
        while self.pane(pane).is_err() && Instant::now() < deadline {
            tokio::time::sleep(POLL).await;
        }
        let object = self.pane(pane)?;
        if matches!(object.info().liveness, Liveness::Exited(_)) {
            return Err(RpcError::new(
                ErrorCode::PaneGone,
                format!("the program of pane {pane} has ended"),
            ));
        }
        let foreground = *object.foreground().borrow();
        if foreground.is_some_and(|pid| pid != object.child_pid()) {
            return Err(RpcError::new(
                ErrorCode::PaneBusy,
                format!("pane {pane} runs a program; the command needs an idle shell"),
            ));
        }
        let has_run = self
            .actor()?
            .update(move |parts| parts.state.runs.of_pane(pane).is_some())
            .await
            .map_err(gone)?;
        if has_run {
            return Err(RpcError::new(
                ErrorCode::PaneBusy,
                format!("an agent runs in pane {pane}"),
            ));
        }
        Ok(())
    }

    /// Waits for the pane object that the pane service makes after the layout commit.
    async fn pane_started(&self, pane: PaneId, job: &JobHandle) -> Result<Arc<Pane>, RpcError> {
        let deadline = Instant::now() + PANE_WAIT;
        loop {
            if let Ok(object) = self.pane(pane) {
                return Ok(object);
            }
            if job.is_cancelled() {
                return Err(RpcError::new(ErrorCode::Cancelled, "the job was cancelled"));
            }
            if Instant::now() >= deadline {
                return Err(RpcError::new(
                    ErrorCode::Timeout,
                    "the new pane did not start in time",
                ));
            }
            tokio::time::sleep(POLL).await;
        }
    }

    /// Waits until the shell has drawn its first screen, so that it reads what is typed.
    async fn shell_ready(pane: &Arc<Pane>) {
        let deadline = Instant::now() + SHELL_WAIT;
        while Instant::now() < deadline {
            if pane.text().iter().any(|line| !line.trim().is_empty()) {
                return;
            }
            tokio::time::sleep(POLL).await;
        }
    }

    async fn run_launch(&self, job: &JobHandle, plan: Plan) -> Result<Value, RpcError> {
        let actor = self.actor()?;
        job.progress(0.1, "Opening a pane");
        let pane_id = self.place(&plan).await?;
        let pane = self.pane_started(pane_id, job).await?;
        // The run that appears in the pane is the one that the core launched.
        let expected = Expected {
            harness: plan.harness,
            name: plan.name.clone(),
            task: plan.task,
            session: plan.session.clone(),
            model: plan.model.clone(),
        };
        actor
            .update(move |mut parts| runs::expect(&mut parts, pane_id, expected))
            .await
            .map_err(gone)?;
        let fresh = matches!(
            plan.place,
            Where::NewTab(_) | Where::Split(..) | Where::Reuse(..)
        );
        if fresh {
            Self::shell_ready(&pane).await;
        }
        job.progress(0.3, format!("Starting {}", harness_title(plan.harness)));
        let mut line = shell_line(&plan.argv);
        if matches!(plan.place, Where::Pane(_))
            && let Some(cwd) = &plan.cwd
        {
            line = format!("cd {} && {line}", quote(cwd));
        }
        Self::paste_and_send(&pane, &line, true).await?;
        let timeout = plan
            .ready
            .as_ref()
            .map_or(Duration::from_secs(30), |ready| ready.timeout);
        let run = self
            .wait_for_run(job, pane_id, plan.harness, timeout)
            .await?;
        if let Some(prompt) = plan.prompt.as_deref() {
            job.progress(0.7, "Waiting for the prompt");
            if let Some(ready) = &plan.ready {
                self.wait_for_screen(job, &pane, ready).await?;
            }
            job.progress(0.9, "Sending the prompt");
            Self::paste_and_send(&pane, prompt, true).await?;
        }
        Ok(json!({ "run": run.to_string(), "pane": pane_id.to_string() }))
    }

    async fn wait_for_run(
        &self,
        job: &JobHandle,
        pane: PaneId,
        harness: Harness,
        timeout: Duration,
    ) -> Result<RunId, RpcError> {
        let actor = self.actor()?;
        let deadline = Instant::now() + timeout;
        loop {
            let found = actor
                .update(move |parts| {
                    parts
                        .state
                        .runs
                        .of_pane(pane)
                        .filter(|run| run.harness == harness)
                        .map(|run| run.id)
                })
                .await
                .map_err(gone)?;
            if let Some(id) = found {
                return Ok(id);
            }
            if job.is_cancelled() {
                return Err(RpcError::new(ErrorCode::Cancelled, "the job was cancelled"));
            }
            if Instant::now() >= deadline {
                return Err(RpcError::new(
                    ErrorCode::Timeout,
                    format!("{} did not start in the pane", harness_title(harness)),
                ));
            }
            tokio::time::sleep(POLL).await;
        }
    }

    /// Waits until the rule of the screen manifest says that the input box is there.
    async fn wait_for_screen(
        &self,
        job: &JobHandle,
        pane: &Arc<Pane>,
        ready: &Ready,
    ) -> Result<(), RpcError> {
        let Some(compiled) = screen::builtin(&ready.screen) else {
            return Ok(());
        };
        let mut detector = Detector::new(Arc::new(compiled));
        let deadline = Instant::now() + ready.timeout;
        loop {
            let (_, rows) = pane.size();
            let seen = pane.screen_rows(detector.rows_needed(rows));
            if detector
                .evaluate(&seen, Instant::now())
                .rule(&ready.rule)
                .is_some()
            {
                return Ok(());
            }
            if job.is_cancelled() {
                return Err(RpcError::new(ErrorCode::Cancelled, "the job was cancelled"));
            }
            if Instant::now() >= deadline {
                return Err(RpcError::new(
                    ErrorCode::Timeout,
                    "the harness did not show its prompt in time",
                ));
            }
            tokio::time::sleep(POLL).await;
        }
    }
}
