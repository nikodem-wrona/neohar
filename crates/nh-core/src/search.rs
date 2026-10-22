//! `search.query`: the search of the scrollback archive.
//!
//! The archive of a pane has every line that left its screen. The query is words, and
//! each word is a phrase of its own, so a quote or the word `AND` has no meaning of its own. A word
//! of two letters or more also matches the start of a longer word. The best matches come
//! first. A hit names the pane and the absolute line, with the text around the
//! match and the range of the match.

use std::sync::Arc;

use nh_proto::api::{
    calls::system::{SearchHit, SearchQuery, SearchResults},
    ids::PaneId,
    methods::{Call, Method},
    rich::Spans,
};
use nh_store::SearchScope;
use serde_json::{Value, to_value};

use crate::{error::RpcError, layout::model::Layout, pane::lock, runtime::Inner};

/// The hits in an answer when the call names no number.
const DEFAULT_LIMIT: usize = 50;
/// The most hits in an answer.
const MAX_LIMIT: usize = 200;

/// The marks that the store puts around a match.
const OPEN: char = '\u{2}';
const CLOSE: char = '\u{3}';

/// Whether this module runs the method.
pub(crate) fn handles(method: &str) -> bool {
    matches!(Method::from_name(method), Some(Method::SearchQuery))
}

/// Runs a call that [`handles`] accepted.
pub(crate) async fn run(inner: &Arc<Inner>, call: Call) -> Result<Value, RpcError> {
    let Call::SearchQuery(params) = call else {
        return Err(RpcError::internal("not a search call"));
    };
    let results = search(inner, params).await?;
    to_value(&results).map_err(|e| RpcError::internal(e.to_string()))
}

/// What the query asks for: the scope as the store takes it, and the panes to bring up to date.
fn scope_of(layout: &Layout, params: &SearchQuery) -> Result<(SearchScope, Vec<PaneId>), RpcError> {
    match (&params.pane, &params.workspace) {
        (Some(_), Some(_)) => Err(RpcError::invalid_params(
            "name a pane or a workspace, not both",
        )),
        (Some(pane), None) => {
            let id = layout.resolve_pane(pane)?;
            Ok((SearchScope::Pane(id.ulid()), vec![id]))
        }
        (None, Some(workspace)) => {
            let index = layout.resolve_workspace(workspace)?;
            let id = layout.workspaces[index].id;
            let panes = layout
                .panes
                .values()
                .filter(|entry| entry.workspace == id)
                .map(|entry| entry.id)
                .collect();
            Ok((SearchScope::Workspace(id.ulid().to_string()), panes))
        }
        (None, None) => Ok((SearchScope::All, layout.panes.keys().copied().collect())),
    }
}

async fn search(inner: &Arc<Inner>, params: SearchQuery) -> Result<SearchResults, RpcError> {
    let limit = params
        .limit
        .map_or(DEFAULT_LIMIT, |limit| limit as usize)
        .clamp(1, MAX_LIMIT);
    let query = params.query.clone();
    let (scope, panes) = {
        let params = params.clone();
        inner
            .actor
            .update(move |parts| scope_of(&parts.state.layout, &params))
            .await
            .map_err(|e| RpcError::internal(e.to_string()))??
    };
    // The archive has the lines up to the last flush. The panes in scope write what they hold.
    let live: Vec<_> = {
        let table = lock(&inner.panes);
        panes
            .iter()
            .filter_map(|id| table.get(&id.ulid()).cloned())
            .collect()
    };
    for pane in live {
        pane.flush_archive().await;
    }
    // One more than the limit tells that the answer is cut.
    let mut hits = inner
        .actor
        .store(move |store| store.search_scrollback(&scope, &query, limit + 1))
        .await
        .map_err(|e| RpcError::internal(e.to_string()))?;
    let truncated = hits.len() > limit;
    hits.truncate(limit);
    Ok(SearchResults {
        hits: hits
            .into_iter()
            .map(|hit| SearchHit {
                pane: PaneId::new(hit.pane),
                line: hit.line_no,
                text: spans_of(&hit.snippet),
                at: None,
            })
            .collect(),
        truncated,
        job: None,
    })
}

/// The text of a snippet without its marks, and the ranges between them in UTF-16 code units.
/// Each range has the class `match`.
pub(crate) fn spans_of(snippet: &str) -> Spans {
    let mut text = String::with_capacity(snippet.len());
    let mut spans = Vec::new();
    let mut units: u32 = 0;
    let mut start: Option<u32> = None;
    for character in snippet.chars() {
        match character {
            OPEN => start = Some(units),
            CLOSE => {
                if let Some(from) = start.take()
                    && units > from
                {
                    spans.push((from, units, "match".to_owned()));
                }
            }
            other => {
                text.push(other);
                units += other.len_utf16() as u32;
            }
        }
    }
    Spans { text, spans }
}

#[cfg(test)]
mod tests;
