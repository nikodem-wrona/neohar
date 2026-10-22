//! The watch of a directory: `config.toml`, the harness manifests, and the worktree of a
//! task while a window follows its changes.
//!
//! The app uses the watcher of the system (the feature `fsevents`, on by default): FSEvents on
//! macOS and inotify on Linux, through `notify`. `nh` links the core for `nh daemon` without the
//! feature, because FSEvents links CoreServices into the binary, and its load costs the hook shim
//! about 1 ms of its 5 ms budget at each start. There the watch looks at the entries of
//! the directory every [`POLL`]: their modification time, size and inode.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::mpsc::{self, RecvTimeoutError},
    thread::JoinHandle,
    time::{Duration, SystemTime},
};

/// How often a polling watch looks at the directory.
pub const POLL: Duration = Duration::from_secs(1);

/// A change in the watched directory. `paths` names the entries that changed. It is empty when
/// the watcher does not know them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Change {
    pub paths: Vec<PathBuf>,
}

/// How a watch finds changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// The watcher of the system. A build without the feature `fsevents` polls instead.
    System,
    /// A look at the entries every [`POLL`].
    Poll,
}

impl Kind {
    /// The kind of this build.
    pub const fn of_build() -> Self {
        if cfg!(feature = "fsevents") {
            Self::System
        } else {
            Self::Poll
        }
    }
}

/// A watch that runs until it drops.
pub struct DirWatch {
    _inner: Inner,
}

/// The watcher that the watch keeps alive.
enum Inner {
    #[cfg(feature = "fsevents")]
    System {
        _watcher: notify::RecommendedWatcher,
    },
    Poll {
        _poller: Poller,
    },
}

/// Watches the entries of `dir`, not its subdirectories, and calls `on_change` for each change.
pub fn watch_dir(
    kind: Kind,
    dir: &Path,
    on_change: impl Fn(Change) + Send + 'static,
) -> std::io::Result<DirWatch> {
    let inner = match kind {
        #[cfg(feature = "fsevents")]
        Kind::System => Inner::System {
            _watcher: system(dir, notify::RecursiveMode::NonRecursive, on_change)?,
        },
        #[cfg(not(feature = "fsevents"))]
        Kind::System => Inner::Poll {
            _poller: Poller::start(dir, on_change)?,
        },
        Kind::Poll => Inner::Poll {
            _poller: Poller::start(dir, on_change)?,
        },
    };
    Ok(DirWatch { _inner: inner })
}

/// Watches `dir` with all its directories through the watcher of the system, and calls
/// `on_change` for each change. A build without the feature `fsevents` has no such watch, because
/// a poll of a whole tree costs too much: it gives `None`.
pub fn watch_tree(
    dir: &Path,
    on_change: impl Fn(Change) + Send + 'static,
) -> std::io::Result<Option<DirWatch>> {
    #[cfg(feature = "fsevents")]
    {
        let watcher = system(dir, notify::RecursiveMode::Recursive, on_change)?;
        Ok(Some(DirWatch {
            _inner: Inner::System { _watcher: watcher },
        }))
    }
    #[cfg(not(feature = "fsevents"))]
    {
        let _ = (dir, on_change);
        Ok(None)
    }
}

#[cfg(feature = "fsevents")]
fn system(
    dir: &Path,
    mode: notify::RecursiveMode,
    on_change: impl Fn(Change) + Send + 'static,
) -> std::io::Result<notify::RecommendedWatcher> {
    use notify::Watcher;
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        // An error of the watcher names no path, and the callers ignore it.
        if let Ok(event) = event {
            on_change(Change { paths: event.paths });
        }
    })
    .map_err(std::io::Error::other)?;
    watcher.watch(dir, mode).map_err(std::io::Error::other)?;
    Ok(watcher)
}

/// What a poll knows of an entry.
type Seen = HashMap<PathBuf, (Option<SystemTime>, u64, u64)>;

/// The thread of a polling watch. It ends when the watch drops.
struct Poller {
    stop: Option<mpsc::Sender<()>>,
    thread: Option<JoinHandle<()>>,
}

impl Poller {
    fn start(dir: &Path, on_change: impl Fn(Change) + Send + 'static) -> std::io::Result<Self> {
        let dir = dir.to_owned();
        let mut seen = look(&dir);
        let (stop, stopped) = mpsc::channel::<()>();
        let thread = std::thread::Builder::new()
            .name("nh-watch-poll".to_owned())
            .spawn(move || {
                while let Err(RecvTimeoutError::Timeout) = stopped.recv_timeout(POLL) {
                    let now = look(&dir);
                    let mut paths: Vec<PathBuf> = now
                        .iter()
                        .filter(|(path, state)| seen.get(*path) != Some(state))
                        .map(|(path, _)| path.clone())
                        .collect();
                    paths.extend(seen.keys().filter(|p| !now.contains_key(*p)).cloned());
                    if !paths.is_empty() {
                        paths.sort();
                        on_change(Change { paths });
                    }
                    seen = now;
                }
            })?;
        Ok(Self {
            stop: Some(stop),
            thread: Some(thread),
        })
    }
}

impl Drop for Poller {
    fn drop(&mut self) {
        drop(self.stop.take());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// The entries of `dir` now. A directory that cannot be read has none.
fn look(dir: &Path) -> Seen {
    use std::os::unix::fs::MetadataExt;
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Seen::new();
    };
    entries
        .flatten()
        .filter_map(|entry| {
            let meta = entry.metadata().ok()?;
            Some((entry.path(), (meta.modified().ok(), meta.len(), meta.ino())))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex, PoisonError};

    use super::*;

    #[test]
    fn a_polling_watch_sees_a_write_a_new_file_and_a_removal() {
        let dir = std::env::temp_dir().join(format!("nh-watch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a directory");
        let file = dir.join("config.toml");
        std::fs::write(&file, "a = 1\n").expect("a file");
        let changes = Arc::new(Mutex::new(Vec::<Change>::new()));
        let sink = Arc::clone(&changes);
        let watch = watch_dir(Kind::Poll, &dir, move |change| {
            sink.lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(change);
        })
        .expect("a watch");
        let wait_for = |path: &Path| {
            let deadline = std::time::Instant::now() + POLL * 10;
            loop {
                let found = changes
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .drain(..)
                    .any(|c| c.paths.iter().any(|p| p == path));
                if found {
                    return;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "no change of {path:?}"
                );
                std::thread::sleep(Duration::from_millis(50));
            }
        };
        // A write in place that changes the size.
        std::fs::write(&file, "a = 12\n").expect("a write");
        wait_for(&file);
        // A new file, then the end of it.
        let other = dir.join("claude.toml");
        std::fs::write(&other, "x").expect("a new file");
        wait_for(&other);
        std::fs::remove_file(&other).expect("a removal");
        wait_for(&other);
        drop(watch);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
