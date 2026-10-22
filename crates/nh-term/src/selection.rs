//! Selecting text in the screen model.
//!
//! A selection has two points in **absolute lines**, the unit of `Viewport::top_line`, so it stays
//! where it is while new lines scroll the screen. It covers the lines that the engine holds in
//! memory: the visible grid and the history. A line that only the archive holds can be read with
//! `pane.read`.
//!
//! [`normalize`] puts the two points in order and widens them to the unit of the mode: a word, a
//! logical line, or a wide character. The frame carries the widened points, so the UI draws what
//! the core will copy and does not need to know where a word ends. [`text`] reads the selected
//! cells.

use alacritty_terminal::{
    grid::{Dimensions, Grid},
    index::{Column, Line},
    term::cell::{Cell, Flags},
};
use nh_proto::frame::{Selection, SelectionMode, SelectionPoint};

/// The characters that end a word, as in the default of `alacritty_terminal`.
const SEPARATORS: &str = ",│`|:\"' ()[]{}<>\t";

/// The grid of the primary screen, with the number of lines that ever left it.
pub(crate) struct Lines<'a> {
    grid: &'a Grid<Cell>,
    /// The absolute line of the top row of the live screen.
    scrolled: u64,
}

impl<'a> Lines<'a> {
    pub(crate) fn new(grid: &'a Grid<Cell>, scrolled: u64) -> Self {
        Self { grid, scrolled }
    }

    /// The oldest absolute line in memory.
    fn first(&self) -> u64 {
        self.scrolled
            .saturating_sub(self.grid.history_size() as u64)
    }

    /// The newest absolute line, the bottom row of the live screen.
    fn last(&self) -> u64 {
        self.scrolled + self.grid.screen_lines() as u64 - 1
    }

    fn cols(&self) -> u16 {
        u16::try_from(self.grid.columns()).unwrap_or(u16::MAX)
    }

    /// The cell at an absolute line and a column, if the line is in memory.
    fn cell(&self, line: u64, col: u16) -> Option<&Cell> {
        if line < self.first() || line > self.last() || usize::from(col) >= self.grid.columns() {
            return None;
        }
        let index = i64::try_from(line).ok()? - i64::try_from(self.scrolled).ok()?;
        Some(&self.grid[Line(i32::try_from(index).ok()?)][Column(usize::from(col))])
    }

    /// Whether the row continues in the next row (a soft wrap).
    fn wraps(&self, line: u64) -> bool {
        self.cols()
            .checked_sub(1)
            .and_then(|last| self.cell(line, last))
            .is_some_and(|cell| cell.flags.contains(Flags::WRAPLINE))
    }

    fn is_separator(&self, line: u64, col: u16) -> bool {
        self.cell(line, col)
            .is_none_or(|cell| SEPARATORS.contains(cell.c) || cell.c == '\0')
    }
}

/// The two points of `selection` in order, widened to the unit of its mode, and cut to the lines
/// in memory. `None` when none of its lines is in memory.
pub(crate) fn normalize(lines: &Lines<'_>, selection: Selection) -> Option<Selection> {
    let Selection { start, end, mode } = selection;
    let (mut a, mut b) = if mode == SelectionMode::Block {
        (
            SelectionPoint {
                col: start.col.min(end.col),
                line: start.line.min(end.line),
            },
            SelectionPoint {
                col: start.col.max(end.col),
                line: start.line.max(end.line),
            },
        )
    } else if (start.line, start.col) <= (end.line, end.col) {
        (start, end)
    } else {
        (end, start)
    };
    let last_col = lines.cols().checked_sub(1)?;
    if b.line < lines.first() || a.line > lines.last() {
        return None;
    }
    if a.line < lines.first() {
        a = SelectionPoint {
            col: 0,
            line: lines.first(),
        };
    }
    if b.line > lines.last() {
        b = SelectionPoint {
            col: last_col,
            line: lines.last(),
        };
    }
    a.col = a.col.min(last_col);
    b.col = b.col.min(last_col);
    match mode {
        SelectionMode::Simple | SelectionMode::Block => {}
        SelectionMode::Word => {
            // A word is a run of cells of one kind: letters, or separators.
            let kind = lines.is_separator(a.line, a.col);
            while a.col > 0 && lines.is_separator(a.line, a.col - 1) == kind {
                a.col -= 1;
            }
            let kind = lines.is_separator(b.line, b.col);
            while b.col < last_col && lines.is_separator(b.line, b.col + 1) == kind {
                b.col += 1;
            }
        }
        SelectionMode::Line => {
            // A logical line is the rows that soft wraps join.
            while a.line > lines.first() && lines.wraps(a.line - 1) {
                a.line -= 1;
            }
            while b.line < lines.last() && lines.wraps(b.line) {
                b.line += 1;
            }
            a.col = 0;
            b.col = last_col;
        }
    }
    // A wide character is both of its cells.
    if lines
        .cell(a.line, a.col)
        .is_some_and(|cell| cell.flags.contains(Flags::WIDE_CHAR_SPACER))
    {
        a.col = a.col.saturating_sub(1);
    }
    if lines
        .cell(b.line, b.col)
        .is_some_and(|cell| cell.flags.contains(Flags::WIDE_CHAR))
    {
        b.col = (b.col + 1).min(last_col);
    }
    Some(Selection {
        start: a,
        end: b,
        mode,
    })
}

/// The text of a selection that [`normalize`] made. A soft wrap joins two rows, the blank cells at
/// the end of a row are dropped when the selection reaches the end of it, and a block has a line
/// for each row.
pub(crate) fn text(lines: &Lines<'_>, selection: &Selection) -> String {
    let Selection { start, end, mode } = *selection;
    let last_col = lines.cols().saturating_sub(1);
    let mut out = String::new();
    for line in start.line..=end.line {
        let (from, to) = match mode {
            SelectionMode::Block => (start.col, end.col),
            _ => (
                if line == start.line { start.col } else { 0 },
                if line == end.line { end.col } else { last_col },
            ),
        };
        let mut row = String::new();
        for col in from..=to {
            let Some(cell) = lines.cell(line, col) else {
                continue;
            };
            if cell
                .flags
                .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
            {
                continue;
            }
            row.push(cell.c);
            if let Some(extra) = cell.zerowidth() {
                row.extend(extra);
            }
        }
        if to == last_col {
            row.truncate(row.trim_end_matches([' ', '\0']).len());
        }
        out.push_str(&row);
        if line != end.line && (mode == SelectionMode::Block || !lines.wraps(line)) {
            out.push('\n');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AlacrittyEngine, MIN_HISTORY, Recorder, VtEngine, xterm_palette};

    /// An engine of 20 columns and 5 rows that has read `text`.
    fn engine(text: &str) -> AlacrittyEngine {
        let mut engine = AlacrittyEngine::new(20, 5, MIN_HISTORY, xterm_palette());
        engine.advance(text.as_bytes(), &mut Recorder::default());
        engine
    }

    fn point(col: u16, line: u64) -> SelectionPoint {
        SelectionPoint { col, line }
    }

    fn select(
        engine: &mut AlacrittyEngine,
        from: (u16, u64),
        to: (u16, u64),
        mode: SelectionMode,
    ) -> Option<String> {
        engine.set_selection(Some(Selection {
            start: point(from.0, from.1),
            end: point(to.0, to.1),
            mode,
        }));
        engine.selection_text()
    }

    #[test]
    fn a_simple_selection_takes_the_cells_between_the_points_in_either_order() {
        let mut e = engine("hello world\r\nsecond line\r\nthird");
        assert_eq!(
            select(&mut e, (0, 0), (4, 0), SelectionMode::Simple).as_deref(),
            Some("hello")
        );
        assert_eq!(
            select(&mut e, (4, 0), (0, 0), SelectionMode::Simple).as_deref(),
            Some("hello"),
            "the head can come before the anchor"
        );
        // Over several lines: the first from its column, the last up to its column, and no blank
        // cells at the end of a line that the selection reaches.
        assert_eq!(
            select(&mut e, (6, 0), (4, 2), SelectionMode::Simple).as_deref(),
            Some("world\nsecond line\nthird")
        );
        // A blank cell inside the selection stays.
        assert_eq!(
            select(&mut e, (6, 0), (5, 2), SelectionMode::Simple).as_deref(),
            Some("world\nsecond line\nthird ")
        );
        assert_eq!(
            select(&mut e, (6, 0), (2, 2), SelectionMode::Simple).as_deref(),
            Some("world\nsecond line\nthi")
        );
    }

    #[test]
    fn the_points_of_the_frame_are_in_order_and_widened() {
        let mut e = engine("hello world");
        e.set_selection(Some(Selection {
            start: point(8, 0),
            end: point(2, 0),
            mode: SelectionMode::Word,
        }));
        // Two words, from the start of "hello" to the end of "world".
        let shown = e.selection().expect("a selection");
        assert_eq!((shown.start, shown.end), (point(0, 0), point(10, 0)));
        assert_eq!(shown.mode, SelectionMode::Word);
        assert_eq!(e.selection_text().as_deref(), Some("hello world"));
    }

    #[test]
    fn a_word_ends_at_a_separator() {
        let mut e = engine("/usr/local-bin (now)");
        // The dash and the slashes are part of a word. The space and the brackets end it.
        assert_eq!(
            select(&mut e, (8, 0), (8, 0), SelectionMode::Word).as_deref(),
            Some("/usr/local-bin")
        );
        assert_eq!(
            select(&mut e, (17, 0), (17, 0), SelectionMode::Word).as_deref(),
            Some("now")
        );
        // A run of separators is a word of its own.
        assert_eq!(
            select(&mut e, (14, 0), (14, 0), SelectionMode::Word).as_deref(),
            Some(" (")
        );
    }

    #[test]
    fn a_line_selection_takes_the_whole_logical_line() {
        // 44 characters wrap over three rows of 20 columns.
        let long = "0123456789".repeat(4) + "abcd";
        let mut e = engine(&format!("short\r\n{long}\r\nlast"));
        assert_eq!(
            select(&mut e, (3, 0), (1, 0), SelectionMode::Line).as_deref(),
            Some("short")
        );
        // A point in the middle row takes the three rows, joined by the wrap.
        assert_eq!(
            select(&mut e, (4, 2), (4, 2), SelectionMode::Line).as_deref(),
            Some(long.as_str())
        );
        assert_eq!(
            select(&mut e, (0, 0), (0, 4), SelectionMode::Line).as_deref(),
            Some(format!("short\n{long}\nlast").as_str())
        );
    }

    #[test]
    fn a_block_is_a_rectangle() {
        let mut e = engine("abcdef\r\nghijkl\r\nmnopqr");
        assert_eq!(
            select(&mut e, (1, 0), (3, 2), SelectionMode::Block).as_deref(),
            Some("bcd\nhij\nnop")
        );
        // The corners can be given in any order.
        assert_eq!(
            select(&mut e, (3, 2), (1, 0), SelectionMode::Block).as_deref(),
            Some("bcd\nhij\nnop")
        );
        assert_eq!(
            select(&mut e, (3, 0), (1, 2), SelectionMode::Block).as_deref(),
            Some("bcd\nhij\nnop")
        );
    }

    #[test]
    fn a_wide_character_is_selected_whole() {
        let mut e = engine("日本語 x");
        // Column 1 is the second half of the first character.
        assert_eq!(
            select(&mut e, (1, 0), (2, 0), SelectionMode::Simple).as_deref(),
            Some("日本")
        );
        let shown = e.selection().expect("a selection");
        assert_eq!((shown.start.col, shown.end.col), (0, 3));
    }

    #[test]
    fn lines_of_the_history_can_be_selected_and_lines_over_the_end_are_cut() {
        // 20 lines on a screen of 5 rows: lines 0 to 15 are in the history.
        let text: String = (0..20).map(|n| format!("line{n}\r\n")).collect();
        let mut e = engine(&text);
        assert_eq!(
            select(&mut e, (0, 2), (4, 3), SelectionMode::Simple).as_deref(),
            Some("line2\nline3")
        );
        // The end is far below the last line: the selection stops at the last row of the screen.
        let all = select(&mut e, (0, 17), (3, 900), SelectionMode::Simple).expect("text");
        assert!(all.starts_with("line17"), "{all:?}");
        let shown = e.selection().expect("a selection");
        assert_eq!(shown.end.line, e.viewport().total_lines - 1);
        // A selection after the last line is nothing.
        assert_eq!(
            select(&mut e, (0, 900), (3, 901), SelectionMode::Simple),
            None
        );
    }

    #[test]
    fn a_resize_and_the_alternate_screen_clear_the_selection() {
        let mut e = engine("hello");
        select(&mut e, (0, 0), (4, 0), SelectionMode::Simple);
        e.resize(30, 5);
        assert_eq!(e.selection(), None);
        select(&mut e, (0, 0), (4, 0), SelectionMode::Simple);
        e.advance(b"\x1b[?1049h", &mut Recorder::default());
        assert_eq!(e.selection(), None);
        // The alternate screen cannot be selected, and the selection does not come back.
        assert_eq!(select(&mut e, (0, 0), (4, 0), SelectionMode::Simple), None);
        e.advance(b"\x1b[?1049l", &mut Recorder::default());
        assert_eq!(e.selection(), None);
    }

    #[test]
    fn a_change_of_the_selection_starts_a_revision_and_each_frame_carries_it() {
        use crate::{ClientState, FrameBuilder};
        use nh_proto::frame::FrameKind;
        let mut e = engine("hello");
        let mut builder = FrameBuilder::new(1, &e);
        let mut client = ClientState::new();
        builder.absorb(&mut e);
        let first = builder.build(&e, &mut client, FrameKind::Keyframe);
        assert_eq!(first.selection, None);

        select(&mut e, (0, 0), (4, 0), SelectionMode::Simple);
        assert!(builder.absorb(&mut e).changed, "a selection is a change");
        let delta = builder.build(&e, &mut client, FrameKind::Delta);
        assert!(delta.rev > first.rev);
        assert_eq!(
            delta.selection.map(|s| (s.start, s.end)),
            Some((point(0, 0), point(4, 0)))
        );
        // The frames after it carry the selection, too: the client takes the state of each one.
        builder.absorb(&mut e);
        let again = builder.build(&e, &mut client, FrameKind::Delta);
        assert!(again.selection.is_some());
        let key = builder.build(&e, &mut client, FrameKind::Keyframe);
        assert!(key.selection.is_some());

        e.set_selection(None);
        assert!(builder.absorb(&mut e).changed);
        let cleared = builder.build(&e, &mut client, FrameKind::Delta);
        assert_eq!(cleared.selection, None);
    }

    #[test]
    fn a_selection_stays_on_its_lines_while_the_screen_scrolls() {
        let text: String = (0..6).map(|n| format!("row{n}\r\n")).collect();
        let mut e = engine(&text);
        assert_eq!(
            select(&mut e, (0, 1), (3, 1), SelectionMode::Simple).as_deref(),
            Some("row1")
        );
        e.advance(b"more\r\nand more\r\n", &mut Recorder::default());
        assert_eq!(e.selection_text().as_deref(), Some("row1"));
        assert_eq!(e.selection().expect("selected").start.line, 1);
    }
}
