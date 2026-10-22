//! `session.info` and `machine.info`.

use nh_proto::{
    api::{
        calls::workspace::{MachineInfo, Protocols, SessionInfo},
        methods::{Call, Method},
        time::Timestamp,
    },
    version::{API_VERSION, FRAME_PROTO, HOLDER_PROTO},
};
use nh_store::Tx;
use serde_json::to_value;

use super::{Ctx, Handled, HandlerTable};
use crate::error::RpcError;

fn missing(key: &str) -> RpcError {
    RpcError::internal(format!("the database has no {key}"))
}

fn session_info(ctx: &Ctx<'_>, tx: &mut Tx<'_>, _: &Call) -> Result<Handled, RpcError> {
    let cursor = tx.cursor().map_err(|e| RpcError::internal(e.to_string()))?;
    let info = SessionInfo {
        name: ctx.state.session.clone(),
        api_version: API_VERSION.to_owned(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        pid: std::process::id(),
        started_at: Timestamp(ctx.state.started_at_ms),
        session_uuid: cursor.session_uuid,
        machine_uuid: cursor.machine_uuid,
        log_epoch: cursor.log_epoch,
        protocols: Protocols {
            frame: FRAME_PROTO,
            holder: HOLDER_PROTO,
        },
    };
    Ok(Handled::result(
        to_value(info).map_err(|e| RpcError::internal(e.to_string()))?,
    ))
}

fn machine_info(ctx: &Ctx<'_>, tx: &mut Tx<'_>, _: &Call) -> Result<Handled, RpcError> {
    let uuid = tx
        .meta("machine_uuid")
        .map_err(|e| RpcError::internal(e.to_string()))?
        .ok_or_else(|| missing("machine_uuid"))?;
    let uname = rustix::system::uname();
    let info = MachineInfo {
        label: ctx.state.machine_label.clone(),
        uuid,
        os: std::env::consts::OS.to_owned(),
        arch: std::env::consts::ARCH.to_owned(),
        hostname: uname.nodename().to_str().ok().map(str::to_owned),
    };
    Ok(Handled::result(
        to_value(info).map_err(|e| RpcError::internal(e.to_string()))?,
    ))
}

/// The handlers of this module.
pub(crate) fn handlers(table: &mut HandlerTable) {
    table.insert(Method::SessionInfo, session_info);
    table.insert(Method::MachineInfo, machine_info);
}
