//! The commands on workspaces, groups, tabs and the layout of a tab.
//!
//! A handler runs the command on a copy of the layout, checks the copy, writes the difference and
//! the events in the transaction, and returns a mutation that puts the copy in the state and
//! publishes the `tree` projection after the commit. If the command or the write fails, the
//! transaction rolls back and the layout is as it was.

use nh_proto::api::{
    calls::{
        pane::{PaneListResult, PaneShown},
        phone::PhoneWindowResult,
        workspace::{
            GroupCreated, GroupListResult, WindowOpen, WorkspaceListResult, WorkspaceShown,
        },
    },
    common::Empty,
    ids::{PaneId, WindowId},
    methods::{Call, Method},
};
use nh_store::Tx;
use serde::Serialize;
use serde_json::{Value, to_value};
use ulid::Ulid;

use super::{Ctx, Handled, HandlerTable, Parts};
use crate::{
    ErrorCode,
    error::RpcError,
    layout::{
        model::{Alloc, Layout, LayoutError, Report, Runtime, WindowFacts},
        ops::Done,
        persist::persist,
        project::{publish, publish_pane, publish_window},
    },
};

/// Numbers from the handle counters of the database, and new ULIDs.
struct TxAlloc<'a, 'b>(&'a Tx<'b>);

impl Alloc for TxAlloc<'_, '_> {
    fn ulid(&mut self) -> Result<Ulid, LayoutError> {
        crate::ids::new_ulid().map_err(|e| LayoutError::Internal(e.message))
    }

    fn number(&mut self, scope: &str) -> Result<u32, LayoutError> {
        self.0
            .next_handle(scope)
            .map_err(|e| LayoutError::Internal(e.to_string()))
    }
}

fn to_json<T: Serialize>(value: &T) -> Result<Value, RpcError> {
    to_value(value).map_err(|e| RpcError::internal(e.to_string()))
}

/// Runs a command that changes the layout.
pub(super) fn run<T: Serialize>(
    ctx: &Ctx<'_>,
    tx: &mut Tx<'_>,
    command: impl FnOnce(&mut Layout, &mut dyn Alloc) -> Result<Done<T>, LayoutError>,
) -> Result<Handled, RpcError> {
    let mut next = ctx.state.layout.clone();
    let done = command(&mut next, &mut TxAlloc(tx))?;
    // A command must keep the rules. If it did not, it is a bug, and nothing is written.
    next.validate()
        .map_err(|e| RpcError::internal(format!("a layout command broke a rule: {e}")))?;
    persist(tx, &ctx.state.layout, &next).map_err(|e| RpcError::internal(e.to_string()))?;
    for event in &done.events {
        tx.append(Some(&event.subject), &event.payload)
            .map_err(|e| RpcError::internal(e.to_string()))?;
    }
    let result = to_json(&done.value)?;
    Ok(Handled {
        result,
        mutation: Some(Box::new(move |state, hub| {
            publish(hub, &state.layout, &next);
            let old = std::mem::replace(&mut state.layout, next);
            // The pane runtime starts and ends holders, and the windows draw other panes.
            state.layout_changed(&old);
            state.publish_palette(hub);
            crate::agents::runs::sync_windows(hub, &mut state.runs, &old, &state.layout);
        })),
    })
}

/// Takes what a pane task reports: the state of the child, the size and the facts. The state is in
/// memory and in the `panes` projection. The task stores the exit itself, before it lets the holder
/// go; here the `pane.exited` event is appended, in a transaction of its own.
pub(crate) fn apply_report(mut parts: Parts<'_>, pane: PaneId, report: Report) {
    let applied = parts.state.layout.apply_report(pane, report);
    if !applied.changed {
        return;
    }
    publish_pane(parts.projections, &parts.state.layout, pane);
    // The title of the pane is in the command that goes to it.
    parts.state.publish_palette(parts.projections);
    if let Some(event) = applied.event
        && let Err(error) =
            parts.commit(|tx| tx.append(Some(&event.subject), &event.payload).map(|_| ()))
    {
        tracing::error!(%pane, %error, "the event pane.exited is not stored");
    }
}

/// A pane whose holder is gone starts again with the login shell where it ran last. This is one
/// transaction like a command: the row, the event, the projections, and the start. Returns whether
/// the pane is in the layout and was recreated. A pane that is not in the layout (a pane of
/// `debug.spawn_panes` or of a test) is not recreated.
pub(crate) fn recreate(mut parts: Parts<'_>, pane: PaneId) -> bool {
    let old = parts.state.layout.clone();
    let mut next = old.clone();
    let Some(event) = next.recreate_pane(pane) else {
        return false;
    };
    let stored = parts.commit(|tx| {
        persist(tx, &old, &next)?;
        tx.append(Some(&event.subject), &event.payload).map(|_| ())
    });
    if let Err(error) = stored {
        tracing::error!(%pane, %error, "the pane is not recreated");
        return false;
    }
    publish(parts.projections, &old, &next);
    parts.state.layout = next;
    parts.state.layout_changed(&old);
    parts.state.publish_palette(parts.projections);
    crate::agents::runs::rebooted(&mut parts, pane);
    true
}

/// A window asked for a size of a pane that has no process yet.
pub(crate) fn note_size(parts: Parts<'_>, pane: PaneId, cols: u16, rows: u16) {
    if parts.state.layout.note_size(pane, cols, rows) {
        publish_pane(parts.projections, &parts.state.layout, pane);
    }
}

/// The size of a pane in the layout, as `(cols, rows)`.
pub(crate) fn size_of(parts: Parts<'_>, pane: PaneId) -> Option<(u16, u16)> {
    parts
        .state
        .layout
        .panes
        .get(&pane)
        .map(|entry| (entry.cols, entry.rows))
}

/// Takes what the host reports about a window (`window.focus_report`): the state goes in the
/// layout and in the `windows` projection. Nothing is stored in the database.
pub(crate) fn apply_window_report(parts: Parts<'_>, window: WindowId, facts: WindowFacts) {
    let layout = &mut parts.state.layout;
    let Some(index) = layout.window_index(window) else {
        return;
    };
    if layout.windows[index].facts == facts {
        return;
    }
    layout.windows[index].facts = facts;
    publish_window(parts.projections, &parts.state.layout, window);
    // A window that has the focus and shows a pane looks at it: its run is seen.
    let facts = &parts.state.layout.windows[index].facts;
    let looked_at = (facts.focused && facts.visible)
        .then_some(facts.active_pane)
        .flatten();
    let mut parts = parts;
    if let Some(pane) = looked_at {
        crate::agents::runs::viewed(&mut parts, pane, window);
    }
    // The focus of the panes settles after the debounce.
    crate::focus::refresh(&mut parts);
}

/// A pane has no process and no holder: its holder did not start, or did not come back.
pub(crate) fn apply_lost(parts: Parts<'_>, pane: PaneId) {
    let Some(entry) = parts.state.layout.panes.get(&pane) else {
        return;
    };
    let report = Report {
        runtime: Runtime::Lost,
        cols: entry.cols,
        rows: entry.rows,
        facts: entry.facts.clone(),
    };
    apply_report(parts, pane, report);
}

macro_rules! handler {
    ($name:ident, $variant:ident, |$layout:ident, $alloc:ident, $p:ident| $body:expr) => {
        fn $name(ctx: &Ctx<'_>, tx: &mut Tx<'_>, call: &Call) -> Result<Handled, RpcError> {
            let Call::$variant($p) = call else {
                return Err(RpcError::internal(concat!("not ", stringify!($variant))));
            };
            run(ctx, tx, |$layout, $alloc| $body)
        }
    };
}

handler!(workspace_create, WorkspaceCreate, |layout, alloc, p| layout
    .create_workspace(p.clone(), alloc));
handler!(workspace_rename, WorkspaceRename, |layout, _alloc, p| {
    layout.rename_workspace(&p.workspace, &p.name).map(empty)
});
handler!(workspace_close, WorkspaceClose, |layout, _alloc, p| layout
    .close_workspace(&p.workspace, p.force)
    .map(empty));
handler!(workspace_move, WorkspaceMove, |layout, _alloc, p| layout
    .move_workspace(&p.workspace, p.group.as_ref(), p.before.as_ref())
    .map(empty));
handler!(group_create, GroupCreate, |layout, alloc, p| layout
    .create_group(&p.name, alloc)
    .map(|d| Done {
        value: GroupCreated { group: d.value },
        events: d.events
    }));
handler!(group_rename, GroupRename, |layout, _alloc, p| layout
    .rename_group(&p.group, &p.name)
    .map(empty));
handler!(group_close, GroupClose, |layout, _alloc, p| layout
    .close_group(&p.group)
    .map(empty));
handler!(group_move, GroupMove, |layout, _alloc, p| layout
    .move_group(&p.group, p.before.as_ref())
    .map(empty));
handler!(tab_create, TabCreate, |layout, alloc, p| layout
    .create_tab(p.clone(), alloc));
handler!(tab_rename, TabRename, |layout, _alloc, p| layout
    .rename_tab(&p.tab, &p.name)
    .map(empty));
handler!(tab_close, TabClose, |layout, _alloc, p| layout
    .close_tab(&p.tab, p.force)
    .map(empty));
handler!(tab_move, TabMove, |layout, _alloc, p| layout
    .move_tab(&p.tab, p.workspace.as_ref(), p.index)
    .map(empty));
handler!(tab_select, TabSelect, |layout, _alloc, p| layout
    .select_tab(&p.tab, p.window)
    .map(empty));
handler!(layout_set_ratios, LayoutSetRatios, |layout, _alloc, p| {
    layout.set_ratios(&p.tab, &p.path, &p.ratios).map(empty)
});
handler!(layout_equalize, LayoutEqualize, |layout, _alloc, p| layout
    .equalize(&p.tab, &p.path)
    .map(empty));
handler!(layout_zoom, LayoutZoom, |layout, _alloc, p| layout
    .zoom(&p.tab, p.pane.as_ref())
    .map(empty));
handler!(pane_split, PaneSplit, |layout, alloc, p| layout
    .split_pane(p.clone(), alloc));
handler!(pane_close, PaneClose, |layout, _alloc, p| layout
    .close_pane(p)
    .map(empty));
handler!(pane_move, PaneMove, |layout, _alloc, p| layout
    .move_pane(p)
    .map(empty));
handler!(pane_float, PaneFloat, |layout, _alloc, p| layout
    .float_pane(&p.pane, p.rect)
    .map(empty));
handler!(pane_unfloat, PaneUnfloat, |layout, _alloc, p| layout
    .unfloat_pane(&p.pane)
    .map(empty));
handler!(
    pane_set_float_rect,
    PaneSetFloatRect,
    |layout, _alloc, p| layout.set_float_rect(&p.pane, p.rect).map(empty)
);
handler!(pane_restart, PaneRestart, |layout, _alloc, p| layout
    .restart_pane(&p.pane, p.command.clone())
    .map(empty));
/// The client that a new window belongs to: the device of the caller, else the
/// default client of the router.
fn client_of(ctx: &Ctx<'_>) -> Option<Ulid> {
    ctx.origin.device.or_else(|| {
        ctx.state
            .host
            .as_ref()
            .and_then(|router| router.default_device())
    })
}

/// `window.open`: the new row gets the device of the client that asked.
fn window_open(ctx: &Ctx<'_>, tx: &mut Tx<'_>, call: &Call) -> Result<Handled, RpcError> {
    let Call::WindowOpen(p) = call else {
        return Err(RpcError::internal("not WindowOpen"));
    };
    let client = client_of(ctx);
    run(ctx, tx, |layout, alloc| {
        let done = layout.open_window(p.clone(), alloc)?;
        layout.set_window_client(done.value.window, client);
        Ok(done)
    })
}

/// `pane.popout`: the window of the pane belongs to the client that asked, as for `window.open`.
fn pane_popout(ctx: &Ctx<'_>, tx: &mut Tx<'_>, call: &Call) -> Result<Handled, RpcError> {
    let Call::PanePopout(p) = call else {
        return Err(RpcError::internal("not PanePopout"));
    };
    let client = client_of(ctx);
    run(ctx, tx, |layout, alloc| {
        let done = layout.popout_pane(&p.pane, alloc)?;
        layout.set_window_client(done.value.window, client);
        Ok(done)
    })
}
/// `window.close`: only the client that owns the row may close it. A caller
/// with no device, for example the CLI, may close any window.
fn window_close(ctx: &Ctx<'_>, tx: &mut Tx<'_>, call: &Call) -> Result<Handled, RpcError> {
    let Call::WindowClose(p) = call else {
        return Err(RpcError::internal("not WindowClose"));
    };
    let owner = ctx
        .state
        .layout
        .window_index(p.window)
        .and_then(|index| ctx.state.layout.windows[index].client);
    if let (Some(device), Some(owner)) = (ctx.origin.device, owner)
        && device != owner
    {
        return Err(RpcError::new(
            ErrorCode::PermissionDenied,
            format!("the window {} belongs to another client", p.window),
        ));
    }
    run(ctx, tx, |layout, _alloc| {
        layout.close_window(p.window).map(empty)
    })
}
handler!(
    window_set_visible_panes,
    WindowSetVisiblePanes,
    |layout, _alloc, p| layout.set_visible_panes(p.window, &p.panes).map(empty)
);
handler!(window_set_bounds, WindowSetBounds, |layout, _alloc, p| {
    layout.set_window_bounds(p.window, p.bounds).map(empty)
});

/// `pane.focus`: the focus of a window is a state in memory. The command checks it on the layout,
/// and the mutation changes the layout and the projections after the transaction. No row is
/// written (`needs_persistence` is false for it).
fn pane_focus(ctx: &Ctx<'_>, _: &mut Tx<'_>, call: &Call) -> Result<Handled, RpcError> {
    let Call::PaneFocus(p) = call else {
        return Err(RpcError::internal("not pane.focus"));
    };
    let pane = ctx.state.layout.focus_target(p.window, &p.pane)?;
    let window = p.window;
    Ok(Handled {
        result: to_json(&Empty {})?,
        mutation: Some(Box::new(move |state, hub| {
            state.layout.set_focus(window, pane);
            publish_window(hub, &state.layout, window);
            if let Some(render) = &state.render {
                render.sync_layout(&state.layout);
            }
            // In the single view the focus chooses the pane that the window draws.
            if state
                .layout
                .window_index(window)
                .is_some_and(|index| state.layout.windows[index].single)
            {
                state.focus.nudge();
            }
        })),
    })
}

/// `phone.window`: the phone window of the device that calls. The core makes the
/// row when the device has none, with the tab that a new window shows. A phone has one row on
/// each core.
fn phone_window(ctx: &Ctx<'_>, tx: &mut Tx<'_>, call: &Call) -> Result<Handled, RpcError> {
    let Call::PhoneWindow(_) = call else {
        return Err(RpcError::internal("not phone.window"));
    };
    let device = match (ctx.origin.scope, ctx.origin.device) {
        (super::Scope::Phone, Some(device)) => device,
        _ => {
            return Err(RpcError::new(
                ErrorCode::PermissionDenied,
                "only a phone calls phone.window, for itself",
            ));
        }
    };
    if let Some(window) = ctx
        .state
        .layout
        .windows
        .iter()
        .find(|w| w.client == Some(device))
    {
        return Ok(Handled::result(to_json(&PhoneWindowResult {
            window: window.id,
        })?));
    }
    run(ctx, tx, |layout, alloc| {
        let opened = layout.open_window(
            WindowOpen {
                panes: Vec::new(),
                tab: None,
                bounds: None,
            },
            alloc,
        )?;
        let window = opened.value.window;
        layout.set_window_client(window, Some(device));
        Ok(Done {
            value: PhoneWindowResult { window },
            events: opened.events,
        })
    })
}

/// `window.set_single`: the single view of a phone is a state in memory, as the
/// focus is. Only the client that owns the row may set it.
fn window_set_single(ctx: &Ctx<'_>, _: &mut Tx<'_>, call: &Call) -> Result<Handled, RpcError> {
    let Call::WindowSetSingle(p) = call else {
        return Err(RpcError::internal("not window.set_single"));
    };
    let Some(index) = ctx.state.layout.window_index(p.window) else {
        return Err(RpcError::new(
            ErrorCode::NotFound,
            format!("no window {}", p.window),
        ));
    };
    if let (Some(device), Some(owner)) = (ctx.origin.device, ctx.state.layout.windows[index].client)
        && device != owner
    {
        return Err(RpcError::new(
            ErrorCode::PermissionDenied,
            format!("the window {} belongs to another client", p.window),
        ));
    }
    let (window, single) = (p.window, p.single);
    Ok(Handled {
        result: to_json(&Empty {})?,
        mutation: Some(Box::new(move |state, hub| {
            state.layout.set_window_single(window, single);
            publish_window(hub, &state.layout, window);
            if let Some(render) = &state.render {
                render.sync_layout(&state.layout);
            }
            // The window draws other panes, so the focus of a pane can change.
            state.focus.nudge();
        })),
    })
}

/// The result of a command that returns nothing.
fn empty(done: Done<()>) -> Done<Empty> {
    Done {
        value: Empty {},
        events: done.events,
    }
}

// --- reads ----------------------------------------------------------------------------------

fn workspace_list(ctx: &Ctx<'_>, _: &mut Tx<'_>, call: &Call) -> Result<Handled, RpcError> {
    let Call::WorkspaceList(p) = call else {
        return Err(RpcError::internal("not workspace.list"));
    };
    let layout = &ctx.state.layout;
    let group = p
        .group
        .as_ref()
        .map(|g| layout.resolve_group(g).map(|i| layout.groups[i].id))
        .transpose()?;
    let runs = &ctx.state.runs;
    let has_agent = |pane| runs.of_pane(pane).is_some_and(|run| run.live());
    Ok(Handled::result(to_json(&WorkspaceListResult {
        workspaces: layout.workspace_summaries(group, &has_agent),
    })?))
}

fn workspace_show(ctx: &Ctx<'_>, _: &mut Tx<'_>, call: &Call) -> Result<Handled, RpcError> {
    let Call::WorkspaceShow(p) = call else {
        return Err(RpcError::internal("not workspace.show"));
    };
    let layout = &ctx.state.layout;
    let index = layout.resolve_workspace(&p.workspace)?;
    Ok(Handled::result(to_json(&WorkspaceShown {
        workspace: layout.workspace_value(&layout.workspaces[index]),
    })?))
}

fn pane_list(ctx: &Ctx<'_>, _: &mut Tx<'_>, call: &Call) -> Result<Handled, RpcError> {
    let Call::PaneList(p) = call else {
        return Err(RpcError::internal("not pane.list"));
    };
    let panes = ctx
        .state
        .layout
        .pane_values(p.workspace.as_ref(), p.tab.as_ref())?;
    Ok(Handled::result(to_json(&PaneListResult { panes })?))
}

fn pane_show(ctx: &Ctx<'_>, _: &mut Tx<'_>, call: &Call) -> Result<Handled, RpcError> {
    let Call::PaneShow(p) = call else {
        return Err(RpcError::internal("not pane.show"));
    };
    let layout = &ctx.state.layout;
    let id = layout.resolve_pane(&p.pane)?;
    Ok(Handled::result(to_json(&PaneShown {
        pane: layout.pane_value(&layout.panes[&id]),
    })?))
}

fn group_list(ctx: &Ctx<'_>, _: &mut Tx<'_>, _: &Call) -> Result<Handled, RpcError> {
    Ok(Handled::result(to_json(&GroupListResult {
        groups: ctx.state.layout.group_summaries(),
    })?))
}

fn group_show(ctx: &Ctx<'_>, _: &mut Tx<'_>, call: &Call) -> Result<Handled, RpcError> {
    let Call::GroupShow(p) = call else {
        return Err(RpcError::internal("not group.show"));
    };
    let layout = &ctx.state.layout;
    let index = layout.resolve_group(&p.group)?;
    let id = layout.groups[index].id;
    let summary = layout
        .group_summaries()
        .into_iter()
        .find(|g| g.id == id)
        .ok_or_else(|| RpcError::internal("a group is gone"))?;
    Ok(Handled::result(to_json(&summary)?))
}

/// The handlers of this module.
pub(crate) fn handlers(table: &mut HandlerTable) {
    table.insert(Method::WorkspaceCreate, workspace_create);
    table.insert(Method::WorkspaceRename, workspace_rename);
    table.insert(Method::WorkspaceClose, workspace_close);
    table.insert(Method::WorkspaceMove, workspace_move);
    table.insert(Method::WorkspaceList, workspace_list);
    table.insert(Method::WorkspaceShow, workspace_show);
    table.insert(Method::GroupCreate, group_create);
    table.insert(Method::GroupRename, group_rename);
    table.insert(Method::GroupClose, group_close);
    table.insert(Method::GroupMove, group_move);
    table.insert(Method::GroupList, group_list);
    table.insert(Method::GroupShow, group_show);
    table.insert(Method::TabCreate, tab_create);
    table.insert(Method::TabRename, tab_rename);
    table.insert(Method::TabClose, tab_close);
    table.insert(Method::TabMove, tab_move);
    table.insert(Method::TabSelect, tab_select);
    table.insert(Method::LayoutSetRatios, layout_set_ratios);
    table.insert(Method::LayoutEqualize, layout_equalize);
    table.insert(Method::LayoutZoom, layout_zoom);
    table.insert(Method::PaneSplit, pane_split);
    table.insert(Method::PaneClose, pane_close);
    table.insert(Method::PaneMove, pane_move);
    table.insert(Method::PaneFloat, pane_float);
    table.insert(Method::PaneUnfloat, pane_unfloat);
    table.insert(Method::PaneSetFloatRect, pane_set_float_rect);
    table.insert(Method::PanePopout, pane_popout);
    table.insert(Method::PaneRestart, pane_restart);
    table.insert(Method::PaneFocus, pane_focus);
    table.insert(Method::PaneList, pane_list);
    table.insert(Method::PaneShow, pane_show);
    table.insert(Method::WindowOpen, window_open);
    table.insert(Method::WindowClose, window_close);
    table.insert(Method::WindowSetVisiblePanes, window_set_visible_panes);
    table.insert(Method::WindowSetBounds, window_set_bounds);
    table.insert(Method::WindowSetSingle, window_set_single);
    table.insert(Method::PhoneWindow, phone_window);
}
