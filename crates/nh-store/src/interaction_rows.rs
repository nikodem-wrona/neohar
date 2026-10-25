//! The rows of the interactions and of their delivery records.
//!
//! The core keeps the interactions in memory and writes each change here in the same transaction
//! as the event of the change. A start of the core reads the interactions that are not final back
//! with [`Store::open_interactions`]. The payload and the decision are JSON of the types of the
//! API, so this module does not interpret them.
//!
//! These are the tables `interactions` and `deliveries` of `migrations/0015_schema.sql`.

use rusqlite::{OptionalExtension, Row, params};
use serde_json::Value;
use ulid::Ulid;

use crate::{Store, StoreError, Tx};

/// An interaction.
#[derive(Clone, Debug, PartialEq)]
pub struct InteractionRow {
    pub id: Ulid,
    /// The short name, such as `i42`.
    pub handle: String,
    pub run: Option<Ulid>,
    pub pane: Option<Ulid>,
    /// `approval`, `question`, `plan_review` or `notice`.
    pub kind: String,
    pub native_ref: Option<String>,
    pub title: String,
    /// `low`, `medium`, `high` or `unknown`.
    pub risk: String,
    /// The `InteractionPayload` that `nh-rich` made when the interaction opened.
    pub payload: Value,
    /// The state of the interaction, in the words of `InteractionState`.
    pub state: String,
    pub answerable: bool,
    /// `gate` or `observe`.
    pub gate_mode: String,
    /// Where it came from: `hook`, `protocol` or `screen`.
    pub source: String,
    pub confidence: f64,
    pub opened_at: i64,
    pub deadline: Option<i64>,
    pub resolved_at: Option<i64>,
    /// How it ended, when it ended.
    pub resolution: Option<Value>,
    pub fingerprint: Option<String>,
}

/// The delivery record of an interaction.
#[derive(Clone, Debug, PartialEq)]
pub struct DeliveryRow {
    pub interaction: Ulid,
    pub native_ref: Option<String>,
    pub rev: u32,
    /// The `Decision` of the API.
    pub decision: Option<Value>,
    pub decided_by: Option<String>,
    pub via: Option<String>,
    pub key: Option<String>,
    /// `native` or `keystroke`.
    pub channel: Option<String>,
    /// Who holds the lease: `{holder, conn, expires_at}`. A lease does not outlive the core.
    pub lease: Option<Value>,
    pub deadline: Option<i64>,
    pub state: String,
    /// The attempts: `[{at, channel, via, outcome}]`.
    pub attempts: Value,
}

const COLUMNS: &str = "id, handle, run_id, pane_id, kind, native_ref, title, risk, payload_json,
    state, answerable, gate_mode, source, confidence, opened_at, deadline, resolved_at,
    resolution_json, fingerprint";

const DELIVERY_COLUMNS: &str = "interaction_id, native_ref, decision_rev, decision_json,
    decided_by, via, idempotency_key, channel, lease_json, deadline, state, attempts_json";

fn bad(what: &str, error: impl std::fmt::Display) -> StoreError {
    StoreError::BadValue(format!("{what}: {error}"))
}

fn ulid(text: &str, what: &str) -> Result<Ulid, StoreError> {
    text.parse().map_err(|e| bad(what, e))
}

fn json(row: &Row<'_>, index: usize, what: &str) -> Result<Value, StoreError> {
    let text: String = row.get(index)?;
    serde_json::from_str(&text).map_err(|e| bad(what, e))
}

fn optional_json(row: &Row<'_>, index: usize, what: &str) -> Result<Option<Value>, StoreError> {
    row.get::<_, Option<String>>(index)?
        .map(|text| serde_json::from_str(&text).map_err(|e| bad(what, e)))
        .transpose()
}

fn interaction_from_row(row: &Row<'_>) -> Result<InteractionRow, StoreError> {
    let text = |index: usize| row.get::<_, Option<String>>(index);
    Ok(InteractionRow {
        id: ulid(&row.get::<_, String>(0)?, "interaction id")?,
        handle: row.get(1)?,
        run: text(2)?.map(|t| ulid(&t, "interaction run")).transpose()?,
        pane: text(3)?.map(|t| ulid(&t, "interaction pane")).transpose()?,
        kind: row.get(4)?,
        native_ref: text(5)?,
        title: row.get(6)?,
        risk: row.get(7)?,
        payload: json(row, 8, "interaction payload")?,
        state: row.get(9)?,
        answerable: row.get::<_, i64>(10)? != 0,
        gate_mode: row.get(11)?,
        source: row.get(12)?,
        confidence: row.get(13)?,
        opened_at: row.get(14)?,
        deadline: row.get(15)?,
        resolved_at: row.get(16)?,
        resolution: optional_json(row, 17, "interaction resolution")?,
        fingerprint: text(18)?,
    })
}

fn delivery_from_row(row: &Row<'_>) -> Result<DeliveryRow, StoreError> {
    let text = |index: usize| row.get::<_, Option<String>>(index);
    Ok(DeliveryRow {
        interaction: ulid(&row.get::<_, String>(0)?, "delivery interaction")?,
        native_ref: text(1)?,
        rev: u32::try_from(row.get::<_, i64>(2)?).map_err(|e| bad("decision rev", e))?,
        decision: optional_json(row, 3, "decision")?,
        decided_by: text(4)?,
        via: text(5)?,
        key: text(6)?,
        channel: text(7)?,
        lease: optional_json(row, 8, "lease")?,
        deadline: row.get(9)?,
        state: row.get(10)?,
        attempts: json(row, 11, "attempts")?,
    })
}

impl Tx<'_> {
    /// Adds an interaction or changes it. The time of opening does not change.
    pub fn put_interaction(&self, row: &InteractionRow) -> Result<(), StoreError> {
        self.writable()?;
        self.sql().execute(
            "INSERT INTO interactions (id, handle, run_id, pane_id, kind, native_ref, title, risk,
                 payload_json, state, answerable, gate_mode, source, confidence, opened_at,
                 deadline, resolved_at, resolution_json, fingerprint)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17,
                 ?18, ?19)
             ON CONFLICT (id) DO UPDATE SET run_id = excluded.run_id, pane_id = excluded.pane_id,
                 native_ref = excluded.native_ref, title = excluded.title, risk = excluded.risk,
                 payload_json = excluded.payload_json, state = excluded.state,
                 answerable = excluded.answerable, gate_mode = excluded.gate_mode,
                 source = excluded.source, confidence = excluded.confidence,
                 deadline = excluded.deadline, resolved_at = excluded.resolved_at,
                 resolution_json = excluded.resolution_json, fingerprint = excluded.fingerprint",
            params![
                row.id.to_string(),
                row.handle,
                row.run.map(|r| r.to_string()),
                row.pane.map(|p| p.to_string()),
                row.kind,
                row.native_ref,
                row.title,
                row.risk,
                row.payload.to_string(),
                row.state,
                i64::from(row.answerable),
                row.gate_mode,
                row.source,
                row.confidence,
                row.opened_at,
                row.deadline,
                row.resolved_at,
                row.resolution.as_ref().map(Value::to_string),
                row.fingerprint,
            ],
        )?;
        Ok(())
    }

    /// Adds the delivery record of an interaction or changes it.
    pub fn put_delivery(&self, row: &DeliveryRow) -> Result<(), StoreError> {
        self.writable()?;
        self.sql().execute(
            "INSERT INTO deliveries (interaction_id, native_ref, decision_rev, decision_json,
                 decided_by, via, idempotency_key, channel, lease_json, deadline, state,
                 attempts_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
             ON CONFLICT (interaction_id) DO UPDATE SET native_ref = excluded.native_ref,
                 decision_rev = excluded.decision_rev, decision_json = excluded.decision_json,
                 decided_by = excluded.decided_by, via = excluded.via,
                 idempotency_key = excluded.idempotency_key, channel = excluded.channel,
                 lease_json = excluded.lease_json, deadline = excluded.deadline,
                 state = excluded.state, attempts_json = excluded.attempts_json",
            params![
                row.interaction.to_string(),
                row.native_ref,
                i64::from(row.rev),
                row.decision.as_ref().map(Value::to_string),
                row.decided_by,
                row.via,
                row.key,
                row.channel,
                row.lease.as_ref().map(Value::to_string),
                row.deadline,
                row.state,
                row.attempts.to_string(),
            ],
        )?;
        Ok(())
    }
}

impl Store {
    /// The interactions that are not final, the oldest first, with their delivery records. A start
    /// of the core continues them.
    pub fn open_interactions(
        &self,
    ) -> Result<Vec<(InteractionRow, Option<DeliveryRow>)>, StoreError> {
        let sql = format!(
            "SELECT {COLUMNS} FROM interactions
             WHERE state IN ('open', 'decision_recorded', 'delivering', 'delivery_unknown')
             ORDER BY opened_at, id"
        );
        self.interactions_with_deliveries(&sql, [])
    }

    /// An interaction with its delivery record.
    pub fn interaction_row(
        &self,
        id: Ulid,
    ) -> Result<Option<(InteractionRow, Option<DeliveryRow>)>, StoreError> {
        let sql = format!("SELECT {COLUMNS} FROM interactions WHERE id = ?1");
        Ok(self
            .interactions_with_deliveries(&sql, [id.to_string()])?
            .into_iter()
            .next())
    }

    /// The most recent interactions of any state, the newest first.
    pub fn recent_interactions(
        &self,
        limit: u32,
    ) -> Result<Vec<(InteractionRow, Option<DeliveryRow>)>, StoreError> {
        let sql = format!(
            "SELECT {COLUMNS} FROM interactions ORDER BY opened_at DESC, id DESC LIMIT {limit}"
        );
        self.interactions_with_deliveries(&sql, [])
    }

    fn interactions_with_deliveries<P: rusqlite::Params>(
        &self,
        sql: &str,
        params: P,
    ) -> Result<Vec<(InteractionRow, Option<DeliveryRow>)>, StoreError> {
        let mut statement = self.conn.prepare(sql)?;
        let mut rows = statement.query(params)?;
        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            out.push(interaction_from_row(row)?);
        }
        let mut result = Vec::with_capacity(out.len());
        for interaction in out {
            let delivery = self.delivery_row(interaction.id)?;
            result.push((interaction, delivery));
        }
        Ok(result)
    }

    /// Deletes the interactions that ended before `before_ms`, with their delivery records. An
    /// interaction that is not final stays, also an old one, because a start of the core continues
    /// it. Returns the number of interactions that went.
    pub fn prune_interactions(&mut self, now_ms: i64, before_ms: i64) -> Result<usize, StoreError> {
        let ended = "state NOT IN ('open', 'decision_recorded', 'delivering', 'delivery_unknown')
             AND COALESCE(resolved_at, opened_at) < ?1";
        let (count, _) = self.tx(now_ms, |tx| {
            tx.writable()?;
            let sql = tx.sql();
            // The foreign key deletes them too. The delete does not rely on it.
            sql.execute(
                &format!(
                    "DELETE FROM deliveries WHERE interaction_id IN
                         (SELECT id FROM interactions WHERE {ended})"
                ),
                [before_ms],
            )?;
            Ok(sql.execute(
                &format!("DELETE FROM interactions WHERE {ended}"),
                [before_ms],
            )?)
        })?;
        Ok(count)
    }

    /// The delivery record of an interaction.
    pub fn delivery_row(&self, interaction: Ulid) -> Result<Option<DeliveryRow>, StoreError> {
        let sql = format!("SELECT {DELIVERY_COLUMNS} FROM deliveries WHERE interaction_id = ?1");
        let mut statement = self.conn.prepare(&sql)?;
        statement
            .query_row([interaction.to_string()], |row| Ok(delivery_from_row(row)))
            .optional()?
            .transpose()
    }
}

#[cfg(test)]
mod tests;
