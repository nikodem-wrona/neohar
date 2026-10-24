//! The migration runner. `PRAGMA user_version` holds the number of the last migration that ran.
//!
//! The schema starts at [`BASELINE`]: `migrations/0015_schema.sql` makes the whole schema of that
//! version in one step. The development builds before the first release made the same schema in
//! 15 steps. Thus a database of version 15 from a development build is at the baseline, and the
//! runner keeps it as it is. The runner refuses a database of a version from 1 to 14, because no
//! build can upgrade it. A change to the schema is a new migration with the next version, in
//! [`MIGRATIONS`].

use std::path::Path;

use rusqlite::{Connection, TransactionBehavior};

use crate::StoreError;

struct Migration {
    version: u32,
    sql: &'static str,
    /// The migration builds a table again. The runner turns foreign key enforcement off for it
    /// (the setting has no effect inside a transaction), and checks the keys before the commit.
    rebuilds_tables: bool,
}

/// The version that the baseline makes. A new database gets it in one step.
pub(crate) const BASELINE: u32 = 15;

/// The schema at [`BASELINE`]. It never changes after a release.
const BASELINE_SQL: &str = include_str!("../migrations/0015_schema.sql");

/// The migrations after the baseline, in order. A change to the schema appends a row with the next
/// version and a file `migrations/NNNN_name.sql`. It also adds the snapshot of the schema of the
/// new version (`INSTA_UPDATE=always`, see `migrate/tests.rs`). It never changes a row that
/// shipped.
const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 16,
        sql: include_str!("../migrations/0016_drop_tasks.sql"),
        rebuilds_tables: false,
    },
    Migration {
        version: 17,
        sql: include_str!("../migrations/0017_git_repos.sql"),
        rebuilds_tables: false,
    },
];

/// The schema version that this build writes.
pub const SCHEMA_VERSION: u32 = match MIGRATIONS.last() {
    Some(last) => last.version,
    None => BASELINE,
};

fn version(conn: &Connection) -> Result<u32, StoreError> {
    Ok(conn.pragma_query_value(None, "user_version", |row| row.get(0))?)
}

/// Refuses a database that this build cannot use: one of a development build from before the
/// baseline, or one of a newer build. It only reads the version, so a refused file stays as it is.
/// `path` is the file, for the error.
pub(crate) fn check(conn: &Connection, path: &Path) -> Result<(), StoreError> {
    check_version(conn, path, SCHEMA_VERSION).map(|_| ())
}

/// Returns the version of the database, or refuses it. `latest` is the version that the runner
/// writes.
fn check_version(conn: &Connection, path: &Path, latest: u32) -> Result<u32, StoreError> {
    let found = version(conn)?;
    if found > latest {
        return Err(StoreError::TooNew {
            found,
            supported: latest,
        });
    }
    if found != 0 && found < BASELINE {
        return Err(StoreError::TooOld {
            path: path.to_owned(),
            found,
            baseline: BASELINE,
        });
    }
    Ok(found)
}

/// Brings the schema of `conn` to [`SCHEMA_VERSION`]. An empty database gets the baseline, and a
/// database at a version after it gets only the migrations after its version. Each step runs in one
/// transaction with the change of the version, so that a crash leaves the database at a whole
/// version.
pub(crate) fn run(conn: &mut Connection, path: &Path) -> Result<(), StoreError> {
    run_with(conn, path, BASELINE_SQL, MIGRATIONS)
}

/// [`run`] with a baseline and migrations that a test can give.
fn run_with(
    conn: &mut Connection,
    path: &Path,
    baseline: &'static str,
    migrations: &[Migration],
) -> Result<(), StoreError> {
    let latest = migrations.last().map_or(BASELINE, |last| last.version);
    let found = check_version(conn, path, latest)?;
    let first = Migration {
        version: BASELINE,
        sql: baseline,
        rebuilds_tables: false,
    };
    for migration in std::iter::once(&first)
        .chain(migrations)
        .filter(|m| m.version > found)
    {
        if migration.rebuilds_tables {
            conn.pragma_update(None, "foreign_keys", false)?;
        }
        let result = apply(conn, migration);
        if migration.rebuilds_tables {
            // Back on, also when the migration failed.
            conn.pragma_update(None, "foreign_keys", true)?;
        }
        result?;
    }
    Ok(())
}

/// Runs one migration in a transaction, together with the change of the version.
fn apply(conn: &mut Connection, migration: &Migration) -> Result<(), StoreError> {
    // `Immediate` takes the write lock first. A second process that opens the file at the same
    // time then waits, and finds the migration done.
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let current: u32 = tx.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if current >= migration.version {
        return Ok(());
    }
    tx.execute_batch(migration.sql)?;
    if migration.rebuilds_tables {
        let broken: i64 = tx
            .prepare("PRAGMA foreign_key_check")?
            .query_map([], |_| Ok(()))?
            .count()
            .try_into()
            .unwrap_or(i64::MAX);
        if broken > 0 {
            return Err(StoreError::BadValue(format!(
                "migration {} left {broken} broken foreign keys",
                migration.version
            )));
        }
    }
    tx.pragma_update(None, "user_version", migration.version)?;
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests;
