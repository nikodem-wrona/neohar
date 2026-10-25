//! The repositories that the person added to the Git view, and the repositories that the person
//! trusts.
//!
//! `git_repos` has one row for each path that `git.repo_add` got. The core finds the repository of
//! each path again when it reads the rows, so a row whose folder went away is only left out.
//!
//! `git_trust` has one row for each repository that the person trusts (`git.trust`), by its common
//! git directory. The core runs git only in such a repository.

use rusqlite::params;

use crate::{Store, StoreError, Tx};

/// A repository that the person trusts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitTrustRow {
    /// The canonical path of the common git directory.
    pub common_dir: String,
    /// The fingerprint of the repository at the time of the trust: a JSON array of `[key, value]`
    /// pairs, for its settings and for its hook files (`hooks/<name>`).
    pub settings: String,
    /// Unix milliseconds.
    pub trusted_at: i64,
}

fn read_trust(conn: &rusqlite::Connection) -> Result<Vec<GitTrustRow>, StoreError> {
    let mut statement =
        conn.prepare("SELECT common_dir, settings, trusted_at FROM git_trust ORDER BY trusted_at")?;
    let rows = statement
        .query_map([], |row| {
            Ok(GitTrustRow {
                common_dir: row.get(0)?,
                settings: row.get(1)?,
                trusted_at: row.get(2)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

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

    /// Trusts a repository with its settings now. A trust again replaces the settings and the
    /// time.
    pub fn trust_git_repo(
        &self,
        common_dir: &str,
        settings: &str,
        at: i64,
    ) -> Result<(), StoreError> {
        self.writable()?;
        self.sql().execute(
            "INSERT INTO git_trust (common_dir, settings, trusted_at) VALUES (?1, ?2, ?3)
             ON CONFLICT (common_dir) DO UPDATE SET settings = ?2, trusted_at = ?3",
            params![common_dir, settings, at],
        )?;
        Ok(())
    }

    /// Takes back the trust of a repository. Returns whether it had a row.
    pub fn untrust_git_repo(&self, common_dir: &str) -> Result<bool, StoreError> {
        self.writable()?;
        let removed = self.sql().execute(
            "DELETE FROM git_trust WHERE common_dir = ?1",
            params![common_dir],
        )?;
        Ok(removed > 0)
    }

    /// The repositories that the person trusts, the oldest trust first.
    pub fn git_trust(&self) -> Result<Vec<GitTrustRow>, StoreError> {
        read_trust(self.sql())
    }
}

impl Store {
    /// The repositories that the person added, the oldest first.
    pub fn git_repos(&self) -> Result<Vec<GitRepoRow>, StoreError> {
        read_all(&self.conn)
    }

    /// The repositories that the person trusts, the oldest trust first.
    pub fn git_trust(&self) -> Result<Vec<GitTrustRow>, StoreError> {
        read_trust(&self.conn)
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

    #[test]
    fn a_trust_again_replaces_the_settings_and_a_removal_says_whether_it_had_a_row() {
        let mut store = Store::open_in_memory().expect("a store");
        store
            .tx(1, |tx| {
                tx.trust_git_repo("/work/app/.git", "[]", 10)?;
                tx.trust_git_repo("/work/lib/.git", "[]", 15)?;
                tx.trust_git_repo("/work/app/.git", r#"[["core.pager","less"]]"#, 20)
            })
            .expect("a transaction");
        let rows = store.git_trust().expect("rows");
        assert_eq!(
            rows,
            [
                GitTrustRow {
                    common_dir: "/work/lib/.git".to_owned(),
                    settings: "[]".to_owned(),
                    trusted_at: 15,
                },
                GitTrustRow {
                    common_dir: "/work/app/.git".to_owned(),
                    settings: r#"[["core.pager","less"]]"#.to_owned(),
                    trusted_at: 20,
                },
            ]
        );
        let (removed, _) = store
            .tx(2, |tx| {
                Ok((
                    tx.untrust_git_repo("/work/app/.git")?,
                    tx.untrust_git_repo("/work/app/.git")?,
                ))
            })
            .expect("a transaction");
        assert_eq!(removed, (true, false));
        assert_eq!(store.git_trust().expect("rows").len(), 1);
    }
}
