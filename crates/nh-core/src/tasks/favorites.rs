//! The favorite folders of the form of a new task: `favorite.list`, `favorite.add` and
//! `favorite.remove`.
//!
//! A favorite is the root of a git repository, as `task.create` keeps the repository of a task.
//! The rows are in the table `favorite_folders` and in [`super::Tasks`], and the projection
//! `favorites` shows them, the one used last first: a task that the person makes with a favorite,
//! or a draft that moves to one, makes it the first (`super::add`, `super::change`). A new
//! favorite is the first too. The form of a new task chooses the first, so the person who works in
//! one repository picks nothing.

use std::path::{Path, PathBuf};

use nh_proto::api::{
    calls::agent::FavoriteAdded,
    events::{EventPayload, FavoriteEvent},
    model::FavoriteValue,
    time::Timestamp,
};
use nh_store::FavoriteRow;

use super::service::{TaskService, expand, gone};
use crate::{ErrorCode, error::RpcError, persist::now_ms};

/// The last `count` parts of a path, joined with `/`.
fn tail(path: &str, count: usize) -> String {
    let mut parts: Vec<&str> = path
        .rsplit('/')
        .filter(|part| !part.is_empty())
        .take(count)
        .collect();
    parts.reverse();
    parts.join("/")
}

/// The favorites for the API. The name is the last part of the path. When two favorites have the
/// same last part, each of them shows its last two parts.
pub(crate) fn values(rows: &[FavoriteRow]) -> Vec<FavoriteValue> {
    rows.iter()
        .map(|row| {
            let short = tail(&row.path, 1);
            let shared = rows
                .iter()
                .filter(|other| tail(&other.path, 1) == short)
                .count()
                > 1;
            let name = if shared { tail(&row.path, 2) } else { short };
            FavoriteValue {
                name: if name.is_empty() {
                    row.path.clone()
                } else {
                    name
                },
                path: row.path.clone(),
                used_at: Timestamp(row.used_at),
            }
        })
        .collect()
}

/// A path without a `/` at the end and without `.` parts, so that it compares with a stored one.
fn plain(path: &Path) -> String {
    path.components().collect::<PathBuf>().display().to_string()
}

impl TaskService {
    /// The favorites, the one used last first.
    pub(crate) async fn favorites(&self) -> Result<Vec<FavoriteValue>, RpcError> {
        self.actor()?
            .update(|parts| values(parts.state.tasks.favorites()))
            .await
            .map_err(gone)
    }

    /// Adds the repository of `path` to the favorites. The row and the event are one transaction.
    pub(crate) async fn favorite_add(&self, path: &str) -> Result<FavoriteAdded, RpcError> {
        let root = self.repo_of(path).await?.display().to_string();
        let kept = root.clone();
        self.actor()?
            .update(move |mut parts| {
                if parts.state.tasks.favorite(&root).is_none() {
                    let now = now_ms();
                    let row = FavoriteRow {
                        path: root,
                        added_at: now,
                        used_at: now,
                    };
                    let event = EventPayload::FavoriteAdded(FavoriteEvent {
                        path: row.path.clone(),
                    });
                    parts
                        .commit(|tx| {
                            tx.put_favorite(&row)?;
                            tx.append(Some(&row.path), &event)
                        })
                        .map_err(gone)?;
                    parts.state.tasks.put_favorite(row);
                    super::publish_favorites(&parts, None);
                }
                Ok::<_, RpcError>(())
            })
            .await
            .map_err(gone)??;
        Ok(FavoriteAdded { path: kept })
    }

    /// Removes a favorite: the one with this path, else the one of the repository of this folder.
    /// The path of a favorite works also when its folder is gone.
    pub(crate) async fn favorite_remove(&self, path: &str) -> Result<(), RpcError> {
        let given = plain(&expand(path, &self.env));
        let known = self
            .actor()?
            .update({
                let given = given.clone();
                move |parts| parts.state.tasks.favorite(&given).is_some()
            })
            .await
            .map_err(gone)?;
        let wanted = if known {
            given
        } else {
            let root = self.repo_of(path).await.map_err(|_| not_a_favorite(path))?;
            root.display().to_string()
        };
        self.actor()?
            .update(move |mut parts| {
                if parts.state.tasks.favorite(&wanted).is_none() {
                    return Err(not_a_favorite(&wanted));
                }
                let event = EventPayload::FavoriteRemoved(FavoriteEvent {
                    path: wanted.clone(),
                });
                parts
                    .commit(|tx| {
                        tx.delete_favorite(&wanted)?;
                        tx.append(Some(&wanted), &event)
                    })
                    .map_err(gone)?;
                parts.state.tasks.remove_favorite(&wanted);
                super::publish_favorites(&parts, Some(&wanted));
                Ok(())
            })
            .await
            .map_err(gone)?
    }
}

fn not_a_favorite(path: &str) -> RpcError {
    RpcError::new(
        ErrorCode::NotFound,
        format!("{path} is not a favorite folder"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(path: &str, used_at: i64) -> FavoriteRow {
        FavoriteRow {
            path: path.to_owned(),
            added_at: 1,
            used_at,
        }
    }

    #[test]
    fn a_favorite_is_named_by_its_folder_and_by_two_parts_when_the_name_repeats() {
        let rows = [
            row("/work/neohar", 3),
            row("/work/client/app", 2),
            row("/work/server/app", 1),
        ];
        let names: Vec<String> = values(&rows).into_iter().map(|value| value.name).collect();
        assert_eq!(names, ["neohar", "client/app", "server/app"]);
        assert_eq!(values(&rows)[0].used_at, Timestamp(3));
        assert_eq!(values(&[row("/", 1)])[0].name, "/");
    }

    #[test]
    fn a_path_compares_without_a_slash_at_the_end() {
        assert_eq!(plain(Path::new("/work/app/")), "/work/app");
        assert_eq!(plain(Path::new("/work/./app")), "/work/app");
    }
}
