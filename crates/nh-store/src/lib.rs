//! The SQLite database of a session.
//!
//! The state tables are the source of truth, and every mutation appends its events in the same
//! transaction (`Store::tx`, the transactional outbox). `migrations/0015_schema.sql` has the
//! schema, and `migrate.rs` brings a database to the version of this build. The event log has
//! cursors and a retention (`events.rs`).
//!
//! The store is synchronous and has no async runtime. The core calls it from one place at a time
//! (the state actor). Times are unix milliseconds that the caller passes in,
//! so that no function reads a clock.

mod blobs;
mod client_rows;
mod degraded;
mod drift_rows;
mod events;
mod git_rows;
mod hermes_rows;
mod interaction_rows;
mod layout;
mod migrate;
mod panes;
mod phone_rows;
mod policy_rows;
mod runs;
mod scrollback;
mod usage_rows;
mod workflow_rows;

use std::{
    cell::RefCell,
    fs::OpenOptions,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    time::Duration,
};

use rusqlite::{Connection, OptionalExtension};

pub use blobs::BlobInfo;
pub use client_rows::ClientRow;
#[cfg(any(test, feature = "fault-injection"))]
pub use degraded::Fault;
pub use degraded::PROBE_INTERVAL_MS;
pub use drift_rows::DriftRow;
pub use events::{EventsRead, Retention, Swept, TruncatedReason, Tx, is_audit_kind};
pub use git_rows::{GitRepoRow, GitTrustRow};
pub use hermes_rows::HermesChatRow;
pub use interaction_rows::{DeliveryRow, InteractionRow};
pub use layout::{GroupRow, LayoutRows, LoadedPane, PaneSlot, TabRow, WindowRow, WorkspaceRow};
pub use migrate::SCHEMA_VERSION;
pub use panes::{PaneLiveness, PaneRecord, SnapshotRecord};
pub use phone_rows::{PhoneRow, PushRow};
pub use policy_rows::{FingerprintRow, PolicyRuleRow};
pub use runs::RunRow;
pub use scrollback::{
    ArchivedLine, ArchivedRun, FLUSH_INTERVAL_MS, Flushed, PageBuilder, PageError, SEGMENT_BYTES,
    SEGMENT_LINES, ScrollbackWriter, SegmentInfo,
};
pub use usage_rows::{TranscriptState, UsageTotals};
pub use workflow_rows::{ExecutionRow, NodeRow};

/// How long a statement waits for a lock that another connection holds.
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

/// An error of the store.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Sqlite(rusqlite::Error),
    /// The disk is full, or the database has an I/O error, or it is read-only. The store refuses
    /// writes until a probe works (`Store::probe`).
    #[error("the store is degraded: {reason}")]
    Degraded { reason: String },
    #[error("cannot prepare {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    /// A newer build of neohar wrote the database. This build must not change it.
    #[error("the database has schema version {found}, and this build knows version {supported}")]
    TooNew { found: u32, supported: u32 },
    /// A development build from before the first release wrote the database, and no build can
    /// upgrade its schema. The store does not change the file.
    #[error(
        "{} has schema version {found}, from a development build of neohar. This build cannot \
         upgrade a schema older than version {baseline}. Move the file (and its -wal and -shm files, \
         if there are any) to a different place, and start neohar again. neohar then makes a new, \
         empty database.",
        .path.display()
    )]
    TooOld {
        path: PathBuf,
        found: u32,
        baseline: u32,
    },
    /// A row holds a value that no build of neohar writes.
    #[error("the database holds a bad value: {0}")]
    BadValue(String),
}

impl From<rusqlite::Error> for StoreError {
    fn from(error: rusqlite::Error) -> Self {
        match degraded::classify(&error) {
            Some(reason) => Self::Degraded { reason },
            None => Self::Sqlite(error),
        }
    }
}

/// An open database.
pub struct Store {
    conn: Connection,
    /// The directory of the database, which also holds the blobs and the scrollback archive. A
    /// database in memory has none.
    dir: Option<PathBuf>,
    health: RefCell<degraded::Health>,
}

impl Store {
    /// Opens the database at `path`, creates it when it does not exist, and brings its schema to
    /// [`SCHEMA_VERSION`].
    ///
    /// The file has mode 0600, and so have its `-wal` and `-shm` files, which SQLite makes with the
    /// mode of the main file. The directory must exist.
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        let io = |source| StoreError::Io {
            path: path.to_owned(),
            source,
        };
        // Make the file first, so that it never exists with a wider mode than 0600.
        OpenOptions::new()
            .create(true)
            .append(true)
            .mode(0o600)
            .open(path)
            .map_err(io)?;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).map_err(io)?;
        let conn = Connection::open(path)?;
        conn.busy_timeout(BUSY_TIMEOUT)?;
        // Before the first write: a database that this build refuses stays as it is.
        migrate::check(&conn, path)?;
        // `journal_mode` answers with a row, so `pragma_update` would fail.
        let mode: String =
            conn.pragma_update_and_check(None, "journal_mode", "WAL", |row| row.get(0))?;
        if !mode.eq_ignore_ascii_case("wal") {
            return Err(StoreError::BadValue(format!("journal_mode is {mode}")));
        }
        Self::configure(conn, Some(path))
    }

    /// A database in memory, for tests. It has no directory for blobs or the archive.
    pub fn open_in_memory() -> Result<Self, StoreError> {
        Self::configure(Connection::open_in_memory()?, None)
    }

    /// `path` is the file of the database, or `None` for a database in memory.
    fn configure(mut conn: Connection, path: Option<&Path>) -> Result<Self, StoreError> {
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", true)?;
        migrate::run(&mut conn, path.unwrap_or(Path::new(":memory:")))?;
        let store = Self {
            conn,
            dir: path.and_then(Path::parent).map(Path::to_owned),
            health: RefCell::default(),
        };
        store.ensure_identity()?;
        Ok(store)
    }

    /// The schema version of the open database.
    pub fn schema_version(&self) -> Result<u32, StoreError> {
        Ok(self
            .conn
            .pragma_query_value(None, "user_version", |row| row.get(0))?)
    }

    /// The value of a `meta` key.
    pub fn meta(&self, key: &str) -> Result<Option<String>, StoreError> {
        Ok(self
            .conn
            .query_row("SELECT value FROM meta WHERE key = ?1", [key], |row| {
                row.get(0)
            })
            .optional()?)
    }

    /// Sets a `meta` key.
    pub fn set_meta(&self, key: &str, value: &str) -> Result<(), StoreError> {
        self.exec(
            "INSERT INTO meta (key, value) VALUES (?1, ?2)
             ON CONFLICT (key) DO UPDATE SET value = excluded.value",
            [key, value],
        )?;
        Ok(())
    }

    /// Runs `PRAGMA integrity_check`. `Ok(())` means that SQLite found nothing wrong.
    pub fn integrity_check(&self) -> Result<(), StoreError> {
        let answer: String = self
            .conn
            .pragma_query_value(None, "integrity_check", |row| row.get(0))?;
        if answer == "ok" {
            Ok(())
        } else {
            Err(StoreError::BadValue(format!("integrity_check: {answer}")))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("nh-store-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("the test directory is made");
        dir
    }

    #[test]
    fn an_empty_file_gets_the_current_schema() {
        let dir = temp_dir("empty");
        let store = Store::open(&dir.join("state.db")).expect("the store opens");
        assert_eq!(store.schema_version().expect("a version"), SCHEMA_VERSION);
        store.integrity_check().expect("a new database is sound");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_database_and_its_wal_files_have_mode_0600() {
        let dir = temp_dir("mode");
        let path = dir.join("state.db");
        let store = Store::open(&path).expect("the store opens");
        store.set_meta("k", "v").expect("a write makes the wal");
        let mode = |p: &Path| std::fs::metadata(p).map(|m| m.permissions().mode() & 0o777);
        assert_eq!(mode(&path).expect("the file exists"), 0o600);
        let wal = dir.join("state.db-wal");
        assert_eq!(mode(&wal).expect("the wal exists"), 0o600);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_second_open_of_the_same_file_keeps_the_data() {
        let dir = temp_dir("reopen");
        let path = dir.join("state.db");
        Store::open(&path)
            .expect("the store opens")
            .set_meta("session_uuid", "abc")
            .expect("a write");
        let again = Store::open(&path).expect("the store opens again");
        assert_eq!(
            again.meta("session_uuid").expect("a read").as_deref(),
            Some("abc")
        );
        assert_eq!(again.meta("missing").expect("a read"), None);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_reader_and_a_writer_work_at_once() {
        let dir = temp_dir("wal");
        let path = dir.join("state.db");
        let writer = Store::open(&path).expect("the writer opens");
        let reader = Store::open(&path).expect("the reader opens");
        writer.set_meta("n", "1").expect("a write");
        assert_eq!(reader.meta("n").expect("a read").as_deref(), Some("1"));
        let _ = std::fs::remove_dir_all(dir);
    }
}
