//! The agents of each worktree. The core knows the live folder of each pane (`panes`, from the
//! pane watcher) and the run in it with its facets (`agents`). An agent is in the worktree whose
//! root holds the folder of its pane. A worktree inside another one, as Claude Code makes them
//! under `.claude/worktrees/`, holds its own agents.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use nh_proto::api::{
    ids::PaneId,
    model::{AgentValue, Execution, GitAgent, GitAgentState, Liveness, PaneValue},
    projections::Projection,
    time::Timestamp,
};

use crate::projection::ProjectionHub;

/// An agent with the folder of its pane, as the system names it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Placed {
    pub folder: PathBuf,
    pub agent: GitAgent,
}

/// The state of a run, as the Git view shows it.
pub(crate) fn state_of(agent: &AgentValue) -> GitAgentState {
    if matches!(agent.facets.liveness.value, Liveness::Exited { .. }) {
        return GitAgentState::Exited;
    }
    if !agent.facets.pending.is_empty() {
        return GitAgentState::Waiting;
    }
    if matches!(agent.facets.liveness.value, Liveness::Starting) {
        return GitAgentState::Starting;
    }
    match agent.facets.execution.value {
        Execution::Starting => GitAgentState::Starting,
        // An agent at its rate limit goes on when the limit resets.
        Execution::Working | Execution::RateLimited { .. } => GitAgentState::Working,
        // A turn that ended with an error leaves the agent idle.
        Execution::Idle | Execution::Error => GitAgentState::Idle,
        Execution::Exited => GitAgentState::Exited,
        Execution::Unknown => GitAgentState::Unknown,
    }
}

/// Whether a write in the worktree of the agent asks first: the agent starts, works, waits for
/// the person, or its state is not known.
pub(crate) fn busy(state: GitAgentState) -> bool {
    !matches!(state, GitAgentState::Idle | GitAgentState::Exited)
}

/// The folders that the core canonicalized, so that a pane that stays in its folder costs no
/// look at the disk each second.
#[derive(Default)]
pub(crate) struct Folders {
    known: HashMap<String, Option<PathBuf>>,
}

/// The most folders that [`Folders`] keeps. More start the map again.
const KEPT: usize = 1_000;

impl Folders {
    /// The folder as the system names it, or `None` for a folder that is gone.
    pub fn canonical(&mut self, folder: &str) -> Option<PathBuf> {
        if self.known.len() > KEPT {
            self.known.clear();
        }
        self.known
            .entry(folder.to_owned())
            .or_insert_with(|| std::fs::canonicalize(folder).ok())
            .clone()
    }
}

/// The folders of the panes, by pane id, as the `panes` projection has them.
pub(crate) fn pane_folders(hub: &ProjectionHub) -> HashMap<String, String> {
    hub.snapshot(Projection::Panes)
        .items
        .values()
        .filter_map(|value| serde_json::from_value::<PaneValue>(value.clone()).ok())
        .filter_map(|pane| Some((pane.id.to_string(), pane.cwd?)))
        .collect()
}

/// Every agent that has a pane with a folder, with that folder. A pane that had runs before keeps
/// their rows in `agents`, so its newest run stands for it.
pub(crate) fn placed(hub: &ProjectionHub, folders: &mut Folders) -> Vec<Placed> {
    let panes = pane_folders(hub);
    let snapshot = hub.snapshot(Projection::Agents);
    let mut newest: HashMap<PaneId, (Timestamp, Placed)> = HashMap::new();
    let mut order: Vec<PaneId> = Vec::new();
    for value in snapshot
        .order
        .iter()
        .filter_map(|key| snapshot.items.get(key))
    {
        let Ok(agent) = serde_json::from_value::<AgentValue>(value.clone()) else {
            continue;
        };
        let Some(pane) = agent.pane else {
            continue;
        };
        let Some(folder) = panes
            .get(&pane.to_string())
            .and_then(|f| folders.canonical(f))
        else {
            continue;
        };
        if newest
            .get(&pane)
            .is_some_and(|(at, _)| *at >= agent.started_at)
        {
            continue;
        }
        if !newest.contains_key(&pane) {
            order.push(pane);
        }
        let placed = Placed {
            folder,
            agent: GitAgent {
                run: agent.id,
                pane,
                harness: agent.harness,
                name: agent.name.clone(),
                state: state_of(&agent),
            },
        };
        newest.insert(pane, (agent.started_at, placed));
    }
    order
        .into_iter()
        .filter_map(|pane| newest.remove(&pane).map(|(_, placed)| placed))
        .collect()
}

/// The root of `roots` that holds `folder`: the longest one, so that a worktree inside another
/// one wins.
pub(crate) fn root_of<'a>(folder: &Path, roots: &'a [PathBuf]) -> Option<&'a PathBuf> {
    roots
        .iter()
        .filter(|root| folder.starts_with(root))
        .max_by_key(|root| root.components().count())
}

/// The agents in the worktree at `root`, among the worktrees `roots` of its repository.
pub(crate) fn agents_in(root: &Path, roots: &[PathBuf], placed: &[Placed]) -> Vec<GitAgent> {
    placed
        .iter()
        .filter(|placed| root_of(&placed.folder, roots).is_some_and(|found| found == root))
        .map(|placed| placed.agent.clone())
        .collect()
}
