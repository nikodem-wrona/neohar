//! The commands on panes and windows: split, close, move, float, pop out, and the
//! windows that show tabs and panes.
//!
//! A pane is in the tree of its tab, floating over it, or in a window of its own. A tab keeps one
//! pane in its tree as long as it has a pane. When the last tiled pane leaves a tab, another pane
//! of the tab takes its place (a floating one first, then a popped-out one), and if there is none
//! the tab closes. The same rule holds for a close and for a move, so a tab is never empty.

use nh_proto::api::{
    calls::{
        pane::{PaneClose, PaneCreated, PaneMove, PaneSplit},
        workspace::{WindowOpen, WindowOpened},
    },
    common::{Bounds, ProcessSpec, Rect, SplitDirection},
    events::{EventPayload, LayoutChanged, PaneEvent, WindowEvent},
    ids::{PaneId, Ref, TabId, WindowId},
    model::{FloatingPane, PaneValue},
};

/// What [`Layout::apply_report`] did.
#[derive(Debug, Default)]
pub struct Applied {
    /// The report changed the pane.
    pub changed: bool,
    /// The event for a child that ended.
    pub event: Option<NewEvent>,
}

use super::{
    model::{
        Alloc, Facts, Layout, LayoutError, PaneEntry, Placement, Report, Runtime, Window,
        WindowFacts,
    },
    ops::{Done, NewEvent, effective_spec},
    tree,
};

type Outcome<T> = Result<Done<T>, LayoutError>;

fn done<T>(value: T, events: Vec<NewEvent>) -> Outcome<T> {
    Ok(Done { value, events })
}

fn event(subject: impl ToString, payload: EventPayload) -> NewEvent {
    NewEvent {
        subject: subject.to_string(),
        payload,
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

fn layout_changed(tab: TabId) -> NewEvent {
    event(tab, EventPayload::LayoutChanged(LayoutChanged { tab }))
}

/// What a floating pane gets when the call gives no rectangle: a half of the tab, shifted for each
/// floating pane that is there.
fn default_rect(index: usize) -> Rect {
    let step = 0.04 * (index % 6) as f32;
    Rect {
        x: 0.2 + step,
        y: 0.2 + step,
        w: 0.5,
        h: 0.5,
    }
}

/// The smallest side of a floating pane, as a share of the tab.
const MIN_SIDE: f32 = 0.05;

/// A rectangle in fractions of the tab area that lies inside the tab.
fn check_rect(rect: Rect) -> Result<Rect, LayoutError> {
    let finite = [rect.x, rect.y, rect.w, rect.h]
        .iter()
        .all(|v| v.is_finite());
    let inside = rect.x >= 0.0
        && rect.y >= 0.0
        && rect.w >= MIN_SIDE
        && rect.h >= MIN_SIDE
        && rect.x + rect.w <= 1.001
        && rect.y + rect.h <= 1.001;
    if finite && inside {
        Ok(rect)
    } else {
        Err(LayoutError::Invalid(format!(
            "a floating pane lies inside the tab, in fractions from 0 to 1, with sides of at least {MIN_SIDE}"
        )))
    }
}

/// What to do when the pane that leaves a tab is its last tiled pane.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Last {
    /// Refuse: the command would leave the tab with no pane in its tree.
    Refuse,
    /// Another pane of the tab takes the place, or the tab closes.
    Replace,
}

/// What `detach` did to the tab.
struct Detached {
    /// The tab has no pane left, and it was removed.
    tab_closed: bool,
    events: Vec<NewEvent>,
}

impl Layout {
    /// The workspace index and the tab index of the tab of a pane.
    fn tab_of(&self, pane: PaneId) -> Result<(usize, usize), LayoutError> {
        let entry = self
            .panes
            .get(&pane)
            .ok_or_else(|| LayoutError::NotFound(format!("no pane {pane}")))?;
        self.tab_position(entry.tab)
            .ok_or_else(|| LayoutError::Internal("a pane without a tab".to_owned()))
    }

    /// Takes a pane out of its place in its tab. The pane stays in `panes`: the caller closes it,
    /// moves it or puts it somewhere else. A zoom on the pane ends.
    fn detach(&mut self, pane: PaneId, last: Last) -> Result<Detached, LayoutError> {
        let (w, t) = self.tab_of(pane)?;
        let mut events = Vec::new();
        match self
            .placement(pane)
            .ok_or_else(|| LayoutError::Internal("a pane that is nowhere".to_owned()))?
        {
            Placement::Floating => {
                self.workspaces[w].tabs[t]
                    .floating
                    .retain(|f| f.pane != pane);
            }
            Placement::PoppedOut(window) => {
                self.windows.retain(|win| win.id != window);
                events.push(event(
                    window,
                    EventPayload::WindowClosed(WindowEvent { window }),
                ));
            }
            Placement::Tiled => {
                let only_tiled = tree::panes(&self.workspaces[w].tabs[t].layout).len() == 1;
                if only_tiled {
                    if last == Last::Refuse {
                        return Err(LayoutError::Invalid(
                            "the tab keeps one pane in its layout".to_owned(),
                        ));
                    }
                    // A floating pane of the tab takes the place. Else a popped-out pane comes
                    // back. Else the tab has no pane left.
                    let tab_id = self.workspaces[w].tabs[t].id;
                    let floating = (!self.workspaces[w].tabs[t].floating.is_empty())
                        .then(|| self.workspaces[w].tabs[t].floating.remove(0).pane);
                    let heir = floating.or_else(|| {
                        self.windows
                            .iter()
                            .find_map(|win| win.popout.filter(|p| self.panes[p].tab == tab_id))
                    });
                    match heir {
                        Some(heir) => {
                            if let Some(win) = self.windows.iter().find(|w| w.popout == Some(heir))
                            {
                                let window = win.id;
                                self.windows.retain(|w| w.id != window);
                                events.push(event(
                                    window,
                                    EventPayload::WindowClosed(WindowEvent { window }),
                                ));
                            }
                            self.workspaces[w].tabs[t].layout = tree::leaf(heir);
                        }
                        None => {
                            let tab = self.drop_tab(w, t);
                            events.push(tab);
                            return Ok(Detached {
                                tab_closed: true,
                                events,
                            });
                        }
                    }
                } else {
                    let layout =
                        std::mem::replace(&mut self.workspaces[w].tabs[t].layout, tree::leaf(pane));
                    self.workspaces[w].tabs[t].layout = tree::remove(layout, pane)?
                        .ok_or_else(|| LayoutError::Internal("the tree vanished".to_owned()))?;
                }
            }
        }
        let tab = &mut self.workspaces[w].tabs[t];
        if tab.zoomed == Some(pane) {
            tab.zoomed = None;
        }
        Ok(Detached {
            tab_closed: false,
            events,
        })
    }

    /// Removes a tab that has no pane, and selects another tab if it was the selected one.
    fn drop_tab(&mut self, w: usize, t: usize) -> NewEvent {
        let workspace = &mut self.workspaces[w];
        let closed = workspace.tabs.remove(t);
        if workspace.selected_tab == Some(closed.id) {
            workspace.selected_tab = workspace.visible_near(t);
        }
        event(
            closed.id,
            EventPayload::TabClosed(nh_proto::api::events::TabEvent {
                tab: closed.id,
                handle: closed.handle,
                workspace: workspace.id,
                name: Some(closed.name),
            }),
        )
    }

    /// Keeps the windows right after a command: a popout window whose pane is not popped out any
    /// more is closed, a window forgets a tab or a pane that is gone, and a window that showed a
    /// tab that closed shows the selected tab of a workspace instead.
    pub(super) fn normalize_windows(&mut self) -> Vec<NewEvent> {
        let mut events = Vec::new();
        let mut closing = Vec::new();
        for window in &self.windows {
            if let Some(pane) = window.popout
                && self.placement(pane) != Some(Placement::PoppedOut(window.id))
            {
                closing.push(window.id);
            }
        }
        for id in closing {
            self.windows.retain(|w| w.id != id);
            events.push(event(
                id,
                EventPayload::WindowClosed(WindowEvent { window: id }),
            ));
        }
        let tabs: Vec<(nh_proto::api::ids::WorkspaceId, Vec<TabId>, Option<TabId>)> = self
            .workspaces
            .iter()
            .map(|w| (w.id, w.tabs.iter().map(|t| t.id).collect(), w.selected_tab))
            .collect();
        let tab_exists = |tab: TabId| tabs.iter().any(|(_, list, _)| list.contains(&tab));
        let first_selected = tabs.iter().find_map(|(_, _, selected)| *selected);
        let panes: std::collections::HashSet<PaneId> = self.panes.keys().copied().collect();
        for window in &mut self.windows {
            window.by_workspace.retain(|workspace, tab| {
                tabs.iter()
                    .any(|(w, list, _)| w == workspace && list.contains(tab))
            });
            if let Some(tab) = window.tab
                && !tab_exists(tab)
            {
                window.tab = window
                    .by_workspace
                    .values()
                    .next()
                    .copied()
                    .or(first_selected);
            }
            // A window that shows nothing shows the first workspace that appears. The first window
            // of a first launch opens before any workspace exists.
            if window.tab.is_none() && window.explicit.is_none() && window.popout.is_none() {
                window.tab = first_selected;
            }
            if let Some(list) = &mut window.explicit {
                list.retain(|p| panes.contains(p));
            }
            if window.focused_pane.is_some_and(|p| !panes.contains(&p)) {
                window.focused_pane = None;
            }
        }
        events
    }

    // --- panes ----------------------------------------------------------------------------

    /// Splits a pane of the tree: the new pane goes on a side of it.
    pub fn split_pane(&mut self, params: PaneSplit, alloc: &mut dyn Alloc) -> Outcome<PaneCreated> {
        let target = self.resolve_pane(&params.pane)?;
        if self.placement(target) != Some(Placement::Tiled) {
            return Err(LayoutError::Invalid(
                "only a pane in the tree of its tab can be split".to_owned(),
            ));
        }
        let (w, t) = self.tab_of(target)?;
        let (workspace, tab) = (self.workspaces[w].id, self.workspaces[w].tabs[t].id);
        // Check the ratio before anything is made.
        let mut probe = self.workspaces[w].tabs[t].layout.clone();
        tree::split(
            &mut probe,
            target,
            PaneId::new(ulid::Ulid::nil()),
            params.direction,
            params.ratio,
        )?;
        let pane = self.make_pane(workspace, tab, params.command, alloc)?;
        let tab_ref = &mut self.workspaces[w].tabs[t];
        tree::split(
            &mut tab_ref.layout,
            target,
            pane.id,
            params.direction,
            params.ratio,
        )?;
        // The new pane must be seen, so a zoom ends.
        tab_ref.zoomed = None;
        let mut events = vec![
            event(pane.id, EventPayload::PaneCreated(pane_event(&pane))),
            layout_changed(tab),
        ];
        events.extend(self.normalize_windows());
        done(
            PaneCreated {
                pane: pane.id,
                handle: pane.handle,
            },
            events,
        )
    }

    /// Closes a pane. A pane that runs a program needs `force`. When it was the last pane of its
    /// tab, the tab closes with it.
    pub fn close_pane(&mut self, params: &PaneClose) -> Outcome<()> {
        let pane = self.resolve_pane(&params.pane)?;
        let entry = self.panes[&pane].clone();
        if !params.force && entry.runtime.is_live() {
            return Err(LayoutError::Confirm {
                process: None,
                is_agent: entry.run.is_some(),
            });
        }
        let detached = self.detach(pane, Last::Replace)?;
        self.panes.remove(&pane);
        let mut events = vec![event(pane, EventPayload::PaneClosed(pane_event(&entry)))];
        events.extend(detached.events);
        if !detached.tab_closed {
            events.push(layout_changed(entry.tab));
        }
        events.extend(self.normalize_windows());
        done((), events)
    }

    /// Moves a pane to another tab: beside a pane of that tab, or at the edge of its tree.
    pub fn move_pane(&mut self, params: &PaneMove) -> Outcome<()> {
        let pane = self.resolve_pane(&params.pane)?;
        let (dest_w, dest_t) = self.resolve_tab(&params.tab)?;
        let beside = params
            .beside
            .as_ref()
            .map(|b| self.resolve_pane(b))
            .transpose()?;
        if beside == Some(pane) {
            return Err(LayoutError::Invalid(
                "a pane cannot go beside itself".to_owned(),
            ));
        }
        let dest = self.workspaces[dest_w].tabs[dest_t].id;
        if let Some(beside) = beside
            && (self.panes[&beside].tab != dest || self.placement(beside) != Some(Placement::Tiled))
        {
            return Err(LayoutError::Invalid(
                "the pane to go beside is not in the tree of that tab".to_owned(),
            ));
        }
        let direction = params.direction.unwrap_or(SplitDirection::Right);
        let source = self.panes[&pane].tab;
        if source == dest && beside.is_none() && self.placement(pane) == Some(Placement::Tiled) {
            // To the edge of its own tree: take it out and put it back.
            if tree::panes(&self.workspaces[dest_w].tabs[dest_t].layout).len() == 1 {
                return Err(LayoutError::Invalid("there is nothing to move".to_owned()));
            }
        }
        let detached = self.detach(pane, Last::Replace)?;
        // The destination may have moved when the source tab closed.
        let (dest_w, dest_t) = self
            .tab_position(dest)
            .ok_or_else(|| LayoutError::Internal("the destination tab is gone".to_owned()))?;
        let workspace = self.workspaces[dest_w].id;
        if let Some(entry) = self.panes.get_mut(&pane) {
            entry.tab = dest;
            entry.workspace = workspace;
        }
        let layout = &mut self.workspaces[dest_w].tabs[dest_t].layout;
        match beside {
            Some(beside) => tree::split(layout, beside, pane, direction, None)?,
            None => tree::insert_at_root(layout, pane, direction, None)?,
        }
        self.workspaces[dest_w].tabs[dest_t].zoomed = None;
        let entry = &self.panes[&pane];
        let mut events = vec![event(pane, EventPayload::PaneMoved(pane_event(entry)))];
        events.extend(detached.events);
        if !detached.tab_closed && source != dest {
            events.push(layout_changed(source));
        }
        events.push(layout_changed(dest));
        events.extend(self.normalize_windows());
        done((), events)
    }

    /// Moves a tiled pane over the tree.
    pub fn float_pane(&mut self, pane: &Ref<PaneId>, rect: Option<Rect>) -> Outcome<()> {
        let pane = self.resolve_pane(pane)?;
        if self.placement(pane) != Some(Placement::Tiled) {
            return Err(LayoutError::Invalid(
                "only a pane in the tree of its tab can float".to_owned(),
            ));
        }
        let rect = rect.map(check_rect).transpose()?;
        let (w, t) = self.tab_of(pane)?;
        let tab = self.workspaces[w].tabs[t].id;
        let rect = rect.unwrap_or_else(|| default_rect(self.workspaces[w].tabs[t].floating.len()));
        self.detach(pane, Last::Refuse)?;
        self.workspaces[w].tabs[t]
            .floating
            .push(FloatingPane { pane, rect });
        let entry = &self.panes[&pane];
        let mut events = vec![
            event(pane, EventPayload::PaneFloated(pane_event(entry))),
            layout_changed(tab),
        ];
        events.extend(self.normalize_windows());
        done((), events)
    }

    /// Puts a floating pane back in the tree, at the right edge.
    pub fn unfloat_pane(&mut self, pane: &Ref<PaneId>) -> Outcome<()> {
        let pane = self.resolve_pane(pane)?;
        if self.placement(pane) != Some(Placement::Floating) {
            return Err(LayoutError::Invalid("the pane does not float".to_owned()));
        }
        let (w, t) = self.tab_of(pane)?;
        let tab = &mut self.workspaces[w].tabs[t];
        tab.floating.retain(|f| f.pane != pane);
        tree::insert_at_root(&mut tab.layout, pane, SplitDirection::Right, None)?;
        let tab_id = tab.id;
        let entry = &self.panes[&pane];
        let events = vec![
            event(pane, EventPayload::PaneUnfloated(pane_event(entry))),
            layout_changed(tab_id),
        ];
        done((), events)
    }

    /// Changes the rectangle of a floating pane. A drag sends it for each animation frame,
    /// so it causes no event.
    pub fn set_float_rect(&mut self, pane: &Ref<PaneId>, rect: Rect) -> Outcome<()> {
        let pane = self.resolve_pane(pane)?;
        let rect = check_rect(rect)?;
        let (w, t) = self.tab_of(pane)?;
        let Some(floating) = self.workspaces[w].tabs[t]
            .floating
            .iter_mut()
            .find(|f| f.pane == pane)
        else {
            return Err(LayoutError::Invalid("the pane does not float".to_owned()));
        };
        floating.rect = rect;
        done((), Vec::new())
    }

    /// Moves a pane into a window of its own.
    pub fn popout_pane(
        &mut self,
        pane: &Ref<PaneId>,
        alloc: &mut dyn Alloc,
    ) -> Outcome<WindowOpened> {
        let pane = self.resolve_pane(pane)?;
        if self.placement(pane) == Some(Placement::Tiled)
            || self.placement(pane) == Some(Placement::Floating)
        {
            self.detach(pane, Last::Refuse)?;
        } else {
            return Err(LayoutError::Invalid(
                "the pane is popped out already".to_owned(),
            ));
        }
        let id = WindowId::new(alloc.ulid()?);
        self.windows.push(Window {
            id,
            bounds: None,
            tab: None,
            by_workspace: Default::default(),
            explicit: None,
            popout: Some(pane),
            focused_pane: Some(pane),
            facts: WindowFacts::default(),
            reveal: None,
            client: None,
            single: false,
        });
        let entry = &self.panes[&pane];
        let events = vec![
            event(pane, EventPayload::PaneMoved(pane_event(entry))),
            event(id, EventPayload::WindowOpened(WindowEvent { window: id })),
            layout_changed(entry.tab),
        ];
        done(WindowOpened { window: id }, events)
    }

    /// Starts the program of a pane again after it ended. The pane goes back to the state
    /// `created`, and the pane runtime starts a new holder for it. `command` replaces what the pane
    /// runs.
    pub fn restart_pane(
        &mut self,
        pane: &Ref<PaneId>,
        command: Option<ProcessSpec>,
    ) -> Outcome<()> {
        let id = self.resolve_pane(pane)?;
        let root = self
            .panes
            .get(&id)
            .and_then(|p| self.workspace(p.workspace))
            .and_then(|w| w.root.clone());
        let entry = self
            .panes
            .get_mut(&id)
            .ok_or_else(|| LayoutError::NotFound(format!("no pane {id}")))?;
        match entry.runtime {
            Runtime::Exited(_) | Runtime::Lost => {}
            Runtime::Running => {
                return Err(LayoutError::Invalid(
                    "the program of the pane runs; close the pane, or wait until it ends"
                        .to_owned(),
                ));
            }
            Runtime::Created => {
                return Err(LayoutError::Invalid("the pane starts now".to_owned()));
            }
        }
        if let Some(spec) = command {
            entry.spec = effective_spec(Some(spec), root.as_deref());
        }
        entry.runtime = Runtime::Created;
        entry.cwd = None;
        entry.facts = Facts::default();
        let events = vec![event(id, EventPayload::PaneRestarted(pane_event(entry)))];
        done((), events)
    }

    /// The pane that `pane.focus` would focus in a window: one of the panes that the window draws.
    /// The focus is a state in memory, so the check and the change are apart.
    pub fn focus_target(
        &self,
        window: WindowId,
        pane: &Ref<PaneId>,
    ) -> Result<PaneId, LayoutError> {
        let id = self.resolve_pane(pane)?;
        let index = self
            .window_index(window)
            .ok_or_else(|| LayoutError::NotFound(format!("no window {window}")))?;
        if !self.focusable_panes(&self.windows[index]).contains(&id) {
            return Err(LayoutError::Invalid(
                "the window does not draw this pane".to_owned(),
            ));
        }
        Ok(id)
    }

    /// Gives keyboard focus in a window to a pane that [`Layout::focus_target`] accepted.
    pub fn set_focus(&mut self, window: WindowId, pane: PaneId) {
        if let Some(index) = self.window_index(window) {
            self.windows[index].focused_pane = Some(pane);
        }
    }

    /// A pane whose holder is gone, after a reboot or a kill while no core ran, starts again with
    /// the login shell, in the directory where it ran last ("Reboot"). Only a pane that
    /// was running is recreated: one that ended stays as it is. Returns the event, or `None` when
    /// the pane is not in the layout or did not run.
    pub fn recreate_pane(&mut self, pane: PaneId) -> Option<NewEvent> {
        let entry = self.panes.get_mut(&pane)?;
        if entry.runtime != Runtime::Running {
            return None;
        }
        let cwd = entry
            .cwd
            .clone()
            .or_else(|| entry.spec.as_ref().and_then(|spec| spec.cwd.clone()));
        entry.spec = Some(ProcessSpec {
            argv: Vec::new(),
            cwd,
            env: std::collections::BTreeMap::new(),
        });
        entry.runtime = Runtime::Created;
        entry.cwd = None;
        entry.facts = Facts::default();
        Some(event(pane, EventPayload::PaneRestarted(pane_event(entry))))
    }

    /// A window asked for a size of a pane that has no process yet. The holder starts with it, or
    /// the service resizes the PTY when the holder is up. Returns whether the layout changed.
    pub fn note_size(&mut self, pane: PaneId, cols: u16, rows: u16) -> bool {
        let Some(entry) = self.panes.get_mut(&pane) else {
            return false;
        };
        if entry.runtime != Runtime::Created || (entry.cols, entry.rows) == (cols, rows) {
            return false;
        }
        (entry.cols, entry.rows) = (cols, rows);
        true
    }

    /// Takes a report of the pane runtime: the state of the child, the size and the facts. The
    /// pane may be gone already; then nothing changes.
    pub fn apply_report(&mut self, pane: PaneId, report: Report) -> Applied {
        let Some(entry) = self.panes.get_mut(&pane) else {
            return Applied::default();
        };
        let was_live = entry.runtime.is_live();
        // The first report of a new holder says the size that the holder started with. A window
        // may have asked for another size while the holder started (`note_size`), and that size
        // stays until the PTY has it.
        let starting = entry.runtime == Runtime::Created && report.runtime == Runtime::Running;
        let size = if starting {
            (entry.cols, entry.rows)
        } else {
            (report.cols, report.rows)
        };
        let same = entry.runtime == report.runtime
            && (entry.cols, entry.rows) == size
            && entry.facts == report.facts;
        if same {
            return Applied::default();
        }
        entry.runtime = report.runtime;
        (entry.cols, entry.rows) = size;
        entry.facts = report.facts;
        let event = match report.runtime {
            Runtime::Exited(code) if was_live => Some(event(
                pane,
                EventPayload::PaneExited(nh_proto::api::events::PaneExited { pane, code }),
            )),
            _ => None,
        };
        Applied {
            changed: true,
            event,
        }
    }

    /// The panes of the layout as the `panes` projection shows them, for the CLI.
    pub fn pane_values(
        &self,
        workspace: Option<&Ref<nh_proto::api::ids::WorkspaceId>>,
        tab: Option<&Ref<TabId>>,
    ) -> Result<Vec<PaneValue>, LayoutError> {
        let workspace = workspace
            .map(|w| self.resolve_workspace(w).map(|i| self.workspaces[i].id))
            .transpose()?;
        let tab = tab
            .map(|t| {
                self.resolve_tab(t)
                    .map(|(w, t)| self.workspaces[w].tabs[t].id)
            })
            .transpose()?;
        Ok(self
            .panes
            .values()
            .filter(|p| workspace.is_none_or(|w| p.workspace == w))
            .filter(|p| tab.is_none_or(|t| p.tab == t))
            .map(|p| self.pane_value(p))
            .collect())
    }

    // --- windows --------------------------------------------------------------------------

    /// Opens a window that shows a tab, or the given panes.
    pub fn open_window(
        &mut self,
        params: WindowOpen,
        alloc: &mut dyn Alloc,
    ) -> Outcome<WindowOpened> {
        if !params.panes.is_empty() && params.tab.is_some() {
            return Err(LayoutError::Invalid(
                "give the panes or a tab, not both".to_owned(),
            ));
        }
        let panes = params
            .panes
            .iter()
            .map(|p| self.resolve_pane(p))
            .collect::<Result<Vec<_>, _>>()?;
        let tab = match &params.tab {
            Some(r) => {
                let (w, t) = self.resolve_tab(r)?;
                Some(self.workspaces[w].tabs[t].id)
            }
            None if panes.is_empty() => self.workspaces.iter().find_map(|w| w.selected_tab),
            None => None,
        };
        let mut by_workspace = std::collections::BTreeMap::new();
        if let Some(tab) = tab
            && let Some((w, _)) = self.tab_position(tab)
        {
            by_workspace.insert(self.workspaces[w].id, tab);
        }
        let id = WindowId::new(alloc.ulid()?);
        self.windows.push(Window {
            id,
            bounds: params.bounds,
            tab,
            by_workspace,
            explicit: (!panes.is_empty()).then_some(panes),
            popout: None,
            focused_pane: None,
            facts: WindowFacts::default(),
            reveal: None,
            client: None,
            single: false,
        });
        let events = vec![event(
            id,
            EventPayload::WindowOpened(WindowEvent { window: id }),
        )];
        done(WindowOpened { window: id }, events)
    }

    fn window(&mut self, id: WindowId) -> Result<&mut Window, LayoutError> {
        self.windows
            .iter_mut()
            .find(|w| w.id == id)
            .ok_or_else(|| LayoutError::NotFound(format!("no window {id}")))
    }

    /// Closes a window. A pane that was popped out goes back to its tab.
    pub fn close_window(&mut self, id: WindowId) -> Outcome<()> {
        let index = self
            .window_index(id)
            .ok_or_else(|| LayoutError::NotFound(format!("no window {id}")))?;
        let window = self.windows.remove(index);
        let mut events = vec![event(
            id,
            EventPayload::WindowClosed(WindowEvent { window: id }),
        )];
        if let Some(pane) = window.popout
            && let Some(entry) = self.panes.get(&pane).cloned()
            && let Some((w, t)) = self.tab_position(entry.tab)
        {
            tree::insert_at_root(
                &mut self.workspaces[w].tabs[t].layout,
                pane,
                SplitDirection::Right,
                None,
            )?;
            events.push(event(pane, EventPayload::PaneMoved(pane_event(&entry))));
            events.push(layout_changed(entry.tab));
        }
        events.extend(self.normalize_windows());
        done((), events)
    }

    /// Sets the panes that a window shows. The window then shows no tab.
    pub fn set_visible_panes(&mut self, id: WindowId, panes: &[Ref<PaneId>]) -> Outcome<()> {
        let panes = panes
            .iter()
            .map(|p| self.resolve_pane(p))
            .collect::<Result<Vec<_>, _>>()?;
        let window = self.window(id)?;
        if window.popout.is_some() {
            return Err(LayoutError::Invalid(
                "a window of a popped-out pane shows that pane".to_owned(),
            ));
        }
        window.tab = None;
        window.explicit = Some(panes);
        done((), Vec::new())
    }

    pub fn set_window_bounds(&mut self, id: WindowId, bounds: Bounds) -> Outcome<()> {
        self.window(id)?.bounds = Some(bounds);
        done((), Vec::new())
    }

    /// The window shows this tab now, and remembers it for the workspace of the tab.
    pub(super) fn show_tab_in_window(
        &mut self,
        id: WindowId,
        tab: TabId,
    ) -> Result<(), LayoutError> {
        let (w, _) = self
            .tab_position(tab)
            .ok_or_else(|| LayoutError::NotFound(format!("no tab {tab}")))?;
        let workspace = self.workspaces[w].id;
        let window = self.window(id)?;
        if window.popout.is_some() {
            return Err(LayoutError::Invalid(
                "a window of a popped-out pane shows that pane".to_owned(),
            ));
        }
        window.tab = Some(tab);
        window.explicit = None;
        window.by_workspace.insert(workspace, tab);
        Ok(())
    }
}

#[cfg(test)]
mod tests;
