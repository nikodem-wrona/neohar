//! The methods of the contract that act on the process of a pane: `pane.input`, `pane.resize`,
//! `pane.paste`, `pane.send_bytes`, `pane.drop_files` and `pane.clipboard_allow`.
//!
//! They need a pane that runs, which the state actor does not hold, so they run on the task of the
//! caller. A reference by handle (`w1:p2`) asks the actor to look it up in the layout. A reference
//! by id goes straight to the map of the core.
//!
//! The window that sends the request (`Origin::window`) takes the geometry lease when it sends
//! input.

use std::sync::Arc;

use nh_proto::api::{
    calls::pane::{CopyResult, PaneText, SelectMode},
    common::Empty,
    ids::{PaneId, Ref, RefValue, WindowId},
    methods::{Call, Method},
};
use nh_proto::frame::{Selection, SelectionMode, SelectionPoint};
use serde::Serialize;
use serde_json::{Value, to_value};
use ulid::Ulid;

use crate::{
    ErrorCode,
    actor::{Origin, Scope, layout::note_size},
    error::RpcError,
    pane::{Pane, lock},
    runtime::Inner,
};

/// Whether this module runs the method.
pub(crate) fn handles(method: &str) -> bool {
    matches!(
        Method::from_name(method),
        Some(
            Method::PaneInput
                | Method::PaneClaim
                | Method::PaneResize
                | Method::PaneScroll
                | Method::PaneSelect
                | Method::PaneSelectClear
                | Method::PaneCopy
                | Method::PanePeekOpen
                | Method::PanePeekClose
                | Method::PaneRead
                | Method::PanePaste
                | Method::PaneSendBytes
                | Method::PaneDropFiles
                | Method::PaneDropBlobs
                | Method::PaneInsertPaths
                | Method::PaneClipboardAllow
        )
    )
}

/// The most files in one drop.
const MAX_DROPPED: usize = 20;

/// Writes the blobs of a drop to files of this machine and returns their paths. The
/// files are in `<data>/drops/<pane>/<ulid>/`, with mode 0600 in a directory of mode 0700. The
/// name is the last part of the name that the client gave, and a second file of the same name
/// gets `-2`.
async fn write_dropped(
    inner: &Arc<Inner>,
    pane: &str,
    blobs: &[nh_proto::api::calls::pane::DroppedBlob],
) -> Result<Vec<String>, RpcError> {
    use std::{
        io::Write,
        os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    };

    if blobs.is_empty() || blobs.len() > MAX_DROPPED {
        return Err(RpcError::invalid_params(format!(
            "a drop has 1 to {MAX_DROPPED} files"
        )));
    }
    let data = inner
        .factory
        .paths
        .data_dir(&inner.factory.session)
        .map_err(|e| RpcError::internal(e.to_string()))?;
    let dir = data
        .join("drops")
        .join(pane)
        .join(Ulid::from_parts(crate::persist::now_ms().unsigned_abs(), rand_u128()).to_string());
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&dir)
        .map_err(|e| RpcError::internal(format!("cannot make {}: {e}", dir.display())))?;
    let core = crate::runtime::CoreHandle::from_inner(Arc::clone(inner));
    let mut paths = Vec::new();
    let mut used: Vec<String> = Vec::new();
    for blob in blobs {
        let Some((bytes, _)) = core.blob(&blob.hash).await? else {
            return Err(RpcError::new(
                ErrorCode::NotFound,
                format!("no blob {} in the store", blob.hash),
            ));
        };
        let base = crate::uploads::clean_name(&blob.name);
        let mut name = base.clone();
        let mut n = 1;
        while used.contains(&name) {
            n += 1;
            name = format!("{base}-{n}");
        }
        used.push(name.clone());
        let path = dir.join(&name);
        let written = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
            .and_then(|mut file| file.write_all(&bytes));
        written.map_err(|e| RpcError::internal(format!("cannot write {}: {e}", path.display())))?;
        let text = path
            .into_os_string()
            .into_string()
            .map_err(|_| RpcError::internal("the path of a drop is not UTF-8"))?;
        paths.push(text);
    }
    Ok(paths)
}

/// Random bits for the name of a directory.
fn rand_u128() -> u128 {
    let mut bytes = [0u8; 16];
    let _ = getrandom::fill(&mut bytes);
    u128::from_le_bytes(bytes) & ((1u128 << 80) - 1)
}

fn json<T: Serialize>(value: &T) -> Result<Value, RpcError> {
    to_value(value).map_err(|e| RpcError::internal(e.to_string()))
}

/// The id of the pane that a reference names. A handle asks the layout.
pub(crate) async fn resolve_id(inner: &Arc<Inner>, pane: &Ref<PaneId>) -> Result<Ulid, RpcError> {
    match &pane.0 {
        RefValue::Id(id) => Ok(id.ulid()),
        _ => {
            let reference = pane.clone();
            Ok(inner
                .actor
                .update(move |parts| parts.state.layout.resolve_pane(&reference))
                .await
                .map_err(|e| RpcError::internal(e.to_string()))??
                .ulid())
        }
    }
}

/// The pane object of a reference. A pane that is in the layout and has no process (it starts, it
/// ended and the core restarted, or its holder is lost) is an error of its own.
pub(crate) async fn resolve(inner: &Arc<Inner>, pane: &Ref<PaneId>) -> Result<Arc<Pane>, RpcError> {
    let id = resolve_id(inner, pane).await?;
    if let Some(found) = lock(&inner.panes).get(&id).cloned() {
        return Ok(found);
    }
    // No pane object. The layout says whether the pane exists without a process.
    let known = inner
        .actor
        .update(move |parts| parts.state.layout.panes.contains_key(&PaneId::new(id)))
        .await
        .map_err(|e| RpcError::internal(e.to_string()))?;
    Err(if known {
        RpcError::new(ErrorCode::PaneGone, format!("pane {pane} has no process"))
    } else {
        RpcError::new(ErrorCode::NotFound, format!("no pane {pane}"))
    })
}

/// Tells the geometry lease that the window sent something to the pane, and notes the client
/// that sent it, whose clipboard gets the OSC 52 writes of the pane. A run that waits
/// after its turn in this pane does not wait any more.
fn touch(inner: &Inner, pane: &Pane, origin: &Origin) {
    if let Some(window) = origin.window {
        pane.input_from(window);
    }
    pane.input_from_device(origin.device);
    let id = PaneId::new(pane.id());
    if inner.turn_waits.contains(id) {
        inner
            .actor
            .update_detached(move |mut parts| crate::agents::turn::seen(&mut parts, id));
    }
}

/// Runs a call that [`handles`] accepted.
pub(crate) async fn run(inner: &Arc<Inner>, call: Call, origin: Origin) -> Result<Value, RpcError> {
    match call {
        Call::PaneInput(p) => {
            let pane = resolve(inner, &p.pane).await?;
            touch(inner, &pane, &origin);
            pane.input(p.input_id, &p.event)?;
            json(&Empty {})
        }
        Call::PaneClaim(p) => {
            crate::window_calls::check_owner(inner, &origin, p.window)?;
            resolve(inner, &p.pane).await?.input_from(p.window);
            json(&Empty {})
        }
        Call::PaneResize(p) => resize(inner, &p.pane, p.window, p.cols, p.rows).await,
        Call::PaneScroll(p) => {
            resolve(inner, &p.pane).await?.scroll(p.to);
            json(&Empty {})
        }
        Call::PaneSelect(p) => {
            let point = |p: nh_proto::api::calls::pane::SelectPoint| SelectionPoint {
                col: p.col,
                line: p.line,
            };
            let mode = match p.mode {
                SelectMode::Simple => SelectionMode::Simple,
                SelectMode::Word => SelectionMode::Word,
                SelectMode::Line => SelectionMode::Line,
                SelectMode::Block => SelectionMode::Block,
            };
            resolve(inner, &p.pane).await?.select(Selection {
                start: point(p.anchor),
                end: point(p.head),
                mode,
            });
            json(&Empty {})
        }
        Call::PaneSelectClear(p) => {
            resolve(inner, &p.pane).await?.select_clear();
            json(&Empty {})
        }
        Call::PaneCopy(p) => {
            let write = origin.scope != Scope::Phone;
            let text = resolve(inner, &p.pane).await?.copy(origin.device, write)?;
            json(&CopyResult {
                text: p.return_text.then_some(text),
            })
        }
        Call::PanePeekOpen(p) => {
            // The stream is the subscription. This call tells the UI at once that the pane has
            // no process and cannot be peeked at.
            resolve(inner, &p.pane).await?;
            json(&Empty {})
        }
        Call::PanePeekClose(p) => {
            let id = resolve_id(inner, &p.pane).await?;
            inner.render.close_peek(id);
            json(&Empty {})
        }
        Call::PaneRead(p) => {
            let pane = resolve(inner, &p.pane).await?;
            let (text, from_line) = match p.lines {
                Some(range) if !p.visible => pane.read_lines(range.from, range.to).await?,
                _ => pane.read_visible(),
            };
            json(&PaneText { text, from_line })
        }
        Call::PanePaste(p) => {
            let pane = resolve(inner, &p.pane).await?;
            touch(inner, &pane, &origin);
            pane.paste(p.input_id, &p.text)?;
            json(&Empty {})
        }
        Call::PaneSendBytes(p) => {
            let pane = resolve(inner, &p.pane).await?;
            touch(inner, &pane, &origin);
            pane.send_bytes(p.input_id, p.bytes.0)?;
            json(&Empty {})
        }
        Call::PaneDropBlobs(p) => {
            let pane = resolve(inner, &p.pane).await?;
            touch(inner, &pane, &origin);
            let paths = write_dropped(inner, &pane.id().to_string(), &p.blobs).await?;
            pane.drop_files(p.input_id, &paths)?;
            json(&Empty {})
        }
        Call::PaneDropFiles(p) => {
            let pane = resolve(inner, &p.pane).await?;
            touch(inner, &pane, &origin);
            pane.drop_files(p.input_id, &p.paths)?;
            json(&Empty {})
        }
        // Paths of this machine from the Files tab: the paste of a drop, and no upload.
        Call::PaneInsertPaths(p) => {
            if p.paths.is_empty() || p.paths.len() > MAX_DROPPED {
                return Err(RpcError::invalid_params(format!(
                    "an insert has 1 to {MAX_DROPPED} paths"
                )));
            }
            let pane = resolve(inner, &p.pane).await?;
            touch(inner, &pane, &origin);
            pane.drop_files(p.input_id, &p.paths)?;
            json(&Empty {})
        }
        Call::PaneClipboardAllow(p) => {
            let pane = resolve(inner, &p.pane).await?;
            // The person who answers is at this client, so the text goes to its clipboard.
            pane.input_from_device(origin.device);
            let write = !p.return_text && origin.scope != Scope::Phone;
            let text = pane.clipboard_answer(p.allow, p.remember, write).await?;
            json(&CopyResult {
                text: text.filter(|_| p.return_text),
            })
        }
        other => Err(RpcError::internal(format!("not a pane call: {other:?}"))),
    }
}

/// `pane.resize`. A pane without a process keeps the size in the layout, and the holder starts
/// with it or gets it when it is up.
async fn resize(
    inner: &Arc<Inner>,
    pane: &Ref<PaneId>,
    window: WindowId,
    cols: u16,
    rows: u16,
) -> Result<Value, RpcError> {
    let id = resolve_id(inner, pane).await?;
    let running = lock(&inner.panes).get(&id).cloned();
    match running {
        Some(running) => running.resize_for(window, cols, rows)?,
        None => {
            inner
                .actor
                .update(move |parts| note_size(parts, PaneId::new(id), cols, rows))
                .await
                .map_err(|e| RpcError::internal(e.to_string()))?;
        }
    }
    json(&Empty {})
}
