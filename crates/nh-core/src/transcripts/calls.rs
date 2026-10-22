//! `timeline.page`: older items of `timeline:<run>`. `timeline.item`: the full text
//! of one item, as sanitized HTML.
//!
//! The items are history. They are made again from the file of the transcript, so neohar keeps no
//! copy of it.

use std::{path::PathBuf, sync::Arc};

use nh_proto::api::{
    calls::system::{TimelineItemGet, TimelineItemText, TimelinePageParams, TimelinePageResult},
    common::Harness,
    ids::{Ref, RunId},
    methods::{Call, Method},
};
use serde_json::{Value, to_value};

use super::tail::{RING, all_items, format_of, item_text};
use crate::{ErrorCode, error::RpcError, runtime::Inner};

/// The items in a page when the call names no number.
const DEFAULT_PAGE: usize = 100;
/// The longest text of `timeline.item`, in bytes. A longer text is cut, and `truncated` is true.
pub(crate) const ITEM_CAP: usize = 64 * 1024;

/// Whether this module runs the method.
pub(crate) fn handles(method: &str) -> bool {
    matches!(
        Method::from_name(method),
        Some(Method::TimelinePage | Method::TimelineItem)
    )
}

/// Runs a call that [`handles`] accepted.
pub(crate) async fn run(inner: &Arc<Inner>, call: Call) -> Result<Value, RpcError> {
    match call {
        Call::TimelinePage(params) => {
            let page = page(inner, params).await?;
            to_value(&page).map_err(|e| RpcError::internal(e.to_string()))
        }
        Call::TimelineItem(params) => {
            let item = item(inner, params).await?;
            to_value(&item).map_err(|e| RpcError::internal(e.to_string()))
        }
        _ => Err(RpcError::internal("not a timeline call")),
    }
}

/// The harness and the transcript file of a run.
async fn transcript(
    inner: &Arc<Inner>,
    reference: Ref<RunId>,
) -> Result<(Harness, Option<PathBuf>), RpcError> {
    let found = inner
        .actor
        .update(move |parts| {
            let id = parts.state.runs.resolve(&reference)?;
            let run = parts.state.runs.by_id.get(&id)?;
            Some((run.harness, run.transcript_path.clone()))
        })
        .await
        .map_err(|e| RpcError::internal(e.to_string()))?;
    let (harness, path) =
        found.ok_or_else(|| RpcError::new(ErrorCode::NotFound, "no such agent"))?;
    Ok((harness, path.map(PathBuf::from)))
}

async fn item(inner: &Arc<Inner>, params: TimelineItemGet) -> Result<TimelineItemText, RpcError> {
    let (harness, path) = transcript(inner, params.run).await?;
    let missing = || {
        RpcError::new(
            ErrorCode::NotFound,
            format!("no timeline item {}", params.item),
        )
    };
    let path = path.ok_or_else(missing)?;
    let id = params.item.clone();
    let text = tokio::task::spawn_blocking(move || item_text(&path, format_of(harness), &id))
        .await
        .map_err(|e| RpcError::internal(format!("the item task failed: {e}")))?
        .ok_or_else(missing)?;
    Ok(render_item(&text))
}

/// The HTML of the text of an item, cut at [`ITEM_CAP`] on a character boundary.
pub(crate) fn render_item(text: &str) -> TimelineItemText {
    let mut end = text.len().min(ITEM_CAP);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    let markdown = nh_rich::markdown(&text[..end]);
    TimelineItemText {
        html: markdown.html,
        truncated: end < text.len() || markdown.truncated,
    }
}

async fn page(
    inner: &Arc<Inner>,
    params: TimelinePageParams,
) -> Result<TimelinePageResult, RpcError> {
    let (harness, path) = transcript(inner, params.run).await?;
    let Some(path) = path else {
        return Ok(TimelinePageResult {
            items: Vec::new(),
            more: false,
        });
    };
    let limit = params
        .limit
        .map_or(DEFAULT_PAGE, |limit| limit as usize)
        .clamp(1, RING);
    let before = params.before_item;
    tokio::task::spawn_blocking(move || {
        let items = all_items(&path, format_of(harness));
        let end = match &before {
            Some(id) => items
                .iter()
                .position(|item| &item.id == id)
                .ok_or_else(|| {
                    RpcError::new(ErrorCode::NotFound, format!("no timeline item {id}"))
                })?,
            None => items.len(),
        };
        let start = end.saturating_sub(limit);
        Ok(TimelinePageResult {
            items: items[start..end].to_vec(),
            more: start > 0,
        })
    })
    .await
    .map_err(|e| RpcError::internal(format!("the page task failed: {e}")))?
}
