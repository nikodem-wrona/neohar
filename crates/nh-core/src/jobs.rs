//! Jobs: commands that take long.
//!
//! A command that needs more than a moment (`agent.start`) starts a job and returns its id. The
//! worker reports progress through a [`JobHandle`], and the `jobs` projection shows it. A job ends
//! in one of three states: `done`, `failed` or `cancelled`, and an end state does not change again.
//!
//! - **Cancel.** `jobs.cancel` marks the job `cancelled` at once and tells the worker through its
//!   [`CancelToken`]. The worker stops when it can. What it reports after that is ignored.
//! - **No lost job.** A [`JobHandle`] that is dropped without a result fails its job, so that a
//!   worker that panics or returns early does not leave a job that runs for ever.
//! - **Memory only.** Jobs are not in the database. After a restart there are none.
//! - **Limit.** The projection keeps the last [`FINISHED_KEPT`] jobs that ended.

use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex, MutexGuard, PoisonError},
};

use nh_proto::api::{
    envelope::{CallError, ErrorCode},
    ids::JobId,
    model::{JobStatus, JobValue},
    projections::Projection,
    time::Timestamp,
};
use serde_json::Value;
use tokio::sync::watch;

use crate::{error::RpcError, ids::new_ulid, persist::now_ms, projection::ProjectionHub};

/// How many jobs that ended stay in the projection.
pub const FINISHED_KEPT: usize = 50;

/// How a worker learns that its job was cancelled. Cloning is cheap.
#[derive(Clone, Debug)]
pub struct CancelToken(watch::Receiver<bool>);

impl CancelToken {
    pub fn is_cancelled(&self) -> bool {
        *self.0.borrow()
    }

    /// Completes when the job is cancelled. It never completes for a job that is not.
    pub async fn cancelled(&self) {
        let mut receiver = self.0.clone();
        if receiver.wait_for(|cancelled| *cancelled).await.is_err() {
            // The registry dropped the job, and nobody can cancel it any more.
            std::future::pending::<()>().await;
        }
    }
}

struct Entry {
    value: JobValue,
    cancel: watch::Sender<bool>,
}

#[derive(Default)]
struct Inner {
    entries: HashMap<JobId, Entry>,
    /// The jobs that ended, the oldest first.
    finished: VecDeque<JobId>,
}

/// The registry of the jobs of the core.
pub struct Jobs {
    hub: Arc<ProjectionHub>,
    inner: Mutex<Inner>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Jobs {
    pub fn new(hub: Arc<ProjectionHub>) -> Arc<Self> {
        Arc::new(Self {
            hub,
            inner: Mutex::new(Inner::default()),
        })
    }

    /// Starts a job. The projection shows it as `running`.
    pub fn start(self: &Arc<Self>, title: impl Into<String>) -> Result<JobHandle, RpcError> {
        let id = JobId::new(new_ulid()?);
        let (cancel, token) = watch::channel(false);
        let value = JobValue {
            id,
            title: title.into(),
            status: JobStatus::Running,
            progress: None,
            message: None,
            result: None,
            error: None,
            started_at: Timestamp(now_ms()),
            finished_at: None,
        };
        let mut inner = lock(&self.inner);
        self.publish(&value);
        inner.entries.insert(id, Entry { value, cancel });
        Ok(JobHandle {
            jobs: Arc::clone(self),
            id,
            token: CancelToken(token),
            reported: false,
        })
    }

    /// The row of a job, if the registry has it.
    pub fn get(&self, id: JobId) -> Option<JobValue> {
        lock(&self.inner)
            .entries
            .get(&id)
            .map(|entry| entry.value.clone())
    }

    /// Cancels a job that runs. An unknown job is `not_found`, and a job that ended is
    /// `invalid_params`.
    pub fn cancel(&self, id: JobId) -> Result<(), RpcError> {
        let mut inner = lock(&self.inner);
        let Some(entry) = inner.entries.get_mut(&id) else {
            return Err(RpcError::new(ErrorCode::NotFound, format!("no job {id}")));
        };
        if entry.value.status != JobStatus::Running {
            return Err(RpcError::invalid_params(format!(
                "the job {id} already ended ({})",
                status_name(entry.value.status)
            )));
        }
        // The worker is told after the row says `cancelled`, so a worker that reacts at once
        // cannot find the row `running`.
        entry.value.status = JobStatus::Cancelled;
        entry.value.finished_at = Some(Timestamp(now_ms()));
        entry.value.message = Some("Cancelled".to_owned());
        let _ = entry.cancel.send(true);
        self.publish(&entry.value);
        Self::ended(&self.hub, &mut inner, id);
        Ok(())
    }

    fn publish(&self, value: &JobValue) {
        if let Ok(json) = serde_json::to_value(value) {
            self.hub.set(Projection::Jobs, &value.id.to_string(), json);
        }
    }

    /// Changes the row of a job that runs. A job that ended is left alone.
    fn update(&self, id: JobId, change: impl FnOnce(&mut JobValue)) {
        let mut inner = lock(&self.inner);
        let Some(entry) = inner.entries.get_mut(&id) else {
            return;
        };
        if entry.value.status != JobStatus::Running {
            return;
        }
        change(&mut entry.value);
        self.publish(&entry.value);
        if entry.value.status != JobStatus::Running {
            Self::ended(&self.hub, &mut inner, id);
        }
    }

    /// Notes that a job ended, and drops the oldest rows beyond the limit.
    fn ended(hub: &ProjectionHub, inner: &mut Inner, id: JobId) {
        inner.finished.push_back(id);
        while inner.finished.len() > FINISHED_KEPT {
            if let Some(old) = inner.finished.pop_front() {
                inner.entries.remove(&old);
                hub.remove(Projection::Jobs, &old.to_string());
            }
        }
    }
}

fn status_name(status: JobStatus) -> &'static str {
    match status {
        JobStatus::Running => "running",
        JobStatus::Done => "done",
        JobStatus::Failed => "failed",
        JobStatus::Cancelled => "cancelled",
    }
}

/// What the worker of a job holds. It reports progress and the end.
pub struct JobHandle {
    jobs: Arc<Jobs>,
    id: JobId,
    token: CancelToken,
    /// The worker reported the end.
    reported: bool,
}

impl JobHandle {
    pub fn id(&self) -> JobId {
        self.id
    }

    pub fn token(&self) -> CancelToken {
        self.token.clone()
    }

    pub fn is_cancelled(&self) -> bool {
        self.token.is_cancelled()
    }

    /// Reports the progress, from 0 to 1, and a text for the person.
    pub fn progress(&self, fraction: f32, message: impl Into<String>) {
        let fraction = if fraction.is_nan() {
            0.0
        } else {
            fraction.clamp(0.0, 1.0)
        };
        let message = message.into();
        self.jobs.update(self.id, |job| {
            job.progress = Some(fraction);
            job.message = Some(message);
        });
    }

    /// Reports a text and keeps the progress.
    pub fn message(&self, message: impl Into<String>) {
        let message = message.into();
        self.jobs.update(self.id, |job| job.message = Some(message));
    }

    /// The job is done. `result` says what it made, for example `{"task": "…"}`.
    pub fn done(mut self, result: Value) {
        self.reported = true;
        self.jobs.update(self.id, |job| {
            job.status = JobStatus::Done;
            job.progress = Some(1.0);
            job.result = Some(result);
            job.finished_at = Some(Timestamp(now_ms()));
        });
    }

    /// The job failed.
    pub fn fail(mut self, error: CallError) {
        self.reported = true;
        self.finish_failed(error);
    }

    fn finish_failed(&self, error: CallError) {
        self.jobs.update(self.id, |job| {
            job.status = JobStatus::Failed;
            job.error = Some(error);
            job.finished_at = Some(Timestamp(now_ms()));
        });
    }
}

impl Drop for JobHandle {
    fn drop(&mut self) {
        if !self.reported {
            self.finish_failed(CallError {
                code: ErrorCode::Internal,
                message: "the job ended without a result".to_owned(),
                details: Value::Null,
            });
        }
    }
}

#[cfg(test)]
mod tests;
