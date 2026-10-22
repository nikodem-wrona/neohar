//! The life of a run: when a run starts and ends, what names it, how sure the core is
//! about its pane, and who has seen it.
//!
//! These are the pure parts. The core owns the table of runs, asks a [`Tracker`] what a new
//! observation of the process tree means, and records the answer.

use std::{collections::HashMap, hash::Hash};

use nh_proto::api::model::{Binding, ReadState};

use crate::matcher::Detection;

/// What a new observation of a pane means for its run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    /// The run goes on, or there is none.
    Nothing,
    /// A harness process appeared in a pane that had no run.
    Start(Detection),
    /// The process of the run is gone.
    End { pid: i32, start: u64 },
    /// Another process took the place of the process of the run (`claude` quit, `codex` started).
    /// The old run ends and a new one starts.
    Replace {
        ended: (i32, u64),
        started: Detection,
    },
}

/// Follows the run of one pane.
///
/// A run is identified by the process that matched, `(pid, start)`. It ends when that process is
/// gone, or when another process matches. It does **not** end when the walk finds nothing while
/// the process lives: the harness can start an editor, and the editor becomes the foreground
/// group for a time.
#[derive(Clone, Debug, Default)]
pub struct Tracker {
    current: Option<(i32, u64)>,
}

impl Tracker {
    /// The key of the process of the run, if there is one.
    pub fn current(&self) -> Option<(i32, u64)> {
        self.current
    }

    /// Gives the tracker the run that the core loaded from the table after a restart.
    pub fn resume(&mut self, key: (i32, u64)) {
        self.current = Some(key);
    }

    /// One observation: what the matcher found in the foreground tree, and whether the process
    /// of the current run still lives. `alive` is asked only when it matters.
    pub fn observe(
        &mut self,
        found: Option<Detection>,
        alive: impl FnOnce((i32, u64)) -> bool,
    ) -> Step {
        match (self.current, found) {
            (None, None) => Step::Nothing,
            (None, Some(detection)) => {
                self.current = Some(detection.key());
                Step::Start(detection)
            }
            (Some(key), Some(detection)) if detection.key() == key => Step::Nothing,
            (Some(key), Some(detection)) => {
                self.current = Some(detection.key());
                Step::Replace {
                    ended: key,
                    started: detection,
                }
            }
            (Some(key), None) => {
                if alive(key) {
                    Step::Nothing
                } else {
                    self.current = None;
                    Step::End {
                        pid: key.0,
                        start: key.1,
                    }
                }
            }
        }
    }

    /// The pane has no process any more (the holder said so). The run ends without a walk.
    pub fn gone(&mut self) -> Option<(i32, u64)> {
        self.current.take()
    }
}

/// How sure the core is that a process in the foreground group of a pane is a run of that pane.
/// It is a correlation and never an authorization.
pub const DETECTED_CONFIDENCE: f32 = 0.9;

/// The binding of a run that was just found.
///
/// A run that neohar launched in the pane is bound. A run that the process tree showed is only
/// suggested, until a signal carries the token of the pane or the user links it.
pub fn binding_for_detection(launched_by_neohar: bool) -> Binding {
    if launched_by_neohar {
        Binding::Bound
    } else {
        Binding::Suggested {
            confidence: DETECTED_CONFIDENCE,
        }
    }
}

/// What a signal that carried the token of the pane, or a link of the user, does to a binding.
pub fn promote() -> Binding {
    Binding::Bound
}

/// Whether a binding authorizes anything: answers, prompts, keys, policy decisions.
pub fn authorizes(binding: &Binding) -> bool {
    matches!(binding, Binding::Bound)
}

/// The session ids of a run. The identity of a run is `(pane, harness, session id)`, and a
/// different id in the same process keeps the run (`/clear`, `/resume`, a fork).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Sessions {
    current: Option<String>,
    previous: Vec<String>,
}

/// What a session id changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Identified {
    /// The first id of the run.
    First,
    /// Another id: `agent.identified` fires again and the old id joins the previous ones.
    Changed,
    /// The id the run has. Nothing to do.
    Same,
}

impl Sessions {
    pub fn from_parts(current: Option<String>, previous: Vec<String>) -> Self {
        Self { current, previous }
    }

    pub fn current(&self) -> Option<&str> {
        self.current.as_deref()
    }

    pub fn previous(&self) -> &[String] {
        &self.previous
    }

    pub fn identify(&mut self, id: &str) -> Identified {
        match self.current.as_deref() {
            None => {
                self.current = Some(id.to_owned());
                Identified::First
            }
            Some(current) if current == id => Identified::Same,
            Some(current) => {
                let old = current.to_owned();
                if !self.previous.contains(&old) {
                    self.previous.push(old);
                }
                // Going back to an id that the run had is a change too, and the id leaves the
                // list of the previous ones.
                self.previous.retain(|previous| previous != id);
                self.current = Some(id.to_owned());
                Identified::Changed
            }
        }
    }
}

/// Which clients have seen the last change of a run. "Done" is `idle` and `unseen`.
///
/// A client has seen the run when it focused the pane or peeked at it after the last change of
/// the execution state. A count of changes orders the views and the changes, so two things in the
/// same millisecond keep their order. A client that is not in the map has seen the run, and a new
/// window joins at the count of the moment: it does not start with a row of runs that finished
/// before it opened.
#[derive(Clone, Debug, Default)]
pub struct ReadStates<K: Eq + Hash + Clone> {
    changes: u64,
    seen: HashMap<K, u64>,
}

impl<K: Eq + Hash + Clone> ReadStates<K> {
    pub fn new() -> Self {
        Self {
            changes: 0,
            seen: HashMap::new(),
        }
    }

    /// The execution state changed. The clients that are looking at the pane see the change; the
    /// others have not.
    pub fn changed(&mut self, viewing: impl IntoIterator<Item = K>) {
        self.changes += 1;
        for client in viewing {
            self.seen.insert(client, self.changes);
        }
    }

    /// A client focused the pane or peeked at it.
    pub fn viewed(&mut self, client: K) {
        self.seen.insert(client, self.changes);
    }

    /// A client that appeared. It has seen what the run did until now.
    pub fn joined(&mut self, client: K) {
        let changes = self.changes;
        self.seen.entry(client).or_insert(changes);
    }

    pub fn left(&mut self, client: &K) {
        self.seen.remove(client);
    }

    pub fn get(&self, client: &K) -> ReadState {
        match self.seen.get(client) {
            Some(seen) if *seen < self.changes => ReadState::Unseen,
            _ => ReadState::Seen,
        }
    }
}

#[cfg(test)]
mod tests;
