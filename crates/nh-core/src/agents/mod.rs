//! The agents in the core.
//!
//! A [`watch::Agents`] starts one watcher for each pane. The watcher walks the process tree,
//! finds a harness with the matcher of `nh-agents`, and tells the state actor. The actor keeps the
//! [`Runs`], gives every input to the arbiter of the run, writes the rows and the events in one
//! transaction, and publishes `agents` and `attention`.

pub(crate) mod control;
pub(crate) mod drift;
pub(crate) mod host;
pub(crate) mod keys;
pub(crate) mod project;
pub(crate) mod reboot;
pub(crate) mod runs;
mod screen;
pub(crate) mod turn;
mod watch;

pub(crate) use control::{Control, HiddenStart};
pub(crate) use host::AdapterHosts;
pub(crate) use runs::Runs;
pub(crate) use watch::Agents;

#[cfg(test)]
pub(crate) mod tests;
