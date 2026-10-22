//! The scrollback stream: styled lines from the archive, for the part of the
//! history that the engine no longer holds in memory.
//!
//! The client asks for `count` lines from `from_line`. The stream sends pages of up to
//! [`PAGE_LINES`] lines, with style and grapheme ids interned for the stream. A page ends at the
//! last line that the archive holds. The stream ends with an **empty page**, whose `from_line` is
//! the line after the last one that was sent. A line that the archive does not hold (the archive
//! is shorter than the range, or the live screen has the line) is not in a page.

use std::sync::Arc;

use nh_store::PageBuilder;

use super::FrameSink;
use crate::pane::Pane;

/// The most lines in one page.
pub const PAGE_LINES: u64 = 200;

/// Sends the pages of lines `from..from + count` of the archive of `pane` to `sink`.
pub(super) async fn run(pane: Arc<Pane>, from: u64, count: u64, sink: Arc<dyn FrameSink>) {
    // The archive has the lines up to the last flush. Ask for the rest.
    pane.flush_archive().await;
    let mut builder = PageBuilder::new();
    let end = from.saturating_add(count);
    let mut next = from;
    while next < end {
        if sink.is_closed() {
            return;
        }
        let want = (end - next).min(PAGE_LINES);
        let lines = match pane
            .archived(next, usize::try_from(want).unwrap_or(usize::MAX))
            .await
        {
            Ok(lines) => lines,
            Err(error) => {
                tracing::warn!(pane = %pane.id(), %error, "the scrollback stream cannot read the archive");
                break;
            }
        };
        let Some(last) = lines.last().map(|line| line.line_no) else {
            break;
        };
        let page = match builder.page(pane.render_id(), next, &lines) {
            Ok(page) => page,
            Err(error) => {
                // The client opens a new stream for the rest.
                tracing::warn!(pane = %pane.id(), %error, "a scrollback page is not made");
                break;
            }
        };
        let Ok(bytes) = postcard::to_stdvec(&page) else {
            return;
        };
        if !sink.send(bytes) {
            return;
        }
        if lines.len() < usize::try_from(want).unwrap_or(usize::MAX) {
            next = last + 1;
            break;
        }
        next += want;
        // Let the other work of the runtime go between two pages of a long range.
        tokio::task::yield_now().await;
    }
    let end_page = nh_proto::api::streams::ScrollbackPage {
        pane: pane.render_id(),
        from_line: next,
        lines: Vec::new(),
        new_styles: Vec::new(),
        new_graphemes: Vec::new(),
    };
    if let Ok(bytes) = postcard::to_stdvec(&end_page) {
        let _ = sink.send(bytes);
    }
}
