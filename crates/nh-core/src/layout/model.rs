//! The layout in memory: groups, workspaces with their tabs, and the panes.
//!
//! [`Layout`] is plain data. The commands in [`super::ops`] change it, and they are pure: they take
//! the numbers and ids they need from an [`Alloc`], and they report the events they cause. The
//! handlers of the state actor run a command on a **copy** of the layout, write the difference to
//! the database in one transaction ([`super::persist`]), and replace the layout after the commit.
//! So a failed command changes nothing.

use std::collections::BTreeMap;

use nh_proto::api::{
    common::{Bounds, ProcessSpec},
    ids::{DeviceId, GroupId, PaneId, Ref, RefValue, RunId, TabId, WindowId, WorkspaceId},
    model::{
        ClipboardDropped, ClipboardRequest, FloatingPane, GroupRef, LayoutNode, MouseMode,
        PaneLiveness, PaneValue, Reveal, TabValue, WindowValue, WorkspaceValue,
    },
};
use nh_store::{LayoutRows, LoadedPane};
use serde_json::Value;
use thiserror::Error;
use ulid::Ulid;

use super::tree::{self, TreeError};
use crate::{ErrorCode, error::RpcError};

/// The label of the local machine.
pub const LOCAL: &str = "local";

/// The size that a pane has before the first window measures it.
pub const DEFAULT_SIZE: (u16, u16) = (80, 24);

#[derive(Clone, Debug, PartialEq)]
pub struct Group {
    pub id: GroupId,
    pub name: String,
    pub parent: Option<GroupId>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Tab {
    pub id: TabId,
    pub handle: String,
    pub name: String,
    pub layout: LayoutNode,
    pub floating: Vec<FloatingPane>,
    pub zoomed: Option<PaneId>,
    /// The tab bar does not show the tab, and no window selects it until something selects it
    /// on purpose. It holds the agent of a workflow node.
    pub hidden: bool,
}

impl Tab {
    /// Every pane of the tab that is in the tree or floating. A popped-out pane is in neither.
    pub fn shown_panes(&self) -> Vec<PaneId> {
        let mut panes = tree::panes(&self.layout);
        panes.extend(self.floating.iter().map(|f| f.pane));
        panes
    }

    pub fn shows(&self, pane: PaneId) -> bool {
        tree::contains(&self.layout, pane) || self.floating.iter().any(|f| f.pane == pane)
    }

    pub fn value(&self) -> TabValue {
        TabValue {
            id: self.id,
            handle: self.handle.clone(),
            name: self.name.clone(),
            layout: self.layout.clone(),
            floating: self.floating.clone(),
            zoomed: self.zoomed,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Workspace {
    pub id: WorkspaceId,
    pub handle: String,
    /// The `3` of `w3`. Tab and pane handles start with it.
    pub number: u32,
    pub name: String,
    pub root: Option<String>,
    pub machine: String,
    pub group: Option<GroupId>,
    pub tabs: Vec<Tab>,
    pub selected_tab: Option<TabId>,
}

impl Workspace {
    /// The tab to select when the tab at `at` goes: the visible tab before that place, else the
    /// first visible tab after it.
    pub fn visible_near(&self, at: usize) -> Option<TabId> {
        let before = self.tabs[..at.min(self.tabs.len())]
            .iter()
            .rev()
            .find(|tab| !tab.hidden);
        before
            .or_else(|| {
                self.tabs[at.min(self.tabs.len())..]
                    .iter()
                    .find(|tab| !tab.hidden)
            })
            .map(|tab| tab.id)
    }
}

/// How far the process of a pane is. The pane runtime reports it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Runtime {
    /// The layout made the pane, and no holder runs yet.
    Created,
    Running,
    Exited(Option<i32>),
    Lost,
}

impl Runtime {
    fn parse(liveness: &str, exit_code: Option<i32>) -> Self {
        match liveness {
            "running" => Self::Running,
            "exited" => Self::Exited(exit_code),
            "lost" => Self::Lost,
            _ => Self::Created,
        }
    }

    /// Whether closing the pane stops a program that runs.
    pub const fn is_live(self) -> bool {
        matches!(self, Self::Running)
    }
}

/// What the pane runtime reports about a pane. The `panes` projection shows it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Facts {
    /// The title that the program set.
    pub title: Option<String>,
    /// The pane shows the restored screen, and the holder has not caught up.
    pub restoring: bool,
    /// The newest inputs that the holder did not confirm (`pane.input_unconfirmed`).
    pub input_unconfirmed: Vec<u64>,
    /// An OSC 52 write that waits for an answer.
    pub clipboard_request: Option<ClipboardRequest>,
    /// The last OSC 52 write that no client got.
    pub clipboard_dropped: Option<ClipboardDropped>,
    /// The window whose size the PTY follows (the geometry lease).
    pub geometry_owner: Option<WindowId>,
    /// The working directory of the foreground group of the pane, now.
    pub cwd: Option<String>,
}

/// A pane as the layout knows it: its place and its facts. The screen and the like belong to the
/// pane runtime.
#[derive(Clone, Debug, PartialEq)]
pub struct PaneEntry {
    pub id: PaneId,
    pub handle: String,
    pub render_id: u32,
    pub workspace: WorkspaceId,
    pub tab: TabId,
    pub cols: u16,
    pub rows: u16,
    pub spec: Option<ProcessSpec>,
    /// The working directory of the holder. `None` until a holder runs.
    pub cwd: Option<String>,
    pub runtime: Runtime,
    pub facts: Facts,
    /// The run of an agent in the pane. The agents of the core set it. The database does not
    /// keep it: a run has its own row.
    pub run: Option<RunId>,
}

/// What a pane task reports to the layout: the state of the child, the size and the facts.
#[derive(Clone, Debug, PartialEq)]
pub struct Report {
    pub runtime: Runtime,
    pub cols: u16,
    pub rows: u16,
    pub facts: Facts,
}

/// A window of the app. It shows a tab, or an explicit list of panes, or one popped-out pane.
#[derive(Clone, Debug, PartialEq)]
pub struct Window {
    pub id: WindowId,
    pub bounds: Option<Bounds>,
    /// The tab that the window shows.
    pub tab: Option<TabId>,
    /// The tab that the window showed last in each workspace.
    pub by_workspace: BTreeMap<WorkspaceId, TabId>,
    /// The panes that the window shows when it shows no tab (`window.open {panes}`,
    /// `window.set_visible_panes`).
    pub explicit: Option<Vec<PaneId>>,
    /// The pane of a window that shows one popped-out pane.
    pub popout: Option<PaneId>,
    /// The pane with keyboard focus.
    pub focused_pane: Option<PaneId>,
    /// What the host reports about the window. The database does not keep it.
    pub facts: WindowFacts,
    /// The last request to show an interaction in the window. The database does not keep it.
    pub reveal: Option<Reveal>,
    /// The device id of the client that owns the window. `None` for a row that no client has
    /// adopted yet: a row of a development build from before the windows had a client, or a window
    /// that the CLI opened when no client was there.
    pub client: Option<Ulid>,
    /// The single view of a phone: the window draws only its focused pane, or
    /// the first pane of its tab. The database does not keep it.
    pub single: bool,
}

/// The state of a window that only the host knows (`window.focus_report`).
#[derive(Clone, Debug, PartialEq)]
pub struct WindowFacts {
    /// The OS gives the window keyboard focus.
    pub focused: bool,
    /// The window can be seen: it is on screen and not minimized or covered.
    pub visible: bool,
    pub minimized: bool,
    /// The refresh rate of the display of the window.
    pub refresh_hz: Option<f32>,
    /// The pane whose surface is the active element of the page. `None` when focus is in chrome.
    pub active_pane: Option<PaneId>,
}

impl Default for WindowFacts {
    fn default() -> Self {
        Self {
            focused: false,
            visible: true,
            minimized: false,
            refresh_hz: None,
            active_pane: None,
        }
    }
}

/// Why a command of the layout failed.
#[derive(Clone, Debug, PartialEq, Error)]
pub enum LayoutError {
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    Unsupported(String),
    #[error("the pane has a program that runs; close it with force to stop it")]
    Confirm {
        process: Option<String>,
        is_agent: bool,
    },
    #[error("{0}")]
    Internal(String),
}

impl From<TreeError> for LayoutError {
    fn from(error: TreeError) -> Self {
        match error {
            TreeError::NoSuchPane => Self::NotFound(error.to_string()),
            other => Self::Invalid(other.to_string()),
        }
    }
}

impl From<LayoutError> for RpcError {
    fn from(error: LayoutError) -> Self {
        match &error {
            LayoutError::NotFound(_) => Self::new(ErrorCode::NotFound, error.to_string()),
            LayoutError::Invalid(_) => Self::invalid_params(error.to_string()),
            LayoutError::Unsupported(_) => Self::new(ErrorCode::Unsupported, error.to_string()),
            LayoutError::Internal(_) => Self::internal(error.to_string()),
            LayoutError::Confirm { process, is_agent } => {
                let mut rpc = Self::new(ErrorCode::ConfirmRequired, error.to_string());
                rpc.details = serde_json::json!({ "process": process, "is_agent": is_agent });
                rpc
            }
        }
    }
}

/// Where a command takes the numbers and the ids that it needs. The handlers give counters of the
/// database. The tests give counters in memory.
pub trait Alloc {
    fn ulid(&mut self) -> Result<Ulid, LayoutError>;
    /// The next number of a handle counter. `scope` is `workspace`, `render`,
    /// `tab:<workspace id>` or `pane:<workspace id>`.
    fn number(&mut self, scope: &str) -> Result<u32, LayoutError>;
}

/// Where a pane of a tab is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Placement {
    /// In the tree of the tab.
    Tiled,
    /// Over the tree, with a rectangle.
    Floating,
    /// In a window of its own.
    PoppedOut(WindowId),
}

/// The layout of the session.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Layout {
    /// In the order of the sidebar.
    pub groups: Vec<Group>,
    /// In the order of the sidebar.
    pub workspaces: Vec<Workspace>,
    pub panes: BTreeMap<PaneId, PaneEntry>,
    /// In the order in which the windows were opened.
    pub windows: Vec<Window>,
}

/// The text of a handle without the label of the local machine.
fn local_handle_text(r: &nh_proto::api::ids::Handle) -> Result<String, LayoutError> {
    match r.machine.as_deref() {
        None | Some(LOCAL) => Ok(nh_proto::api::ids::Handle {
            machine: None,
            target: r.target,
        }
        .to_string()),
        Some(other) => Err(LayoutError::NotFound(format!(
            "no machine {other:?}: only the local machine exists in this version"
        ))),
    }
}

impl Layout {
    // --- lookups ---------------------------------------------------------------------------

    pub fn workspace_index(&self, id: WorkspaceId) -> Option<usize> {
        self.workspaces.iter().position(|w| w.id == id)
    }

    pub fn workspace(&self, id: WorkspaceId) -> Option<&Workspace> {
        self.workspaces.iter().find(|w| w.id == id)
    }

    /// The workspace index and the tab index of a tab.
    pub fn tab_position(&self, id: TabId) -> Option<(usize, usize)> {
        self.workspaces
            .iter()
            .enumerate()
            .find_map(|(w, ws)| ws.tabs.iter().position(|t| t.id == id).map(|t| (w, t)))
    }

    pub fn group_index(&self, id: GroupId) -> Option<usize> {
        self.groups.iter().position(|g| g.id == id)
    }

    /// The workspace, by id or by handle.
    pub fn resolve_workspace(&self, r: &Ref<WorkspaceId>) -> Result<usize, LayoutError> {
        match &r.0 {
            RefValue::Id(id) => self.workspace_index(*id),
            RefValue::Handle(handle) => {
                let text = local_handle_text(handle)?;
                self.workspaces.iter().position(|w| w.handle == text)
            }
            RefValue::Name(_) => None,
        }
        .ok_or_else(|| LayoutError::NotFound(format!("no workspace {r}")))
    }

    /// The workspace index and the tab index, by id or by handle.
    pub fn resolve_tab(&self, r: &Ref<TabId>) -> Result<(usize, usize), LayoutError> {
        match &r.0 {
            RefValue::Id(id) => self.tab_position(*id),
            RefValue::Handle(handle) => {
                let text = local_handle_text(handle)?;
                self.workspaces.iter().enumerate().find_map(|(w, ws)| {
                    ws.tabs
                        .iter()
                        .position(|t| t.handle == text)
                        .map(|t| (w, t))
                })
            }
            RefValue::Name(_) => None,
        }
        .ok_or_else(|| LayoutError::NotFound(format!("no tab {r}")))
    }

    pub fn resolve_group(&self, r: &Ref<GroupId>) -> Result<usize, LayoutError> {
        match &r.0 {
            RefValue::Id(id) => self.group_index(*id),
            RefValue::Handle(_) | RefValue::Name(_) => None,
        }
        .ok_or_else(|| LayoutError::NotFound(format!("no group {r}")))
    }

    pub fn resolve_pane(&self, r: &Ref<PaneId>) -> Result<PaneId, LayoutError> {
        match &r.0 {
            RefValue::Id(id) => self.panes.contains_key(id).then_some(*id),
            RefValue::Handle(handle) => {
                let text = local_handle_text(handle)?;
                self.panes.values().find(|p| p.handle == text).map(|p| p.id)
            }
            RefValue::Name(_) => None,
        }
        .ok_or_else(|| LayoutError::NotFound(format!("no pane {r}")))
    }

    /// The panes of a tab, tiled, floating and popped out.
    pub fn panes_of_tab(&self, tab: TabId) -> Vec<&PaneEntry> {
        self.panes.values().filter(|p| p.tab == tab).collect()
    }

    pub fn panes_of_workspace(&self, workspace: WorkspaceId) -> Vec<&PaneEntry> {
        self.panes
            .values()
            .filter(|p| p.workspace == workspace)
            .collect()
    }

    pub fn window_index(&self, id: WindowId) -> Option<usize> {
        self.windows.iter().position(|w| w.id == id)
    }

    /// Where a pane is: in the tree of its tab, floating over it, or in a window of its own.
    pub fn placement(&self, pane: PaneId) -> Option<Placement> {
        let entry = self.panes.get(&pane)?;
        let (w, t) = self.tab_position(entry.tab)?;
        let tab = &self.workspaces[w].tabs[t];
        if tree::contains(&tab.layout, pane) {
            Some(Placement::Tiled)
        } else if tab.floating.iter().any(|f| f.pane == pane) {
            Some(Placement::Floating)
        } else {
            self.windows
                .iter()
                .find(|win| win.popout == Some(pane))
                .map(|win| Placement::PoppedOut(win.id))
        }
    }

    // --- views -----------------------------------------------------------------------------

    /// The panes that a window draws. The render pacer keys on this (`windows`).
    ///
    /// A popout window draws its pane. A window that shows a tab draws the panes of the tab, the
    /// tiled ones and the floating ones, or only the zoomed pane. A window with `single` draws only
    /// its focused pane, or the first pane of its tab when no pane of the tab has the focus.
    /// A window with a list of panes draws those that exist.
    pub fn visible_panes(&self, window: &Window) -> Vec<PaneId> {
        if let Some(pane) = window.popout {
            return vec![pane];
        }
        if let Some(tab) = window.tab
            && let Some((w, t)) = self.tab_position(tab)
        {
            let tab = &self.workspaces[w].tabs[t];
            if window.single {
                let shown = tab.shown_panes();
                let pane = window
                    .focused_pane
                    .filter(|pane| shown.contains(pane))
                    .or_else(|| shown.first().copied());
                return pane.into_iter().collect();
            }
            return match tab.zoomed {
                Some(zoomed) => vec![zoomed],
                None => tab.shown_panes(),
            };
        }
        window
            .explicit
            .iter()
            .flatten()
            .copied()
            .filter(|p| self.panes.contains_key(p))
            .collect()
    }

    /// The panes that can get the focus of a window: the panes that it draws, and in a window with
    /// `single` every pane of its tab, because the focus chooses the pane that it draws.
    pub fn focusable_panes(&self, window: &Window) -> Vec<PaneId> {
        if window.single
            && window.popout.is_none()
            && let Some(tab) = window.tab
            && let Some((w, t)) = self.tab_position(tab)
        {
            return self.workspaces[w].tabs[t].shown_panes();
        }
        self.visible_panes(window)
    }

    /// Sets the single view of a window. A window that is gone changes nothing.
    pub fn set_window_single(&mut self, id: WindowId, single: bool) {
        if let Some(index) = self.window_index(id) {
            self.windows[index].single = single;
        }
    }

    /// Gives a window to a client. A window that is gone changes nothing.
    pub fn set_window_client(&mut self, id: WindowId, client: Option<Ulid>) {
        if let Some(index) = self.window_index(id) {
            self.windows[index].client = client;
        }
    }

    /// The row of the `windows` projection.
    pub fn window_value(&self, window: &Window) -> WindowValue {
        WindowValue {
            id: window.id,
            bounds: window.bounds,
            visible_panes: self.visible_panes(window),
            focused_pane: window.focused_pane,
            tab: window.tab,
            workspace: window
                .tab
                .and_then(|tab| self.tab_position(tab))
                .map(|(w, _)| self.workspaces[w].id),
            refresh_hz: window.facts.refresh_hz,
            focused: window.facts.focused,
            visible: window.facts.visible,
            reveal: window.reveal,
            client: window.client.map(DeviceId::new),
            single: window.single,
        }
    }

    /// The row of the `panes` projection that the layout knows. The title, the screen and the
    /// like come from the pane runtime.
    /// Whether a tab exists and is hidden.
    pub fn tab_hidden(&self, tab: TabId) -> bool {
        self.workspaces
            .iter()
            .flat_map(|workspace| &workspace.tabs)
            .any(|t| t.id == tab && t.hidden)
    }

    /// The tab of a pane, and its workspace.
    pub fn tab_of_pane(&self, pane: PaneId) -> Option<(WorkspaceId, TabId)> {
        let entry = self.panes.get(&pane)?;
        Some((entry.workspace, entry.tab))
    }

    pub fn pane_value(&self, pane: &PaneEntry) -> PaneValue {
        PaneValue {
            id: pane.id,
            handle: pane.handle.clone(),
            render_id: pane.render_id,
            workspace: pane.workspace,
            tab: pane.tab,
            title: pane.facts.title.clone().unwrap_or_default(),
            // The directory where the pane is now, else the one where its holder started.
            cwd: pane.facts.cwd.clone().or_else(|| pane.cwd.clone()),
            fg_process: None,
            cols: pane.cols,
            rows: pane.rows,
            liveness: match pane.runtime {
                Runtime::Created => PaneLiveness::Starting,
                Runtime::Running => PaneLiveness::Alive,
                Runtime::Exited(code) => PaneLiveness::Exited { code },
                Runtime::Lost => PaneLiveness::Exited { code: None },
            },
            restoring: pane.facts.restoring,
            alt_screen: false,
            mouse_mode: MouseMode::None,
            bracketed_paste: false,
            input_unconfirmed: pane.facts.input_unconfirmed.clone(),
            geometry_owner: pane.facts.geometry_owner,
            clipboard_request: pane.facts.clipboard_request.clone(),
            clipboard_dropped: pane.facts.clipboard_dropped,
            run: pane.run,
        }
    }

    pub fn group_ref(&self, id: GroupId) -> Option<GroupRef> {
        self.groups.iter().find(|g| g.id == id).map(|g| GroupRef {
            id: g.id,
            name: g.name.clone(),
        })
    }

    /// The row of the `tree` projection for a workspace.
    pub fn workspace_value(&self, workspace: &Workspace) -> WorkspaceValue {
        WorkspaceValue {
            id: workspace.id,
            handle: workspace.handle.clone(),
            name: workspace.name.clone(),
            root: workspace.root.clone(),
            machine: workspace.machine.clone(),
            group: workspace.group.and_then(|g| self.group_ref(g)),
            tabs: workspace
                .tabs
                .iter()
                .filter(|tab| !tab.hidden)
                .map(Tab::value)
                .collect(),
            selected_tab: workspace.selected_tab,
        }
    }

    // --- checks ----------------------------------------------------------------------------

    /// Every rule that the commands keep. The tests call it after each command.
    pub fn validate(&self) -> Result<(), String> {
        let mut handles = std::collections::HashSet::new();
        for workspace in &self.workspaces {
            if !handles.insert(workspace.handle.clone()) {
                return Err(format!("handle {} twice", workspace.handle));
            }
            if let Some(group) = workspace.group
                && self.group_index(group).is_none()
            {
                return Err(format!(
                    "workspace {} is in an unknown group",
                    workspace.handle
                ));
            }
            if let Some(selected) = workspace.selected_tab
                && !workspace.tabs.iter().any(|t| t.id == selected)
            {
                return Err(format!(
                    "workspace {} selects a tab it has not",
                    workspace.handle
                ));
            }
            if workspace.selected_tab.is_none() && workspace.tabs.iter().any(|t| !t.hidden) {
                return Err(format!(
                    "workspace {} has tabs and selects none",
                    workspace.handle
                ));
            }
            if let Some(selected) = workspace.selected_tab
                && workspace.tabs.iter().any(|t| t.id == selected && t.hidden)
            {
                return Err(format!(
                    "workspace {} selects a hidden tab",
                    workspace.handle
                ));
            }
            for tab in &workspace.tabs {
                if !handles.insert(tab.handle.clone()) {
                    return Err(format!("handle {} twice", tab.handle));
                }
                tree::validate(&tab.layout).map_err(|e| format!("tab {}: {e}", tab.handle))?;
                let shown = tab.shown_panes();
                let unique: std::collections::HashSet<_> = shown.iter().collect();
                if unique.len() != shown.len() {
                    return Err(format!("tab {} shows a pane twice", tab.handle));
                }
                for pane in &shown {
                    match self.panes.get(pane) {
                        Some(p) if p.tab == tab.id && p.workspace == workspace.id => {}
                        _ => {
                            return Err(format!(
                                "tab {} shows a pane that is not its own",
                                tab.handle
                            ));
                        }
                    }
                }
                if let Some(zoomed) = tab.zoomed
                    && !tab.shows(zoomed)
                {
                    return Err(format!("tab {} zooms a pane it does not show", tab.handle));
                }
            }
        }
        for pane in self.panes.values() {
            if !handles.insert(pane.handle.clone()) {
                return Err(format!("handle {} twice", pane.handle));
            }
            let Some((w, t)) = self.tab_position(pane.tab) else {
                return Err(format!("pane {} has no tab", pane.handle));
            };
            if self.workspaces[w].id != pane.workspace {
                return Err(format!("pane {} is in the wrong workspace", pane.handle));
            }
            let _ = t;
            // A pane is in the tree, floating, or in a window of its own, and in one place only.
            if self.placement(pane.id).is_none() {
                return Err(format!("pane {} is nowhere", pane.handle));
            }
        }
        let mut popped = std::collections::HashSet::new();
        for window in &self.windows {
            if let Some(tab) = window.tab
                && self.tab_position(tab).is_none()
            {
                return Err("a window shows a tab that is gone".to_owned());
            }
            for (workspace, tab) in &window.by_workspace {
                match self.tab_position(*tab) {
                    Some((w, _)) if self.workspaces[w].id == *workspace => {}
                    _ => {
                        return Err(
                            "a window remembers a tab that is not in its workspace".to_owned()
                        );
                    }
                }
            }
            for pane in window.explicit.iter().flatten() {
                if !self.panes.contains_key(pane) {
                    return Err("a window shows a pane that is gone".to_owned());
                }
            }
            if let Some(focused) = window.focused_pane
                && !self.panes.contains_key(&focused)
            {
                return Err("a window focuses a pane that is gone".to_owned());
            }
            if let Some(pane) = window.popout {
                if self.placement(pane) != Some(Placement::PoppedOut(window.id)) {
                    return Err("a popout window has a pane that is somewhere else".to_owned());
                }
                if window.tab.is_some() || window.explicit.is_some() {
                    return Err("a popout window shows more than its pane".to_owned());
                }
                if !popped.insert(pane) {
                    return Err("a pane is popped out twice".to_owned());
                }
            }
        }
        let render: std::collections::HashSet<u32> =
            self.panes.values().map(|p| p.render_id).collect();
        if render.len() != self.panes.len() {
            return Err("a render id twice".to_owned());
        }
        Ok(())
    }

    // --- loading ---------------------------------------------------------------------------

    /// Builds the layout from the rows that the store read.
    pub fn from_rows(rows: LayoutRows) -> Result<Self, LayoutError> {
        let internal = |what: &str, error: &dyn std::fmt::Display| {
            LayoutError::Internal(format!("{what}: {error}"))
        };
        let mut layout = Self {
            groups: rows
                .groups
                .iter()
                .map(|g| Group {
                    id: GroupId::new(g.id),
                    name: g.name.clone(),
                    parent: g.parent.map(GroupId::new),
                })
                .collect(),
            ..Self::default()
        };
        for row in &rows.workspaces {
            let number = row
                .handle
                .strip_prefix('w')
                .and_then(|n| n.parse().ok())
                .ok_or_else(|| {
                    LayoutError::Internal(format!("handle {:?} of a workspace", row.handle))
                })?;
            layout.workspaces.push(Workspace {
                id: WorkspaceId::new(row.id),
                handle: row.handle.clone(),
                number,
                name: row.name.clone(),
                root: row.root.clone(),
                machine: row.machine.clone(),
                group: row.group.map(GroupId::new),
                tabs: Vec::new(),
                selected_tab: row.selected_tab.map(TabId::new),
            });
        }
        for row in &rows.tabs {
            let Some(workspace) = layout.workspace_index(WorkspaceId::new(row.workspace)) else {
                continue;
            };
            let tab = Tab {
                id: TabId::new(row.id),
                handle: row.handle.clone(),
                name: row.name.clone(),
                layout: serde_json::from_value(row.layout.clone())
                    .map_err(|e| internal("the layout of a tab", &e))?,
                floating: serde_json::from_value(row.floating.clone())
                    .map_err(|e| internal("the floating panes of a tab", &e))?,
                zoomed: row.zoomed.map(PaneId::new),
                hidden: row.hidden,
            };
            layout.workspaces[workspace].tabs.push(tab);
        }
        for LoadedPane {
            slot,
            liveness,
            cwd,
            exit_code,
        } in rows.panes
        {
            let id = PaneId::new(slot.id);
            let spec: Option<ProcessSpec> = slot
                .spec
                .map(|v: Value| serde_json::from_value(v))
                .transpose()
                .map_err(|e| internal("what a pane runs", &e))?;
            layout.panes.insert(
                id,
                PaneEntry {
                    id,
                    handle: slot.handle,
                    render_id: slot.render_id,
                    workspace: WorkspaceId::new(slot.workspace),
                    tab: TabId::new(slot.tab),
                    cols: slot.cols,
                    rows: slot.rows,
                    spec,
                    cwd,
                    runtime: Runtime::parse(&liveness, exit_code),
                    facts: Facts::default(),
                    run: None,
                },
            );
        }
        for row in &rows.windows {
            let state = &row.tab_by_workspace;
            let tab = |value: &Value| -> Option<TabId> { value.as_str()?.parse().ok() };
            let by_workspace = state
                .get("by_workspace")
                .and_then(Value::as_object)
                .map(|map| {
                    map.iter()
                        .filter_map(|(w, t)| Some((w.parse().ok()?, tab(t)?)))
                        .collect()
                })
                .unwrap_or_default();
            let explicit = state.get("explicit").and_then(Value::as_array).map(|list| {
                list.iter()
                    .filter_map(|p| p.as_str()?.parse().ok())
                    .collect()
            });
            layout.windows.push(Window {
                id: WindowId::new(row.id),
                bounds: row
                    .bounds
                    .clone()
                    .map(serde_json::from_value)
                    .transpose()
                    .map_err(|e| internal("the bounds of a window", &e))?,
                tab: state.get("tab").and_then(tab),
                by_workspace,
                explicit,
                popout: row.popout_pane.map(PaneId::new),
                focused_pane: row.focused_pane.map(PaneId::new),
                facts: WindowFacts::default(),
                reveal: None,
                client: row.client,
                single: false,
            });
        }
        // A workspace with tabs selects one. The row may say none (a tab that was closed).
        for workspace in &mut layout.workspaces {
            let valid = workspace
                .selected_tab
                .is_some_and(|s| workspace.tabs.iter().any(|t| t.id == s && !t.hidden));
            if !valid {
                workspace.selected_tab = workspace.visible_near(0);
            }
        }
        layout
            .validate()
            .map_err(|e| LayoutError::Internal(format!("the stored layout is wrong: {e}")))?;
        Ok(layout)
    }
}

#[cfg(test)]
mod tests;
