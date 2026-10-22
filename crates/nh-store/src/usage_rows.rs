//! The usage of the runs and the state of the count of a transcript.
//!
//! `usage` has one row for each run, UTC day and model. The core adds to a row when it reads
//! new lines of a transcript. The state of the count (the offset in the file and the latest
//! messages that the ledger counted) lives in `agent_runs`, and a restart goes on from it.

use rusqlite::{OptionalExtension, params};
use ulid::Ulid;

use crate::{Store, StoreError, Tx};

/// Tokens and cost. The cost is `None` when no row had a cost.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct UsageTotals {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    pub cost_usd: Option<f64>,
}

/// What the count of a transcript remembers.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TranscriptState {
    /// The first byte of the file that no whole line has used.
    pub offset: u64,
    /// The state of the ledger, as the core writes it.
    pub ledger_json: Option<String>,
}

fn totals(row: &rusqlite::Row<'_>, from: usize) -> rusqlite::Result<UsageTotals> {
    let count = |index: usize| -> rusqlite::Result<u64> {
        Ok(u64::try_from(row.get::<_, i64>(index)?).unwrap_or(0))
    };
    Ok(UsageTotals {
        input: count(from)?,
        output: count(from + 1)?,
        cache_read: count(from + 2)?,
        cache_write: count(from + 3)?,
        cost_usd: row.get(from + 4)?,
    })
}

const SUM: &str = "SUM(input), SUM(output), SUM(cache_read), SUM(cache_write), SUM(cost_usd)";

fn signed(count: u64) -> i64 {
    i64::try_from(count).unwrap_or(i64::MAX)
}

impl Tx<'_> {
    /// Adds to the usage of a run on a day. `cost_usd` adds to the cost, and `None` leaves it.
    #[allow(clippy::too_many_arguments, reason = "the columns of one row")]
    pub fn add_usage(
        &self,
        run: Ulid,
        day: &str,
        model: &str,
        input: u64,
        output: u64,
        cache_read: u64,
        cache_write: u64,
        cost_usd: Option<f64>,
    ) -> Result<(), StoreError> {
        self.writable()?;
        self.sql().execute(
            "INSERT INTO usage (run_id, day, model, input, output, cache_read, cache_write, cost_usd)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT (run_id, day, model) DO UPDATE SET
                 input = input + excluded.input,
                 output = output + excluded.output,
                 cache_read = cache_read + excluded.cache_read,
                 cache_write = cache_write + excluded.cache_write,
                 cost_usd = CASE
                     WHEN excluded.cost_usd IS NULL THEN cost_usd
                     ELSE COALESCE(cost_usd, 0) + excluded.cost_usd
                 END",
            params![
                run.to_string(),
                day,
                model,
                signed(input),
                signed(output),
                signed(cache_read),
                signed(cache_write),
                cost_usd
            ],
        )?;
        Ok(())
    }

    /// The usage of every run that has some, summed over days and models.
    pub fn usage_by_run(&self) -> Result<Vec<(Ulid, UsageTotals)>, StoreError> {
        read_by_run(self.sql())
    }

    /// The usage of a day for each harness.
    pub fn usage_on_day(&self, day: &str) -> Result<Vec<(String, UsageTotals)>, StoreError> {
        let mut statement = self.sql().prepare(&format!(
            "SELECT agent_runs.harness, {SUM}
             FROM usage JOIN agent_runs ON agent_runs.id = usage.run_id
             WHERE usage.day = ?1 GROUP BY agent_runs.harness ORDER BY agent_runs.harness"
        ))?;
        let rows = statement
            .query_map(params![day], |row| {
                Ok((row.get::<_, String>(0)?, totals(row, 1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// The usage of a run for each day and model, oldest day first.
    pub fn usage_of_run(
        &self,
        run: Ulid,
    ) -> Result<Vec<(String, String, UsageTotals)>, StoreError> {
        let mut statement = self.sql().prepare(
            "SELECT day, model, input, output, cache_read, cache_write, cost_usd
             FROM usage WHERE run_id = ?1 ORDER BY day, model",
        )?;
        let rows = statement
            .query_map(params![run.to_string()], |row| {
                Ok((row.get(0)?, row.get(1)?, totals(row, 2)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Saves where the count of a transcript stands.
    pub fn set_transcript_state(
        &self,
        run: Ulid,
        offset: u64,
        ledger_json: &str,
    ) -> Result<(), StoreError> {
        self.writable()?;
        self.sql().execute(
            "UPDATE agent_runs SET transcript_offset = ?2, transcript_ledger_json = ?3 WHERE id = ?1",
            params![run.to_string(), signed(offset), ledger_json],
        )?;
        Ok(())
    }

    /// Where the count of a transcript stands. A run that is not in the table has none.
    pub fn transcript_state(&self, run: Ulid) -> Result<Option<TranscriptState>, StoreError> {
        read_state(self.sql(), run)
    }
}

fn read_state(
    conn: &rusqlite::Connection,
    run: Ulid,
) -> Result<Option<TranscriptState>, StoreError> {
    Ok(conn
        .query_row(
            "SELECT transcript_offset, transcript_ledger_json FROM agent_runs WHERE id = ?1",
            params![run.to_string()],
            |row| {
                Ok(TranscriptState {
                    offset: u64::try_from(row.get::<_, i64>(0)?).unwrap_or(0),
                    ledger_json: row.get(1)?,
                })
            },
        )
        .optional()?)
}

impl Store {
    /// The usage of every run, for a start of the core.
    pub fn usage_by_run(&self) -> Result<Vec<(Ulid, UsageTotals)>, StoreError> {
        read_by_run(&self.conn)
    }

    /// Where the count of a transcript stands.
    pub fn transcript_state(&self, run: Ulid) -> Result<Option<TranscriptState>, StoreError> {
        read_state(&self.conn, run)
    }
}

fn read_by_run(conn: &rusqlite::Connection) -> Result<Vec<(Ulid, UsageTotals)>, StoreError> {
    let mut statement =
        conn.prepare(&format!("SELECT run_id, {SUM} FROM usage GROUP BY run_id"))?;
    let rows = statement
        .query_map([], |row| Ok((row.get::<_, String>(0)?, totals(row, 1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    rows.into_iter()
        .map(|(id, totals)| {
            id.parse()
                .map(|id| (id, totals))
                .map_err(|e| StoreError::BadValue(format!("usage run id: {e}")))
        })
        .collect()
}

#[cfg(test)]
mod tests;
