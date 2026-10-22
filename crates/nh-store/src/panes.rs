//! The panes and their snapshots.

use nh_proto::holder::ExitStatus;
use rusqlite::{OptionalExtension, Row, params};
use ulid::Ulid;

use crate::{Store, StoreError};

/// How many snapshots the store keeps for each pane. The second one is a fallback when the
/// newest does not decode.
pub const SNAPSHOTS_KEPT: usize = 2;

/// Whether the child of a pane runs, as the store records it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaneLiveness {
    /// The core attached to the holder of this pane and has seen no exit.
    Running,
    /// The core saw the child end.
    Exited,
    /// The holder is gone and the child with it: a reboot, a kill of the holder, or a lost socket.
    Lost,
}

impl PaneLiveness {
    fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Exited => "exited",
            Self::Lost => "lost",
        }
    }

    fn parse(text: &str) -> Result<Self, StoreError> {
        match text {
            "running" => Ok(Self::Running),
            "exited" => Ok(Self::Exited),
            "lost" => Ok(Self::Lost),
            other => Err(StoreError::BadValue(format!("pane liveness {other:?}"))),
        }
    }
}

/// A pane as the store keeps it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaneRecord {
    pub id: Ulid,
    /// The number that frames use for the pane. It stays the same across restarts.
    pub render_id: u32,
    /// The path of the socket of the holder.
    pub socket: String,
    /// The pid of the holder. A holder is a daemon, so the pid is the only way to find it again.
    pub holder_pid: i32,
    /// The secret that the holder asks for in `Hello`.
    pub secret: [u8; 32],
    /// The fencing token of the core that attached last.
    pub fencing_token: u64,
    pub argv: Vec<String>,
    pub cwd: String,
    pub cols: u16,
    pub rows: u16,
    pub liveness: PaneLiveness,
    /// How the child ended. Set when `liveness` is [`PaneLiveness::Exited`].
    pub exit: Option<ExitStatus>,
}

/// A snapshot of the VT engine of a pane. The core encodes and decodes `data` (`nh-term`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnapshotRecord {
    pub pane: Ulid,
    /// The holder journal offset that the snapshot was taken at.
    pub offset: u64,
    pub created_at_ms: i64,
    pub data: Vec<u8>,
}

fn exit_columns(exit: Option<ExitStatus>) -> (Option<&'static str>, Option<i32>) {
    match exit {
        None => (None, None),
        Some(ExitStatus::Code(v)) => (Some("code"), Some(v)),
        Some(ExitStatus::Signal(v)) => (Some("signal"), Some(v)),
        Some(ExitStatus::ExecFailed(v)) => (Some("exec_failed"), Some(v)),
    }
}

fn exit_status(kind: Option<String>, value: Option<i32>) -> Result<Option<ExitStatus>, StoreError> {
    match (kind.as_deref(), value) {
        (None, _) => Ok(None),
        (Some("code"), Some(v)) => Ok(Some(ExitStatus::Code(v))),
        (Some("signal"), Some(v)) => Ok(Some(ExitStatus::Signal(v))),
        (Some("exec_failed"), Some(v)) => Ok(Some(ExitStatus::ExecFailed(v))),
        (Some(kind), value) => Err(StoreError::BadValue(format!("exit {kind:?} {value:?}"))),
    }
}

const PANE_COLUMNS: &str = "id, render_id, socket, secret, fencing_token, argv_json, cwd, cols, \
                            rows, liveness, exit_kind, exit_value, holder_pid";

fn pane_from_row(row: &Row<'_>) -> Result<PaneRecord, StoreError> {
    let id: String = row.get(0)?;
    let secret: Vec<u8> = row.get(3)?;
    let argv_json: String = row.get(5)?;
    let liveness: String = row.get(9)?;
    Ok(PaneRecord {
        id: id
            .parse()
            .map_err(|_| StoreError::BadValue(format!("pane id {id:?}")))?,
        render_id: row.get(1)?,
        socket: row.get(2)?,
        secret: secret
            .try_into()
            .map_err(|_| StoreError::BadValue("a secret that is not 32 bytes".to_owned()))?,
        fencing_token: row.get(4)?,
        argv: serde_json::from_str(&argv_json)
            .map_err(|e| StoreError::BadValue(format!("argv of pane {id}: {e}")))?,
        cwd: row.get(6)?,
        cols: row.get(7)?,
        rows: row.get(8)?,
        liveness: PaneLiveness::parse(&liveness)?,
        exit: exit_status(row.get(10)?, row.get(11)?)?,
        holder_pid: row.get(12)?,
    })
}

impl Store {
    /// Adds a pane. The id must be new.
    pub fn insert_pane(&self, pane: &PaneRecord, now_ms: i64) -> Result<(), StoreError> {
        let argv = serde_json::to_string(&pane.argv)
            .map_err(|e| StoreError::BadValue(format!("argv: {e}")))?;
        let (exit_kind, exit_value) = exit_columns(pane.exit);
        self.exec(
            "INSERT INTO panes (id, render_id, socket, secret, fencing_token, argv_json, cwd, \
                                cols, rows, liveness, exit_kind, exit_value, holder_pid, \
                                created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?14)",
            params![
                pane.id.to_string(),
                pane.render_id,
                pane.socket,
                pane.secret.as_slice(),
                pane.fencing_token,
                argv,
                pane.cwd,
                pane.cols,
                pane.rows,
                pane.liveness.as_str(),
                exit_kind,
                exit_value,
                pane.holder_pid,
                now_ms,
            ],
        )?;
        Ok(())
    }

    /// Gives a pane that the layout made its holder: the runtime columns, the state `running` and
    /// the pane token. A restart of a pane that ended does the same, so the end of the old child
    /// is cleared. Returns whether the pane had a row; if not, nothing was written.
    pub fn attach_holder(
        &self,
        pane: &PaneRecord,
        token: Option<&str>,
        now_ms: i64,
    ) -> Result<bool, StoreError> {
        let argv = serde_json::to_string(&pane.argv)
            .map_err(|e| StoreError::BadValue(format!("argv: {e}")))?;
        let changed = self.exec(
            "UPDATE panes SET socket = ?2, holder_pid = ?3, secret = ?4, fencing_token = ?5, \
                              argv_json = ?6, cwd = ?7, cols = ?8, rows = ?9, liveness = 'running', \
                              exit_kind = NULL, exit_value = NULL, pane_token = ?10, updated_at = ?11 \
             WHERE id = ?1",
            params![
                pane.id.to_string(),
                pane.socket,
                pane.holder_pid,
                pane.secret.as_slice(),
                pane.fencing_token,
                argv,
                pane.cwd,
                pane.cols,
                pane.rows,
                token,
                now_ms,
            ],
        )?;
        Ok(changed > 0)
    }

    /// What the user asked to remember for the OSC 52 writes of a pane: `Some(true)`
    /// allows them, `Some(false)` drops them, and `None` asks.
    pub fn clipboard_policy(&self, id: Ulid) -> Result<Option<bool>, StoreError> {
        let policy: Option<Option<String>> = self
            .conn
            .query_row(
                "SELECT clipboard_policy FROM panes WHERE id = ?1",
                [id.to_string()],
                |row| row.get(0),
            )
            .optional()?;
        Ok(match policy.flatten().as_deref() {
            Some("allow") => Some(true),
            Some("deny") => Some(false),
            _ => None,
        })
    }

    /// Stores the answer for the OSC 52 writes of a pane. `None` asks again.
    pub fn set_clipboard_policy(
        &self,
        id: Ulid,
        allow: Option<bool>,
        now_ms: i64,
    ) -> Result<(), StoreError> {
        let text = allow.map(|allow| if allow { "allow" } else { "deny" });
        self.exec(
            "UPDATE panes SET clipboard_policy = ?2, updated_at = ?3 WHERE id = ?1",
            params![id.to_string(), text, now_ms],
        )?;
        Ok(())
    }

    /// The tokens of the panes: `(pane, token)`. The core loads them when it starts, so that a shim
    /// that waits for a restarted core finds its token valid.
    pub fn pane_tokens(&self) -> Result<Vec<(Ulid, String)>, StoreError> {
        let mut statement = self
            .conn
            .prepare("SELECT id, pane_token FROM panes WHERE pane_token IS NOT NULL")?;
        let mut rows = statement.query([])?;
        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            let id: String = row.get(0)?;
            out.push((
                id.parse()
                    .map_err(|_| StoreError::BadValue(format!("pane id {id:?}")))?,
                row.get(1)?,
            ));
        }
        Ok(out)
    }

    /// Records the fencing token of the core that attached to the holder of a pane.
    pub fn set_attached(
        &self,
        id: Ulid,
        fencing_token: u64,
        now_ms: i64,
    ) -> Result<(), StoreError> {
        self.exec(
            "UPDATE panes SET fencing_token = ?2, updated_at = ?3 WHERE id = ?1",
            params![id.to_string(), fencing_token, now_ms],
        )?;
        Ok(())
    }

    /// Records whether the child of a pane runs, and how it ended.
    pub fn set_liveness(
        &self,
        id: Ulid,
        liveness: PaneLiveness,
        exit: Option<ExitStatus>,
        now_ms: i64,
    ) -> Result<(), StoreError> {
        let (exit_kind, exit_value) = exit_columns(exit);
        self.exec(
            "UPDATE panes SET liveness = ?2, exit_kind = ?3, exit_value = ?4, updated_at = ?5 \
             WHERE id = ?1",
            params![
                id.to_string(),
                liveness.as_str(),
                exit_kind,
                exit_value,
                now_ms
            ],
        )?;
        Ok(())
    }

    /// The pane with this id, if it has a holder. A pane that the layout made and no holder runs
    /// yet is not a [`PaneRecord`].
    pub fn pane(&self, id: Ulid) -> Result<Option<PaneRecord>, StoreError> {
        let sql =
            format!("SELECT {PANE_COLUMNS} FROM panes WHERE id = ?1 AND holder_pid IS NOT NULL");
        let mut statement = self.conn.prepare(&sql)?;
        let mut rows = statement.query([id.to_string()])?;
        rows.next()?.map(pane_from_row).transpose()
    }

    /// The panes with this liveness, in the order of their render ids.
    pub fn panes(&self, liveness: PaneLiveness) -> Result<Vec<PaneRecord>, StoreError> {
        let sql = format!(
            "SELECT {PANE_COLUMNS} FROM panes WHERE liveness = ?1 AND holder_pid IS NOT NULL ORDER BY render_id"
        );
        let mut statement = self.conn.prepare(&sql)?;
        let mut rows = statement.query([liveness.as_str()])?;
        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            out.push(pane_from_row(row)?);
        }
        Ok(out)
    }

    /// The largest render id of any pane, or 0. The next core numbers its panes after it.
    pub fn max_render_id(&self) -> Result<u32, StoreError> {
        Ok(self
            .conn
            .query_row("SELECT coalesce(max(render_id), 0) FROM panes", [], |row| {
                row.get(0)
            })?)
    }

    /// The largest fencing token of any pane, or 0. The next core takes a higher one.
    pub fn max_fencing_token(&self) -> Result<u64, StoreError> {
        Ok(self.conn.query_row(
            "SELECT coalesce(max(fencing_token), 0) FROM panes",
            [],
            |row| row.get(0),
        )?)
    }

    /// Stores a snapshot and the size of the pane at that time, and drops the snapshots of the
    /// pane except the newest [`SNAPSHOTS_KEPT`]. One transaction does all of it.
    pub fn save_snapshot(
        &mut self,
        id: Ulid,
        offset: u64,
        data: &[u8],
        size: (u16, u16),
        now_ms: i64,
    ) -> Result<(), StoreError> {
        self.guard()?;
        let result = Self::write_snapshot(&mut self.conn, id, offset, data, size, now_ms);
        self.note(result)
    }

    fn write_snapshot(
        conn: &mut rusqlite::Connection,
        id: Ulid,
        offset: u64,
        data: &[u8],
        size: (u16, u16),
        now_ms: i64,
    ) -> Result<(), StoreError> {
        let tx = conn.transaction()?;
        let key = id.to_string();
        tx.execute(
            "INSERT OR REPLACE INTO snapshots (pane, offset, created_at, data) \
             VALUES (?1, ?2, ?3, ?4)",
            params![key, offset, now_ms, data],
        )?;
        tx.execute(
            "DELETE FROM snapshots WHERE pane = ?1 AND offset NOT IN \
             (SELECT offset FROM snapshots WHERE pane = ?1 ORDER BY offset DESC LIMIT ?2)",
            params![key, SNAPSHOTS_KEPT],
        )?;
        tx.execute(
            "UPDATE panes SET cols = ?2, rows = ?3, updated_at = ?4 WHERE id = ?1",
            params![key, size.0, size.1, now_ms],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// The snapshots of a pane, the newest first.
    pub fn snapshots(&self, id: Ulid) -> Result<Vec<SnapshotRecord>, StoreError> {
        let mut statement = self.conn.prepare(
            "SELECT offset, created_at, data FROM snapshots WHERE pane = ?1 ORDER BY offset DESC",
        )?;
        let rows = statement.query_map([id.to_string()], |row| {
            Ok(SnapshotRecord {
                pane: id,
                offset: row.get(0)?,
                created_at_ms: row.get(1)?,
                data: row.get(2)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pane(n: u128, render_id: u32) -> PaneRecord {
        PaneRecord {
            id: Ulid::from(n),
            render_id,
            socket: format!("/tmp/h{n}.sock"),
            holder_pid: 4000 + n as i32,
            secret: [n as u8; 32],
            fencing_token: 100 + n as u64,
            argv: vec!["/bin/sh".to_owned(), "-c".to_owned(), "echo é".to_owned()],
            cwd: "/tmp".to_owned(),
            cols: 80,
            rows: 24,
            liveness: PaneLiveness::Running,
            exit: None,
        }
    }

    fn store() -> Store {
        Store::open_in_memory().expect("a store")
    }

    #[test]
    fn a_pane_comes_back_as_it_went_in() {
        let store = store();
        let record = pane(1, 1);
        store.insert_pane(&record, 5).expect("an insert");
        assert_eq!(store.pane(record.id).expect("a read"), Some(record));
        assert_eq!(store.pane(Ulid::from(9u128)).expect("a read"), None);
    }

    #[test]
    fn the_same_id_cannot_be_added_twice() {
        let store = store();
        store.insert_pane(&pane(1, 1), 5).expect("an insert");
        store
            .insert_pane(&pane(1, 2), 6)
            .expect_err("a second insert of the id");
    }

    #[test]
    fn live_panes_are_listed_by_render_id() {
        let store = store();
        store.insert_pane(&pane(1, 3), 5).expect("an insert");
        store.insert_pane(&pane(2, 1), 5).expect("an insert");
        store.insert_pane(&pane(3, 2), 5).expect("an insert");
        store
            .set_liveness(Ulid::from(3u128), PaneLiveness::Lost, None, 6)
            .expect("an update");
        let live: Vec<u32> = store
            .panes(PaneLiveness::Running)
            .expect("a list")
            .iter()
            .map(|p| p.render_id)
            .collect();
        assert_eq!(live, [1, 3]);
        assert_eq!(store.panes(PaneLiveness::Lost).expect("a list").len(), 1);
    }

    #[test]
    fn an_exit_status_of_each_kind_survives() {
        let store = store();
        for (n, exit) in [
            ExitStatus::Code(3),
            ExitStatus::Signal(9),
            ExitStatus::ExecFailed(2),
        ]
        .into_iter()
        .enumerate()
        {
            let record = pane(n as u128 + 1, n as u32 + 1);
            store.insert_pane(&record, 1).expect("an insert");
            store
                .set_liveness(record.id, PaneLiveness::Exited, Some(exit), 2)
                .expect("an update");
            let back = store.pane(record.id).expect("a read").expect("the pane");
            assert_eq!(
                (back.liveness, back.exit),
                (PaneLiveness::Exited, Some(exit))
            );
        }
    }

    #[test]
    fn the_maxima_start_at_zero_and_follow_the_panes() {
        let store = store();
        assert_eq!(
            (
                store.max_render_id().expect("max"),
                store.max_fencing_token().expect("max")
            ),
            (0, 0)
        );
        store.insert_pane(&pane(1, 4), 1).expect("an insert");
        store.insert_pane(&pane(2, 2), 1).expect("an insert");
        assert_eq!(store.max_render_id().expect("max"), 4);
        assert_eq!(store.max_fencing_token().expect("max"), 102);
        store
            .set_attached(Ulid::from(1u128), 500, 2)
            .expect("an update");
        assert_eq!(store.max_fencing_token().expect("max"), 500);
    }

    #[test]
    fn only_the_newest_two_snapshots_are_kept() {
        let mut store = store();
        let record = pane(1, 1);
        store.insert_pane(&record, 1).expect("an insert");
        for offset in [10u64, 30, 20, 40] {
            store
                .save_snapshot(record.id, offset, &offset.to_le_bytes(), (80, 24), 2)
                .expect("a snapshot");
        }
        let kept: Vec<u64> = store
            .snapshots(record.id)
            .expect("a list")
            .iter()
            .map(|s| s.offset)
            .collect();
        assert_eq!(
            kept,
            [40, 30],
            "the newest first, and the older ones are gone"
        );
    }

    #[test]
    fn a_snapshot_updates_the_size_and_keeps_its_bytes() {
        let mut store = store();
        let record = pane(1, 1);
        store.insert_pane(&record, 1).expect("an insert");
        store
            .save_snapshot(record.id, 7, &[1, 2, 3], (120, 40), 9)
            .expect("a snapshot");
        let back = store.pane(record.id).expect("a read").expect("the pane");
        assert_eq!((back.cols, back.rows), (120, 40));
        let snapshots = store.snapshots(record.id).expect("a list");
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].data, [1, 2, 3]);
        assert_eq!(snapshots[0].created_at_ms, 9);
    }

    #[test]
    fn a_snapshot_of_an_unknown_pane_is_refused() {
        let mut store = store();
        store
            .save_snapshot(Ulid::from(5u128), 1, &[], (80, 24), 1)
            .expect_err("the foreign key refuses it");
    }

    #[test]
    fn the_snapshots_of_one_pane_do_not_touch_another() {
        let mut store = store();
        for n in 1..=2u128 {
            store.insert_pane(&pane(n, n as u32), 1).expect("an insert");
            for offset in 1..=3u64 {
                store
                    .save_snapshot(Ulid::from(n), offset, &[n as u8], (80, 24), 1)
                    .expect("a snapshot");
            }
        }
        for n in 1..=2u128 {
            assert_eq!(
                store.snapshots(Ulid::from(n)).expect("a list").len(),
                SNAPSHOTS_KEPT
            );
        }
    }
}
