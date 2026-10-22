//! `config.get`, `config.set` and `config.reload`. `config.reload` reads the harness
//! manifests again too.
//!
//! The configuration is a file, and not a table of the database. These methods need no write
//! transaction, so they work while the store is degraded.

use std::sync::Arc;

use nh_proto::api::{
    calls::system::{ConfigReloaded, ConfigValues},
    common::Empty,
    methods::{Call, Method},
};
use nh_store::Tx;
use serde_json::to_value;

use super::{Ctx, Handled, HandlerTable, Mutation};
use crate::{ErrorCode, error::RpcError};

fn to_json<T: serde::Serialize>(value: &T) -> Result<serde_json::Value, RpcError> {
    to_value(value).map_err(|e| RpcError::internal(e.to_string()))
}

fn get(ctx: &Ctx<'_>, _: &mut Tx<'_>, call: &Call) -> Result<Handled, RpcError> {
    let Call::ConfigGet(params) = call else {
        return Err(RpcError::internal("not config.get"));
    };
    let mut sections = ctx.state.config.sections();
    if let Some(wanted) = &params.section {
        sections.retain(|name, _| name == wanted);
        if sections.is_empty() {
            return Err(RpcError::new(
                ErrorCode::NotFound,
                format!("no section {wanted:?} in the configuration"),
            ));
        }
    }
    Ok(Handled::result(to_json(&ConfigValues { sections })?))
}

/// Publishes the settings and the config warnings after the change, once the file is written.
fn publish(ctx: &Ctx<'_>) -> Mutation {
    let config = Arc::clone(&ctx.state.config);
    let health = Arc::clone(&ctx.state.health);
    Box::new(move |_, hub| crate::config::publish_all(&config, hub, &health))
}

fn set(ctx: &Ctx<'_>, _: &mut Tx<'_>, call: &Call) -> Result<Handled, RpcError> {
    let Call::ConfigSet(params) = call else {
        return Err(RpcError::internal("not config.set"));
    };
    ctx.state.config.set(&params.key, params.value.clone())?;
    Ok(Handled {
        result: to_json(&Empty {})?,
        mutation: Some(publish(ctx)),
    })
}

fn reload(ctx: &Ctx<'_>, _: &mut Tx<'_>, _: &Call) -> Result<Handled, RpcError> {
    let report = ctx.state.config.reload();
    // The files of the harness manifests are configuration too.
    if let Some(harnesses) = &ctx.state.harnesses {
        harnesses.reload();
    }
    Ok(Handled {
        result: to_json(&ConfigReloaded {
            errors: report.errors,
        })?,
        mutation: Some(publish(ctx)),
    })
}

/// The handlers of this module.
pub(crate) fn handlers(table: &mut HandlerTable) {
    table.insert(Method::ConfigGet, get);
    table.insert(Method::ConfigSet, set);
    table.insert(Method::ConfigReload, reload);
}
