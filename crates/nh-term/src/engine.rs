//! The VT engine interface: bytes in, screen model out.
//!
//! `nh-term` has one implementation, [`crate::AlacrittyEngine`]. The trait is the seam that lets
//! `wezterm-term`, the named fallback, take its place.

use nh_proto::{
    frame::{Cursor, Palette, Selection, Viewport},
    modes::ModeFlags,
    style::Style,
};
use serde::{Deserialize, Serialize};

use crate::{
    effects::EffectSink,
    snapshot::{RestoreError, Snapshot, SnapshotError},
};

/// One cell of a visible row, as the frame builder reads it.
#[derive(Clone, Copy, Debug)]
pub struct CellView<'a> {
    /// The base character. A cell of a wide glyph's second column holds a space.
    pub ch: char,
    /// Zero-width characters that combine with `ch`.
    pub zerowidth: &'a [char],
    pub style: Style,
}

/// The rows that changed since the last [`VtEngine::take_damage`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Damage {
    /// Every row changed, or the size changed.
    pub full: bool,
    /// The changed rows, in order, when `full` is false.
    pub rows: Vec<u16>,
    /// At most 2 rows and 16 cells changed, and the cursor stayed on its row. A spinner
    /// does this. The pacer sends such damage on a pane that has no focus at most 4 times a second.
    pub minor: bool,
}

impl Damage {
    /// Whether row `y` is in the damage.
    pub fn contains(&self, y: u16) -> bool {
        self.full || self.rows.binary_search(&y).is_ok()
    }

    /// Whether nothing changed.
    pub fn is_empty(&self) -> bool {
        !self.full && self.rows.is_empty()
    }
}

/// A run of cells with one style in a [`HistoryLine`]. Columns count cells, not characters.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StyleRun {
    pub start: u16,
    pub len: u16,
    pub style: Style,
}

/// A row that left the screen and went into the scrollback.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryLine {
    /// The absolute number of the line: 0 for the first line that ever left the screen.
    pub line_no: u64,
    /// The text of the row without the blank cells at its end.
    pub text: String,
    /// The cells that have a style other than the default one.
    pub runs: Vec<StyleRun>,
    /// The row continues on the next row (a soft wrap). A reader joins the two rows.
    pub wrapped: bool,
}

/// What a restored engine needs that a snapshot does not hold.
#[derive(Clone, Debug)]
pub struct EngineConfig {
    /// The capacity of the scrollback in memory, in lines (at least [`crate::MIN_HISTORY`]).
    pub history: usize,
    /// The colours before a program changes them.
    pub theme: Palette,
    /// The size of a cell in pixels, for `CSI 14 t` replies. `(0, 0)` when it is not known.
    pub cell_px: (u16, u16),
}

/// The screen model of one pane.
pub trait VtEngine: Send {
    /// Feeds output of the child. Effects go to `fx` in the order they happened.
    fn advance(&mut self, bytes: &[u8], fx: &mut dyn EffectSink);

    /// Changes the grid size.
    fn resize(&mut self, cols: u16, rows: u16);

    /// The size of the grid, as `(cols, rows)`.
    fn size(&self) -> (u16, u16);

    /// The modes that the UI needs.
    fn modes(&self) -> ModeFlags;

    /// The cursor, in viewport coordinates.
    fn cursor(&self) -> Cursor;

    fn title(&self) -> Option<&str>;

    /// The colours that the program set, over the theme.
    fn palette(&self) -> Palette;

    /// Calls `f` for each cell of row `y` of the viewport, from the first column to the last.
    fn visit_row(&self, y: u16, f: &mut dyn FnMut(CellView<'_>));

    /// The rows that changed since the last call. The damage is reset.
    fn take_damage(&mut self) -> Damage;

    /// Appends the lines that went into the scrollback since the last call, oldest first.
    fn take_new_history(&mut self, out: &mut Vec<HistoryLine>);

    /// How often a chunk of output filled the scrollback in memory, so that lines may be missing
    /// from [`VtEngine::take_new_history`]. It never happens with plain output.
    fn history_gaps(&self) -> u64;

    /// Scrolls the viewport so that absolute line `top_line` is on top. `None` follows the
    /// output. The line is clamped to the lines that the grid holds.
    fn scroll_display(&mut self, top_line: Option<u64>) -> Viewport;

    fn viewport(&self) -> Viewport;

    /// Sets the selection, or clears it with `None`. The points are absolute lines. The primary
    /// screen only: the alternate screen has no selection. A resize and a switch of the screen
    /// clear it.
    fn set_selection(&mut self, selection: Option<Selection>);

    /// The selection as the frame carries it: the points in order and widened to the unit of the
    /// mode, cut to the lines in memory. `None` when nothing is selected.
    fn selection(&self) -> Option<Selection>;

    /// The text of the selection.
    fn selection_text(&self) -> Option<String>;

    /// How often the bell rang. The count never decreases.
    fn bell_count(&self) -> u64;

    /// Tells the engine the size of a cell in pixels, for `CSI 14 t` replies.
    fn set_cell_size(&mut self, width: u16, height: u16);

    /// When a synchronized update (`CSI ? 2026 h`) must end by itself, if one is open. Until
    /// then the engine holds the output back, so that the screen never shows half a frame.
    fn sync_deadline(&self) -> Option<std::time::Instant>;

    /// Ends a synchronized update that has timed out and shows what it held back.
    fn flush_sync(&mut self, fx: &mut dyn EffectSink);

    /// Serializes the screen model, the modes and the other state that the engine needs to
    /// continue. `journal_offset` is the holder journal offset after the last byte
    /// that the engine has read.
    ///
    /// Call it only at a safe cut point of the holder: the parser must be between sequences. The
    /// engine is the same afterwards, with two exceptions. Taking a snapshot while the alternate
    /// screen is on show sets the saved cursor of the primary screen to its cursor, because the
    /// terminal offers no other way to read that screen. The saved cursor of the primary screen
    /// differs from its cursor only after a resize in the alternate screen. And the damage that
    /// the pane did not take yet stays.
    ///
    /// It takes `&mut self` for that reason. It fails while a synchronized update is open
    /// ([`SnapshotError::SyncOpen`]).
    fn snapshot(&mut self, journal_offset: u64) -> Result<Snapshot, SnapshotError>;

    /// Builds an engine in the state that `snapshot` holds. It continues from the snapshot's
    /// journal offset, with the screen shown from the top (not scrolled back) and all rows damaged.
    ///
    /// The scrollback in memory is empty: it is in the archive. A grid that grows later takes
    /// blank lines where the engine that was never restored takes lines of the scrollback.
    fn restore(snapshot: &Snapshot, config: &EngineConfig) -> Result<Self, RestoreError>
    where
        Self: Sized;
}
