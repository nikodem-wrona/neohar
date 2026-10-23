//! The rows of the agent runs.
//!
//! The core keeps the runs in memory and writes each change here in the same transaction as the
//! event of the change. A start of the core reads the open runs back with [`Store::open_runs`], and
//! the watcher of each pane continues the run whose process it finds again.

use rusqlite::{OptionalExtension, Row, params};
use serde_json::Value;
use ulid::Ulid;

use crate::{Store, StoreError, Tx};

/// A run of Claude Code or Codex. The JSON columns hold the types of the API: `facets` is `Facets`
/// and `capabilities` is `Capabilities`.
#[derive(Clone, Debug, PartialEq)]
pub struct RunRow {
    pub id: Ulid,
    pub pane: Option<Ulid>,
    /// `claude` or `codex`.
    pub harness: String,
    /// The id of the manifest that matched. A wrapper of the user has its own.
    pub manifest: String,
    pub name: Option<String>,
    pub session_id: Option<String>,
    pub previous_session_ids: Vec<String>,
    pub transcript_path: Option<String>,
    /// `bound`, `suggested` or `unlinked`.
    pub binding: String,
    pub binding_confidence: f64,
    /// `tui` or `headless`.
    pub launch_mode: String,
    pub version: Option<String>,
    pub capabilities: Value,
    pub facets: Value,
    pub yolo: bool,
    pub resume: Option<Value>,
    pub model: Option<String>,
    /// The process that the run is: its pid and its start time.
    pub process: Option<(i32, u64)>,
    pub started_at: i64,
    pub ended_at: Option<i64>,
}

const COLUMNS: &str = "id, pane_id, harness, manifest, name, harness_session_id,
    previous_session_ids_json, transcript_path, binding, binding_confidence, launch_mode, version,
    capabilities_json, facets_json, yolo, resume_json, model, pid, pid_start, started_at, ended_at";

fn bad(what: &str, error: impl std::fmt::Display) -> StoreError {
    StoreError::BadValue(format!("{what}: {error}"))
}

fn ulid(text: &str, what: &str) -> Result<Ulid, StoreError> {
    text.parse().map_err(|e| bad(what, e))
}

fn from_row(row: &Row<'_>) -> Result<RunRow, StoreError> {
    let text = |index: usize| row.get::<_, Option<String>>(index);
    let pid: Option<i64> = row.get(17)?;
    let start: Option<i64> = row.get(18)?;
    let json = |index: usize, what: &str| -> Result<Value, StoreError> {
        let text: String = row.get(index)?;
        serde_json::from_str(&text).map_err(|e| bad(what, e))
    };
    Ok(RunRow {
        id: ulid(&row.get::<_, String>(0)?, "run id")?,
        pane: text(1)?.map(|t| ulid(&t, "run pane")).transpose()?,
        harness: row.get(2)?,
        manifest: text(3)?.unwrap_or_default(),
        name: text(4)?,
        session_id: text(5)?,
        previous_session_ids: serde_json::from_str(&row.get::<_, String>(6)?)
            .map_err(|e| bad("previous session ids", e))?,
        transcript_path: text(7)?,
        binding: row.get(8)?,
        binding_confidence: row.get(9)?,
        launch_mode: row.get(10)?,
        version: text(11)?,
        capabilities: json(12, "capabilities")?,
        facets: json(13, "facets")?,
        yolo: row.get::<_, i64>(14)? != 0,
        resume: text(15)?
            .map(|t| serde_json::from_str(&t).map_err(|e| bad("resume", e)))
            .transpose()?,
        model: text(16)?,
        process: match (pid, start) {
            (Some(pid), Some(start)) => Some((
                i32::try_from(pid).map_err(|e| bad("run pid", e))?,
                u64::try_from(start).map_err(|e| bad("run start", e))?,
            )),
            _ => None,
        },
        started_at: row.get(19)?,
        ended_at: row.get(20)?,
    })
}

impl Tx<'_> {
    /// Adds a run or changes it.
    pub fn put_run(&self, run: &RunRow) -> Result<(), StoreError> {
        self.writable()?;
        let sessions = serde_json::to_string(&run.previous_session_ids)
            .map_err(|e| bad("previous session ids", e))?;
        self.sql().execute(
            "INSERT INTO agent_runs (id, pane_id, harness, manifest, name, harness_session_id,
                 previous_session_ids_json, transcript_path, binding, binding_confidence,
                 launch_mode, version, capabilities_json, facets_json, yolo, resume_json, model,
                 pid, pid_start, started_at, ended_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17,
                 ?18, ?19, ?20, ?21)
             ON CONFLICT (id) DO UPDATE SET pane_id = excluded.pane_id,
                 manifest = excluded.manifest, name = excluded.name,
                 harness_session_id = excluded.harness_session_id,
                 previous_session_ids_json = excluded.previous_session_ids_json,
                 transcript_path = excluded.transcript_path, binding = excluded.binding,
                 binding_confidence = excluded.binding_confidence, version = excluded.version,
                 capabilities_json = excluded.capabilities_json, facets_json = excluded.facets_json,
                 yolo = excluded.yolo, resume_json = excluded.resume_json, model = excluded.model,
                 pid = excluded.pid, pid_start = excluded.pid_start,
                 ended_at = excluded.ended_at",
            params![
                run.id.to_string(),
                run.pane.map(|p| p.to_string()),
                run.harness,
                run.manifest,
                run.name,
                run.session_id,
                sessions,
                run.transcript_path,
                run.binding,
                run.binding_confidence,
                run.launch_mode,
                run.version,
                run.capabilities.to_string(),
                run.facets.to_string(),
                i64::from(run.yolo),
                run.resume.as_ref().map(Value::to_string),
                run.model,
                run.process.map(|(pid, _)| i64::from(pid)),
                run.process
                    .map(|(_, start)| i64::try_from(start).unwrap_or(i64::MAX)),
                run.started_at,
                run.ended_at,
            ],
        )?;
        Ok(())
    }

    /// Writes the order of the board: each run gets its place in `runs`, counted from 0.
    pub fn put_board_order(&self, runs: &[Ulid]) -> Result<(), StoreError> {
        self.writable()?;
        let mut statement = self
            .sql()
            .prepare("UPDATE agent_runs SET board_rank = ?2 WHERE id = ?1")?;
        for (rank, id) in runs.iter().enumerate() {
            statement.execute(params![
                id.to_string(),
                i64::try_from(rank).unwrap_or(i64::MAX)
            ])?;
        }
        Ok(())
    }
}

impl Store {
    /// The runs that have not ended, in the order of the board: the runs with a place first, then
    /// the others, the oldest first. A start of the core continues them.
    pub fn open_runs(&self) -> Result<Vec<RunRow>, StoreError> {
        let sql = format!(
            "SELECT {COLUMNS} FROM agent_runs WHERE ended_at IS NULL
             ORDER BY board_rank IS NULL, board_rank, started_at, id"
        );
        let mut statement = self.conn.prepare(&sql)?;
        let mut rows = statement.query([])?;
        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            out.push(from_row(row)?);
        }
        Ok(out)
    }

    /// One run, ended or not.
    pub fn run(&self, id: Ulid) -> Result<Option<RunRow>, StoreError> {
        let sql = format!("SELECT {COLUMNS} FROM agent_runs WHERE id = ?1");
        let mut statement = self.conn.prepare(&sql)?;
        statement
            .query_row([id.to_string()], |row| Ok(from_row(row)))
            .optional()?
            .transpose()
    }
}

#[cfg(test)]
mod tests;
