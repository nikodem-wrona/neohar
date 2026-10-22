//! The commands on workspaces, groups, tabs and the layout of a tab.
//!
//! Each command changes a [`Layout`] and returns the events that it caused. A command that fails
//! may have changed the layout half way, so the caller runs it on a copy and keeps the copy only
//! when the command works.

use nh_proto::api::{
    calls::workspace::{
        GroupSummary, TabCreate, TabCreated, WorkspaceCreate, WorkspaceCreated, WorkspaceSummary,
    },
    common::ProcessSpec,
    events::{EventPayload, GroupEvent, LayoutChanged, PaneEvent, TabEvent, WorkspaceEvent},
    ids::{GroupId, PaneId, Ref, TabId, WindowId, WorkspaceId},
};

use super::{
    model::{
        Alloc, DEFAULT_SIZE, Facts, Group, LOCAL, Layout, LayoutError, PaneEntry, Runtime, Tab,
        Workspace,
    },
    tree,
};

/// An event that a command caused, with the id of its main object.
#[derive(Clone, Debug, PartialEq)]
pub struct NewEvent {
    pub subject: String,
    pub payload: EventPayload,
}

/// What a command gives back.
#[derive(Debug, PartialEq)]
pub struct Done<T> {
    pub value: T,
    pub events: Vec<NewEvent>,
}

type Outcome<T> = Result<Done<T>, LayoutError>;

fn done<T>(value: T, events: Vec<NewEvent>) -> Outcome<T> {
    Ok(Done { value, events })
}

/// The longest name of a workspace, a tab or a group, in characters.
pub const NAME_MAX: usize = 100;

/// A name without the space around it. It must not be empty or hold a control character.
pub fn clean_name(name: &str) -> Result<String, LayoutError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(LayoutError::Invalid("a name must not be empty".to_owned()));
    }
    if name.chars().count() > NAME_MAX {
        return Err(LayoutError::Invalid(format!(
            "a name has at most {NAME_MAX} characters"
        )));
    }
    if name.chars().any(char::is_control) {
        return Err(LayoutError::Invalid(
            "a name must not hold a control character".to_owned(),
        ));
    }
    Ok(name.to_owned())
}

fn workspace_event(workspace: &Workspace) -> WorkspaceEvent {
    WorkspaceEvent {
        workspace: workspace.id,
        handle: workspace.handle.clone(),
        name: Some(workspace.name.clone()),
    }
}

fn tab_event(workspace: &Workspace, tab: &Tab) -> TabEvent {
    TabEvent {
        tab: tab.id,
        handle: tab.handle.clone(),
        workspace: workspace.id,
        name: Some(tab.name.clone()),
    }
}

fn pane_event(pane: &PaneEntry) -> PaneEvent {
    PaneEvent {
        pane: pane.id,
        handle: pane.handle.clone(),
        tab: pane.tab,
        workspace: pane.workspace,
    }
}

fn event(subject: impl ToString, payload: EventPayload) -> NewEvent {
    NewEvent {
        subject: subject.to_string(),
        payload,
    }
}

/// What a new pane runs. Without a command the pane runs the login shell, in the root of the
/// workspace when it has one.
pub(super) fn effective_spec(spec: Option<ProcessSpec>, root: Option<&str>) -> Option<ProcessSpec> {
    match (spec, root) {
        (None, None) => None,
        (None, Some(root)) => Some(ProcessSpec {
            argv: Vec::new(),
            cwd: Some(root.to_owned()),
            env: std::collections::BTreeMap::new(),
        }),
        (Some(mut spec), Some(root)) => {
            if spec.cwd.is_none() {
                spec.cwd = Some(root.to_owned());
            }
            Some(spec)
        }
        (Some(spec), None) => Some(spec),
    }
}

impl Layout {
    // --- panes (the part that the commands of this file need) -----------------------------------

    /// Makes a pane in a tab: ids, handle, render id. The caller puts it in the tree.
    pub(super) fn make_pane(
        &mut self,
        workspace: WorkspaceId,
        tab: TabId,
        spec: Option<ProcessSpec>,
        alloc: &mut dyn Alloc,
    ) -> Result<PaneEntry, LayoutError> {
        let index = self
            .workspace_index(workspace)
            .ok_or_else(|| LayoutError::Internal("a pane for an unknown workspace".to_owned()))?;
        let number = self.workspaces[index].number;
        let spec = effective_spec(spec, self.workspaces[index].root.as_deref());
        let counter = alloc.number(&format!("pane:{workspace}"))?;
        let pane = PaneEntry {
            id: PaneId::new(alloc.ulid()?),
            handle: format!("w{number}:p{counter}"),
            render_id: alloc.number("render")?,
            workspace,
            tab,
            cols: DEFAULT_SIZE.0,
            rows: DEFAULT_SIZE.1,
            // The working directory of the holder, once one runs. The spec says where it starts.
            cwd: None,
            spec,
            runtime: Runtime::Created,
            facts: Facts::default(),
            run: None,
        };
        self.panes.insert(pane.id, pane.clone());
        Ok(pane)
    }

    /// Adds a tab with one pane to a workspace and selects it.
    fn add_tab(
        &mut self,
        workspace: usize,
        name: Option<&str>,
        spec: Option<ProcessSpec>,
        alloc: &mut dyn Alloc,
    ) -> Result<(TabId, String, PaneEntry), LayoutError> {
        let id = self.workspaces[workspace].id;
        let counter = alloc.number(&format!("tab:{id}"))?;
        let handle = format!("w{}:t{counter}", self.workspaces[workspace].number);
        let name = match name {
            Some(name) => clean_name(name)?,
            None => format!("Tab {counter}"),
        };
        let tab_id = TabId::new(alloc.ulid()?);
        let pane = self.make_pane(id, tab_id, spec, alloc)?;
        let tab = Tab {
            id: tab_id,
            handle: handle.clone(),
            name,
            layout: tree::leaf(pane.id),
            floating: Vec::new(),
            zoomed: None,
        };
        let workspace = &mut self.workspaces[workspace];
        workspace.tabs.push(tab);
        workspace.selected_tab = Some(tab_id);
        Ok((tab_id, handle, pane))
    }

    /// The error for a close that would stop a program, unless `force`.
    fn confirm_close(&self, panes: &[&PaneEntry], force: bool) -> Result<(), LayoutError> {
        if !force && panes.iter().any(|p| p.runtime.is_live()) {
            // The pane runtime knows the program. A pane with a run is an agent.
            return Err(LayoutError::Confirm {
                process: None,
                is_agent: panes.iter().any(|p| p.run.is_some()),
            });
        }
        Ok(())
    }

    // --- workspaces -----------------------------------------------------------------------

    pub fn create_workspace(
        &mut self,
        params: WorkspaceCreate,
        alloc: &mut dyn Alloc,
    ) -> Outcome<WorkspaceCreated> {
        if let Some(machine) = &params.machine
            && machine != LOCAL
        {
            return Err(LayoutError::Unsupported(format!(
                "there is no machine {machine:?}: only {LOCAL:?} exists in this version"
            )));
        }
        let group = params
            .group
            .as_ref()
            .map(|g| self.resolve_group(g).map(|i| self.groups[i].id))
            .transpose()?;
        let number = alloc.number("workspace")?;
        let root = params.root.filter(|r| !r.is_empty());
        let name = match params.name {
            Some(name) => clean_name(&name)?,
            None => root
                .as_deref()
                .and_then(|r| std::path::Path::new(r).file_name())
                .and_then(|n| n.to_str())
                .map_or_else(|| format!("Workspace {number}"), str::to_owned),
        };
        let workspace = Workspace {
            id: WorkspaceId::new(alloc.ulid()?),
            handle: format!("w{number}"),
            number,
            name,
            root,
            machine: LOCAL.to_owned(),
            group,
            tabs: Vec::new(),
            selected_tab: None,
        };
        let id = workspace.id;
        self.workspaces.push(workspace);
        let index = self.workspaces.len() - 1;
        let (tab, tab_handle, pane) = self.add_tab(index, None, params.command, alloc)?;
        let workspace = &self.workspaces[index];
        let created = WorkspaceCreated {
            workspace: id,
            handle: workspace.handle.clone(),
            tab,
            tab_handle,
            pane: pane.id,
            pane_handle: pane.handle.clone(),
        };
        let mut events = vec![
            event(
                workspace.id,
                EventPayload::WorkspaceCreated(workspace_event(workspace)),
            ),
            event(
                tab,
                EventPayload::TabCreated(tab_event(workspace, &workspace.tabs[0])),
            ),
            event(pane.id, EventPayload::PaneCreated(pane_event(&pane))),
        ];
        // A window that shows nothing takes the new workspace.
        events.extend(self.normalize_windows());
        done(created, events)
    }

    pub fn rename_workspace(&mut self, workspace: &Ref<WorkspaceId>, name: &str) -> Outcome<()> {
        let index = self.resolve_workspace(workspace)?;
        let name = clean_name(name)?;
        let ws = &mut self.workspaces[index];
        ws.name = name;
        let payload = EventPayload::WorkspaceRenamed(workspace_event(ws));
        done((), vec![event(ws.id, payload)])
    }

    pub fn close_workspace(&mut self, workspace: &Ref<WorkspaceId>, force: bool) -> Outcome<()> {
        let index = self.resolve_workspace(workspace)?;
        let id = self.workspaces[index].id;
        self.confirm_close(&self.panes_of_workspace(id), force)?;
        let ws = self.workspaces.remove(index);
        let mut events = Vec::new();
        for tab in &ws.tabs {
            let doomed: Vec<PaneId> = self.panes_of_tab(tab.id).iter().map(|p| p.id).collect();
            for pane in doomed {
                if let Some(entry) = self.panes.remove(&pane) {
                    events.push(event(pane, EventPayload::PaneClosed(pane_event(&entry))));
                }
            }
            events.push(event(tab.id, EventPayload::TabClosed(tab_event(&ws, tab))));
        }
        events.push(event(
            ws.id,
            EventPayload::WorkspaceClosed(workspace_event(&ws)),
        ));
        events.extend(self.normalize_windows());
        done((), events)
    }

    /// Puts a workspace in a group (or in none) and before another workspace (or at the end).
    pub fn move_workspace(
        &mut self,
        workspace: &Ref<WorkspaceId>,
        group: Option<&Ref<GroupId>>,
        before: Option<&Ref<WorkspaceId>>,
    ) -> Outcome<()> {
        let index = self.resolve_workspace(workspace)?;
        let group = group
            .map(|g| self.resolve_group(g).map(|i| self.groups[i].id))
            .transpose()?;
        let target = before.map(|b| self.resolve_workspace(b)).transpose()?;
        if target == Some(index) {
            return Err(LayoutError::Invalid(
                "a workspace cannot go before itself".to_owned(),
            ));
        }
        let mut ws = self.workspaces.remove(index);
        ws.group = group;
        let at = match target {
            // The index of the target moved down by one when the workspace came out before it.
            Some(t) if t > index => t - 1,
            Some(t) => t,
            None => self.workspaces.len(),
        };
        self.workspaces.insert(at, ws);
        let ws = &self.workspaces[at];
        let payload = EventPayload::WorkspaceMoved(workspace_event(ws));
        done((), vec![event(ws.id, payload)])
    }

    /// The workspaces of `group`, or all of them. `has_agent` tells whether a pane has a live
    /// agent run.
    pub fn workspace_summaries(
        &self,
        group: Option<GroupId>,
        has_agent: &dyn Fn(PaneId) -> bool,
    ) -> Vec<WorkspaceSummary> {
        let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
        self.workspaces
            .iter()
            .filter(|w| group.is_none_or(|g| w.group == Some(g)))
            .map(|w| {
                let panes = self.panes_of_workspace(w.id);
                WorkspaceSummary {
                    id: w.id,
                    handle: w.handle.clone(),
                    name: w.name.clone(),
                    root: w.root.clone(),
                    group: w.group,
                    tabs: count(w.tabs.len()),
                    panes: count(panes.len()),
                    agents: count(panes.iter().filter(|p| has_agent(p.id)).count()),
                }
            })
            .collect()
    }

    // --- groups ---------------------------------------------------------------------------

    pub fn create_group(&mut self, name: &str, alloc: &mut dyn Alloc) -> Outcome<GroupId> {
        let name = clean_name(name)?;
        let group = Group {
            id: GroupId::new(alloc.ulid()?),
            name: name.clone(),
            parent: None,
        };
        let id = group.id;
        self.groups.push(group);
        let payload = EventPayload::GroupCreated(GroupEvent {
            group: id,
            name: Some(name),
        });
        done(id, vec![event(id, payload)])
    }

    pub fn rename_group(&mut self, group: &Ref<GroupId>, name: &str) -> Outcome<()> {
        let index = self.resolve_group(group)?;
        let name = clean_name(name)?;
        self.groups[index].name = name.clone();
        let id = self.groups[index].id;
        let payload = EventPayload::GroupRenamed(GroupEvent {
            group: id,
            name: Some(name),
        });
        done((), vec![event(id, payload)])
    }

    /// Closes a group. Its workspaces stay, with no group.
    pub fn close_group(&mut self, group: &Ref<GroupId>) -> Outcome<()> {
        let index = self.resolve_group(group)?;
        let gone = self.groups.remove(index);
        for ws in &mut self.workspaces {
            if ws.group == Some(gone.id) {
                ws.group = None;
            }
        }
        for g in &mut self.groups {
            if g.parent == Some(gone.id) {
                g.parent = None;
            }
        }
        let payload = EventPayload::GroupClosed(GroupEvent {
            group: gone.id,
            name: Some(gone.name),
        });
        done((), vec![event(gone.id, payload)])
    }

    pub fn move_group(
        &mut self,
        group: &Ref<GroupId>,
        before: Option<&Ref<GroupId>>,
    ) -> Outcome<()> {
        let index = self.resolve_group(group)?;
        let target = before.map(|b| self.resolve_group(b)).transpose()?;
        if target == Some(index) {
            return Err(LayoutError::Invalid(
                "a group cannot go before itself".to_owned(),
            ));
        }
        let moved = self.groups.remove(index);
        let at = match target {
            Some(t) if t > index => t - 1,
            Some(t) => t,
            None => self.groups.len(),
        };
        let (id, name) = (moved.id, moved.name.clone());
        self.groups.insert(at, moved);
        let payload = EventPayload::GroupMoved(GroupEvent {
            group: id,
            name: Some(name),
        });
        done((), vec![event(id, payload)])
    }

    pub fn group_summaries(&self) -> Vec<GroupSummary> {
        self.groups
            .iter()
            .map(|g| GroupSummary {
                id: g.id,
                name: g.name.clone(),
                workspaces: self
                    .workspaces
                    .iter()
                    .filter(|w| w.group == Some(g.id))
                    .map(|w| w.id)
                    .collect(),
            })
            .collect()
    }

    // --- tabs -----------------------------------------------------------------------------

    pub fn create_tab(&mut self, params: TabCreate, alloc: &mut dyn Alloc) -> Outcome<TabCreated> {
        let index = self.resolve_workspace(&params.workspace)?;
        let (tab, handle, pane) =
            self.add_tab(index, params.name.as_deref(), params.command, alloc)?;
        let workspace = &self.workspaces[index];
        let created = workspace
            .tabs
            .last()
            .map(|t| tab_event(workspace, t))
            .ok_or_else(|| LayoutError::Internal("a tab was added and is gone".to_owned()))?;
        let events = vec![
            event(tab, EventPayload::TabCreated(created)),
            event(pane.id, EventPayload::PaneCreated(pane_event(&pane))),
        ];
        done(
            TabCreated {
                tab,
                handle,
                pane: pane.id,
                pane_handle: pane.handle,
            },
            events,
        )
    }

    pub fn rename_tab(&mut self, tab: &Ref<TabId>, name: &str) -> Outcome<()> {
        let (w, t) = self.resolve_tab(tab)?;
        let name = clean_name(name)?;
        self.workspaces[w].tabs[t].name = name;
        let workspace = &self.workspaces[w];
        let tab = &workspace.tabs[t];
        let payload = EventPayload::TabRenamed(tab_event(workspace, tab));
        done((), vec![event(tab.id, payload)])
    }

    pub fn close_tab(&mut self, tab: &Ref<TabId>, force: bool) -> Outcome<()> {
        let (w, t) = self.resolve_tab(tab)?;
        let id = self.workspaces[w].tabs[t].id;
        self.confirm_close(&self.panes_of_tab(id), force)?;
        let mut events = Vec::new();
        let doomed: Vec<PaneId> = self.panes_of_tab(id).iter().map(|p| p.id).collect();
        for pane in doomed {
            if let Some(entry) = self.panes.remove(&pane) {
                events.push(event(pane, EventPayload::PaneClosed(pane_event(&entry))));
            }
        }
        let workspace = &mut self.workspaces[w];
        let closed = workspace.tabs.remove(t);
        if workspace.selected_tab == Some(closed.id) {
            // The tab before it, else the one that took its place.
            workspace.selected_tab = workspace.tabs.get(t.saturating_sub(1)).map(|tab| tab.id);
        }
        events.push(event(
            closed.id,
            EventPayload::TabClosed(tab_event(workspace, &closed)),
        ));
        events.extend(self.normalize_windows());
        done((), events)
    }

    /// Moves a tab to another place in its workspace or to another workspace. The panes keep
    /// their handles.
    pub fn move_tab(
        &mut self,
        tab: &Ref<TabId>,
        workspace: Option<&Ref<WorkspaceId>>,
        index: Option<u32>,
    ) -> Outcome<()> {
        let (from, at) = self.resolve_tab(tab)?;
        let to = match workspace {
            Some(r) => self.resolve_workspace(r)?,
            None => from,
        };
        let moved = self.workspaces[from].tabs.remove(at);
        if self.workspaces[from].selected_tab == Some(moved.id) {
            self.workspaces[from].selected_tab = self.workspaces[from]
                .tabs
                .get(at.saturating_sub(1))
                .map(|t| t.id);
        }
        let target = &mut self.workspaces[to];
        let place = index
            .and_then(|i| usize::try_from(i).ok())
            .map_or(target.tabs.len(), |i| i.min(target.tabs.len()));
        let id = moved.id;
        target.tabs.insert(place, moved);
        if target.selected_tab.is_none() {
            target.selected_tab = Some(id);
        }
        let target_id = target.id;
        if to != from {
            for pane in self.panes.values_mut().filter(|p| p.tab == id) {
                pane.workspace = target_id;
            }
        }
        let target = &self.workspaces[to];
        let payload = EventPayload::TabMoved(tab_event(target, &target.tabs[place]));
        let mut events = vec![event(id, payload)];
        events.extend(self.normalize_windows());
        done((), events)
    }

    /// Selects the tab that the workspace shows. With a window, the window shows the tab, too.
    pub fn select_tab(&mut self, tab: &Ref<TabId>, window: Option<WindowId>) -> Outcome<()> {
        let (w, t) = self.resolve_tab(tab)?;
        let id = self.workspaces[w].tabs[t].id;
        if let Some(window) = window {
            self.show_tab_in_window(window, id)?;
        }
        self.workspaces[w].selected_tab = Some(id);
        let workspace = &self.workspaces[w];
        let payload = EventPayload::TabSelected(tab_event(workspace, &workspace.tabs[t]));
        done((), vec![event(id, payload)])
    }

    // --- the layout of a tab ----------------------------------------------------------------

    fn layout_changed(&self, tab: TabId) -> Vec<NewEvent> {
        vec![event(
            tab,
            EventPayload::LayoutChanged(LayoutChanged { tab }),
        )]
    }

    pub fn set_ratios(&mut self, tab: &Ref<TabId>, path: &[u32], ratios: &[f32]) -> Outcome<()> {
        let (w, t) = self.resolve_tab(tab)?;
        let tab = &mut self.workspaces[w].tabs[t];
        tree::set_ratios(&mut tab.layout, path, ratios)?;
        let id = tab.id;
        done((), self.layout_changed(id))
    }

    pub fn equalize(&mut self, tab: &Ref<TabId>, path: &[u32]) -> Outcome<()> {
        let (w, t) = self.resolve_tab(tab)?;
        let tab = &mut self.workspaces[w].tabs[t];
        tree::equalize(&mut tab.layout, path)?;
        let id = tab.id;
        done((), self.layout_changed(id))
    }

    /// Zooms a pane of the tab, or ends the zoom.
    pub fn zoom(&mut self, tab: &Ref<TabId>, pane: Option<&Ref<PaneId>>) -> Outcome<()> {
        let (w, t) = self.resolve_tab(tab)?;
        let pane = pane.map(|p| self.resolve_pane(p)).transpose()?;
        let tab = &mut self.workspaces[w].tabs[t];
        if let Some(pane) = pane
            && !tab.shows(pane)
        {
            return Err(LayoutError::Invalid(
                "the pane is not in this tab, or it is popped out".to_owned(),
            ));
        }
        tab.zoomed = pane;
        let id = tab.id;
        done((), self.layout_changed(id))
    }
}

#[cfg(test)]
mod tests;
