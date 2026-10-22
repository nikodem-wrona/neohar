//! The serialized form of a pane.
//!
//! A snapshot is `nh-term`'s own storage format, not a wire format between peers. The core keeps
//! it in `state.db` next to the holder journal offset that it was taken at. It holds what the
//! engine needs to continue from that offset: the screens, the cursors, the modes, the tab stops,
//! the scroll region, the charsets, the titles, the palette overrides and the keyboard stacks. The
//! scrollback in memory is not part of it: it is in the archive.
//!
//! Cells keep everything that `alacritty_terminal` keeps, not only what a frame shows. So a screen
//! restores with its hyperlinks and its named colours.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::engine::HistoryLine;

/// The version of the format. A snapshot of another version does not decode.
pub const SNAPSHOT_VERSION: u32 = 1;

/// A colour of a cell, as the terminal holds it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SnapColor {
    /// A named colour. The number is the index of `NamedColor`: 0 to 15, then 256 and up.
    Named(u16),
    Indexed(u8),
    Rgb(u8, u8, u8),
}

/// A distinct cell of a screen. Screens refer to cells by their number in [`Snapshot::cells`].
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SnapCell {
    pub ch: char,
    /// The zero-width characters that combine with `ch`.
    pub zerowidth: Vec<char>,
    pub fg: SnapColor,
    pub bg: SnapColor,
    /// The `Flags` of the cell, as bits.
    pub flags: u16,
    pub underline: Option<SnapColor>,
    /// The identifier and the URI of a hyperlink.
    pub hyperlink: Option<(String, String)>,
}

impl SnapCell {
    /// The cell of a new screen.
    pub fn blank() -> Self {
        Self {
            ch: ' ',
            zerowidth: Vec::new(),
            fg: SnapColor::Named(256),
            bg: SnapColor::Named(257),
            flags: 0,
            underline: None,
            hyperlink: None,
        }
    }
}

/// A run of equal cells in a row.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Run {
    pub len: u16,
    /// The number of the cell in [`Snapshot::cells`].
    pub cell: u32,
}

/// A cursor with the state that `DECSC` saves.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapCursor {
    pub line: i32,
    pub column: u32,
    /// The cell that holds the current attributes (colours, flags, hyperlink).
    pub template: u32,
    /// The charsets G0 to G3: 0 for ASCII, 1 for DEC special graphics.
    pub charsets: [u8; 4],
    /// The cursor stands on the last column and the next character wraps (pending wrap).
    pub pending_wrap: bool,
}

/// One screen: the cells of its rows and its two cursors.
#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Screen {
    /// One list of runs per row, top to bottom. It is empty for a screen that is not on show and
    /// has no content to keep.
    pub rows: Vec<Vec<Run>>,
    pub cursor: Option<SnapCursor>,
    /// The cursor that `DECSC` saved. It survives when its screen goes off.
    pub saved: Option<SnapCursor>,
}

/// The lines counter of the history, with the lines that the pane did not take yet.
#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct HistoryState {
    /// How many lines ever left the screen.
    pub scrolled: u64,
    pub gaps: u64,
    pub pending: Vec<HistoryLine>,
    /// A resize in the alternate screen made the length of the history of the primary screen
    /// unknown. The count starts again when the screen goes off.
    pub stale: bool,
}

/// The state of a pane at a safe cut point of the holder journal.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    /// [`SNAPSHOT_VERSION`]. It is the first field, so that a reader can check it first.
    pub version: u32,
    /// The holder journal offset after the last byte that the engine had read.
    pub journal_offset: u64,
    pub cols: u16,
    pub rows: u16,
    /// The distinct cells of both screens. Cell 0 is the blank cell.
    pub cells: Vec<SnapCell>,
    /// The primary screen. Its content is always kept.
    pub primary: Screen,
    /// The alternate screen. Its content and cursor are kept only when it is on show. Its saved
    /// cursor is always kept: the terminal does not clear it when the screen opens again.
    pub alternate: Screen,
    pub alt_active: bool,
    /// The `TermMode` bits of `alacritty_terminal`.
    pub term_mode: u32,
    /// The kitty keyboard stack of the screen on show, from the bottom. Each entry is the flag bits.
    pub kitty: Vec<u8>,
    /// The stack of the other screen.
    pub kitty_inactive: Vec<u8>,
    pub mouse_x10: bool,
    pub mouse_urxvt: bool,
    /// The level of `modifyOtherKeys`: 0, 1 or 2.
    pub modify_other_keys: u8,
    /// The columns that have a tab stop, in order.
    pub tabs: Vec<u16>,
    /// The scroll region as lines `start..end`.
    pub region: (u16, u16),
    /// The active charset: 0 to 3 for G0 to G3.
    pub active_charset: u8,
    pub title: Option<String>,
    /// The saved titles, from the bottom.
    pub title_stack: Vec<Option<String>>,
    /// The shape of the cursor: 0 block, 1 underline, 2 beam, 3 hollow block, 4 hidden.
    pub cursor_shape: u8,
    pub cursor_blinking: bool,
    /// The colours that a program set: the slot, then `0x00RRGGBB`.
    pub colors: Vec<(u16, u32)>,
    /// The character that `CSI n b` repeats.
    pub last_char: Option<char>,
    pub history: HistoryState,
    pub bells: u64,
}

/// Why a snapshot cannot be made.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SnapshotError {
    /// A synchronized update is open. The engine holds output back that is in no screen yet.
    /// Take the snapshot at a later cut point, or call `flush_sync` first.
    #[error("a synchronized update is open")]
    SyncOpen,
    /// The engine could not rebuild itself from its own snapshot ([`crate::AlacrittyEngine::checkpoint`]).
    /// That is a bug in the restore, and the engine is unchanged.
    #[error("the engine did not rebuild from its snapshot: {0}")]
    Rebuild(&'static str),
}

/// Why a snapshot does not decode or restore.
#[derive(Debug, thiserror::Error)]
pub enum RestoreError {
    #[error("snapshot version {found}, expected {expected}")]
    Version { found: u32, expected: u32 },
    #[error("the snapshot does not decode: {0}")]
    Decode(#[from] postcard::Error),
    #[error("the snapshot is not consistent: {0}")]
    Invalid(&'static str),
}

impl Snapshot {
    /// The bytes to store. The format is `postcard`.
    pub fn encode(&self) -> Vec<u8> {
        postcard::to_stdvec(self).expect("a snapshot always serializes into a vector")
    }

    /// Reads a snapshot. A version other than [`SNAPSHOT_VERSION`] is an error, whatever follows.
    pub fn decode(bytes: &[u8]) -> Result<Self, RestoreError> {
        let (version, _) = postcard::take_from_bytes::<u32>(bytes)?;
        if version != SNAPSHOT_VERSION {
            return Err(RestoreError::Version {
                found: version,
                expected: SNAPSHOT_VERSION,
            });
        }
        let snapshot: Self = postcard::from_bytes(bytes)?;
        snapshot.validate()?;
        Ok(snapshot)
    }

    /// Checks that the numbers in the snapshot refer to things that exist, so that a restore
    /// cannot panic on a snapshot that was damaged on disk.
    pub fn validate(&self) -> Result<(), RestoreError> {
        let invalid = |what| Err(RestoreError::Invalid(what));
        if self.version != SNAPSHOT_VERSION {
            return Err(RestoreError::Version {
                found: self.version,
                expected: SNAPSHOT_VERSION,
            });
        }
        if self.cols < 2 || self.rows < 1 {
            return invalid("the size is too small");
        }
        if self.cells.is_empty() {
            return invalid("the cell table is empty");
        }
        let cells = self.cells.len() as u32;
        let check_cursor = |c: &SnapCursor| {
            c.template < cells
                && c.line >= 0
                && c.line < i32::from(self.rows)
                && c.column < u32::from(self.cols)
                && c.charsets.iter().all(|&s| s <= 1)
        };
        for (screen, shown) in [(&self.primary, true), (&self.alternate, self.alt_active)] {
            if shown {
                if screen.rows.len() != usize::from(self.rows) {
                    return invalid("a screen has the wrong number of rows");
                }
                for row in &screen.rows {
                    let width: u32 = row.iter().map(|r| u32::from(r.len)).sum();
                    if width != u32::from(self.cols) || row.iter().any(|r| r.cell >= cells) {
                        return invalid("a row does not fit the width or names a missing cell");
                    }
                }
                if !screen.cursor.as_ref().is_some_and(check_cursor) {
                    return invalid("the cursor is missing or out of range");
                }
            } else if !screen.rows.is_empty() {
                return invalid("a screen that is not shown has content");
            }
            if !screen.saved.as_ref().is_some_and(check_cursor) {
                return invalid("the saved cursor is missing or out of range");
            }
        }
        if self.tabs.iter().any(|&t| t >= self.cols) || !self.tabs.is_sorted() {
            return invalid("a tab stop is out of range or out of order");
        }
        if self.region.0 > self.rows || self.region.1 > self.rows {
            return invalid("the scroll region is out of range");
        }
        if self.active_charset > 3 || self.cursor_shape > 4 || self.modify_other_keys > 2 {
            return invalid("a mode value is out of range");
        }
        if self
            .kitty
            .iter()
            .chain(&self.kitty_inactive)
            .any(|&k| k > 0b1_1111)
        {
            return invalid("a keyboard flag is out of range");
        }
        if self.colors.iter().any(|&(slot, _)| slot >= 269) {
            return invalid("a colour slot is out of range");
        }
        Ok(())
    }
}

/// Builds the cell table of a snapshot, and numbers equal cells alike.
#[derive(Debug)]
pub(crate) struct CellTable {
    cells: Vec<SnapCell>,
    index: HashMap<SnapCell, u32>,
}

impl CellTable {
    pub fn new() -> Self {
        let blank = SnapCell::blank();
        let mut index = HashMap::new();
        index.insert(blank.clone(), 0);
        Self {
            cells: vec![blank],
            index,
        }
    }

    /// The number of `cell`. It joins the table when it is new.
    pub fn number(&mut self, cell: SnapCell) -> u32 {
        if let Some(&n) = self.index.get(&cell) {
            return n;
        }
        let n = self.cells.len() as u32;
        self.index.insert(cell.clone(), n);
        self.cells.push(cell);
        n
    }

    pub fn into_cells(self) -> Vec<SnapCell> {
        self.cells
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal() -> Snapshot {
        let cursor = SnapCursor {
            line: 0,
            column: 0,
            template: 0,
            charsets: [0; 4],
            pending_wrap: false,
        };
        let shown = Screen {
            rows: vec![vec![Run { len: 4, cell: 0 }]; 2],
            cursor: Some(cursor.clone()),
            saved: Some(cursor.clone()),
        };
        Snapshot {
            version: SNAPSHOT_VERSION,
            journal_offset: 77,
            cols: 4,
            rows: 2,
            cells: vec![SnapCell::blank()],
            primary: shown,
            alternate: Screen {
                rows: Vec::new(),
                cursor: None,
                saved: Some(cursor),
            },
            alt_active: false,
            term_mode: 0,
            kitty: Vec::new(),
            kitty_inactive: Vec::new(),
            mouse_x10: false,
            mouse_urxvt: false,
            modify_other_keys: 0,
            tabs: vec![0],
            region: (0, 2),
            active_charset: 0,
            title: None,
            title_stack: Vec::new(),
            cursor_shape: 0,
            cursor_blinking: false,
            colors: Vec::new(),
            last_char: None,
            history: HistoryState::default(),
            bells: 0,
        }
    }

    #[test]
    fn a_snapshot_survives_the_encoding() {
        let snapshot = minimal();
        let bytes = snapshot.encode();
        assert_eq!(Snapshot::decode(&bytes).expect("decodes"), snapshot);
    }

    #[test]
    fn the_version_is_checked_before_anything_else() {
        let mut bytes = minimal().encode();
        bytes[0] = (SNAPSHOT_VERSION + 1) as u8;
        // A version from the future may have another layout after the first field.
        bytes.truncate(3);
        assert!(matches!(
            Snapshot::decode(&bytes),
            Err(RestoreError::Version {
                found: 2,
                expected: 1
            })
        ));
    }

    #[test]
    fn a_damaged_snapshot_is_an_error_and_never_a_panic() {
        let mut s = minimal();
        s.primary.rows[0] = vec![Run { len: 3, cell: 0 }];
        assert!(matches!(s.validate(), Err(RestoreError::Invalid(_))));
        let mut s = minimal();
        s.primary.rows[1] = vec![Run { len: 4, cell: 9 }];
        assert!(matches!(s.validate(), Err(RestoreError::Invalid(_))));
        let mut s = minimal();
        s.tabs = vec![3, 1];
        assert!(matches!(s.validate(), Err(RestoreError::Invalid(_))));
        let mut s = minimal();
        s.primary.cursor.as_mut().expect("has a cursor").column = 4;
        assert!(matches!(s.validate(), Err(RestoreError::Invalid(_))));
        let bytes = minimal().encode();
        for cut in 0..bytes.len() {
            // Every prefix either fails to decode or fails to validate. None panics.
            let _ = Snapshot::decode(&bytes[..cut]);
        }
    }

    #[test]
    fn equal_cells_share_a_number() {
        let mut table = CellTable::new();
        let mut a = SnapCell::blank();
        a.ch = 'x';
        let n = table.number(a.clone());
        assert_eq!(n, 1);
        assert_eq!(table.number(a), 1);
        assert_eq!(table.number(SnapCell::blank()), 0);
        assert_eq!(table.into_cells().len(), 2);
    }
}
