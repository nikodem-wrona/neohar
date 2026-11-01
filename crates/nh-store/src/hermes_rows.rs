//! The open chats with Hermes Agent: `hermes_chats`.

use rusqlite::{Row, params};

use crate::{Store, StoreError, Tx};

/// One open chat.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HermesChatRow {
    /// The chat id, a ULID.
    pub id: String,
    pub cwd: String,
    /// The session id of Hermes.
    pub session: Option<String>,
    pub title: Option<String>,
    pub started_at: i64,
}

const COLUMNS: &str = "id, cwd, session, title, started_at";

fn chat_of(row: &Row<'_>) -> Result<HermesChatRow, rusqlite::Error> {
    Ok(HermesChatRow {
        id: row.get(0)?,
        cwd: row.get(1)?,
        session: row.get(2)?,
        title: row.get(3)?,
        started_at: row.get(4)?,
    })
}

fn read_chats(conn: &rusqlite::Connection) -> Result<Vec<HermesChatRow>, StoreError> {
    let mut statement = conn.prepare(&format!(
        "SELECT {COLUMNS} FROM hermes_chats ORDER BY started_at, id"
    ))?;
    let rows = statement.query_map([], chat_of)?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

impl Tx<'_> {
    /// Adds a chat or changes its session and its title.
    pub fn put_hermes_chat(&self, row: &HermesChatRow) -> Result<(), StoreError> {
        self.writable()?;
        self.sql().execute(
            &format!(
                "INSERT INTO hermes_chats ({COLUMNS}) VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT (id) DO UPDATE SET session = excluded.session,
                     title = excluded.title"
            ),
            params![row.id, row.cwd, row.session, row.title, row.started_at],
        )?;
        Ok(())
    }

    /// Removes a chat. Says whether it existed.
    pub fn delete_hermes_chat(&self, id: &str) -> Result<bool, StoreError> {
        self.writable()?;
        Ok(self
            .sql()
            .execute("DELETE FROM hermes_chats WHERE id = ?1", [id])?
            > 0)
    }
}

impl Store {
    /// The open chats, the oldest first.
    pub fn hermes_chats(&self) -> Result<Vec<HermesChatRow>, StoreError> {
        read_chats(&self.conn)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chat(id: &str, started_at: i64) -> HermesChatRow {
        HermesChatRow {
            id: id.to_owned(),
            cwd: "/work".to_owned(),
            session: None,
            title: None,
            started_at,
        }
    }

    #[test]
    fn a_chat_is_stored_changed_and_removed() {
        let mut store = Store::open_in_memory().expect("a store");
        store
            .tx(1, |tx| {
                tx.put_hermes_chat(&chat("b", 2))?;
                tx.put_hermes_chat(&chat("a", 1))
            })
            .expect("stored");
        let mut changed = chat("b", 99);
        changed.session = Some("local-1".to_owned());
        changed.title = Some("Fix the tests".to_owned());
        store
            .tx(1, |tx| tx.put_hermes_chat(&changed))
            .expect("changed");
        let chats = store.hermes_chats().expect("read");
        assert_eq!(chats.len(), 2);
        assert_eq!(chats[0].id, "a");
        assert_eq!(chats[1].session.as_deref(), Some("local-1"));
        assert_eq!(chats[1].title.as_deref(), Some("Fix the tests"));
        assert_eq!(chats[1].started_at, 2, "the start does not change");
        assert!(
            store
                .tx(1, |tx| tx.delete_hermes_chat("a"))
                .expect("deleted")
                .0
        );
        assert!(
            !store
                .tx(1, |tx| tx.delete_hermes_chat("a"))
                .expect("deleted")
                .0
        );
        assert_eq!(store.hermes_chats().expect("read").len(), 1);
    }
}
