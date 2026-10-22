//! What the layout asks of the pane runtime.
//!
//! The layout decides which panes exist. The pane runtime starts and ends the holders. After each
//! command the state actor compares the layout before and after, and tells the runtime what to
//! do through a [`RuntimeLink`]: start a holder for a pane that is new (or that restarts), and end
//! the holder of a pane that is gone. The runtime reports back with an update job on the actor
//! (`Report` in [`super::model`]).

use std::{
    collections::HashMap,
    sync::{Arc, Mutex, PoisonError},
};

use nh_proto::api::ids::{PaneId, TabId, WorkspaceId};
use tokio::sync::mpsc;
use ulid::Ulid;

use super::model::{Layout, PaneEntry, Runtime};

/// Everything the runtime needs to start the holder of a pane.
#[derive(Clone, Debug, PartialEq)]
pub struct Launch {
    pub pane: PaneEntry,
    pub workspace: WorkspaceId,
    /// The `3` of `w3`.
    pub workspace_number: u32,
    pub workspace_handle: String,
    /// The root path of the workspace: the working directory of a pane that gives none.
    pub root: Option<String>,
    pub tab: TabId,
    pub tab_handle: String,
}

/// A request of the layout to the pane runtime.
#[derive(Clone, Debug, PartialEq)]
pub enum RuntimeCmd {
    /// Start the holder of a pane in the state `created`.
    Spawn(Box<Launch>),
    /// The pane is gone from the layout: end its holder and forget it.
    Close(PaneId),
}

/// The panes whose holder was asked for and has not started yet. The value says that the layout
/// closed the pane meanwhile.
///
/// The link fills it when it sends a command, so a pane is in it from the commit of the layout
/// until its holder runs. A caller that wants no holder in the middle of its start waits until it
/// is empty.
#[derive(Default)]
pub struct Starting(Mutex<HashMap<Ulid, bool>>);

impl Starting {
    fn map(&self) -> std::sync::MutexGuard<'_, HashMap<Ulid, bool>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The start ended. Returns whether the layout closed the pane while it started.
    pub fn finish(&self, pane: Ulid) -> bool {
        self.map().remove(&pane).unwrap_or(false)
    }

    /// Whether the holder of the pane starts now.
    pub fn contains(&self, pane: Ulid) -> bool {
        self.map().contains_key(&pane)
    }

    pub fn is_empty(&self) -> bool {
        self.map().is_empty()
    }
}

/// How the state actor reaches the pane runtime.
#[derive(Clone)]
pub struct RuntimeLink {
    tx: mpsc::UnboundedSender<RuntimeCmd>,
    starting: Arc<Starting>,
}

impl RuntimeLink {
    /// A link and the end that the pane service reads.
    pub fn channel() -> (Self, mpsc::UnboundedReceiver<RuntimeCmd>) {
        let (tx, rx) = mpsc::unbounded_channel();
        let link = Self {
            tx,
            starting: Arc::default(),
        };
        (link, rx)
    }

    /// The panes that start.
    pub fn starting(&self) -> Arc<Starting> {
        Arc::clone(&self.starting)
    }

    pub fn send(&self, command: RuntimeCmd) {
        match &command {
            RuntimeCmd::Spawn(launch) => {
                self.starting.map().insert(launch.pane.id.ulid(), false);
            }
            RuntimeCmd::Close(pane) => {
                if let Some(closed) = self.starting.map().get_mut(&pane.ulid()) {
                    *closed = true;
                }
            }
        }
        // The runtime ends with the core. A command for a core that is ending is not needed.
        let _ = self.tx.send(command);
    }
}

impl Layout {
    /// The launch of a pane, with the facts about its workspace and tab.
    pub fn launch_of(&self, pane: &PaneEntry) -> Option<Launch> {
        let (w, t) = self.tab_position(pane.tab)?;
        let workspace = &self.workspaces[w];
        Some(Launch {
            pane: pane.clone(),
            workspace: workspace.id,
            workspace_number: workspace.number,
            workspace_handle: workspace.handle.clone(),
            root: workspace.root.clone(),
            tab: pane.tab,
            tab_handle: workspace.tabs[t].handle.clone(),
        })
    }
}

/// What the runtime must do after the layout changed from `old` to `new`.
///
/// A pane that is in the state `created` in `new` and was not before gets a holder. That covers a
/// new pane and a restart. A pane that is in `old` and not in `new` loses its holder.
pub fn commands(old: &Layout, new: &Layout) -> Vec<RuntimeCmd> {
    let mut out = Vec::new();
    for id in old.panes.keys().filter(|id| !new.panes.contains_key(id)) {
        out.push(RuntimeCmd::Close(*id));
    }
    for pane in new.panes.values() {
        let was_created = old
            .panes
            .get(&pane.id)
            .is_some_and(|before| before.runtime == Runtime::Created);
        if pane.runtime == Runtime::Created
            && !was_created
            && let Some(launch) = new.launch_of(pane)
        {
            out.push(RuntimeCmd::Spawn(Box::new(launch)));
        }
    }
    out
}

#[cfg(test)]
mod tests;
