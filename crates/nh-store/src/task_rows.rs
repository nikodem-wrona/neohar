//! The rows of the task workspaces: `tasks` and `port_leases`. A task can be a
//! draft with its files.

use rusqlite::{Row, params};
use ulid::Ulid;

use crate::{Store, StoreError, Tx};

/// A task.
#[derive(Clone, Debug, PartialEq)]
pub struct TaskRow {
    pub id: Ulid,
    /// `k7`.
    pub handle: String,
    pub title: String,
    pub slug: String,
    pub workspace: Option<Ulid>,
    pub tab: Option<Ulid>,
    pub repo_root: String,
    pub worktree_path: String,
    pub branch: String,
    pub base: String,
    /// `draft`, `creating`, `setup`, `running`, `idle`, `setup_failed` or `closed`.
    pub status: String,
    /// The variables that the panes of the task get, as a JSON object.
    pub env: serde_json::Value,
    /// The blob with the output of the setup commands.
    pub setup_blob: Option<String>,
    /// The agent that the start runs: `claude` or `codex`.
    pub harness: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub prompt: Option<String>,
    /// How many ports the start leases. `None` is the number of the settings.
    pub ports: Option<u8>,
    pub attachments: Vec<AttachmentRow>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// A file of a task. Its bytes are a blob, and the row counts as one reference of it.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AttachmentRow {
    pub id: String,
    pub name: String,
    pub mime: String,
    pub bytes: u64,
    pub blob: String,
    /// The file in the worktree, after the start.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

const SELECT: &str = "SELECT id, handle, title, slug, workspace_id, tab_id, repo_root,
        worktree_path, branch, base, status, env_json, setup_blob, harness, model, effort, prompt,
        ports, attachments_json, created_at, updated_at
    FROM tasks";

fn ulid(text: String, what: &str) -> Result<Ulid, StoreError> {
    text.parse()
        .map_err(|_| StoreError::BadValue(format!("{what}: {text}")))
}

fn from_row(row: &Row<'_>) -> Result<TaskRow, StoreError> {
    let opt = |index: usize, what: &str| -> Result<Option<Ulid>, StoreError> {
        row.get::<_, Option<String>>(index)?
            .map(|text| ulid(text, what))
            .transpose()
    };
    let env: String = row.get(11)?;
    let attachments: String = row.get(18)?;
    let ports: Option<i64> = row.get(17)?;
    Ok(TaskRow {
        id: ulid(row.get(0)?, "task id")?,
        handle: row.get(1)?,
        title: row.get(2)?,
        slug: row.get(3)?,
        workspace: opt(4, "task workspace")?,
        tab: opt(5, "task tab")?,
        repo_root: row.get(6)?,
        worktree_path: row.get(7)?,
        branch: row.get(8)?,
        base: row.get(9)?,
        status: row.get(10)?,
        env: serde_json::from_str(&env)
            .map_err(|e| StoreError::BadValue(format!("task env: {e}")))?,
        setup_blob: row.get(12)?,
        harness: row.get(13)?,
        model: row.get(14)?,
        effort: row.get(15)?,
        prompt: row.get(16)?,
        ports: ports
            .map(|n| u8::try_from(n).map_err(|_| StoreError::BadValue(format!("task ports: {n}"))))
            .transpose()?,
        attachments: serde_json::from_str(&attachments)
            .map_err(|e| StoreError::BadValue(format!("task attachments: {e}")))?,
        created_at: row.get(19)?,
        updated_at: row.get(20)?,
    })
}

fn read_tasks(conn: &rusqlite::Connection) -> Result<Vec<TaskRow>, StoreError> {
    let mut statement = conn.prepare(&format!("{SELECT} ORDER BY created_at, id"))?;
    let mut rows = statement.query([])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(from_row(row)?);
    }
    Ok(out)
}

fn read_leases(conn: &rusqlite::Connection) -> Result<Vec<(u16, Ulid)>, StoreError> {
    let mut statement = conn.prepare("SELECT port, task_id FROM port_leases ORDER BY port")?;
    let mut rows = statement.query([])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        let port: i64 = row.get(0)?;
        out.push((
            u16::try_from(port).map_err(|_| StoreError::BadValue(format!("port {port}")))?,
            ulid(row.get(1)?, "lease task")?,
        ));
    }
    Ok(out)
}

impl Tx<'_> {
    /// Adds a task, or changes the one with this id. The handle and the time of the creation stay.
    pub fn put_task(&self, row: &TaskRow) -> Result<(), StoreError> {
        self.writable()?;
        let attachments = serde_json::to_string(&row.attachments)
            .map_err(|e| StoreError::BadValue(format!("task attachments: {e}")))?;
        self.sql().execute(
            "INSERT INTO tasks (id, handle, title, slug, workspace_id, tab_id, repo_root,
                 worktree_path, branch, base, status, env_json, setup_blob, harness, model, effort,
                 prompt, ports, attachments_json, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17,
                 ?18, ?19, ?20, ?21)
             ON CONFLICT (id) DO UPDATE SET title = excluded.title, slug = excluded.slug,
                 workspace_id = excluded.workspace_id, tab_id = excluded.tab_id,
                 repo_root = excluded.repo_root, worktree_path = excluded.worktree_path,
                 branch = excluded.branch, base = excluded.base, status = excluded.status,
                 env_json = excluded.env_json, setup_blob = excluded.setup_blob,
                 harness = excluded.harness, model = excluded.model, effort = excluded.effort,
                 prompt = excluded.prompt, ports = excluded.ports,
                 attachments_json = excluded.attachments_json, updated_at = excluded.updated_at",
            params![
                row.id.to_string(),
                row.handle,
                row.title,
                row.slug,
                row.workspace.map(|id| id.to_string()),
                row.tab.map(|id| id.to_string()),
                row.repo_root,
                row.worktree_path,
                row.branch,
                row.base,
                row.status,
                row.env.to_string(),
                row.setup_blob,
                row.harness,
                row.model,
                row.effort,
                row.prompt,
                row.ports,
                attachments,
                row.created_at,
                row.updated_at,
            ],
        )?;
        Ok(())
    }

    /// Removes a task: a draft, or a task that is closed. Its leases go with it. The caller lets
    /// go of the blobs of its files.
    pub fn delete_task(&self, id: Ulid) -> Result<(), StoreError> {
        self.writable()?;
        self.sql()
            .execute("DELETE FROM tasks WHERE id = ?1", [id.to_string()])?;
        Ok(())
    }

    /// Leases ports to a task. A port that is leased already is an error.
    pub fn lease_ports(&self, task: Ulid, ports: &[u16], at: i64) -> Result<(), StoreError> {
        self.writable()?;
        for port in ports {
            self.sql().execute(
                "INSERT INTO port_leases (port, task_id, leased_at) VALUES (?1, ?2, ?3)",
                params![port, task.to_string(), at],
            )?;
        }
        Ok(())
    }

    /// Gives back the ports of a task.
    pub fn release_ports(&self, task: Ulid) -> Result<(), StoreError> {
        self.writable()?;
        self.sql().execute(
            "DELETE FROM port_leases WHERE task_id = ?1",
            [task.to_string()],
        )?;
        Ok(())
    }

    pub fn tasks(&self) -> Result<Vec<TaskRow>, StoreError> {
        read_tasks(self.sql())
    }

    /// The leased ports and the task that holds each.
    pub fn port_leases(&self) -> Result<Vec<(u16, Ulid)>, StoreError> {
        read_leases(self.sql())
    }
}

impl Store {
    pub fn tasks(&self) -> Result<Vec<TaskRow>, StoreError> {
        read_tasks(&self.conn)
    }

    pub fn port_leases(&self) -> Result<Vec<(u16, Ulid)>, StoreError> {
        read_leases(&self.conn)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(n: u128, handle: &str) -> TaskRow {
        TaskRow {
            id: Ulid::from(n),
            handle: handle.to_owned(),
            title: "Fix the login bug".to_owned(),
            slug: "fix-the-login-bug".to_owned(),
            workspace: None,
            tab: None,
            repo_root: "/work/app".to_owned(),
            worktree_path: "/wt/app/fix-the-login-bug".to_owned(),
            branch: "neohar/fix-the-login-bug".to_owned(),
            base: "main".to_owned(),
            status: "creating".to_owned(),
            env: serde_json::json!({"PORT": "41000"}),
            setup_blob: None,
            harness: None,
            model: None,
            effort: None,
            prompt: None,
            ports: None,
            attachments: Vec::new(),
            created_at: 10,
            updated_at: 10,
        }
    }

    #[test]
    fn a_task_is_stored_and_changes_its_status() {
        let mut store = Store::open_in_memory().expect("a store");
        store
            .tx(1, |tx| tx.put_task(&row(1, "k1")))
            .expect("a task");
        let mut changed = row(1, "k1");
        changed.status = "running".to_owned();
        changed.setup_blob = Some("abc".to_owned());
        changed.updated_at = 20;
        store.tx(2, |tx| tx.put_task(&changed)).expect("a change");
        let tasks = store.tasks().expect("tasks");
        assert_eq!(tasks, [changed]);
    }

    #[test]
    fn a_draft_keeps_its_agent_and_files_and_gets_its_worktree_at_the_start() {
        let mut store = Store::open_in_memory().expect("a store");
        let mut draft = row(1, "k1");
        draft.status = "draft".to_owned();
        draft.slug = String::new();
        draft.worktree_path = String::new();
        draft.branch = String::new();
        draft.harness = Some("codex".to_owned());
        draft.model = Some("gpt-5.5".to_owned());
        draft.effort = Some("high".to_owned());
        draft.prompt = Some("look at the screenshot".to_owned());
        draft.ports = Some(2);
        draft.attachments = vec![AttachmentRow {
            id: "a1".to_owned(),
            name: "login.png".to_owned(),
            mime: "image/png".to_owned(),
            bytes: 3,
            blob: "ab".repeat(32),
            path: None,
        }];
        store.tx(1, |tx| tx.put_task(&draft)).expect("a draft");
        assert_eq!(store.tasks().expect("tasks"), [draft.clone()]);
        let mut started = draft;
        started.status = "creating".to_owned();
        started.slug = "fix-the-login-bug".to_owned();
        started.worktree_path = "/wt/app/fix-the-login-bug".to_owned();
        started.branch = "neohar/fix-the-login-bug".to_owned();
        started.attachments[0].path = Some("/wt/app/fix-the-login-bug/.neohar/login.png".into());
        store.tx(2, |tx| tx.put_task(&started)).expect("a start");
        assert_eq!(store.tasks().expect("tasks"), [started]);
    }

    #[test]
    fn leases_are_unique_and_go_with_their_task() {
        let mut store = Store::open_in_memory().expect("a store");
        store
            .tx(1, |tx| {
                tx.put_task(&row(1, "k1"))?;
                tx.put_task(&row(2, "k2"))?;
                tx.lease_ports(Ulid::from(1u128), &[41_000, 41_001], 5)
            })
            .expect("leases");
        let twice = store.tx(2, |tx| tx.lease_ports(Ulid::from(2u128), &[41_001], 6));
        assert!(twice.is_err(), "a port has one task");
        store
            .tx(3, |tx| tx.release_ports(Ulid::from(1u128)))
            .expect("a release");
        assert!(store.port_leases().expect("leases").is_empty());
        // A task that goes takes its leases with it.
        store
            .tx(4, |tx| {
                tx.lease_ports(Ulid::from(2u128), &[41_005], 7)?;
                tx.delete_task(Ulid::from(2u128))
            })
            .expect("a delete");
        assert!(store.port_leases().expect("leases").is_empty());
    }
}
