//! The methods of the contract that the host's windows send to the render server:
//! `window.focus_report`, `window.report_metrics` and `render.ack`. Each one names a window of the
//! layout.
//!
//! They change the pace and the state of a window. The state that the UI shows (`focused`,
//! `visible`, `refresh_hz` in the `windows` projection) goes through the state actor.

use std::sync::Arc;

use nh_proto::api::{
    common::Empty,
    ids::{PaneId, WindowId},
    methods::{Call, Method},
};
use serde_json::{Value, to_value};

use crate::{
    ErrorCode,
    actor::{Origin, layout::apply_window_report},
    error::RpcError,
    layout::model::WindowFacts,
    pane_calls::resolve_id,
    render::{Ack, AckParams, FocusReport},
    runtime::Inner,
};

/// Whether this module runs the method.
pub(crate) fn handles(method: &str) -> bool {
    matches!(
        Method::from_name(method),
        Some(Method::WindowFocusReport | Method::WindowReportMetrics | Method::RenderAck)
    )
}

fn empty() -> Result<Value, RpcError> {
    to_value(Empty {}).map_err(|e| RpcError::internal(e.to_string()))
}

/// Refuses a call of another device for a window. A caller with no device,
/// for example the CLI, and a row with no owner pass.
pub(crate) fn check_owner(
    inner: &Inner,
    origin: &Origin,
    window: WindowId,
) -> Result<(), RpcError> {
    let (Some(device), Some(Some(owner))) = (origin.device, inner.host.owner_of(window)) else {
        return Ok(());
    };
    if device == owner {
        Ok(())
    } else {
        Err(RpcError::new(
            ErrorCode::PermissionDenied,
            format!("the window {window} belongs to another client"),
        ))
    }
}

/// Runs a call that [`handles`] accepted.
pub(crate) async fn run(inner: &Arc<Inner>, call: Call, origin: Origin) -> Result<Value, RpcError> {
    match call {
        Call::WindowFocusReport(p) => {
            inner.render.require_window(p.window)?;
            check_owner(inner, &origin, p.window)?;
            let active_pane = match &p.active_pane {
                Some(reference) => Some(PaneId::new(resolve_id(inner, reference).await?)),
                None => None,
            };
            // An invisible window gets no frames: minimized counts as hidden.
            inner.render.focus_report(&FocusReport {
                window: p.window,
                focused: p.focused,
                visible: p.visible && !p.minimized,
                refresh_hz: p.refresh_hz.map(f64::from),
            });
            let facts = WindowFacts {
                focused: p.focused,
                visible: p.visible,
                minimized: p.minimized,
                refresh_hz: p.refresh_hz,
                active_pane,
            };
            let window = p.window;
            inner
                .actor
                .update_detached(move |parts| apply_window_report(parts, window, facts));
            empty()
        }
        Call::WindowReportMetrics(p) => {
            inner.render.require_window(p.window)?;
            check_owner(inner, &origin, p.window)?;
            // The pixels of the layout of the page are CSS pixels. The engine and the PTY count
            // device pixels.
            let scale = f64::from(p.device_pixel_ratio.max(0.1));
            let device = |css: f32| (f64::from(css) * scale).round().clamp(1.0, 4096.0) as u16;
            inner.render.report_metrics(
                p.window,
                device(p.cell_width_px),
                device(p.cell_height_px),
            );
            inner.health.set_renderer(Some(p.renderer));
            if let Some(gpu) = &p.gpu {
                tracing::debug!(window = %p.window, gpu, "the window reports its gpu");
            }
            empty()
        }
        Call::RenderAck(p) => {
            inner.render.require_window(p.window)?;
            check_owner(inner, &origin, p.window)?;
            inner.render.ack(&AckParams {
                window: p.window,
                acks: p
                    .acks
                    .into_iter()
                    .map(|ack| Ack {
                        pane: ack.pane,
                        rev: ack.rev,
                    })
                    .collect(),
                need_keyframe: p.need_keyframe,
            });
            empty()
        }
        other => Err(RpcError::internal(format!("not a window call: {other:?}"))),
    }
}
