//! The shape of a handler.
//!
//! A handler is a function `(context, transaction, call) -> Result<Handled, RpcError>`. It reads
//! the state through the context and writes through the transaction. It does not change the state
//! itself: it returns a [`Mutation`] that the actor runs after the commit. That way a failing
//! handler (or a failing commit) leaves the state as it was, without any undo code.

use std::collections::HashMap;

use nh_proto::api::methods::{Call, Method};
use nh_store::Tx;
use serde_json::Value;

use super::{Origin, State};
use crate::{error::RpcError, projection::ProjectionHub};

/// What a handler may read.
pub(crate) struct Ctx<'a> {
    pub state: &'a State,
    pub origin: &'a Origin,
    /// The time of the command, in unix milliseconds. It is the time of the transaction too.
    pub now_ms: i64,
}

/// A change of the state and of the projections. It runs once, after the commit.
pub(crate) type Mutation = Box<dyn FnOnce(&mut State, &ProjectionHub) + Send>;

/// What a handler returns.
pub(crate) struct Handled {
    /// The result of the method, as JSON.
    pub result: Value,
    pub mutation: Option<Mutation>,
}

impl Handled {
    /// A result and no change of the state.
    pub fn result(result: Value) -> Self {
        Self {
            result,
            mutation: None,
        }
    }
}

pub(crate) type Handler = fn(&Ctx<'_>, &mut Tx<'_>, &Call) -> Result<Handled, RpcError>;

/// The methods that the actor handles, with their handlers.
pub(crate) type HandlerTable = HashMap<Method, Handler>;
