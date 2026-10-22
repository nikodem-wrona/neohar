//! Streams: binary channels next to the JSON-RPC socket.
//!
//! A stream carries `postcard` bodies, which the WASM module decodes. The params that open a
//! stream are JSON and have a schema. The bodies do not: frames are in [`crate::frame`], and the
//! pages of the scrollback stream are [`ScrollbackPage`]. A stream that falls behind gets fewer,
//! larger frames (state sync), never a growing queue.

use serde::{Deserialize, Serialize};

use super::{
    common::wire,
    ids::{PaneId, Ref, WindowId},
};
use crate::frame::{GraphemeId, RowUpdate, StyleId};

wire! {
    #[derive(Copy, Eq, Hash)]
    #[serde(rename_all = "snake_case")]
    pub enum StreamKind {
        /// Core to UI: frames for the visible panes of a window.
        Render,
        /// Core to UI: downscaled keyframes for thumbnails and card context.
        Peek,
        /// Core to UI: styled lines from the archive, for the viewport beyond the in-memory grid.
        Scrollback,
        /// UI to core: batched mouse motion. Keys always use `pane.input`.
        Input,
    }

    /// Opens the `render` stream of a window.
    pub struct RenderParams {
        pub window: WindowId,
        /// The refresh rate of the window.
        pub viewport_hz: f32,
        /// The version of the frame format that the client decodes (`FRAME_PROTO`).
        pub proto: u16,
    }

    /// Opens the `peek` stream of a pane. A keyframe holds the bottom `max_rows` rows and
    /// the left `max_cols` columns.
    pub struct PeekParams {
        pub pane: Ref<PaneId>,
        #[serde(default = "default_peek_hz")]
        pub max_hz: f32,
        pub max_cols: u16,
        pub max_rows: u16,
    }

    pub struct ScrollbackParams {
        pub pane: Ref<PaneId>,
        pub from_line: u64,
        pub count: u32,
    }

    pub struct InputStreamParams {
        pub window: WindowId,
    }
}

fn default_peek_hz() -> f32 {
    4.0
}

/// A page of the scrollback stream. The ids of styles and graphemes are interned for the
/// stream, not for the pane epoch.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScrollbackPage {
    /// The render id of the pane.
    pub pane: u32,
    /// The absolute line of the first row.
    pub from_line: u64,
    pub lines: Vec<RowUpdate>,
    /// Styles that the lines use and the client has not seen on this stream.
    pub new_styles: Vec<(StyleId, crate::style::Style)>,
    /// Graphemes that the lines use and the client has not seen on this stream.
    pub new_graphemes: Vec<(GraphemeId, String)>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_scrollback_page_round_trips_through_postcard() {
        let page = ScrollbackPage {
            pane: 7,
            from_line: 1_000_000,
            lines: vec![RowUpdate {
                y: 0,
                cells: vec![0; 12],
            }],
            new_styles: vec![(3, crate::style::Style::default())],
            new_graphemes: vec![(1, "é".to_owned())],
        };
        let bytes = postcard::to_stdvec(&page).expect("serializes");
        let back: ScrollbackPage = postcard::from_bytes(&bytes).expect("decodes");
        assert_eq!(back, page);
    }

    #[test]
    fn the_peek_rate_defaults_to_four_hertz() {
        let params: PeekParams =
            serde_json::from_str(r#"{"pane":"w3:p5","max_cols":40,"max_rows":12}"#)
                .expect("decodes");
        assert_eq!(params.max_hz, 4.0);
    }
}
