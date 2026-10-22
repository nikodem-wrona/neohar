//! The methods `task.*`, `changes.*`, `favorite.*`, `worktree.list` and `fs.dirs`.

use std::{path::PathBuf, sync::Arc};

use nh_proto::api::{
    calls::agent::{FavoriteListResult, TaskListResult, TaskShown, WorktreeListResult},
    common::{Empty, JobStarted},
    envelope::{CallError, ErrorCode},
    methods::{Call, Method},
    model::{TaskValue, WorktreeValue},
};
use serde::Serialize;
use serde_json::{Value, to_value};

use crate::{actor::Origin, error::RpcError, runtime::Inner, window_calls::check_owner};

/// Whether this module runs the method.
pub(crate) fn handles(method: &str) -> bool {
    matches!(
        Method::from_name(method),
        Some(
            Method::TaskCreate
                | Method::TaskUpdate
                | Method::TaskStart
                | Method::TaskDelete
                | Method::TaskList
                | Method::TaskShow
                | Method::TaskChanges
                | Method::TaskDiff
                | Method::ChangesFollow
                | Method::ChangesDiff
                | Method::TaskClose
                | Method::FavoriteList
                | Method::FavoriteAdd
                | Method::FavoriteRemove
                | Method::WorktreeList
                | Method::FsDirs
        )
    )
}

fn json_of<T: Serialize>(value: &T) -> Result<Value, RpcError> {
    to_value(value).map_err(|e| RpcError::internal(e.to_string()))
}

fn gone(error: impl std::fmt::Display) -> RpcError {
    RpcError::internal(error.to_string())
}

/// The error of a job from the error of a call.
pub(crate) fn job_error(error: &RpcError) -> CallError {
    CallError {
        code: error.code,
        message: error.message.clone(),
        details: error.details.clone(),
    }
}

/// Runs a call that [`handles`] accepted.
pub(crate) async fn run(inner: &Arc<Inner>, call: Call, origin: Origin) -> Result<Value, RpcError> {
    match call {
        Call::TaskCreate(p) => json_of(&inner.tasks.create(p).await?),
        Call::TaskUpdate(p) => {
            inner.tasks.update(p).await?;
            json_of(&Empty {})
        }
        Call::TaskStart(p) => {
            let job = inner.tasks.start(p.task).await?;
            json_of(&JobStarted { job })
        }
        Call::TaskDelete(p) => {
            inner.tasks.delete(p.task).await?;
            json_of(&Empty {})
        }
        Call::TaskList(p) => {
            let tasks: Vec<TaskValue> = inner
                .actor
                .update(move |parts| {
                    parts
                        .state
                        .tasks
                        .all()
                        .into_iter()
                        .map(|task| super::value(&parts, task))
                        .filter(|task| p.status.is_none_or(|status| task.status == status))
                        .collect()
                })
                .await
                .map_err(gone)?;
            // Someone looks at the tasks: their lines are counted again, in a moment.
            for task in &tasks {
                inner.tasks.schedule_diff(task.id);
            }
            json_of(&TaskListResult { tasks })
        }
        Call::TaskShow(p) => {
            let task = inner
                .actor
                .update(move |parts| {
                    let id = parts.state.tasks.resolve(&p.task)?;
                    parts
                        .state
                        .tasks
                        .get(id)
                        .map(|task| super::value(&parts, task))
                })
                .await
                .map_err(gone)?
                .ok_or_else(|| RpcError::new(ErrorCode::NotFound, "no such task"))?;
            inner.tasks.schedule_diff(task.id);
            json_of(&TaskShown { task })
        }
        Call::TaskChanges(p) => json_of(&inner.tasks.changes(p.task).await?),
        Call::TaskDiff(p) => json_of(&inner.tasks.diff(p.task, p.path).await?),
        Call::ChangesFollow(p) => {
            inner.render.require_window(p.window)?;
            check_owner(inner, &origin, p.window)?;
            let folder = PathBuf::from(&p.path);
            if !folder.is_absolute() {
                return Err(RpcError::invalid_params("the path must be absolute"));
            }
            inner.tasks.follow_folder(p.window, folder);
            json_of(&Empty {})
        }
        Call::ChangesDiff(p) => {
            inner.render.require_window(p.window)?;
            check_owner(inner, &origin, p.window)?;
            json_of(&inner.tasks.window_diff(p.window, p.path).await?)
        }
        Call::TaskClose(p) => {
            inner
                .tasks
                .close(p.task, p.remove_worktree, p.force)
                .await?;
            json_of(&Empty {})
        }
        Call::FavoriteList(_) => json_of(&FavoriteListResult {
            favorites: inner.tasks.favorites().await?,
        }),
        Call::FavoriteAdd(p) => json_of(&inner.tasks.favorite_add(&p.path).await?),
        Call::FavoriteRemove(p) => {
            inner.tasks.favorite_remove(&p.path).await?;
            json_of(&Empty {})
        }
        Call::FsDirs(p) => json_of(&inner.tasks.dirs(p.path).await?),
        Call::WorktreeList(p) => {
            let listed = inner.tasks.worktrees(p.repo).await?;
            let tasks: Vec<(String, nh_proto::api::ids::TaskId)> = inner
                .actor
                .update(|parts| {
                    parts
                        .state
                        .tasks
                        .all()
                        .iter()
                        .map(|task| (task.row.worktree_path.clone(), task.id()))
                        .collect()
                })
                .await
                .map_err(gone)?;
            let worktrees = listed
                .into_iter()
                .map(|(worktree, repo, dirty)| {
                    let path = worktree.path.display().to_string();
                    WorktreeValue {
                        task: tasks.iter().find(|(p, _)| *p == path).map(|(_, id)| *id),
                        path,
                        branch: worktree.branch,
                        repo: repo.display().to_string(),
                        dirty,
                    }
                })
                .collect();
            json_of(&WorktreeListResult { worktrees })
        }
        other => Err(RpcError::internal(format!("not a task call: {other:?}"))),
    }
}
