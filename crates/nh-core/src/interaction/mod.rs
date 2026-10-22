//! The interactions of the core.
//!
//! - [`model`]: one interaction and its delivery record as a state machine. It is pure.
//! - [`render`]: the payload of a card, made once when the interaction opens.
//! - [`output`]: the text that a held hook prints for a decision.
//! - [`rows`], [`state`] and [`project`]: the store, the table in memory and the projection.
//! - [`apply`]: the changes on the state actor.
//! - [`calls`]: the methods `interaction.*`.
//! - [`gate`]: the connections of the shims that hold a hook.
//! - [`screen`]: the provisional interactions that the screen opens.

pub(crate) mod apply;
pub(crate) mod calls;
#[cfg(feature = "debug")]
pub(crate) mod debug;
pub(crate) mod gate;
pub(crate) mod model;
pub(crate) mod output;
pub(crate) mod project;
pub(crate) mod render;
pub(crate) mod rows;
pub(crate) mod screen;
pub(crate) mod state;

pub(crate) use gate::Gates;
pub(crate) use state::Interactions;

#[cfg(test)]
pub(crate) mod tests;
