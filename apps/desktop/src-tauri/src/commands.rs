//! The three commands that connect a webview to the core: `rpc`, `subscribe` and
//! `stream`. The capability file allows these and nothing else.
//!
//! The commands only pass their arguments to the [`Backend`]: the core in this process, or a core
//! behind `link.sock`. A request carries the window that sent it: the label of a window
//! of the app is the id of a window of the layout, and the core needs it for the geometry lease.

use std::sync::Arc;

use nh_core::{ErrorCode, FrameSink, MessageSink, RpcError};
use nh_proto::api::ids::WindowId;
use nh_remote::Device;
use serde_json::{Value, json};
use tauri::{
    State, Window,
    ipc::{Channel, InvokeResponseBody},
};

use crate::{
    machines::{LOCAL, Machines},
    windows::Windows,
};

/// The window of the layout that a window of the app shows, by its label. The window can belong to
/// any machine.
pub fn layout_window(label: &str) -> Option<WindowId> {
    crate::machines::machine_of(label).map(|(_, id)| id)
}

/// A render stream: every frame is one raw body on a channel.
struct FrameChannel(Channel<InvokeResponseBody>);

impl FrameSink for FrameChannel {
    fn send(&self, frame: Vec<u8>) -> bool {
        self.0.send(InvokeResponseBody::Raw(frame)).is_ok()
    }
}

/// A subscription: every message is one JSON value on a channel.
struct MessageChannel(Channel<Value>);

impl MessageSink for MessageChannel {
    fn send(&self, message: Value) -> bool {
        self.0.send(message).is_ok()
    }
}

/// `call(method, params)` of the transport. The call goes to the core of the machine of
/// the window. The host answers two things itself: the end of a subscription to `link`,
/// and the versions of `link` and of the merged `settings` of a remote machine.
#[tauri::command]
pub async fn rpc(
    window: Window,
    machines: State<'_, Arc<Machines>>,
    windows: State<'_, Windows>,
    method: String,
    params: Option<Value>,
) -> Result<Value, RpcError> {
    let mut params = params.unwrap_or(Value::Null);
    #[cfg(feature = "selftest")]
    if method == crate::selftest::REPORT_METHOD {
        crate::selftest::report(params);
        return Ok(Value::Null);
    }
    // The page and the host both know something about the focus of a window. The host joins them.
    if method == "window.focus_report" {
        params = windows.merge_page_report(&window, params);
    }
    let label = window.label();
    route_call(
        &machines,
        machines.machine_of_label(label),
        layout_window(label),
        None,
        &method,
        params,
    )
    .await
}

/// A call of a window of `machine`, or of a phone that the listener relays to it.
/// The host answers two things itself: the end of a subscription to `link` or to the merged
/// `settings`, and their versions.
pub(crate) async fn route_call(
    machines: &Machines,
    machine: &str,
    window: Option<WindowId>,
    phone: Option<&Device>,
    method: &str,
    params: Value,
) -> Result<Value, RpcError> {
    if method == "projection.unsubscribe"
        && let Some(id) = params.get("subscription").and_then(Value::as_str)
        && (machines.unsubscribe_link(id) || machines.unsubscribe_settings(id))
    {
        return Ok(json!({}));
    }
    if method == "projection.versions" {
        return versions(machines, machine, window, phone, params).await;
    }
    let backend = machines.backend(machine)?;
    // A remote window has the sections of the client from this Mac.
    if method.starts_with("config.")
        && machine != LOCAL
        && let Some(done) =
            nh_remote::settings::route_config(machines.local(), &backend, window, method, &params)
                .await
    {
        return done;
    }
    // Files dropped on a pane of a remote machine go up over the link first. A phone
    // has no paths of this Mac: it attaches files with `POST /blob`.
    if method == "pane.drop_files"
        && phone.is_none()
        && matches!(backend, crate::backend::Backend::Link(_))
    {
        return drop_remote(&backend, window, params).await;
    }
    backend.call_for(phone, window, method, params).await
}

/// What a file of a drop may be: a regular file that the core takes.
fn checked_drop(path: &str) -> Result<(String, u64), RpcError> {
    let meta = std::fs::metadata(path)
        .map_err(|e| RpcError::invalid_params(format!("cannot read {path}: {e}")))?;
    if !meta.is_file() {
        return Err(RpcError::invalid_params(format!("{path} is not a file")));
    }
    if meta.len() > nh_core::UPLOAD_MAX_BYTES {
        return Err(RpcError::new(
            ErrorCode::TooLarge,
            format!("{path} is over {} bytes", nh_core::UPLOAD_MAX_BYTES),
        ));
    }
    let name = std::path::Path::new(path)
        .file_name()
        .map_or_else(|| "file".to_owned(), |n| n.to_string_lossy().into_owned());
    Ok((name, meta.len()))
}

/// `pane.drop_files` of a window of a remote machine. The paths are paths of
/// this Mac, so the core of the machine cannot read them. The host checks every file first, puts
/// each one in the blob store of the machine with `BlobPut`, and then calls `pane.drop_blobs`. A
/// file over the limit stops the drop before the first byte goes up.
async fn drop_remote(
    backend: &crate::backend::Backend,
    window: Option<nh_proto::api::ids::WindowId>,
    params: Value,
) -> Result<Value, RpcError> {
    let paths: Vec<String> = params
        .get("paths")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|p| p.as_str().map(str::to_owned))
        .collect();
    if paths.is_empty() || paths.len() > 20 {
        return Err(RpcError::invalid_params("a drop has 1 to 20 files"));
    }
    let checked: Vec<(String, String)> = paths
        .iter()
        .map(|path| checked_drop(path).map(|(name, _)| (path.clone(), name)))
        .collect::<Result<_, _>>()?;
    let crate::backend::Backend::Link(linked) = backend else {
        return Err(RpcError::internal("not a link"));
    };
    let mut blobs = Vec::new();
    for (path, name) in checked {
        let owned = path.clone();
        let bytes = tokio::task::spawn_blocking(move || std::fs::read(owned))
            .await
            .map_err(|e| RpcError::internal(e.to_string()))?
            .map_err(|e| RpcError::invalid_params(format!("cannot read {path}: {e}")))?;
        let hash = linked.client.put_blob(&name, bytes).await?;
        blobs.push(json!({ "hash": hash, "name": name }));
    }
    let mut call = json!({ "blobs": blobs });
    for key in ["pane", "input_id"] {
        call[key] = params.get(key).cloned().unwrap_or(Value::Null);
    }
    backend.call(window, "pane.drop_blobs", call).await
}

/// `projection.versions {names}`: the core answers for its projections, and the host for `link`.
async fn versions(
    machines: &Machines,
    machine: &str,
    window: Option<WindowId>,
    phone: Option<&Device>,
    params: Value,
) -> Result<Value, RpcError> {
    let names: Vec<String> = params
        .get("names")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|name| name.as_str().map(str::to_owned))
        .collect();
    let remote = machine != LOCAL;
    // The host serves `link`, and the merged `settings` of a remote machine.
    let (host, core): (Vec<String>, Vec<String>) = names
        .into_iter()
        .partition(|n| n == "link" || (remote && n == "settings"));
    let mut versions = serde_json::Map::new();
    if !core.is_empty() {
        let reply = machines
            .backend(machine)?
            .call_for(
                phone,
                window,
                "projection.versions",
                json!({ "names": core }),
            )
            .await?;
        if let Some(found) = reply.get("versions").and_then(Value::as_object) {
            versions.extend(found.clone());
        }
    }
    if host.iter().any(|n| n == "link") {
        versions.insert("link".to_owned(), json!(machines.link_version(machine)));
    }
    if host.iter().any(|n| n == "settings")
        && let Some(version) = machines.settings_version(machine)
    {
        versions.insert("settings".to_owned(), json!(version));
    }
    Ok(json!({ "versions": versions }))
}

/// `subscribe(projection)` of the transport. The channel gets a snapshot and then patches. The
/// answer is the id of the subscription, and the page ends it with `projection.unsubscribe`. The
/// host serves `link` itself, and the core of the machine of the window serves the rest.
#[tauri::command]
pub async fn subscribe(
    window: Window,
    machines: State<'_, Arc<Machines>>,
    name: String,
    since: Option<u64>,
    channel: Channel<Value>,
) -> Result<String, RpcError> {
    let machine = machines.machine_of_label(window.label());
    route_subscribe(
        &machines,
        machine,
        &name,
        since,
        Box::new(MessageChannel(channel)),
    )
    .await
}

/// A subscription of a window of `machine`, or of a phone that the listener relays to it. The
/// host serves `link` and the merged `settings` of a remote machine.
pub(crate) async fn route_subscribe(
    machines: &Arc<Machines>,
    machine: &str,
    name: &str,
    since: Option<u64>,
    sink: Box<dyn MessageSink>,
) -> Result<String, RpcError> {
    if name == "link" {
        return Ok(machines.feed(machine).subscribe(sink));
    }
    if name == "settings" && machine != LOCAL {
        let feed = machines.settings_feed(machine).await?;
        return Ok(feed.subscribe(since, sink));
    }
    machines
        .backend(machine)?
        .subscribe_tracked(name, since, sink)
        .await
}

/// `stream(kind, params)` of the transport. The channel gets binary frames, as the core sent them.
#[tauri::command]
pub async fn stream(
    window: Window,
    machines: State<'_, Arc<Machines>>,
    kind: String,
    params: Value,
    channel: Channel<InvokeResponseBody>,
) -> Result<(), RpcError> {
    machines
        .backend_of_label(window.label())?
        .stream(
            &kind,
            params,
            layout_window(window.label()),
            Arc::new(FrameChannel(channel)),
        )
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_label_of_a_window_is_the_id_of_a_window_of_the_layout() {
        assert!(layout_window("01ARZ3NDEKTSV4RRFFQ69G5FAV").is_some());
        // A label that is not a ULID names no window of the layout.
        assert!(layout_window("main").is_none());
        assert!(layout_window("").is_none());
    }
}
