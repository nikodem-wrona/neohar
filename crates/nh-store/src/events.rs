//! The event log and the transactional outbox.
//!
//! A state change and its events go into the database in one transaction ([`Store::tx`]). If the
//! transaction fails, neither the rows nor the events are there. After the commit the caller gets
//! the events with their sequence numbers, and pushes them to the subscribers.
//!
//! A client reads the log with a [`Cursor`]. The store answers with the events after the
//! cursor, or with [`EventsRead::Truncated`] when it cannot promise that the client misses
//! nothing: the cursor is from another log epoch or another database, it is older than the
//! retention allows, or it is ahead of the log.

use std::time::Duration;

use nh_proto::api::{
    events::{Cursor, Event, EventFilter, EventPayload},
    time::Timestamp,
};
use rusqlite::{OptionalExtension, Transaction, params, params_from_iter, types::Value as Sql};
use serde_json::Value;
use ulid::Ulid;

use crate::{Store, StoreError};

/// The meta keys that name the log.
const MACHINE_UUID: &str = "machine_uuid";
const SESSION_UUID: &str = "session_uuid";
const LOG_EPOCH: &str = "log_epoch";
/// The highest `seq` that a sweep deleted. A cursor below it has a gap.
const EVENTS_FLOOR: &str = "events_floor";

/// How long the log keeps events.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Retention {
    /// Events that a client may still need for a catch-up. All other kinds go after this time.
    pub sync: Duration,
    /// Audit kinds (see [`is_audit_kind`]) go after this time.
    pub audit: Duration,
}

impl Default for Retention {
    fn default() -> Self {
        Self {
            sync: Duration::from_secs(7 * 24 * 3600),
            audit: Duration::from_secs(90 * 24 * 3600),
        }
    }
}

/// Whether the log keeps the kind for the audit period: `interaction.*`,
/// `agent.state_changed`, `policy.*` and `task.*`.
pub fn is_audit_kind(kind: &str) -> bool {
    kind.starts_with("interaction.")
        || kind.starts_with("policy.")
        || kind.starts_with("task.")
        || kind == "agent.state_changed"
}

/// Why the store cannot continue a cursor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TruncatedReason {
    /// The cursor is from another machine, another session or another log epoch.
    Epoch,
    /// A sweep deleted events after the cursor.
    Expired,
    /// The cursor is ahead of the log, so the client knows a log that this database does not have.
    Ahead,
}

/// The answer to [`Store::events_since`].
#[derive(Clone, Debug, PartialEq)]
pub enum EventsRead {
    /// The events after the cursor. `cursor` is where the client continues. `more` says that the
    /// limit cut the list.
    Events {
        events: Vec<Event>,
        cursor: Cursor,
        more: bool,
    },
    /// The client must take a fresh snapshot and continue from `cursor`, the end of the log.
    Truncated {
        reason: TruncatedReason,
        cursor: Cursor,
    },
}

/// What a sweep deleted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Swept {
    pub sync: usize,
    pub audit: usize,
}

/// A transaction of the store. Everything that goes through it commits together, with the events
/// that [`Tx::append`] collected.
pub struct Tx<'a> {
    tx: Transaction<'a>,
    now_ms: i64,
    events: Vec<Event>,
    /// A read transaction: it changes nothing, and it works while the store is degraded.
    read_only: bool,
}

impl Tx<'_> {
    /// The time of the transaction, in unix milliseconds. The caller passed it to [`Store::tx`].
    pub fn now_ms(&self) -> i64 {
        self.now_ms
    }

    pub(crate) fn writable(&self) -> Result<(), StoreError> {
        if self.read_only {
            Err(StoreError::BadValue(
                "a read transaction cannot write".to_owned(),
            ))
        } else {
            Ok(())
        }
    }

    /// The SQLite transaction, for the statements of the state tables.
    pub fn sql(&self) -> &Transaction<'_> {
        &self.tx
    }

    /// Adds an event of a typed payload. Returns its sequence number.
    pub fn append(
        &mut self,
        subject: Option<&str>,
        payload: &EventPayload,
    ) -> Result<u64, StoreError> {
        self.append_raw(payload.kind().name(), 1, subject, payload.to_value())
    }

    /// Adds an event of any kind. Returns its sequence number.
    pub fn append_raw(
        &mut self,
        kind: &str,
        v: u16,
        subject: Option<&str>,
        payload: Value,
    ) -> Result<u64, StoreError> {
        self.writable()?;
        // A secret in a command or a message does not reach the log.
        let mut payload = payload;
        nh_redact::redact_value(&mut payload);
        let seq: i64 = self.tx.query_row(
            "INSERT INTO events (at, kind, v, subject, payload_json)
             VALUES (?1, ?2, ?3, ?4, ?5) RETURNING seq",
            params![self.now_ms, kind, v, subject, payload.to_string()],
            |row| row.get(0),
        )?;
        let seq = u64::try_from(seq).map_err(|_| StoreError::BadValue(format!("seq {seq}")))?;
        self.events.push(Event {
            seq,
            at: Timestamp(self.now_ms),
            kind: kind.to_owned(),
            v,
            subject: subject.map(str::to_owned),
            payload,
        });
        Ok(seq)
    }

    /// The value of a `meta` key, as the transaction sees it.
    pub fn meta(&self, key: &str) -> Result<Option<String>, StoreError> {
        Ok(self
            .tx
            .query_row("SELECT value FROM meta WHERE key = ?1", [key], |row| {
                row.get(0)
            })
            .optional()?)
    }

    /// The identity of the log and the number of its last event, as a cursor.
    pub fn cursor(&self) -> Result<Cursor, StoreError> {
        let get = |key: &str| -> Result<String, StoreError> {
            self.meta(key)?
                .ok_or_else(|| StoreError::BadValue(format!("meta {key} is missing")))
        };
        let seq: Option<i64> = self
            .tx
            .query_row(
                "SELECT seq FROM sqlite_sequence WHERE name = 'events'",
                [],
                |row| row.get(0),
            )
            .optional()?;
        Ok(Cursor {
            machine_uuid: get(MACHINE_UUID)?,
            session_uuid: get(SESSION_UUID)?,
            log_epoch: meta_number(self.meta(LOG_EPOCH)?, LOG_EPOCH)?,
            seq: u64::try_from(seq.unwrap_or(0)).unwrap_or(0),
        })
    }

    /// The next number of a handle counter. The first number is 1.
    ///
    /// The scope is `workspace`, `render`, `interaction`, `task`, `preview`, `tab:<workspace id>` or
    /// `pane:<workspace id>`. `render` numbers the panes for the frames. The number is never used again, also when the object goes away.
    pub fn next_handle(&self, scope: &str) -> Result<u32, StoreError> {
        self.writable()?;
        let number: i64 = self.tx.query_row(
            "INSERT INTO handle_counters (scope, next) VALUES (?1, 2)
             ON CONFLICT (scope) DO UPDATE SET next = next + 1
             RETURNING next - 1",
            [scope],
            |row| row.get(0),
        )?;
        u32::try_from(number).map_err(|_| StoreError::BadValue(format!("handle {number}")))
    }

    /// Makes sure that the next number of a counter is above `last_used`. A database whose rows
    /// have numbers from before the counter existed (the render ids of early builds) calls it when
    /// the core starts, so that no old number comes out again.
    pub fn raise_handle_counter(&self, scope: &str, last_used: u32) -> Result<(), StoreError> {
        self.writable()?;
        self.tx.execute(
            "INSERT INTO handle_counters (scope, next) VALUES (?1, ?2)
             ON CONFLICT (scope) DO UPDATE SET next = max(next, excluded.next)",
            rusqlite::params![scope, i64::from(last_used) + 1],
        )?;
        Ok(())
    }

    /// Adds 1 to the version of a projection and returns the new version.
    pub fn bump_projection(&self, name: &str) -> Result<u64, StoreError> {
        self.writable()?;
        let version: i64 = self.tx.query_row(
            "INSERT INTO projection_versions (name, version) VALUES (?1, 1)
             ON CONFLICT (name) DO UPDATE SET version = version + 1
             RETURNING version",
            [name],
            |row| row.get(0),
        )?;
        u64::try_from(version).map_err(|_| StoreError::BadValue(format!("version {version}")))
    }
}

/// Parses a count that the store keeps as text in `meta`.
fn meta_number(value: Option<String>, key: &str) -> Result<u64, StoreError> {
    match value {
        None => Ok(0),
        Some(text) => text
            .parse()
            .map_err(|_| StoreError::BadValue(format!("meta {key} is {text:?}"))),
    }
}

fn random_ulid() -> Result<String, StoreError> {
    let mut random = [0u8; 16];
    getrandom::fill(&mut random)
        .map_err(|e| StoreError::BadValue(format!("no random bytes: {e}")))?;
    let tail = u128::from_be_bytes(random) & ((1u128 << 80) - 1);
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(0));
    Ok(Ulid::from_parts(ms, tail).to_string())
}

impl Store {
    /// The last `limit` events, the oldest first. A debug bundle uses them.
    pub fn recent_events(&self, limit: usize) -> Result<Vec<Event>, StoreError> {
        let mut statement = self.conn.prepare(
            "SELECT seq, at, kind, v, subject, payload_json FROM events
             ORDER BY seq DESC LIMIT ?1",
        )?;
        let mut rows = statement.query([i64::try_from(limit).unwrap_or(i64::MAX)])?;
        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            let payload: String = row.get(5)?;
            out.push(Event {
                seq: u64::try_from(row.get::<_, i64>(0)?).unwrap_or_default(),
                at: Timestamp(row.get(1)?),
                kind: row.get(2)?,
                v: row.get(3)?,
                subject: row.get(4)?,
                payload: serde_json::from_str(&payload)
                    .map_err(|e| StoreError::BadValue(format!("event payload: {e}")))?,
            });
        }
        out.reverse();
        Ok(out)
    }

    /// The number of rows of each table, for a debug bundle. It reads no content.
    pub fn table_counts(&self) -> Result<Vec<(String, u64)>, StoreError> {
        let mut statement = self.conn.prepare(
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'
             ORDER BY name",
        )?;
        let names = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        let mut out = Vec::new();
        for name in names {
            // The name comes from the schema of the database, and it is quoted.
            let quoted = name.replace('"', "\"\"");
            let count: i64 =
                self.conn
                    .query_row(&format!("SELECT count(*) FROM \"{quoted}\""), [], |row| {
                        row.get(0)
                    })?;
            out.push((name, u64::try_from(count).unwrap_or_default()));
        }
        Ok(out)
    }
}

impl Store {
    /// Gives the database its identity when it has none: the UUIDs of the machine and of the
    /// session, and log epoch 1. A database that has them keeps them.
    pub(crate) fn ensure_identity(&self) -> Result<(), StoreError> {
        for key in [MACHINE_UUID, SESSION_UUID] {
            if self.meta(key)?.is_none() {
                self.set_meta(key, &random_ulid()?)?;
            }
        }
        if self.meta(LOG_EPOCH)?.is_none() {
            self.set_meta(LOG_EPOCH, "1")?;
        }
        Ok(())
    }

    /// The identity of the log and the number of its last event, as a cursor.
    pub fn cursor_now(&self) -> Result<Cursor, StoreError> {
        Ok(Cursor {
            machine_uuid: self.require_meta(MACHINE_UUID)?,
            session_uuid: self.require_meta(SESSION_UUID)?,
            log_epoch: meta_number(self.meta(LOG_EPOCH)?, LOG_EPOCH)?,
            seq: self.last_seq()?,
        })
    }

    fn require_meta(&self, key: &str) -> Result<String, StoreError> {
        self.meta(key)?
            .ok_or_else(|| StoreError::BadValue(format!("meta {key} is missing")))
    }

    /// The sequence number of the last event that was ever appended. Sweeps do not lower it.
    pub fn last_seq(&self) -> Result<u64, StoreError> {
        let seq: Option<i64> = self
            .conn
            .query_row(
                "SELECT seq FROM sqlite_sequence WHERE name = 'events'",
                [],
                |row| row.get(0),
            )
            .optional()?;
        Ok(u64::try_from(seq.unwrap_or(0)).unwrap_or(0))
    }

    /// Runs `f` in one transaction and commits it when `f` returns `Ok`. The events that `f`
    /// appended come back with the result, after the commit. When `f` fails, or panics, nothing
    /// stays: no state row and no event.
    pub fn tx<R>(
        &mut self,
        now_ms: i64,
        f: impl FnOnce(&mut Tx<'_>) -> Result<R, StoreError>,
    ) -> Result<(R, Vec<Event>), StoreError> {
        self.guard()?;
        let result = Self::run_tx(&mut self.conn, now_ms, false, f);
        self.note(result)
    }

    /// Runs `f` in a read transaction. It works while the store is degraded. `f` cannot append
    /// events or take handle numbers, and it must not write.
    pub fn tx_read<R>(
        &mut self,
        now_ms: i64,
        f: impl FnOnce(&mut Tx<'_>) -> Result<R, StoreError>,
    ) -> Result<R, StoreError> {
        Self::run_tx(&mut self.conn, now_ms, true, f).map(|(value, _)| value)
    }

    fn run_tx<R>(
        conn: &mut rusqlite::Connection,
        now_ms: i64,
        read_only: bool,
        f: impl FnOnce(&mut Tx<'_>) -> Result<R, StoreError>,
    ) -> Result<(R, Vec<Event>), StoreError> {
        let behavior = if read_only {
            rusqlite::TransactionBehavior::Deferred
        } else {
            rusqlite::TransactionBehavior::Immediate
        };
        let tx = conn.transaction_with_behavior(behavior)?;
        let mut tx = Tx {
            tx,
            now_ms,
            events: Vec::new(),
            read_only,
        };
        let result = f(&mut tx)?;
        let Tx { tx, events, .. } = tx;
        tx.commit()?;
        Ok((result, events))
    }

    /// The events after `cursor` that match `filter`, at most `limit` of them.
    pub fn events_since(
        &self,
        cursor: &Cursor,
        filter: &EventFilter,
        limit: usize,
    ) -> Result<EventsRead, StoreError> {
        let now = self.cursor_now()?;
        let floor = meta_number(self.meta(EVENTS_FLOOR)?, EVENTS_FLOOR)?;
        let truncated = |reason| EventsRead::Truncated {
            reason,
            cursor: now.clone(),
        };
        if cursor.machine_uuid != now.machine_uuid
            || cursor.session_uuid != now.session_uuid
            || cursor.log_epoch != now.log_epoch
        {
            return Ok(truncated(TruncatedReason::Epoch));
        }
        if cursor.seq > now.seq {
            return Ok(truncated(TruncatedReason::Ahead));
        }
        if cursor.seq < floor {
            return Ok(truncated(TruncatedReason::Expired));
        }

        let mut sql = String::from(
            "SELECT seq, at, kind, v, subject, payload_json FROM events WHERE seq > ?",
        );
        let mut args: Vec<Sql> = vec![Sql::Integer(i64::try_from(cursor.seq).unwrap_or(i64::MAX))];
        if !filter.kinds.is_empty() {
            sql.push_str(" AND (");
            for (i, kind) in filter.kinds.iter().enumerate() {
                if i > 0 {
                    sql.push_str(" OR ");
                }
                match kind.strip_suffix('*') {
                    // A prefix. `substr` compares characters, and a `LIKE` would take `_` as a
                    // wildcard.
                    Some(prefix) => {
                        sql.push_str("substr(kind, 1, ?) = ?");
                        args.push(Sql::Integer(
                            i64::try_from(prefix.chars().count()).unwrap_or(i64::MAX),
                        ));
                        args.push(Sql::Text(prefix.to_owned()));
                    }
                    None => {
                        sql.push_str("kind = ?");
                        args.push(Sql::Text(kind.clone()));
                    }
                }
            }
            sql.push(')');
        }
        if let Some(subject) = &filter.subject {
            sql.push_str(" AND subject = ?");
            args.push(Sql::Text(subject.clone()));
        }
        sql.push_str(" ORDER BY seq LIMIT ?");
        let fetch = limit.saturating_add(1);
        args.push(Sql::Integer(i64::try_from(fetch).unwrap_or(i64::MAX)));

        let mut statement = self.conn.prepare(&sql)?;
        let mut events = statement
            .query_map(params_from_iter(args), |row| {
                let seq: i64 = row.get(0)?;
                let at: i64 = row.get(1)?;
                let v: i64 = row.get(3)?;
                let payload: String = row.get(5)?;
                Ok((seq, at, row.get::<_, String>(2)?, v, row.get(4)?, payload))
            })?
            .map(|row| {
                let (seq, at, kind, v, subject, payload) = row?;
                Ok(Event {
                    seq: u64::try_from(seq)
                        .map_err(|_| StoreError::BadValue(format!("seq {seq}")))?,
                    at: Timestamp(at),
                    kind,
                    v: u16::try_from(v).map_err(|_| StoreError::BadValue(format!("v {v}")))?,
                    subject,
                    payload: serde_json::from_str(&payload)
                        .map_err(|e| StoreError::BadValue(format!("event payload: {e}")))?,
                })
            })
            .collect::<Result<Vec<Event>, StoreError>>()?;
        let more = events.len() > limit;
        events.truncate(limit);
        let seq = match events.last() {
            Some(last) => last.seq,
            // Nothing matched. No matching event exists up to the end of the log.
            None => now.seq,
        };
        Ok(EventsRead::Events {
            events,
            cursor: Cursor { seq, ..now },
            more,
        })
    }

    /// Deletes the events that are older than the retention allows. Events of an audit
    /// kind stay for `retention.audit`, all others for `retention.sync`. A cursor below the
    /// highest deleted `seq` is truncated from now on.
    pub fn retention_sweep(
        &mut self,
        now_ms: i64,
        retention: &Retention,
    ) -> Result<Swept, StoreError> {
        let before = |age: Duration| {
            now_ms.saturating_sub(i64::try_from(age.as_millis()).unwrap_or(i64::MAX))
        };
        let sync_cut = before(retention.sync);
        let audit_cut = before(retention.audit);
        let audit_sql = "(kind LIKE 'interaction.%' OR kind LIKE 'policy.%' OR kind LIKE 'task.%'
                          OR kind = 'agent.state_changed')";
        let (swept, _) = self.tx(now_ms, |tx| {
            let sql = tx.sql();
            let mut floor = 0i64;
            let mut deleted = Swept::default();
            for (audit, cut) in [(false, sync_cut), (true, audit_cut)] {
                let condition = if audit {
                    format!("{audit_sql} AND at < ?1")
                } else {
                    format!("NOT {audit_sql} AND at < ?1")
                };
                let top: Option<i64> = sql.query_row(
                    &format!("SELECT max(seq) FROM events WHERE {condition}"),
                    [cut],
                    |row| row.get(0),
                )?;
                let count = sql.execute(&format!("DELETE FROM events WHERE {condition}"), [cut])?;
                if audit {
                    deleted.audit = count;
                } else {
                    deleted.sync = count;
                }
                floor = floor.max(top.unwrap_or(0));
            }
            if floor > 0 {
                raise_floor(sql, floor)?;
            }
            Ok(deleted)
        })?;
        Ok(swept)
    }

    /// The version of a projection. A projection that never changed has version 0.
    pub fn projection_version(&self, name: &str) -> Result<u64, StoreError> {
        let version: Option<i64> = self
            .conn
            .query_row(
                "SELECT version FROM projection_versions WHERE name = ?1",
                [name],
                |row| row.get(0),
            )
            .optional()?;
        Ok(u64::try_from(version.unwrap_or(0)).unwrap_or(0))
    }
}

/// Raises the floor to `to` when it is higher than the floor now.
fn raise_floor(sql: &Transaction<'_>, to: i64) -> Result<(), StoreError> {
    sql.execute(
        "INSERT INTO meta (key, value) VALUES (?1, ?2)
         ON CONFLICT (key) DO UPDATE SET value = max(cast(value AS INTEGER), cast(excluded.value AS INTEGER))",
        params![EVENTS_FLOOR, to.to_string()],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use nh_proto::api::events::{EventKind, PaneExited};
    use serde_json::json;

    use super::*;

    const DAY_MS: i64 = 24 * 3600 * 1000;

    fn store() -> Store {
        Store::open_in_memory().expect("a store")
    }

    fn pane_exited() -> EventPayload {
        EventPayload::PaneExited(PaneExited {
            pane: "01M4F1W7EM2GN3EV6NHXZJEGGA".parse().expect("an id"),
            code: Some(0),
        })
    }

    fn groups(store: &Store) -> i64 {
        store
            .conn
            .query_row("SELECT count(*) FROM groups", [], |row| row.get(0))
            .expect("a count")
    }

    fn insert_group(tx: &Tx<'_>, id: &str) -> Result<(), StoreError> {
        tx.sql().execute(
            "INSERT INTO groups (id, name, sort_key, created_at, updated_at) VALUES (?1, 'g', 'a', 0, 0)",
            [id],
        )?;
        Ok(())
    }

    fn append(store: &mut Store, kind: &str, subject: Option<&str>, at: i64) -> u64 {
        let (seq, _) = store
            .tx(at, |tx| tx.append_raw(kind, 1, subject, json!({})))
            .expect("an event");
        seq
    }

    #[test]
    fn a_secret_in_a_payload_does_not_reach_the_log() {
        let mut store = store();
        let key = format!("{}{}", "ghp_", "a1".repeat(20));
        let (seq, _) = store
            .tx(1, |tx| {
                tx.append_raw(
                    "debug.tick",
                    1,
                    None,
                    json!({"command": format!("git push https://ada:{key}@example.com"), "password": "hunter2hunter2", "n": 3}),
                )
            })
            .expect("an event");
        let stored: String = store
            .conn
            .query_row(
                "SELECT payload_json FROM events WHERE seq = ?1",
                [i64::try_from(seq).expect("a number")],
                |row| row.get(0),
            )
            .expect("a payload");
        assert!(
            !stored.contains(&key) && !stored.contains("hunter2"),
            "{stored}"
        );
        assert!(stored.contains("[redacted:url_credentials]"), "{stored}");
        assert!(stored.contains("\"n\":3"), "{stored}");
    }

    fn events(read: EventsRead) -> (Vec<u64>, Cursor, bool) {
        match read {
            EventsRead::Events {
                events,
                cursor,
                more,
            } => (events.iter().map(|e| e.seq).collect(), cursor, more),
            other => panic!("expected events, got {other:?}"),
        }
    }

    #[test]
    fn a_new_database_has_an_identity_that_a_reopen_keeps() {
        let dir = std::env::temp_dir().join(format!("nh-store-events-id-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a directory");
        let path = dir.join("state.db");
        let first = Store::open(&path)
            .expect("opens")
            .cursor_now()
            .expect("a cursor");
        let again = Store::open(&path)
            .expect("opens again")
            .cursor_now()
            .expect("a cursor");
        assert_eq!(first, again);
        assert_eq!((first.log_epoch, first.seq), (1, 0));
        assert_eq!(first.machine_uuid.len(), 26);
        assert_ne!(first.machine_uuid, first.session_uuid);
        let other = store().cursor_now().expect("a cursor");
        assert_ne!(other.session_uuid, first.session_uuid);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_committed_transaction_has_its_rows_and_its_events() {
        let mut store = store();
        let (value, events) = store
            .tx(5_000, |tx| {
                insert_group(tx, "g1")?;
                let first = tx.append(Some("pane-1"), &pane_exited())?;
                let second = tx.append_raw("workspace.created", 1, None, json!({"n": 1}))?;
                Ok((first, second))
            })
            .expect("commits");
        assert_eq!(value, (1, 2));
        assert_eq!(groups(&store), 1);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].kind, "pane.exited");
        assert_eq!(events[0].subject.as_deref(), Some("pane-1"));
        assert_eq!(events[0].at, Timestamp(5_000));
        assert_eq!(events[1].seq, 2);
        assert_eq!(store.last_seq().expect("a seq"), 2);
    }

    #[test]
    fn a_failed_transaction_leaves_neither_rows_nor_events() {
        let mut store = store();
        let result: Result<((), Vec<Event>), StoreError> = store.tx(1, |tx| {
            insert_group(tx, "g1")?;
            tx.append(None, &pane_exited())?;
            Err(StoreError::BadValue("the work failed".to_owned()))
        });
        assert!(result.is_err());
        assert_eq!(groups(&store), 0);
        assert_eq!(store.last_seq().expect("a seq"), 0);
        let read = store
            .events_since(
                &store.cursor_now().expect("a cursor"),
                &EventFilter::default(),
                10,
            )
            .expect("a read");
        assert_eq!(events(read).0, Vec::<u64>::new());
    }

    #[test]
    fn a_sql_error_in_the_middle_rolls_everything_back() {
        let mut store = store();
        let result = store.tx(1, |tx| {
            insert_group(tx, "g1")?;
            tx.append(None, &pane_exited())?;
            // The same primary key.
            insert_group(tx, "g1")
        });
        assert!(result.is_err());
        assert_eq!(groups(&store), 0);
        assert_eq!(store.last_seq().expect("a seq"), 0);
    }

    #[test]
    fn a_panic_in_the_transaction_rolls_it_back() {
        let mut store = store();
        let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = store.tx(1, |tx| {
                insert_group(tx, "g1")?;
                tx.append(None, &pane_exited())?;
                panic!("the work panicked");
                #[allow(unreachable_code)]
                Ok(())
            });
        }));
        assert!(caught.is_err());
        assert_eq!(groups(&store), 0);
        assert_eq!(store.last_seq().expect("a seq"), 0);
        // The store is usable afterwards.
        assert_eq!(append(&mut store, "x.y", None, 2), 1);
    }

    #[test]
    fn a_cursor_gets_exactly_the_events_after_it() {
        let mut store = store();
        let start = store.cursor_now().expect("a cursor");
        for n in 0..5 {
            append(&mut store, "pane.created", None, n);
        }
        let (seqs, cursor, more) = events(
            store
                .events_since(&start, &EventFilter::default(), 100)
                .expect("a read"),
        );
        assert_eq!(seqs, [1, 2, 3, 4, 5]);
        assert_eq!(cursor.seq, 5);
        assert!(!more);

        let after_two = Cursor {
            seq: 2,
            ..start.clone()
        };
        let (seqs, _, _) = events(
            store
                .events_since(&after_two, &EventFilter::default(), 100)
                .expect("a read"),
        );
        assert_eq!(seqs, [3, 4, 5]);

        let (seqs, cursor, more) = events(
            store
                .events_since(&cursor, &EventFilter::default(), 100)
                .expect("a read"),
        );
        assert!(
            seqs.is_empty() && !more,
            "the cursor at the end gets nothing"
        );
        assert_eq!(cursor.seq, 5);
    }

    #[test]
    fn a_limit_cuts_the_list_and_the_cursor_continues_from_it() {
        let mut store = store();
        let start = store.cursor_now().expect("a cursor");
        for n in 0..5 {
            append(&mut store, "pane.created", None, n);
        }
        let (seqs, cursor, more) = events(
            store
                .events_since(&start, &EventFilter::default(), 2)
                .expect("a read"),
        );
        assert_eq!((seqs, more, cursor.seq), (vec![1, 2], true, 2));
        let (seqs, cursor, more) = events(
            store
                .events_since(&cursor, &EventFilter::default(), 2)
                .expect("a read"),
        );
        assert_eq!((seqs, more, cursor.seq), (vec![3, 4], true, 4));
        let (seqs, _, more) = events(
            store
                .events_since(&cursor, &EventFilter::default(), 2)
                .expect("a read"),
        );
        assert_eq!((seqs, more), (vec![5], false));
    }

    #[test]
    fn a_filter_takes_exact_kinds_prefixes_and_subjects() {
        let mut store = store();
        let start = store.cursor_now().expect("a cursor");
        append(&mut store, "interaction.opened", Some("i1"), 0);
        append(&mut store, "interaction.decided", Some("i1"), 0);
        append(&mut store, "agent.state_changed", Some("r1"), 0);
        append(&mut store, "agentXstate_changed", Some("r1"), 0);
        append(&mut store, "pane.exited", Some("p1"), 0);
        let read = |kinds: &[&str], subject: Option<&str>| {
            let filter = EventFilter {
                kinds: kinds.iter().map(|k| (*k).to_owned()).collect(),
                subject: subject.map(str::to_owned),
            };
            events(store.events_since(&start, &filter, 100).expect("a read")).0
        };
        assert_eq!(read(&["interaction.*"], None), [1, 2]);
        assert_eq!(read(&["pane.exited"], None), [5]);
        assert_eq!(read(&["pane.exited", "agent.state_changed"], None), [3, 5]);
        assert_eq!(read(&[], Some("r1")), [3, 4]);
        assert_eq!(read(&["interaction.*"], Some("i1")), [1, 2]);
        assert_eq!(
            read(&["agent.state_*"], None),
            [3],
            "an underscore is not a wildcard"
        );
        assert_eq!(read(&["nothing.*"], None), Vec::<u64>::new());
    }

    #[test]
    fn a_filter_that_matches_nothing_moves_the_cursor_to_the_end() {
        let mut store = store();
        let start = store.cursor_now().expect("a cursor");
        for n in 0..3 {
            append(&mut store, "pane.created", None, n);
        }
        let filter = EventFilter {
            kinds: vec!["task.*".to_owned()],
            subject: None,
        };
        let (seqs, cursor, more) = events(store.events_since(&start, &filter, 10).expect("a read"));
        assert!(seqs.is_empty() && !more);
        assert_eq!(cursor.seq, 3);
    }

    #[test]
    fn a_cursor_of_another_epoch_session_or_machine_is_truncated() {
        let mut store = store();
        let start = store.cursor_now().expect("a cursor");
        append(&mut store, "pane.created", None, 0);
        let read = |cursor: &Cursor| {
            store
                .events_since(cursor, &EventFilter::default(), 10)
                .expect("a read")
        };
        for other in [
            Cursor {
                session_uuid: "x".to_owned(),
                ..start.clone()
            },
            Cursor {
                machine_uuid: "x".to_owned(),
                ..start.clone()
            },
            Cursor {
                log_epoch: 9,
                ..start.clone()
            },
        ] {
            assert!(matches!(
                read(&other),
                EventsRead::Truncated {
                    reason: TruncatedReason::Epoch,
                    ..
                }
            ));
        }
        assert!(matches!(
            read(&Cursor {
                seq: 99,
                ..start.clone()
            }),
            EventsRead::Truncated {
                reason: TruncatedReason::Ahead,
                ..
            }
        ));
        let EventsRead::Truncated { cursor, .. } = read(&Cursor {
            log_epoch: 9,
            ..start
        }) else {
            panic!("truncated")
        };
        assert_eq!(cursor.seq, 1, "the new cursor is the end of the log");
    }

    #[test]
    fn the_sweep_keeps_audit_kinds_for_the_audit_period() {
        let mut store = store();
        let start = store.cursor_now().expect("a cursor");
        let now = 200 * DAY_MS;
        // Ten days old: past the sync retention, inside the audit retention.
        let old = now - 10 * DAY_MS;
        append(&mut store, "pane.created", None, old); // 1
        append(&mut store, "interaction.opened", None, old); // 2
        append(&mut store, "agent.state_changed", None, old); // 3
        append(&mut store, "policy.added", None, old); // 4
        append(&mut store, "task.created", None, old); // 5
        // A hundred days old: past the audit retention.
        append(&mut store, "task.closed", None, now - 100 * DAY_MS); // 6
        // Fresh.
        append(&mut store, "pane.created", None, now - DAY_MS); // 7

        let swept = store
            .retention_sweep(now, &Retention::default())
            .expect("a sweep");
        assert_eq!(swept, Swept { sync: 1, audit: 1 });
        let kept: Vec<u64> = {
            let cursor = Cursor {
                seq: 6,
                ..store.cursor_now().expect("a cursor")
            };
            // Read from the floor, which is 6 now.
            events(
                store
                    .events_since(&cursor, &EventFilter::default(), 100)
                    .expect("a read"),
            )
            .0
        };
        assert_eq!(kept, [7]);
        let all: i64 = store
            .conn
            .query_row("SELECT count(*) FROM events", [], |row| row.get(0))
            .expect("a count");
        assert_eq!(all, 5, "four audit events and the fresh one stay");
        let audit: Vec<String> = store
            .conn
            .prepare("SELECT kind FROM events ORDER BY seq")
            .expect("a statement")
            .query_map([], |row| row.get(0))
            .expect("a query")
            .collect::<Result<_, _>>()
            .expect("kinds");
        assert_eq!(
            audit,
            [
                "interaction.opened",
                "agent.state_changed",
                "policy.added",
                "task.created",
                "pane.created"
            ]
        );
        // An old cursor has a gap now.
        assert!(matches!(
            store
                .events_since(&start, &EventFilter::default(), 10)
                .expect("a read"),
            EventsRead::Truncated {
                reason: TruncatedReason::Expired,
                ..
            }
        ));
        assert_eq!(
            store.last_seq().expect("a seq"),
            7,
            "the sweep does not lower the sequence"
        );
    }

    #[test]
    fn a_cursor_inside_the_retained_window_stays_valid_after_a_sweep() {
        let mut store = store();
        let now = 100 * DAY_MS;
        append(&mut store, "pane.created", None, now - 20 * DAY_MS); // 1: gone
        append(&mut store, "pane.created", None, now - 2 * DAY_MS); // 2
        append(&mut store, "pane.created", None, now - DAY_MS); // 3
        store
            .retention_sweep(now, &Retention::default())
            .expect("a sweep");
        let cursor = Cursor {
            seq: 1,
            ..store.cursor_now().expect("a cursor")
        };
        let (seqs, _, _) = events(
            store
                .events_since(&cursor, &EventFilter::default(), 10)
                .expect("a read"),
        );
        assert_eq!(
            seqs,
            [2, 3],
            "the deleted event is the cursor itself, so nothing is missing"
        );
        let behind = Cursor { seq: 0, ..cursor };
        assert!(matches!(
            store
                .events_since(&behind, &EventFilter::default(), 10)
                .expect("a read"),
            EventsRead::Truncated {
                reason: TruncatedReason::Expired,
                ..
            }
        ));
    }

    #[test]
    fn a_sweep_with_nothing_to_delete_changes_nothing() {
        let mut store = store();
        append(&mut store, "pane.created", None, 1_000);
        let swept = store
            .retention_sweep(2_000, &Retention::default())
            .expect("a sweep");
        assert_eq!(swept, Swept::default());
        let start = Cursor {
            seq: 0,
            ..store.cursor_now().expect("a cursor")
        };
        assert_eq!(
            events(
                store
                    .events_since(&start, &EventFilter::default(), 10)
                    .expect("a read")
            )
            .0,
            [1]
        );
    }

    #[test]
    fn audit_kinds_are_interactions_policies_tasks_and_agent_states() {
        for kind in [
            "interaction.opened",
            "agent.state_changed",
            "policy.added",
            "task.closed",
        ] {
            assert!(is_audit_kind(kind), "{kind}");
        }
        for kind in [
            "pane.created",
            "agent.detected",
            "workspace.renamed",
            "interactions.x",
        ] {
            assert!(!is_audit_kind(kind), "{kind}");
        }
        assert!(EventKind::ALL.iter().any(|k| is_audit_kind(k.name())));
    }

    #[test]
    fn handle_counters_count_each_scope_and_never_repeat() {
        let mut store = store();
        let (numbers, _) = store
            .tx(1, |tx| {
                Ok([
                    tx.next_handle("workspace")?,
                    tx.next_handle("workspace")?,
                    tx.next_handle("pane:w1")?,
                    tx.next_handle("workspace")?,
                    tx.next_handle("pane:w2")?,
                    tx.next_handle("pane:w1")?,
                ])
            })
            .expect("handles");
        assert_eq!(numbers, [1, 2, 1, 3, 1, 2]);
        let (again, _) = store
            .tx(2, |tx| tx.next_handle("workspace"))
            .expect("a handle");
        assert_eq!(again, 4);
    }

    #[test]
    fn a_handle_of_a_failed_transaction_is_not_spent() {
        let mut store = store();
        let failed: Result<(u32, Vec<Event>), StoreError> = store.tx(1, |tx| {
            tx.next_handle("task")?;
            Err(StoreError::BadValue("no".to_owned()))
        });
        assert!(failed.is_err());
        let (first, _) = store.tx(2, |tx| tx.next_handle("task")).expect("a handle");
        assert_eq!(first, 1, "nothing used number 1");
    }

    #[test]
    fn a_projection_has_a_version_that_grows_with_each_bump() {
        let mut store = store();
        assert_eq!(store.projection_version("panes").expect("a version"), 0);
        let (versions, _) = store
            .tx(1, |tx| {
                Ok((
                    tx.bump_projection("panes")?,
                    tx.bump_projection("panes")?,
                    tx.bump_projection("tree")?,
                ))
            })
            .expect("bumps");
        assert_eq!(versions, (1, 2, 1));
        assert_eq!(store.projection_version("panes").expect("a version"), 2);
        let failed: Result<((), Vec<Event>), StoreError> = store.tx(2, |tx| {
            tx.bump_projection("panes")?;
            Err(StoreError::BadValue("no".to_owned()))
        });
        assert!(failed.is_err());
        assert_eq!(store.projection_version("panes").expect("a version"), 2);
    }
}
