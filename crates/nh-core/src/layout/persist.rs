//! Writing the difference between two layouts to the database.
//!
//! A command runs on a copy of the layout. The handler then calls [`persist`] with the layout
//! before and the copy after, inside the transaction of the command. Only the rows that differ are
//! written. The order of the statements keeps the foreign keys whole: deletions first (panes,
//! tabs, workspaces, groups), then writes (groups, workspaces, tabs, panes), and the selected tab
//! last, because it points to a tab.

use std::collections::BTreeMap;

use nh_store::{GroupRow, PaneSlot, StoreError, TabRow, Tx, WindowRow, WorkspaceRow};
use serde_json::Value;
use ulid::Ulid;

use super::model::{Layout, PaneEntry, Runtime};

fn json<T: serde::Serialize>(value: &T) -> Value {
    serde_json::to_value(value).unwrap_or(Value::Null)
}

fn position(index: usize) -> u32 {
    u32::try_from(index + 1).unwrap_or(u32::MAX)
}

/// The rows of a layout, by id, for the comparison.
struct Rows {
    groups: BTreeMap<Ulid, GroupRow>,
    workspaces: BTreeMap<Ulid, WorkspaceRow>,
    tabs: BTreeMap<Ulid, TabRow>,
    panes: BTreeMap<Ulid, PaneSlot>,
    windows: BTreeMap<Ulid, WindowRow>,
}

fn slot(pane: &PaneEntry) -> PaneSlot {
    PaneSlot {
        id: pane.id.ulid(),
        render_id: pane.render_id,
        workspace: pane.workspace.ulid(),
        tab: pane.tab.ulid(),
        handle: pane.handle.clone(),
        cols: pane.cols,
        rows: pane.rows,
        spec: pane.spec.as_ref().map(json),
    }
}

fn rows(layout: &Layout) -> Rows {
    let mut out = Rows {
        groups: BTreeMap::new(),
        workspaces: BTreeMap::new(),
        tabs: BTreeMap::new(),
        panes: BTreeMap::new(),
        windows: BTreeMap::new(),
    };
    for (i, g) in layout.groups.iter().enumerate() {
        out.groups.insert(
            g.id.ulid(),
            GroupRow {
                id: g.id.ulid(),
                name: g.name.clone(),
                parent: g.parent.map(|p| p.ulid()),
                position: position(i),
            },
        );
    }
    for (i, w) in layout.workspaces.iter().enumerate() {
        out.workspaces.insert(
            w.id.ulid(),
            WorkspaceRow {
                id: w.id.ulid(),
                handle: w.handle.clone(),
                name: w.name.clone(),
                root: w.root.clone(),
                machine: w.machine.clone(),
                group: w.group.map(|g| g.ulid()),
                position: position(i),
                selected_tab: w.selected_tab.map(|t| t.ulid()),
            },
        );
        for (j, t) in w.tabs.iter().enumerate() {
            out.tabs.insert(
                t.id.ulid(),
                TabRow {
                    id: t.id.ulid(),
                    workspace: w.id.ulid(),
                    handle: t.handle.clone(),
                    name: t.name.clone(),
                    position: position(j),
                    layout: json(&t.layout),
                    floating: json(&t.floating),
                    zoomed: t.zoomed.map(|p| p.ulid()),
                    hidden: t.hidden,
                },
            );
        }
    }
    for p in layout.panes.values() {
        out.panes.insert(p.id.ulid(), slot(p));
    }
    for (i, w) in layout.windows.iter().enumerate() {
        let by_workspace: serde_json::Map<String, Value> = w
            .by_workspace
            .iter()
            .map(|(ws, tab)| (ws.to_string(), Value::String(tab.to_string())))
            .collect();
        out.windows.insert(
            w.id.ulid(),
            WindowRow {
                id: w.id.ulid(),
                bounds: w.bounds.as_ref().map(json),
                // What the window shows: the tab, the tab of each workspace, and a list of panes.
                tab_by_workspace: serde_json::json!({
                    "tab": w.tab.map(|t| t.to_string()),
                    "by_workspace": by_workspace,
                    "explicit": w.explicit.as_ref().map(|list| list.iter().map(ToString::to_string).collect::<Vec<_>>()),
                }),
                visible_panes: layout.visible_panes(w).iter().map(|p| p.ulid()).collect(),
                focused_pane: w.focused_pane.map(|p| p.ulid()),
                popout_pane: w.popout.map(|p| p.ulid()),
                position: position(i),
                client: w.client,
            },
        );
    }
    out
}

/// Writes what changed from `old` to `new`.
pub fn persist(tx: &Tx<'_>, old: &Layout, new: &Layout) -> Result<(), StoreError> {
    let (before, after) = (rows(old), rows(new));

    // Windows go first: a window that shows a pane which closes must not depend on the row.
    for id in before
        .windows
        .keys()
        .filter(|id| !after.windows.contains_key(id))
    {
        tx.delete_window(*id)?;
    }
    // Deletions. A closed workspace takes its tabs and panes with it, and a closed tab its panes,
    // so a pane is deleted alone only when its tab and its workspace stay.
    for (id, pane) in before
        .panes
        .iter()
        .filter(|(id, _)| !after.panes.contains_key(id))
    {
        let tab_goes = !after.tabs.contains_key(&pane.tab);
        let workspace_goes = !after.workspaces.contains_key(&pane.workspace);
        if !tab_goes && !workspace_goes {
            tx.delete_pane(*id)?;
        }
    }
    for id in before.tabs.keys().filter(|id| !after.tabs.contains_key(id)) {
        if after.workspaces.contains_key(&before.tabs[id].workspace) {
            tx.delete_tab(*id)?;
        }
    }
    for id in before
        .workspaces
        .keys()
        .filter(|id| !after.workspaces.contains_key(id))
    {
        tx.close_workspace(*id)?;
    }
    for id in before
        .groups
        .keys()
        .filter(|id| !after.groups.contains_key(id))
    {
        tx.delete_group(*id)?;
    }

    // Writes.
    for (id, group) in &after.groups {
        if before.groups.get(id) != Some(group) {
            tx.put_group(group)?;
        }
    }
    for (id, workspace) in &after.workspaces {
        if before.workspaces.get(id) != Some(workspace) {
            tx.put_workspace(workspace)?;
        }
    }
    for (id, tab) in &after.tabs {
        if before.tabs.get(id) != Some(tab) {
            tx.put_tab(tab)?;
        }
    }
    for (id, pane) in &after.panes {
        if before.panes.get(id) != Some(pane) {
            tx.put_pane_slot(pane)?;
        }
    }
    // A pane that starts again loses the holder of its last run.
    for (id, pane) in &new.panes {
        let restarted = pane.runtime == Runtime::Created
            && old
                .panes
                .get(id)
                .is_some_and(|before| before.runtime != Runtime::Created);
        if restarted {
            tx.reset_pane(id.ulid())?;
        }
    }
    // The windows point to panes and tabs, which exist now.
    for (id, window) in &after.windows {
        if before.windows.get(id) != Some(window) {
            tx.put_window(window)?;
        }
    }
    // The selected tab last: it points to a tab, which exists now.
    for (id, workspace) in &after.workspaces {
        let was = before.workspaces.get(id).map(|w| w.selected_tab);
        if was != Some(workspace.selected_tab) {
            tx.select_tab(*id, workspace.selected_tab)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
