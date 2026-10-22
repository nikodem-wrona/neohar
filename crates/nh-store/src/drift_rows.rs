//! The counters of the disagreement telemetry.
//!
//! `drift` has one row for each (harness, version): how many observations the core made, how many
//! of them were bad signals, and whether it raised the alarm. The numbers stay on this machine.

use rusqlite::params;

use crate::{Store, StoreError, Tx};

/// The counters of one version of a harness.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DriftRow {
    pub harness: String,
    pub version: String,
    pub observations: u32,
    pub bad_signals: u32,
    /// The alarm went off. It goes once.
    pub raised: bool,
}

fn row(row: &rusqlite::Row<'_>) -> rusqlite::Result<DriftRow> {
    Ok(DriftRow {
        harness: row.get(0)?,
        version: row.get(1)?,
        observations: row.get(2)?,
        bad_signals: row.get(3)?,
        raised: row.get::<_, i64>(4)? != 0,
    })
}

const SELECT: &str = "SELECT harness, version, observations, bad_signals, raised FROM drift";

impl Tx<'_> {
    /// Adds to the counters of a version, and gives the row after the change.
    pub fn add_drift(
        &self,
        harness: &str,
        version: &str,
        observations: u32,
        bad_signals: u32,
    ) -> Result<DriftRow, StoreError> {
        self.writable()?;
        self.sql().execute(
            "INSERT INTO drift (harness, version, observations, bad_signals)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (harness, version) DO UPDATE SET
                 observations = observations + excluded.observations,
                 bad_signals = bad_signals + excluded.bad_signals",
            params![harness, version, observations, bad_signals],
        )?;
        Ok(self.sql().query_row(
            &format!("{SELECT} WHERE harness = ?1 AND version = ?2"),
            params![harness, version],
            row,
        )?)
    }

    /// Notes that the alarm went off for a version.
    pub fn mark_drift_raised(&self, harness: &str, version: &str) -> Result<(), StoreError> {
        self.writable()?;
        self.sql().execute(
            "UPDATE drift SET raised = 1 WHERE harness = ?1 AND version = ?2",
            params![harness, version],
        )?;
        Ok(())
    }
}

fn read_all(conn: &rusqlite::Connection) -> Result<Vec<DriftRow>, StoreError> {
    let mut statement = conn.prepare(&format!("{SELECT} ORDER BY harness, version"))?;
    let rows = statement
        .query_map([], row)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

impl Tx<'_> {
    /// The counters of every version, by harness and version.
    pub fn drift_rows(&self) -> Result<Vec<DriftRow>, StoreError> {
        read_all(self.sql())
    }
}

impl Store {
    /// The counters of every version, by harness and version.
    pub fn drift_rows(&self) -> Result<Vec<DriftRow>, StoreError> {
        read_all(&self.conn)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_counters_add_up_and_the_alarm_is_kept() {
        let mut store = Store::open_in_memory().expect("a store");
        store
            .tx(1, |tx| {
                tx.add_drift("claude", "2.2.0", 3, 1)?;
                let sum = tx.add_drift("claude", "2.2.0", 2, 2)?;
                assert_eq!(
                    (sum.observations, sum.bad_signals, sum.raised),
                    (5, 3, false)
                );
                tx.mark_drift_raised("claude", "2.2.0")?;
                tx.add_drift("codex", "0.157.0", 1, 0)?;
                Ok(())
            })
            .expect("a transaction");
        let rows = store.drift_rows().expect("rows");
        assert_eq!(rows.len(), 2);
        assert_eq!(
            (
                rows[0].harness.as_str(),
                rows[0].observations,
                rows[0].raised
            ),
            ("claude", 5, true)
        );
        assert!(!rows[1].raised);
    }
}
