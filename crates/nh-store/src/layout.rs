//! The rows of the layout: groups, workspaces, tabs, the place of the panes,
//! and the windows.
//!
//! The core keeps the layout in memory and writes every change here in the same transaction as
//! the change of the state. A start of the core reads it back with [`Store::load_layout`].
//!
//! - **Order.** A list (the groups, the workspaces of the sidebar, the tabs of a workspace, the
//!   windows) has a `position`. The store keeps it in `sort_key`, a decimal number with leading
//!   zeros, so that the text sorts like the number. The core renumbers a list when it moves an
//!   item. The lists are short.
//! - **Closing.** A closed workspace stays as a row with `closed_at`, and its tabs and panes go.
//!   A closed tab and a closed pane go. A pane takes its snapshots and its scrollback archive with
//!   it (`ON DELETE CASCADE`). Handles do not come back, because the counters stay.
//! - **Panes.** A pane row has a placement (workspace, tab, handle, render id, size, the spec of
//!   what to run) and a runtime (the holder). The layout writes the placement only. A pane that no
//!   holder runs yet has the liveness `created`. The pane runtime fills the rest.

use rusqlite::{OptionalExtension, params};
use serde_json::Value;
use ulid::Ulid;

use crate::{Store, StoreError, Tx};

/// A group of workspaces in the sidebar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GroupRow {
    pub id: Ulid,
    pub name: String,
    pub parent: Option<Ulid>,
    pub position: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceRow {
    pub id: Ulid,
    pub handle: String,
    pub name: String,
    pub root: Option<String>,
    pub machine: String,
    pub group: Option<Ulid>,
    pub position: u32,
    /// The tab that the workspace shows when no window says otherwise. `load_layout` reads it,
    /// and [`Tx::select_tab`] writes it, after the tab exists.
    pub selected_tab: Option<Ulid>,
}

/// A tab with its layout. `layout` and `floating` are the JSON of `LayoutNode` and of the list of
/// `FloatingPane` (`nh-proto::api::model`).
#[derive(Clone, Debug, PartialEq)]
pub struct TabRow {
    pub id: Ulid,
    pub workspace: Ulid,
    pub handle: String,
    pub name: String,
    pub position: u32,
    pub layout: Value,
    pub floating: Value,
    /// The pane that fills the tab.
    pub zoomed: Option<Ulid>,
    /// The tab bar does not show the tab: it holds the agent of a workflow node.
    pub hidden: bool,
}

/// The place of a pane in the layout, and the facts that the layout knows about it.
#[derive(Clone, Debug, PartialEq)]
pub struct PaneSlot {
    pub id: Ulid,
    pub render_id: u32,
    pub workspace: Ulid,
    pub tab: Ulid,
    pub handle: String,
    pub cols: u16,
    pub rows: u16,
    /// What the layout asked to run (`ProcessSpec` as JSON). `None` is the login shell.
    pub spec: Option<Value>,
}

/// A pane as `load_layout` reads it: the slot, and how far the runtime is.
#[derive(Clone, Debug, PartialEq)]
pub struct LoadedPane {
    pub slot: PaneSlot,
    /// `created`, `running`, `exited` or `lost`.
    pub liveness: String,
    /// The working directory of the holder, once it runs.
    pub cwd: Option<String>,
    /// How the child ended, for `exited`: the exit code, or `None`.
    pub exit_code: Option<i32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowRow {
    pub id: Ulid,
    /// `Bounds` as JSON.
    pub bounds: Option<Value>,
    /// The tab that the window shows in each workspace: `{workspace id: tab id}`.
    pub tab_by_workspace: Value,
    /// The ids of the panes that the window draws.
    pub visible_panes: Vec<Ulid>,
    pub focused_pane: Option<Ulid>,
    /// The pane of a window that shows one popped-out pane.
    pub popout_pane: Option<Ulid>,
    pub position: u32,
    /// The device id of the client that owns the window. A row from before migration
    /// 0013 has none.
    pub client: Option<Ulid>,
}

/// Everything `load_layout` reads.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LayoutRows {
    pub groups: Vec<GroupRow>,
    pub workspaces: Vec<WorkspaceRow>,
    pub tabs: Vec<TabRow>,
    pub panes: Vec<LoadedPane>,
    pub windows: Vec<WindowRow>,
}

fn sort_key(position: u32) -> String {
    format!("{position:010}")
}

fn position(key: &str) -> u32 {
    key.parse().unwrap_or(0)
}

fn ulid(text: &str, what: &str) -> Result<Ulid, StoreError> {
    text.parse()
        .map_err(|_| StoreError::BadValue(format!("{what} {text:?} is no ULID")))
}

fn optional_ulid(text: Option<String>, what: &str) -> Result<Option<Ulid>, StoreError> {
    text.map(|t| ulid(&t, what)).transpose()
}

fn json(text: &str, what: &str) -> Result<Value, StoreError> {
    serde_json::from_str(text).map_err(|e| StoreError::BadValue(format!("{what}: {e}")))
}

fn json_text(value: &Value) -> String {
    value.to_string()
}

impl Tx<'_> {
    /// Adds or changes a group.
    pub fn put_group(&self, group: &GroupRow) -> Result<(), StoreError> {
        self.writable()?;
        self.sql().execute(
            "INSERT INTO groups (id, name, parent_id, sort_key, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?5)
             ON CONFLICT (id) DO UPDATE SET name = excluded.name, parent_id = excluded.parent_id,
                 sort_key = excluded.sort_key, updated_at = excluded.updated_at",
            params![
                group.id.to_string(),
                group.name,
                group.parent.map(|p| p.to_string()),
                sort_key(group.position),
                self.now_ms()
            ],
        )?;
        Ok(())
    }

    /// Removes a group. The workspaces in it have no group afterwards.
    pub fn delete_group(&self, id: Ulid) -> Result<(), StoreError> {
        self.writable()?;
        self.sql()
            .execute("DELETE FROM groups WHERE id = ?1", [id.to_string()])?;
        Ok(())
    }

    /// Adds or changes a workspace.
    pub fn put_workspace(&self, workspace: &WorkspaceRow) -> Result<(), StoreError> {
        self.writable()?;
        self.sql().execute(
            "INSERT INTO workspaces (id, handle, name, root_path, machine, group_id, sort_key,
                                     created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)
             ON CONFLICT (id) DO UPDATE SET name = excluded.name, root_path = excluded.root_path,
                 machine = excluded.machine, group_id = excluded.group_id,
                 sort_key = excluded.sort_key, updated_at = excluded.updated_at",
            params![
                workspace.id.to_string(),
                workspace.handle,
                workspace.name,
                workspace.root,
                workspace.machine,
                workspace.group.map(|g| g.to_string()),
                sort_key(workspace.position),
                self.now_ms()
            ],
        )?;
        Ok(())
    }

    /// Sets the tab that a workspace shows. The tab must exist, so write it after [`Tx::put_tab`].
    pub fn select_tab(&self, workspace: Ulid, tab: Option<Ulid>) -> Result<(), StoreError> {
        self.writable()?;
        self.sql().execute(
            "UPDATE workspaces SET selected_tab = ?2, updated_at = ?3 WHERE id = ?1",
            params![
                workspace.to_string(),
                tab.map(|t| t.to_string()),
                self.now_ms()
            ],
        )?;
        Ok(())
    }

    /// Closes a workspace: its panes and tabs go, the row stays with `closed_at`.
    pub fn close_workspace(&self, id: Ulid) -> Result<(), StoreError> {
        self.writable()?;
        let id = id.to_string();
        // The panes first: they hold the archive, and the windows that show them go with them.
        self.sql()
            .execute("DELETE FROM panes WHERE workspace_id = ?1", [&id])?;
        self.sql()
            .execute("DELETE FROM tabs WHERE workspace_id = ?1", [&id])?;
        self.sql().execute(
            "UPDATE workspaces SET closed_at = ?2, updated_at = ?2 WHERE id = ?1",
            params![id, self.now_ms()],
        )?;
        Ok(())
    }

    /// Adds or changes a tab.
    pub fn put_tab(&self, tab: &TabRow) -> Result<(), StoreError> {
        self.writable()?;
        self.sql().execute(
            "INSERT INTO tabs (id, workspace_id, handle, name, sort_key, layout_json,
                               floating_json, zoomed_pane, created_at, updated_at, hidden)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?9, ?8, ?8, ?10)
             ON CONFLICT (id) DO UPDATE SET workspace_id = excluded.workspace_id,
                 name = excluded.name, sort_key = excluded.sort_key,
                 layout_json = excluded.layout_json, floating_json = excluded.floating_json,
                 zoomed_pane = excluded.zoomed_pane, updated_at = excluded.updated_at,
                 hidden = excluded.hidden",
            params![
                tab.id.to_string(),
                tab.workspace.to_string(),
                tab.handle,
                tab.name,
                sort_key(tab.position),
                json_text(&tab.layout),
                json_text(&tab.floating),
                self.now_ms(),
                tab.zoomed.map(|p| p.to_string()),
                tab.hidden
            ],
        )?;
        Ok(())
    }

    /// Removes a tab and its panes.
    pub fn delete_tab(&self, id: Ulid) -> Result<(), StoreError> {
        self.writable()?;
        let id = id.to_string();
        self.sql()
            .execute("DELETE FROM panes WHERE tab_id = ?1", [&id])?;
        self.sql()
            .execute("DELETE FROM tabs WHERE id = ?1", [&id])?;
        Ok(())
    }

    /// Adds a pane that no holder runs yet, or changes the placement of a pane. The runtime
    /// columns of an existing pane stay as they are.
    pub fn put_pane_slot(&self, slot: &PaneSlot) -> Result<(), StoreError> {
        self.writable()?;
        self.sql().execute(
            "INSERT INTO panes (id, render_id, workspace_id, tab_id, handle, cols, rows, spec_json,
                                liveness, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'created', ?9, ?9)
             ON CONFLICT (id) DO UPDATE SET workspace_id = excluded.workspace_id,
                 tab_id = excluded.tab_id, cols = excluded.cols, rows = excluded.rows,
                 spec_json = excluded.spec_json, updated_at = excluded.updated_at",
            params![
                slot.id.to_string(),
                slot.render_id,
                slot.workspace.to_string(),
                slot.tab.to_string(),
                slot.handle,
                slot.cols,
                slot.rows,
                slot.spec.as_ref().map(json_text),
                self.now_ms()
            ],
        )?;
        Ok(())
    }

    /// Takes the holder away from a pane that the layout starts again (`pane.restart`): the pane
    /// goes back to `created`, and the snapshots of the old holder go, because the journal of the
    /// new holder starts at offset 0. The archive stays: the new run numbers its lines after it.
    pub fn reset_pane(&self, id: Ulid) -> Result<(), StoreError> {
        self.writable()?;
        let id = id.to_string();
        self.sql().execute(
            "UPDATE panes SET liveness = 'created', socket = NULL, holder_pid = NULL, secret = NULL,
                              fencing_token = NULL, cwd = NULL, exit_kind = NULL, exit_value = NULL,
                              pane_token = NULL, updated_at = ?2
             WHERE id = ?1",
            params![id, self.now_ms()],
        )?;
        self.sql()
            .execute("DELETE FROM snapshots WHERE pane = ?1", [&id])?;
        Ok(())
    }

    /// Removes a pane, with its snapshots and its archive.
    pub fn delete_pane(&self, id: Ulid) -> Result<(), StoreError> {
        self.writable()?;
        self.sql()
            .execute("DELETE FROM panes WHERE id = ?1", [id.to_string()])?;
        Ok(())
    }

    /// Adds or changes a window.
    pub fn put_window(&self, window: &WindowRow) -> Result<(), StoreError> {
        self.writable()?;
        let visible: Vec<String> = window.visible_panes.iter().map(Ulid::to_string).collect();
        self.sql().execute(
            "INSERT INTO windows (id, bounds_json, tab_by_workspace_json, visible_panes_json,
                                  focused_pane, popout_pane, sort_key, open, created_at, updated_at,
                                  client)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1, ?8, ?8, ?9)
             ON CONFLICT (id) DO UPDATE SET bounds_json = excluded.bounds_json,
                 tab_by_workspace_json = excluded.tab_by_workspace_json,
                 visible_panes_json = excluded.visible_panes_json,
                 focused_pane = excluded.focused_pane, popout_pane = excluded.popout_pane,
                 sort_key = excluded.sort_key, updated_at = excluded.updated_at,
                 client = excluded.client",
            params![
                window.id.to_string(),
                window.bounds.as_ref().map(json_text),
                json_text(&window.tab_by_workspace),
                serde_json::to_string(&visible).unwrap_or_else(|_| "[]".to_owned()),
                window.focused_pane.map(|p| p.to_string()),
                window.popout_pane.map(|p| p.to_string()),
                sort_key(window.position),
                self.now_ms(),
                window.client.map(|c| c.to_string())
            ],
        )?;
        Ok(())
    }

    /// Removes a window.
    pub fn delete_window(&self, id: Ulid) -> Result<(), StoreError> {
        self.writable()?;
        self.sql()
            .execute("DELETE FROM windows WHERE id = ?1", [id.to_string()])?;
        Ok(())
    }
}

impl Store {
    /// Reads the whole layout: the open workspaces with their tabs and panes, the groups and the
    /// windows. Every list is in its order.
    pub fn load_layout(&self) -> Result<LayoutRows, StoreError> {
        let mut rows = LayoutRows::default();

        let mut statement = self
            .conn
            .prepare("SELECT id, name, parent_id, sort_key FROM groups ORDER BY sort_key, id")?;
        let mut found = statement.query([])?;
        while let Some(row) = found.next()? {
            rows.groups.push(GroupRow {
                id: ulid(&row.get::<_, String>(0)?, "group id")?,
                name: row.get(1)?,
                parent: optional_ulid(row.get(2)?, "group parent")?,
                position: position(&row.get::<_, String>(3)?),
            });
        }

        let mut statement = self.conn.prepare(
            "SELECT id, handle, name, root_path, machine, group_id, sort_key, selected_tab
             FROM workspaces WHERE closed_at IS NULL ORDER BY sort_key, id",
        )?;
        let mut found = statement.query([])?;
        while let Some(row) = found.next()? {
            rows.workspaces.push(WorkspaceRow {
                id: ulid(&row.get::<_, String>(0)?, "workspace id")?,
                handle: row.get(1)?,
                name: row.get(2)?,
                root: row.get(3)?,
                machine: row.get(4)?,
                group: optional_ulid(row.get(5)?, "workspace group")?,
                position: position(&row.get::<_, String>(6)?),
                selected_tab: optional_ulid(row.get(7)?, "selected tab")?,
            });
        }

        let mut statement = self.conn.prepare(
            "SELECT t.id, t.workspace_id, t.handle, t.name, t.sort_key, t.layout_json,
                    t.floating_json, t.zoomed_pane, t.hidden
             FROM tabs t JOIN workspaces w ON w.id = t.workspace_id
             WHERE w.closed_at IS NULL ORDER BY t.workspace_id, t.sort_key, t.id",
        )?;
        let mut found = statement.query([])?;
        while let Some(row) = found.next()? {
            rows.tabs.push(TabRow {
                id: ulid(&row.get::<_, String>(0)?, "tab id")?,
                workspace: ulid(&row.get::<_, String>(1)?, "tab workspace")?,
                handle: row.get(2)?,
                name: row.get(3)?,
                position: position(&row.get::<_, String>(4)?),
                layout: json(&row.get::<_, String>(5)?, "tab layout")?,
                floating: json(&row.get::<_, String>(6)?, "tab floating panes")?,
                zoomed: optional_ulid(row.get(7)?, "zoomed pane")?,
                hidden: row.get(8)?,
            });
        }

        let mut statement = self.conn.prepare(
            "SELECT id, render_id, workspace_id, tab_id, handle, cols, rows, spec_json, liveness,
                    cwd, exit_kind, exit_value
             FROM panes WHERE workspace_id IS NOT NULL AND tab_id IS NOT NULL
                          AND handle IS NOT NULL
             ORDER BY render_id",
        )?;
        let mut found = statement.query([])?;
        while let Some(row) = found.next()? {
            let spec: Option<String> = row.get(7)?;
            let exit_kind: Option<String> = row.get(10)?;
            let exit_value: Option<i64> = row.get(11)?;
            rows.panes.push(LoadedPane {
                slot: PaneSlot {
                    id: ulid(&row.get::<_, String>(0)?, "pane id")?,
                    render_id: row.get(1)?,
                    workspace: ulid(&row.get::<_, String>(2)?, "pane workspace")?,
                    tab: ulid(&row.get::<_, String>(3)?, "pane tab")?,
                    handle: row.get(4)?,
                    cols: row.get(5)?,
                    rows: row.get(6)?,
                    spec: spec.map(|s| json(&s, "pane spec")).transpose()?,
                },
                liveness: row.get(8)?,
                cwd: row.get(9)?,
                exit_code: match (exit_kind.as_deref(), exit_value) {
                    (Some("code"), Some(code)) => i32::try_from(code).ok(),
                    _ => None,
                },
            });
        }

        let mut statement = self.conn.prepare(
            "SELECT id, bounds_json, tab_by_workspace_json, visible_panes_json, focused_pane,
                    popout_pane, sort_key, client
             FROM windows WHERE open = 1 ORDER BY sort_key, id",
        )?;
        let mut found = statement.query([])?;
        while let Some(row) = found.next()? {
            let bounds: Option<String> = row.get(1)?;
            let visible: Vec<String> = serde_json::from_str(&row.get::<_, String>(3)?)
                .map_err(|e| StoreError::BadValue(format!("window visible panes: {e}")))?;
            rows.windows.push(WindowRow {
                id: ulid(&row.get::<_, String>(0)?, "window id")?,
                bounds: bounds.map(|b| json(&b, "window bounds")).transpose()?,
                tab_by_workspace: json(&row.get::<_, String>(2)?, "window tabs")?,
                visible_panes: visible
                    .iter()
                    .map(|p| ulid(p, "visible pane"))
                    .collect::<Result<_, _>>()?,
                focused_pane: optional_ulid(row.get(4)?, "focused pane")?,
                popout_pane: optional_ulid(row.get(5)?, "popout pane")?,
                position: position(&row.get::<_, String>(6)?),
                client: optional_ulid(row.get(7)?, "window client")?,
            });
        }
        Ok(rows)
    }

    /// Whether a workspace row exists and is open.
    pub fn workspace_is_open(&self, id: Ulid) -> Result<bool, StoreError> {
        let found: Option<i64> = self
            .conn
            .query_row(
                "SELECT 1 FROM workspaces WHERE id = ?1 AND closed_at IS NULL",
                [id.to_string()],
                |row| row.get(0),
            )
            .optional()?;
        Ok(found.is_some())
    }
}

#[cfg(test)]
mod tests;
