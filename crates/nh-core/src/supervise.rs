//! Supervised tasks.
//!
//! A task of the core that panics must not take the core down, and it must not vanish without a
//! trace. [`Supervisor::spawn`] runs a task as a child task of the runtime and watches how it
//! ends:
//!
//! - **It returns.** That is the end. Nothing restarts it.
//! - **It panics.** The panic goes to the log. If the [`Restart`] policy allows, the supervisor
//!   waits a little (100 ms, doubling, at most 2 s) and starts the task again with a new future. A
//!   run that lasted 60 s before it panicked starts the count again. If the policy does not allow
//!   a restart, the supervisor gives up: it puts a warning in `health` (`task:<name>`) and runs the
//!   `on_giveup` hook of the task. A pane task uses the hook to mark its pane degraded.
//! - **It is cancelled** (the runtime stops, or the caller aborts the handle). The child ends
//!   with it.

use std::{any::Any, future::Future, sync::Arc, time::Duration};

use tokio::{
    runtime::Handle,
    task::{AbortHandle, JoinHandle},
};

use crate::health::Health;

/// How often a task may be started again after a panic.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Restart {
    /// A panic is the end.
    Never,
    /// At most this many restarts.
    Times(u32),
}

/// The wait before the first restart, and the longest wait.
const FIRST_DELAY: Duration = Duration::from_millis(100);
const LONGEST_DELAY: Duration = Duration::from_secs(2);
/// A task that ran this long has recovered from its earlier panics.
const STABLE: Duration = Duration::from_secs(60);

/// What runs when the supervisor gives up. It gets the text of the last panic.
type GiveUp = Box<dyn FnOnce(&str) + Send>;

/// A task to supervise: its name and what to do when it panics.
pub struct Task {
    name: String,
    restart: Restart,
    stable_after: Duration,
    on_giveup: Option<GiveUp>,
}

impl Task {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            restart: Restart::Never,
            stable_after: STABLE,
            on_giveup: None,
        }
    }

    pub fn restart(mut self, restart: Restart) -> Self {
        self.restart = restart;
        self
    }

    /// A run that lasts this long before it panics counts as a good run, so the restarts start
    /// again from zero. The default is 60 s.
    pub fn stable_after(mut self, time: Duration) -> Self {
        self.stable_after = time;
        self
    }

    /// Runs `hook` with the text of the last panic when the supervisor gives up.
    pub fn on_giveup(mut self, hook: impl FnOnce(&str) + Send + 'static) -> Self {
        self.on_giveup = Some(Box::new(hook));
        self
    }
}

/// Aborts the child task when the supervising task is dropped.
struct AbortOnDrop(AbortHandle);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// Starts supervised tasks on a runtime.
#[derive(Clone)]
pub struct Supervisor {
    runtime: Handle,
    health: Arc<Health>,
}

impl Supervisor {
    pub fn new(runtime: Handle, health: Arc<Health>) -> Self {
        Self { runtime, health }
    }

    /// Runs the future that `make` returns. After a panic, and if `task` allows it, `make` runs
    /// again for a new future. Abort the handle to stop the task.
    pub fn spawn<F, Fut>(&self, task: Task, make: F) -> JoinHandle<()>
    where
        F: Fn() -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let runtime = self.runtime.clone();
        let health = Arc::clone(&self.health);
        self.runtime.spawn(async move {
            let Task {
                name,
                restart,
                stable_after,
                on_giveup,
            } = task;
            let mut restarts = 0u32;
            loop {
                let run_started = std::time::Instant::now();
                let child = runtime.spawn(make());
                let _guard = AbortOnDrop(child.abort_handle());
                match child.await {
                    Ok(()) => return,
                    Err(error) if error.is_cancelled() => return,
                    Err(error) => {
                        let text = panic_text(error.into_panic());
                        if run_started.elapsed() >= stable_after {
                            restarts = 0;
                        }
                        let allowed = match restart {
                            Restart::Never => false,
                            Restart::Times(max) => restarts < max,
                        };
                        if allowed {
                            tracing::error!(task = %name, panic = %text, "task panicked; starting it again");
                            restarts += 1;
                            let delay = FIRST_DELAY
                                .saturating_mul(1 << restarts.saturating_sub(1).min(8))
                                .min(LONGEST_DELAY);
                            tokio::time::sleep(delay).await;
                            continue;
                        }
                        tracing::error!(task = %name, panic = %text, "task panicked; giving up");
                        health.warn(
                            &format!("task:{name}"),
                            format!("The task {name} stopped after a panic: {text}"),
                            Some("Restart neohar. If this repeats, report it.".to_owned()),
                        );
                        if let Some(hook) = on_giveup {
                            hook(&text);
                        }
                        return;
                    }
                }
            }
        })
    }

    /// Runs one future that cannot be made again, for example a task that owns a listener.
    /// A panic is the end.
    pub fn spawn_once<Fut>(&self, name: impl Into<String>, future: Fut) -> JoinHandle<()>
    where
        Fut: Future<Output = ()> + Send + 'static,
    {
        let slot = std::sync::Mutex::new(Some(future));
        self.spawn(Task::new(name), move || {
            let future = slot.lock().ok().and_then(|mut slot| slot.take());
            async move {
                if let Some(future) = future {
                    future.await;
                }
            }
        })
    }
}

/// The message of a panic, when it has one.
fn panic_text(payload: Box<dyn Any + Send>) -> String {
    if let Some(text) = payload.downcast_ref::<&str>() {
        (*text).to_owned()
    } else if let Some(text) = payload.downcast_ref::<String>() {
        text.clone()
    } else {
        "a panic without a message".to_owned()
    }
}

#[cfg(test)]
mod tests;
