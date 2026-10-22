//! The harness manifests of the core.
//!
//! The registry holds the manifests in force: the built-in ones and the files in
//! `<config>/harnesses/`. It loads them at the start and again when a file changes, and when
//! `config.reload` runs. A file that is not right is left out, and the doctor says so (`health`).
//! A manifest that is new or changed gets the event `harness.manifest_loaded`.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, MutexGuard, PoisonError,
        atomic::{AtomicU64, Ordering},
        mpsc::{Receiver, RecvTimeoutError, Sender, channel},
    },
    time::{Duration, Instant},
};

use arc_swap::ArcSwap;
use nh_agents::{Loaded, load, manifest::loader::write_builtins, version::VersionCache};
use nh_proto::api::events::{EventPayload, HarnessManifestLoaded};

use crate::{actor::ActorHandle, health::Health};

/// The directory of the files of the user, in the configuration directory.
pub const DIR_NAME: &str = "harnesses";

/// How long the files must be quiet before the manifests load again.
const DEBOUNCE: Duration = Duration::from_millis(300);

/// A moment after the start of a watch in which a write can get no event (see the config watcher).
const START_LAG: Duration = Duration::from_millis(1500);

/// The manifests in force.
pub struct Harnesses {
    dir: PathBuf,
    loaded: ArcSwap<Loaded>,
    /// Counts the loads. A manifest that a load brought carries the number.
    serial: AtomicU64,
    health: Arc<Health>,
    /// The version of each executable that was probed, so that a probe runs once.
    versions: VersionCache,
    actor: Mutex<Option<ActorHandle>>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// What a load changed: a manifest that is new, or whose content differs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    pub id: String,
    pub version: String,
    pub source: String,
    pub serial: u64,
}

impl Harnesses {
    /// Loads the manifests. `reference` is where the built-in ones are written for a person to
    /// read. A directory that cannot be made is not an error: the built-in manifests still work.
    pub fn load(dir: PathBuf, reference: Option<&Path>, health: Arc<Health>) -> Arc<Self> {
        if let Some(reference) = reference
            && let Err(error) = write_builtins(reference)
        {
            tracing::debug!(%error, "the built-in manifests are not written for reference");
        }
        let registry = Arc::new(Self {
            loaded: ArcSwap::from_pointee(Loaded::default()),
            dir,
            serial: AtomicU64::new(0),
            health,
            versions: VersionCache::new(),
            actor: Mutex::new(None),
        });
        registry.load_first();
        registry
    }

    /// The manifests in force.
    pub fn current(&self) -> Arc<Loaded> {
        self.loaded.load_full()
    }

    /// The cache of the versions that probes found.
    pub fn versions(&self) -> &VersionCache {
        &self.versions
    }

    /// Gives the registry the state actor, so that it can write its events. It then writes the
    /// event of every manifest in force: the start of a core is a load.
    pub(crate) fn attach(&self, actor: ActorHandle) {
        *lock(&self.actor) = Some(actor);
        self.emit(self.changes_since(&Loaded::default(), &self.current()));
    }

    /// Reads the files again. Gives the manifests that changed.
    pub fn reload(&self) -> Vec<Change> {
        let before = self.current();
        let after = Arc::new(load(Some(&self.dir)));
        let changes = self.changes_since(&before, &after);
        self.loaded.store(Arc::clone(&after));
        self.publish_warnings(&after);
        self.emit(changes.clone());
        changes
    }

    /// The first load, without events: there is no actor yet. [`Harnesses::attach`] writes them.
    fn load_first(&self) {
        let loaded = Arc::new(load(Some(&self.dir)));
        self.loaded.store(Arc::clone(&loaded));
        self.publish_warnings(&loaded);
    }

    fn publish_warnings(&self, loaded: &Loaded) {
        self.health.set_warnings(
            "harness",
            loaded.warnings.iter().map(ToString::to_string),
            Some("The manifest that was in force before stays in force."),
        );
    }

    /// The manifests of `after` that `before` did not have, or that have other content.
    fn changes_since(&self, before: &Loaded, after: &Loaded) -> Vec<Change> {
        let old: BTreeMap<&String, String> = before
            .manifests
            .iter()
            .map(|(id, entry)| (id, entry.fingerprint()))
            .collect();
        let fresh: Vec<_> = after
            .manifests
            .iter()
            .filter(|(id, entry)| old.get(id) != Some(&entry.fingerprint()))
            .collect();
        if fresh.is_empty() {
            return Vec::new();
        }
        let serial = self.serial.fetch_add(1, Ordering::Relaxed) + 1;
        fresh
            .into_iter()
            .map(|(id, entry)| Change {
                id: id.clone(),
                version: entry.fingerprint(),
                source: entry.source.text(),
                serial,
            })
            .collect()
    }

    /// Writes the events through the state actor. Without an actor nothing is written.
    fn emit(&self, changes: Vec<Change>) {
        if changes.is_empty() {
            return;
        }
        let Some(actor) = lock(&self.actor).clone() else {
            return;
        };
        actor.update_detached(move |mut parts| {
            let stored = parts.commit(|tx| {
                for change in &changes {
                    tx.append(
                        Some(&change.id),
                        &EventPayload::HarnessManifestLoaded(HarnessManifestLoaded {
                            id: change.id.clone(),
                            version: change.version.clone(),
                            source: change.source.clone(),
                            serial: change.serial,
                        }),
                    )?;
                }
                Ok(())
            });
            if let Err(error) = stored {
                tracing::warn!(%error, "the events of the manifests are not stored");
            }
        });
    }

    /// Starts a thread that watches the directory of the files. A change makes the manifests load
    /// again once the files are quiet. `on_change` gets what changed.
    pub fn watch(self: &Arc<Self>) -> std::io::Result<HarnessWatcher> {
        std::fs::create_dir_all(&self.dir)?;
        let (tx, rx) = channel::<Message>();
        let events = tx.clone();
        let registry = Arc::clone(self);
        let dir = self.dir.clone();
        let (ready_tx, ready_rx) = channel::<Result<(), String>>();
        let thread = std::thread::Builder::new()
            .name("nh-harness-watch".to_owned())
            .spawn(move || {
                let started =
                    crate::watch::watch_dir(crate::watch::Kind::of_build(), &dir, move |_| {
                        let _ = events.send(Message::Event);
                    });
                let _watcher = match started {
                    Ok(watcher) => watcher,
                    Err(error) => {
                        tracing::warn!(%error, "the harness manifests are not watched; use config.reload");
                        let _ = ready_tx.send(Err(error.to_string()));
                        return;
                    }
                };
                let _ = ready_tx.send(Ok(()));
                // A change between the start of the core and the start of the watch, and one that
                // the system lost in the first moments of the watch.
                registry.reload();
                let settled = Instant::now() + START_LAG;
                loop {
                    match rx.recv_timeout(settled.saturating_duration_since(Instant::now())) {
                        Ok(Message::Stop) | Err(RecvTimeoutError::Disconnected) => return,
                        Ok(Message::Event) => {}
                        Err(RecvTimeoutError::Timeout) => break,
                    }
                }
                registry.reload();
                'watching: while let Ok(first) = rx.recv() {
                    if matches!(first, Message::Stop) {
                        break;
                    }
                    loop {
                        match rx.recv_timeout(DEBOUNCE) {
                            Ok(Message::Stop) | Err(RecvTimeoutError::Disconnected) => {
                                break 'watching;
                            }
                            Ok(Message::Event) => {}
                            Err(RecvTimeoutError::Timeout) => break,
                        }
                    }
                    registry.reload();
                }
            })?;
        drop(thread);
        Ok(HarnessWatcher {
            stop: tx,
            ready: Mutex::new(Some(ready_rx)),
        })
    }
}

enum Message {
    /// Something happened in the directory.
    Event,
    Stop,
}

/// The watch of the directory of the manifests. It ends with its owner.
pub struct HarnessWatcher {
    stop: Sender<Message>,
    ready: Mutex<Option<Receiver<Result<(), String>>>>,
}

impl HarnessWatcher {
    /// Waits until the watch is set up.
    pub fn wait_ready(&self) -> Result<(), String> {
        match lock(&self.ready).take() {
            Some(rx) => rx
                .recv()
                .unwrap_or_else(|_| Err("the watch ended".to_owned())),
            None => Ok(()),
        }
    }
}

impl Drop for HarnessWatcher {
    fn drop(&mut self) {
        // The thread ends by itself. Do not wait: it can be inside the setup of the watch.
        let _ = self.stop.send(Message::Stop);
    }
}
