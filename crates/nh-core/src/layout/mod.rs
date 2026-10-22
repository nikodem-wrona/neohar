//! The layout domain.
//!
//! A workspace has tabs, a tab has a tree of panes, and windows show tabs. The parts:
//!
//! - [`tree`]: the pure operations on the tree of one tab;
//! - [`launch`]: what the layout asks of the pane runtime;
//! - [`model`]: the layout in memory (groups, workspaces, tabs, panes) and how it is loaded;
//! - [`ops`] and [`pane_ops`]: the commands, which change a layout and report their events;
//! - [`persist`]: writes the difference between two layouts to the database;
//! - [`project`]: publishes the `tree` projection.
//!
//! The handlers of the state actor (`actor::layout`) run a command on a copy of the layout and write
//! the difference to the database in the same transaction.

pub(crate) mod launch;
pub(crate) mod model;
pub(crate) mod ops;
pub(crate) mod pane_ops;
pub(crate) mod persist;
pub(crate) mod project;
#[cfg(test)]
pub(crate) mod testing;
pub(crate) mod tree;
