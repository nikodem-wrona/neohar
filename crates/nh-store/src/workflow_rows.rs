//! The executions of workflows: `workflow_executions` and `workflow_nodes`. The state texts
//! (`running`, `done`, ...) and the graph text are what the core writes; the store does not read
//! them.

use rusqlite::{Row, params};

use crate::{Store, StoreError, Tx};

/// One execution of a workflow.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecutionRow {
    /// The execution id, a ULID.
    pub id: String,
    /// The name of the workflow file without `.toml`.
    pub workflow: String,
    /// The name of the workflow at the start.
    pub name: String,
    pub number: u32,
    pub folder: String,
    pub branch: Option<String>,
    pub base: Option<String>,
    pub head: Option<String>,
    /// A hash of the `HEAD` commit and of the changes of the worktree at the start.
    pub tree: Option<String>,
    pub notes: Option<String>,
    /// The TOML text of the workflow at the start.
    pub graph: String,
    /// The node of "Execute this node".
    pub only_node: Option<String>,
    /// The workspace of the hidden tabs of the agents.
    pub workspace: Option<String>,
    /// The folder of the reports.
    pub dir: String,
    pub state: String,
    pub tree_changed: bool,
    pub started_at: i64,
    pub ended_at: Option<i64>,
}

/// One node of an execution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeRow {
    pub execution: String,
    pub node: String,
    pub state: String,
    pub reason: Option<String>,
    /// The agent run of the node, a ULID.
    pub run: Option<String>,
    pub verdict: Option<String>,
    /// The absolute path of the report.
    pub report: Option<String>,
    /// The ends of turns of the run that the core saw.
    pub turns: u32,
    pub started_at: Option<i64>,
    pub ended_at: Option<i64>,
}

const EXECUTION_COLUMNS: &str = "id, workflow, name, number, folder, branch, base, head, tree, \
     notes, graph, only_node, workspace, dir, state, tree_changed, started_at, ended_at";

const NODE_COLUMNS: &str =
    "execution, node, state, reason, run, verdict, report, turns, started_at, ended_at";

fn execution_of(row: &Row<'_>) -> Result<ExecutionRow, StoreError> {
    Ok(ExecutionRow {
        id: row.get(0)?,
        workflow: row.get(1)?,
        name: row.get(2)?,
        number: row.get(3)?,
        folder: row.get(4)?,
        branch: row.get(5)?,
        base: row.get(6)?,
        head: row.get(7)?,
        tree: row.get(8)?,
        notes: row.get(9)?,
        graph: row.get(10)?,
        only_node: row.get(11)?,
        workspace: row.get(12)?,
        dir: row.get(13)?,
        state: row.get(14)?,
        tree_changed: row.get(15)?,
        started_at: row.get(16)?,
        ended_at: row.get(17)?,
    })
}

fn node_of(row: &Row<'_>) -> Result<NodeRow, StoreError> {
    Ok(NodeRow {
        execution: row.get(0)?,
        node: row.get(1)?,
        state: row.get(2)?,
        reason: row.get(3)?,
        run: row.get(4)?,
        verdict: row.get(5)?,
        report: row.get(6)?,
        turns: row.get(7)?,
        started_at: row.get(8)?,
        ended_at: row.get(9)?,
    })
}

/// The executions, the oldest first, with their nodes.
fn read_executions(
    conn: &rusqlite::Connection,
) -> Result<Vec<(ExecutionRow, Vec<NodeRow>)>, StoreError> {
    let mut statement = conn.prepare(&format!(
        "SELECT {EXECUTION_COLUMNS} FROM workflow_executions ORDER BY started_at, id"
    ))?;
    let mut rows = statement.query([])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push((execution_of(row)?, Vec::new()));
    }
    let mut statement = conn.prepare(&format!(
        "SELECT {NODE_COLUMNS} FROM workflow_nodes ORDER BY execution, rowid"
    ))?;
    let mut rows = statement.query([])?;
    while let Some(row) = rows.next()? {
        let node = node_of(row)?;
        if let Some((_, nodes)) = out.iter_mut().find(|(e, _)| e.id == node.execution) {
            nodes.push(node);
        }
    }
    Ok(out)
}

impl Tx<'_> {
    /// Adds an execution or changes it.
    pub fn put_execution(&self, row: &ExecutionRow) -> Result<(), StoreError> {
        self.writable()?;
        self.sql().execute(
            &format!(
                "INSERT INTO workflow_executions ({EXECUTION_COLUMNS})
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16,
                         ?17, ?18)
                 ON CONFLICT (id) DO UPDATE SET state = excluded.state,
                     tree_changed = excluded.tree_changed, ended_at = excluded.ended_at,
                     workspace = excluded.workspace"
            ),
            params![
                row.id,
                row.workflow,
                row.name,
                row.number,
                row.folder,
                row.branch,
                row.base,
                row.head,
                row.tree,
                row.notes,
                row.graph,
                row.only_node,
                row.workspace,
                row.dir,
                row.state,
                row.tree_changed,
                row.started_at,
                row.ended_at,
            ],
        )?;
        Ok(())
    }

    /// Adds a node of an execution or changes it.
    pub fn put_execution_node(&self, row: &NodeRow) -> Result<(), StoreError> {
        self.writable()?;
        self.sql().execute(
            &format!(
                "INSERT INTO workflow_nodes ({NODE_COLUMNS})
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                 ON CONFLICT (execution, node) DO UPDATE SET state = excluded.state,
                     reason = excluded.reason, run = excluded.run, verdict = excluded.verdict,
                     report = excluded.report, turns = excluded.turns,
                     started_at = excluded.started_at, ended_at = excluded.ended_at"
            ),
            params![
                row.execution,
                row.node,
                row.state,
                row.reason,
                row.run,
                row.verdict,
                row.report,
                row.turns,
                row.started_at,
                row.ended_at,
            ],
        )?;
        Ok(())
    }

    /// Removes an execution and its nodes. Says whether it existed.
    pub fn delete_execution(&self, id: &str) -> Result<bool, StoreError> {
        self.writable()?;
        self.sql()
            .execute("DELETE FROM workflow_nodes WHERE execution = ?1", [id])?;
        let removed = self
            .sql()
            .execute("DELETE FROM workflow_executions WHERE id = ?1", [id])?;
        Ok(removed > 0)
    }

    /// The executions with their nodes, the oldest first.
    pub fn executions(&self) -> Result<Vec<(ExecutionRow, Vec<NodeRow>)>, StoreError> {
        read_executions(self.sql())
    }
}

impl Store {
    /// The executions with their nodes, the oldest first.
    pub fn executions(&self) -> Result<Vec<(ExecutionRow, Vec<NodeRow>)>, StoreError> {
        read_executions(&self.conn)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn execution(id: &str, started_at: i64) -> ExecutionRow {
        ExecutionRow {
            id: id.to_owned(),
            workflow: "review".to_owned(),
            name: "Review".to_owned(),
            number: 1,
            folder: "/w/app".to_owned(),
            branch: Some("main".to_owned()),
            base: None,
            head: None,
            tree: None,
            notes: None,
            graph: "name = \"Review\"".to_owned(),
            only_node: None,
            workspace: None,
            dir: "/d/1".to_owned(),
            state: "running".to_owned(),
            tree_changed: false,
            started_at,
            ended_at: None,
        }
    }

    fn node(execution: &str, node: &str) -> NodeRow {
        NodeRow {
            execution: execution.to_owned(),
            node: node.to_owned(),
            state: "waiting".to_owned(),
            reason: None,
            run: None,
            verdict: None,
            report: None,
            turns: 0,
            started_at: None,
            ended_at: None,
        }
    }

    #[test]
    fn an_execution_and_its_nodes_come_back_in_order_and_go_together() {
        let mut store = Store::open_in_memory().expect("store");
        store
            .tx(1, |tx| {
                tx.put_execution(&execution("b", 20))?;
                tx.put_execution(&execution("a", 10))?;
                tx.put_execution_node(&node("a", "start"))?;
                tx.put_execution_node(&node("a", "code"))?;
                tx.put_execution_node(&node("b", "start"))
            })
            .expect("tx");
        store
            .tx(2, |tx| {
                let mut done = execution("a", 10);
                done.state = "done".to_owned();
                done.ended_at = Some(30);
                tx.put_execution(&done)?;
                let mut code = node("a", "code");
                code.state = "done".to_owned();
                code.turns = 2;
                tx.put_execution_node(&code)
            })
            .expect("tx");
        let all = store.executions().expect("read");
        assert_eq!(
            all.iter().map(|(e, _)| e.id.as_str()).collect::<Vec<_>>(),
            ["a", "b"]
        );
        let (first, nodes) = &all[0];
        assert_eq!(first.state, "done");
        assert_eq!(first.ended_at, Some(30));
        assert_eq!(
            nodes.iter().map(|n| n.node.as_str()).collect::<Vec<_>>(),
            ["start", "code"]
        );
        assert_eq!(nodes[1].turns, 2);
        let (removed, _) = store.tx(3, |tx| tx.delete_execution("a")).expect("tx");
        assert!(removed);
        let left = store.executions().expect("read");
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].1.len(), 1);
    }
}
