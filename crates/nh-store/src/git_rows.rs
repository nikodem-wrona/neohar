//! The repositories that the person added to the Git view.
//!
//! `git_repos` has one row for each path that `git.repo_add` got. The core finds the repository of
//! each path again when it reads the rows, so a row whose folder went away is only left out.

use rusqlite::params;

use crate::{Store, StoreError, Tx};

/// A repository that the person added.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitRepoRow {
    /// The path that the person gave: a folder in a worktree of the repository.
    pub path: String,
    /// Unix milliseconds.
    pub added_at: i64,
}

fn read_all(conn: &rusqlite::Connection) -> Result<Vec<GitRepoRow>, StoreError> {
    let mut statement = conn.prepare("SELECT path, added_at FROM git_repos ORDER BY added_at")?;
    let rows = statement
        .query_map([], |row| {
            Ok(GitRepoRow {
                path: row.get(0)?,
                added_at: row.get(1)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

impl Tx<'_> {
    /// Keeps a repository. A path that the table has already keeps its first time. Returns
    /// whether the row is new.
    pub fn add_git_repo(&self, path: &str, at: i64) -> Result<bool, StoreError> {
        self.writable()?;
        let added = self.sql().execute(
            "INSERT INTO git_repos (path, added_at) VALUES (?1, ?2) ON CONFLICT (path) DO NOTHING",
            params![path, at],
        )?;
        Ok(added > 0)
    }

    /// The repositories that the person added, the oldest first.
    pub fn git_repos(&self) -> Result<Vec<GitRepoRow>, StoreError> {
        read_all(self.sql())
    }
}

impl Store {
    /// The repositories that the person added, the oldest first.
    pub fn git_repos(&self) -> Result<Vec<GitRepoRow>, StoreError> {
        read_all(&self.conn)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_is_kept_once_with_its_first_time() {
        let mut store = Store::open_in_memory().expect("a store");
        let (added, _) = store
            .tx(1, |tx| {
                let first = tx.add_git_repo("/work/app", 10)?;
                let again = tx.add_git_repo("/work/app", 20)?;
                tx.add_git_repo("/work/lib", 15)?;
                Ok((first, again))
            })
            .expect("a transaction");
        assert_eq!(added, (true, false));
        let rows = store.git_repos().expect("rows");
        let paths: Vec<(&str, i64)> = rows.iter().map(|r| (r.path.as_str(), r.added_at)).collect();
        assert_eq!(paths, [("/work/app", 10), ("/work/lib", 15)]);
    }
}
