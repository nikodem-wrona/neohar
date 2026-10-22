//! The favorite folders: `favorite_folders`. The form of a new task offers them, and it
//! chooses the one that the person used last.

use rusqlite::{Row, params};

use crate::{Store, StoreError, Tx};

/// A favorite folder: the root of a git repository.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FavoriteRow {
    pub path: String,
    pub added_at: i64,
    /// The last task that the person made or changed with this repository, else `added_at`.
    pub used_at: i64,
}

fn from_row(row: &Row<'_>) -> Result<FavoriteRow, StoreError> {
    Ok(FavoriteRow {
        path: row.get(0)?,
        added_at: row.get(1)?,
        used_at: row.get(2)?,
    })
}

/// The favorites, the one used last first.
fn read_favorites(conn: &rusqlite::Connection) -> Result<Vec<FavoriteRow>, StoreError> {
    let mut statement = conn.prepare(
        "SELECT path, added_at, used_at FROM favorite_folders ORDER BY used_at DESC, path",
    )?;
    let mut rows = statement.query([])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(from_row(row)?);
    }
    Ok(out)
}

impl Tx<'_> {
    /// Adds a favorite. A favorite with the same path stays as it is.
    pub fn put_favorite(&self, row: &FavoriteRow) -> Result<(), StoreError> {
        self.writable()?;
        self.sql().execute(
            "INSERT INTO favorite_folders (path, added_at, used_at) VALUES (?1, ?2, ?3)
             ON CONFLICT (path) DO NOTHING",
            params![row.path, row.added_at, row.used_at],
        )?;
        Ok(())
    }

    /// Removes a favorite. Says whether there was one.
    pub fn delete_favorite(&self, path: &str) -> Result<bool, StoreError> {
        self.writable()?;
        let removed = self
            .sql()
            .execute("DELETE FROM favorite_folders WHERE path = ?1", [path])?;
        Ok(removed > 0)
    }

    /// Notes that a task used the repository at `path`. Says whether it is a favorite.
    pub fn use_favorite(&self, path: &str, at: i64) -> Result<bool, StoreError> {
        self.writable()?;
        let changed = self.sql().execute(
            "UPDATE favorite_folders SET used_at = ?2 WHERE path = ?1",
            params![path, at],
        )?;
        Ok(changed > 0)
    }

    pub fn favorites(&self) -> Result<Vec<FavoriteRow>, StoreError> {
        read_favorites(self.sql())
    }
}

impl Store {
    pub fn favorites(&self) -> Result<Vec<FavoriteRow>, StoreError> {
        read_favorites(&self.conn)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(path: &str, at: i64) -> FavoriteRow {
        FavoriteRow {
            path: path.to_owned(),
            added_at: at,
            used_at: at,
        }
    }

    #[test]
    fn the_favorite_used_last_comes_first() {
        let mut store = Store::open_in_memory().expect("a store");
        store
            .tx(1, |tx| {
                tx.put_favorite(&row("/work/app", 1))?;
                tx.put_favorite(&row("/work/api", 2))
            })
            .expect("two favorites");
        let paths = |store: &Store| -> Vec<String> {
            let rows = store.favorites().expect("the favorites");
            rows.into_iter().map(|row| row.path).collect()
        };
        assert_eq!(paths(&store), ["/work/api", "/work/app"]);

        let (used, _) = store
            .tx(3, |tx| tx.use_favorite("/work/app", 3))
            .expect("a use");
        assert!(used);
        assert_eq!(paths(&store), ["/work/app", "/work/api"]);
        let (other, _) = store
            .tx(4, |tx| tx.use_favorite("/work/web", 4))
            .expect("a use");
        assert!(
            !other,
            "a repository that is not a favorite changes nothing"
        );
        assert_eq!(store.favorites().expect("the favorites")[0].added_at, 1);
    }

    #[test]
    fn a_favorite_is_added_once_and_removed() {
        let mut store = Store::open_in_memory().expect("a store");
        store
            .tx(1, |tx| tx.put_favorite(&row("/work/app", 1)))
            .expect("a favorite");
        store
            .tx(5, |tx| tx.put_favorite(&row("/work/app", 5)))
            .expect("the same favorite");
        assert_eq!(
            store.favorites().expect("the favorites"),
            [row("/work/app", 1)]
        );
        let (removed, _) = store
            .tx(6, |tx| tx.delete_favorite("/work/app"))
            .expect("a removal");
        assert!(removed);
        let (again, _) = store
            .tx(7, |tx| tx.delete_favorite("/work/app"))
            .expect("a removal");
        assert!(!again);
        assert!(store.favorites().expect("the favorites").is_empty());
    }
}
