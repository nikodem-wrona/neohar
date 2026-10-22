//! The rows of the policy.
//!
//! `policy_rules` holds the rules that `policy.add` made and the session rules of "Allow always".
//! The rules of `config.toml` are not here: the file is their only store. `fingerprints` has one
//! row for each answered approval.

use rusqlite::{Row, params};
use serde_json::Value;
use ulid::Ulid;

use crate::{Store, StoreError, Tx};

/// A rule of the table. `rule` is the `PolicyRule` of the API.
#[derive(Clone, Debug, PartialEq)]
pub struct PolicyRuleRow {
    pub id: String,
    /// `config` for a rule that `policy.add` made, `session` for "Allow always".
    pub scope: String,
    pub rule: Value,
    /// Where the rule came from: `api`, or `allow_always`.
    pub source: String,
    pub created_at: i64,
}

/// An answered approval.
#[derive(Clone, Debug, PartialEq)]
pub struct FingerprintRow {
    pub fingerprint: String,
    /// `allow` or `deny`.
    pub decision: String,
    pub interaction: Option<Ulid>,
    pub at: i64,
    pub harness: Option<String>,
    pub tool: Option<String>,
    /// The command prefix such as `pnpm test:*`, or a glob of paths.
    pub pattern: Option<String>,
    pub workspace: Option<String>,
    /// A rule may be suggested for it.
    pub eligible: bool,
}

fn rule_from_row(row: &Row<'_>) -> Result<PolicyRuleRow, StoreError> {
    let text: String = row.get(2)?;
    Ok(PolicyRuleRow {
        id: row.get(0)?,
        scope: row.get(1)?,
        rule: serde_json::from_str(&text)
            .map_err(|e| StoreError::BadValue(format!("policy rule: {e}")))?,
        source: row.get(3)?,
        created_at: row.get(4)?,
    })
}

fn fingerprint_from_row(row: &Row<'_>) -> Result<FingerprintRow, StoreError> {
    let interaction: Option<String> = row.get(2)?;
    Ok(FingerprintRow {
        fingerprint: row.get(0)?,
        decision: row.get(1)?,
        interaction: interaction.and_then(|text| text.parse().ok()),
        at: row.get(3)?,
        harness: row.get(4)?,
        tool: row.get(5)?,
        pattern: row.get(6)?,
        workspace: row.get(7)?,
        eligible: row.get::<_, i64>(8)? != 0,
    })
}

impl Tx<'_> {
    /// Adds a rule. The id is unique: a second rule with the same id replaces the first.
    pub fn put_policy_rule(&self, row: &PolicyRuleRow) -> Result<(), StoreError> {
        self.writable()?;
        self.sql().execute(
            "INSERT INTO policy_rules (id, scope, rule_json, source, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT (id) DO UPDATE SET scope = excluded.scope,
                 rule_json = excluded.rule_json, source = excluded.source",
            params![
                row.id,
                row.scope,
                row.rule.to_string(),
                row.source,
                row.created_at
            ],
        )?;
        Ok(())
    }

    /// Removes a rule. Says whether there was one.
    pub fn delete_policy_rule(&self, id: &str) -> Result<bool, StoreError> {
        self.writable()?;
        let removed = self
            .sql()
            .execute("DELETE FROM policy_rules WHERE id = ?1", [id])?;
        Ok(removed > 0)
    }

    /// Removes the session rules, as a new core session does. Gives their ids.
    pub fn delete_session_policy_rules(&self) -> Result<Vec<String>, StoreError> {
        self.writable()?;
        let mut statement = self
            .sql()
            .prepare("DELETE FROM policy_rules WHERE scope = 'session' RETURNING id")?;
        let ids = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ids)
    }

    /// Notes an answered approval. A client that replaces its decision before the delivery
    /// replaces the row, so one interaction has one row.
    pub fn put_fingerprint(&self, row: &FingerprintRow) -> Result<(), StoreError> {
        self.writable()?;
        if let Some(interaction) = row.interaction {
            self.sql().execute(
                "DELETE FROM fingerprints WHERE interaction_id = ?1",
                [interaction.to_string()],
            )?;
        }
        self.sql().execute(
            "INSERT INTO fingerprints (fingerprint, decision, interaction_id, at, harness, tool,
                 pattern, workspace, eligible)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                row.fingerprint,
                row.decision,
                row.interaction.map(|id| id.to_string()),
                row.at,
                row.harness,
                row.tool,
                row.pattern,
                row.workspace,
                i64::from(row.eligible),
            ],
        )?;
        Ok(())
    }
}

const FINGERPRINT_SQL: &str = "SELECT fingerprint, decision, interaction_id, at, harness, tool,
        pattern, workspace, eligible
    FROM fingerprints WHERE at >= ?1 ORDER BY at, rowid";

fn read_fingerprints(
    conn: &rusqlite::Connection,
    since: i64,
) -> Result<Vec<FingerprintRow>, StoreError> {
    let mut statement = conn.prepare(FINGERPRINT_SQL)?;
    let mut rows = statement.query([since])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(fingerprint_from_row(row)?);
    }
    Ok(out)
}

impl Tx<'_> {
    /// The answered approvals since `since` (unix milliseconds), the oldest first.
    pub fn fingerprints_since(&self, since: i64) -> Result<Vec<FingerprintRow>, StoreError> {
        read_fingerprints(self.sql(), since)
    }

    /// The rules of the table, the oldest first.
    pub fn policy_rules(&self) -> Result<Vec<PolicyRuleRow>, StoreError> {
        read_rules(self.sql())
    }
}

fn read_rules(conn: &rusqlite::Connection) -> Result<Vec<PolicyRuleRow>, StoreError> {
    let mut statement = conn.prepare(
        "SELECT id, scope, rule_json, source, created_at FROM policy_rules
         ORDER BY created_at, id",
    )?;
    let mut rows = statement.query([])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(rule_from_row(row)?);
    }
    Ok(out)
}

impl Store {
    /// The rules of the table, the oldest first.
    pub fn policy_rules(&self) -> Result<Vec<PolicyRuleRow>, StoreError> {
        read_rules(&self.conn)
    }

    /// The answered approvals since `since` (unix milliseconds), the oldest first.
    pub fn fingerprints_since(&self, since: i64) -> Result<Vec<FingerprintRow>, StoreError> {
        read_fingerprints(&self.conn, since)
    }
}

#[cfg(test)]
mod tests;
