//! `jobs.cancel`.
//!
//! The registry of the jobs lives in memory and has its own lock, because the workers of the jobs
//! report their progress from other threads. So the handler does not return a mutation: it asks
//! the registry, which changes the row of the job and tells the worker. The method needs no
//! database, so it works while the store is degraded.

use nh_proto::api::{
    common::Empty,
    methods::{Call, Method},
};
use nh_store::Tx;

use super::{Ctx, Handled, HandlerTable};
use crate::error::RpcError;

fn cancel(ctx: &Ctx<'_>, _: &mut Tx<'_>, call: &Call) -> Result<Handled, RpcError> {
    let Call::JobsCancel(params) = call else {
        return Err(RpcError::internal("not jobs.cancel"));
    };
    ctx.state.jobs.cancel(params.job)?;
    Ok(Handled::result(
        serde_json::to_value(Empty {}).map_err(|e| RpcError::internal(e.to_string()))?,
    ))
}

/// The handlers of this module.
pub(crate) fn handlers(table: &mut HandlerTable) {
    table.insert(Method::JobsCancel, cancel);
}
