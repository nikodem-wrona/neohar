//! `AlacrittyEngine::snapshot` and `restore`.
//!
//! Reading uses what `alacritty_terminal` shows: the active grid, and the mirror of what it does
//! not show (`crate::mirror`). The other screen is a different matter: the terminal has no getter
//! for it, and `Term::swap_alt` is the one way to reach it. A swap changes some state, and the
//! functions below put it back. The one thing that they cannot put back is the saved cursor of
//! the primary screen while the alternate screen is on show: entering the alternate screen sets it
//! to the cursor of the primary screen, and the primary screen is not writable after that.
//!
//! Writing builds a new engine and drives it through the same `Handler` calls that the parser
//! makes, so that the terminal and the mirror both see them. Cells and cursors are written
//! directly into the grids. At the end the engine checks that what it built is what the snapshot
//! says.

use alacritty_terminal::{
    grid::{Charsets, Cursor, Dimensions, Grid, Row},
    index::{Column, Line, Point},
    term::{
        TermMode,
        cell::{Cell, Flags, Hyperlink},
    },
    vte::ansi::{
        CharsetIndex, Color, CursorShape, CursorStyle, Handler, KeyboardModes,
        KeyboardModesApplyBehavior, Mode, ModifyOtherKeys, NamedColor, NamedMode, NamedPrivateMode,
        PrivateMode, Rgb, StandardCharset, TabulationClearMode,
    },
};

use crate::{
    alacritty::AlacrittyEngine,
    engine::{EngineConfig, VtEngine},
    history::HistoryTracker,
    mirror::{Tee, default_tabs},
    snapshot::{
        CellTable, HistoryState, RestoreError, Run, SNAPSHOT_VERSION, Screen, SnapCell, SnapColor,
        SnapCursor, Snapshot, SnapshotError,
    },
};

/// Every `NamedColor`, to turn the number in a snapshot back into the colour.
const NAMED: [NamedColor; 29] = [
    NamedColor::Black,
    NamedColor::Red,
    NamedColor::Green,
    NamedColor::Yellow,
    NamedColor::Blue,
    NamedColor::Magenta,
    NamedColor::Cyan,
    NamedColor::White,
    NamedColor::BrightBlack,
    NamedColor::BrightRed,
    NamedColor::BrightGreen,
    NamedColor::BrightYellow,
    NamedColor::BrightBlue,
    NamedColor::BrightMagenta,
    NamedColor::BrightCyan,
    NamedColor::BrightWhite,
    NamedColor::Foreground,
    NamedColor::Background,
    NamedColor::Cursor,
    NamedColor::DimBlack,
    NamedColor::DimRed,
    NamedColor::DimGreen,
    NamedColor::DimYellow,
    NamedColor::DimBlue,
    NamedColor::DimMagenta,
    NamedColor::DimCyan,
    NamedColor::DimWhite,
    NamedColor::BrightForeground,
    NamedColor::DimForeground,
];

/// The slots of the colour table of the terminal.
const COLOR_SLOTS: usize = 269;

const CHARSET_SLOTS: [CharsetIndex; 4] = [
    CharsetIndex::G0,
    CharsetIndex::G1,
    CharsetIndex::G2,
    CharsetIndex::G3,
];

/// The kitty flags of the terminal and of the keyboard stack, in the same order.
const KITTY: [(TermMode, KeyboardModes); 5] = [
    (
        TermMode::DISAMBIGUATE_ESC_CODES,
        KeyboardModes::DISAMBIGUATE_ESC_CODES,
    ),
    (
        TermMode::REPORT_EVENT_TYPES,
        KeyboardModes::REPORT_EVENT_TYPES,
    ),
    (
        TermMode::REPORT_ALTERNATE_KEYS,
        KeyboardModes::REPORT_ALTERNATE_KEYS,
    ),
    (
        TermMode::REPORT_ALL_KEYS_AS_ESC,
        KeyboardModes::REPORT_ALL_KEYS_AS_ESC,
    ),
    (
        TermMode::REPORT_ASSOCIATED_TEXT,
        KeyboardModes::REPORT_ASSOCIATED_TEXT,
    ),
];

/// How a mode of the terminal is turned on and off.
#[derive(Clone, Copy)]
enum Way {
    Private(NamedPrivateMode),
    Ansi(NamedMode),
    Keypad,
}

/// The modes that a restore sets, except the screen and the kitty flags.
const MODES: [(TermMode, Way); 16] = [
    (
        TermMode::SHOW_CURSOR,
        Way::Private(NamedPrivateMode::ShowCursor),
    ),
    (
        TermMode::APP_CURSOR,
        Way::Private(NamedPrivateMode::CursorKeys),
    ),
    (TermMode::APP_KEYPAD, Way::Keypad),
    (
        TermMode::MOUSE_REPORT_CLICK,
        Way::Private(NamedPrivateMode::ReportMouseClicks),
    ),
    (
        TermMode::MOUSE_DRAG,
        Way::Private(NamedPrivateMode::ReportCellMouseMotion),
    ),
    (
        TermMode::MOUSE_MOTION,
        Way::Private(NamedPrivateMode::ReportAllMouseMotion),
    ),
    (
        TermMode::BRACKETED_PASTE,
        Way::Private(NamedPrivateMode::BracketedPaste),
    ),
    (
        TermMode::SGR_MOUSE,
        Way::Private(NamedPrivateMode::SgrMouse),
    ),
    (
        TermMode::UTF8_MOUSE,
        Way::Private(NamedPrivateMode::Utf8Mouse),
    ),
    (
        TermMode::FOCUS_IN_OUT,
        Way::Private(NamedPrivateMode::ReportFocusInOut),
    ),
    (
        TermMode::ALTERNATE_SCROLL,
        Way::Private(NamedPrivateMode::AlternateScroll),
    ),
    (
        TermMode::LINE_WRAP,
        Way::Private(NamedPrivateMode::LineWrap),
    ),
    (TermMode::ORIGIN, Way::Private(NamedPrivateMode::Origin)),
    (TermMode::INSERT, Way::Ansi(NamedMode::Insert)),
    (
        TermMode::LINE_FEED_NEW_LINE,
        Way::Ansi(NamedMode::LineFeedNewLine),
    ),
    (
        TermMode::URGENCY_HINTS,
        Way::Private(NamedPrivateMode::UrgencyHints),
    ),
];

fn snap_color(color: Color) -> SnapColor {
    match color {
        Color::Named(name) => SnapColor::Named(name as u16),
        Color::Indexed(i) => SnapColor::Indexed(i),
        Color::Spec(Rgb { r, g, b }) => SnapColor::Rgb(r, g, b),
    }
}

fn color_of(color: SnapColor) -> Result<Color, RestoreError> {
    Ok(match color {
        SnapColor::Named(n) => Color::Named(
            *NAMED
                .iter()
                .find(|name| **name as u16 == n)
                .ok_or(RestoreError::Invalid("a named colour does not exist"))?,
        ),
        SnapColor::Indexed(i) => Color::Indexed(i),
        SnapColor::Rgb(r, g, b) => Color::Spec(Rgb { r, g, b }),
    })
}

fn snap_cell(cell: &Cell) -> SnapCell {
    SnapCell {
        ch: cell.c,
        zerowidth: cell.zerowidth().map(<[char]>::to_vec).unwrap_or_default(),
        fg: snap_color(cell.fg),
        bg: snap_color(cell.bg),
        flags: cell.flags.bits(),
        underline: cell.underline_color().map(snap_color),
        hyperlink: cell
            .hyperlink()
            .map(|link| (link.id().to_owned(), link.uri().to_owned())),
    }
}

fn build_cell(snap: &SnapCell) -> Result<Cell, RestoreError> {
    let mut cell = Cell {
        c: snap.ch,
        fg: color_of(snap.fg)?,
        bg: color_of(snap.bg)?,
        flags: Flags::from_bits_retain(snap.flags),
        extra: None,
    };
    for &z in &snap.zerowidth {
        cell.push_zerowidth(z);
    }
    if let Some(color) = snap.underline {
        cell.set_underline_color(Some(color_of(color)?));
    }
    if let Some((id, uri)) = &snap.hyperlink {
        cell.set_hyperlink(Some(Hyperlink::new(Some(id.clone()), uri.clone())));
    }
    Ok(cell)
}

fn charset_number(charset: StandardCharset) -> u8 {
    match charset {
        StandardCharset::Ascii => 0,
        StandardCharset::SpecialCharacterAndLineDrawing => 1,
    }
}

fn charset_of(number: u8) -> StandardCharset {
    if number == 1 {
        StandardCharset::SpecialCharacterAndLineDrawing
    } else {
        StandardCharset::Ascii
    }
}

fn shape_number(shape: CursorShape) -> u8 {
    match shape {
        CursorShape::Block => 0,
        CursorShape::Underline => 1,
        CursorShape::Beam => 2,
        CursorShape::HollowBlock => 3,
        CursorShape::Hidden => 4,
    }
}

fn shape_of(number: u8) -> CursorShape {
    match number {
        1 => CursorShape::Underline,
        2 => CursorShape::Beam,
        3 => CursorShape::HollowBlock,
        4 => CursorShape::Hidden,
        _ => CursorShape::Block,
    }
}

fn charset_slot(number: u8) -> CharsetIndex {
    CHARSET_SLOTS[usize::from(number.min(3))]
}

fn modify_other_keys_of(level: u8) -> ModifyOtherKeys {
    match level {
        0 => ModifyOtherKeys::Reset,
        1 => ModifyOtherKeys::EnableExceptWellDefined,
        _ => ModifyOtherKeys::EnableAll,
    }
}

/// The keyboard flags that are on in `mode`, in the form of the keyboard stack.
fn kitty_flags(mode: TermMode) -> KeyboardModes {
    KITTY
        .iter()
        .filter(|(term, _)| mode.contains(*term))
        .fold(KeyboardModes::NO_MODE, |acc, (_, kitty)| acc | *kitty)
}

fn snap_cursor(cursor: &Cursor<Cell>, table: &mut CellTable) -> SnapCursor {
    SnapCursor {
        line: cursor.point.line.0,
        column: cursor.point.column.0 as u32,
        template: table.number(snap_cell(&cursor.template)),
        charsets: CHARSET_SLOTS.map(|slot| charset_number(cursor.charsets[slot])),
        pending_wrap: cursor.input_needs_wrap,
    }
}

fn cursor_of(snap: &SnapCursor, cells: &[Cell]) -> Cursor<Cell> {
    let mut charsets = Charsets::default();
    for (slot, number) in CHARSET_SLOTS.iter().zip(snap.charsets) {
        charsets[*slot] = charset_of(number);
    }
    Cursor {
        point: Point::new(Line(snap.line), Column(snap.column as usize)),
        template: cells
            .get(snap.template as usize)
            .cloned()
            .unwrap_or_default(),
        charsets,
        input_needs_wrap: snap.pending_wrap,
    }
}

/// A screen as the snapshot holds it: the runs of its rows and its two cursors.
fn read_screen(grid: &Grid<Cell>, table: &mut CellTable) -> Screen {
    Screen {
        rows: read_rows(grid, table),
        cursor: Some(snap_cursor(&grid.cursor, table)),
        saved: Some(snap_cursor(&grid.saved_cursor, table)),
    }
}

/// The rows of the live screen as runs of cells. The viewport does not matter: `Line(0)` is the
/// top row of the live screen whatever the display offset is.
fn read_rows(grid: &Grid<Cell>, table: &mut CellTable) -> Vec<Vec<Run>> {
    let (cols, rows) = (grid.columns(), grid.screen_lines());
    let mut out = Vec::with_capacity(rows);
    for y in 0..rows {
        let row = &grid[Line(y as i32)];
        let mut runs: Vec<Run> = Vec::new();
        let mut previous: Option<&Cell> = None;
        for x in 0..cols {
            let cell = &row[Column(x)];
            if let (Some(prev), Some(run)) = (previous, runs.last_mut())
                && prev == cell
                && run.len < u16::MAX
            {
                run.len += 1;
                continue;
            }
            runs.push(Run {
                len: 1,
                cell: table.number(snap_cell(cell)),
            });
            previous = Some(cell);
        }
        out.push(runs);
    }
    out
}

/// The rows of the scrollback in memory, the oldest first. The grid must be the primary one.
fn read_scrollback(grid: &Grid<Cell>) -> Vec<Row<Cell>> {
    (1..=grid.history_size())
        .rev()
        .map(|k| grid[Line(-(k as i32))].clone())
        .collect()
}

/// A copy of the scrollback of an engine, for tests (`nh-snapcheck`).
#[cfg(feature = "test-support")]
pub struct Scrollback(Vec<Row<Cell>>);

#[cfg(feature = "test-support")]
impl Scrollback {
    /// How many rows it holds.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl AlacrittyEngine {
    /// Takes a snapshot and rebuilds the engine from it, so that the engine that goes on is the
    /// engine that a restart would make.
    ///
    /// `alacritty_terminal` keeps state that it does not show and that a snapshot cannot hold: the
    /// rows that its grid reserves for the scrollback and then leaves untouched, and a counter per
    /// row that lets a reset skip the cells it thinks are clean. Output that uses them can leave
    /// a stale cell on the screen. A restored engine starts without them, so a restored engine and
    /// the engine that was never restored can show different cells after the same output. After a
    /// checkpoint both are the same engine, built the same way. It costs one rebuild: a copy of the
    /// scrollback in memory and of the screens, about a millisecond.
    ///
    /// The damage that the pane did not take stays. Everything else that [`VtEngine::snapshot`]
    /// says holds.
    pub fn checkpoint(&mut self, journal_offset: u64) -> Result<Snapshot, SnapshotError> {
        self.checkpoint_impl(journal_offset)
            .map(|(snapshot, _)| snapshot)
    }

    fn checkpoint_impl(
        &mut self,
        journal_offset: u64,
    ) -> Result<(Snapshot, Vec<Row<Cell>>), SnapshotError> {
        let (snapshot, scrollback) = self.snapshot_impl(journal_offset, true)?;
        let config = EngineConfig {
            history: self.history.capacity(),
            theme: self.theme.clone(),
            cell_px: self.cell_px,
        };
        let mut rebuilt = Self::from_snapshot_with(&snapshot, &config, &scrollback)
            .map_err(|_| SnapshotError::Rebuild("the snapshot does not restore"))?;
        // The new engine starts with every row damaged. The pane has seen the screen, so it
        // gets the damage that the old engine had.
        rebuilt.drain_term_damage();
        rebuilt.carried = self.carried.take();
        rebuilt.last_cursor_row = self.last_cursor_row;
        *self = rebuilt;
        Ok((snapshot, scrollback))
    }
}

#[cfg(feature = "test-support")]
impl AlacrittyEngine {
    /// Like [`AlacrittyEngine::checkpoint`], and it also gives the copy of the scrollback.
    pub fn checkpoint_with_scrollback(
        &mut self,
        journal_offset: u64,
    ) -> Result<(Snapshot, Scrollback), SnapshotError> {
        self.checkpoint_impl(journal_offset)
            .map(|(snapshot, rows)| (snapshot, Scrollback(rows)))
    }

    /// Takes a snapshot, and copies the scrollback in memory cell by cell.
    ///
    /// The snapshot does not hold the scrollback. After a restore, a resize that
    /// pulls rows out of the scrollback, or that joins a wrapped row with the row above the
    /// screen, gives another result than in the engine that was never restored. Given this copy,
    /// [`AlacrittyEngine::restore_with_scrollback`] makes an engine that does the same.
    pub fn snapshot_with_scrollback(
        &mut self,
        journal_offset: u64,
    ) -> Result<(Snapshot, Scrollback), SnapshotError> {
        self.snapshot_impl(journal_offset, true)
            .map(|(snapshot, rows)| (snapshot, Scrollback(rows)))
    }

    /// Like [`VtEngine::restore`], with the scrollback of the engine that the snapshot came from.
    pub fn restore_with_scrollback(
        snapshot: &Snapshot,
        config: &EngineConfig,
        scrollback: &Scrollback,
    ) -> Result<Self, RestoreError> {
        Self::from_snapshot_with(snapshot, config, &scrollback.0)
    }
}

/// Writes every cell of the rows into the grid.
fn write_rows(grid: &mut Grid<Cell>, rows: &[Vec<Run>], cells: &[Cell]) {
    for (y, runs) in rows.iter().enumerate() {
        let row = &mut grid[Line(y as i32)];
        let mut x = 0;
        for run in runs {
            let len = usize::from(run.len);
            if let Some(cell) = cells.get(run.cell as usize) {
                for column in x..x + len {
                    row[Column(column)] = cell.clone();
                }
            }
            x += len;
        }
    }
}

impl AlacrittyEngine {
    /// The implementation of [`crate::VtEngine::snapshot`].
    pub(crate) fn take_snapshot(&mut self, journal_offset: u64) -> Result<Snapshot, SnapshotError> {
        self.snapshot_impl(journal_offset, false)
            .map(|(snapshot, _)| snapshot)
    }

    /// Takes a snapshot, and with `scrollback` also copies the rows of the scrollback in memory.
    fn snapshot_impl(
        &mut self,
        journal_offset: u64,
        scrollback: bool,
    ) -> Result<(Snapshot, Vec<Row<Cell>>), SnapshotError> {
        if self.sync_deadline().is_some() {
            return Err(SnapshotError::SyncOpen);
        }
        // The swaps below mark every row damaged. Keep the real damage and drop the swaps'.
        let real = self.drain_term_damage();
        match &mut self.carried {
            Some(carried) => carried.merge(real),
            none => *none = Some(real),
        }

        let alt = self.term.mode().contains(TermMode::ALT_SCREEN);
        let kitty = *self.term.mode() & TermMode::KITTY_KEYBOARD_PROTOCOL;
        let mut table = CellTable::new();
        let (primary, alternate);
        let mut history_rows = Vec::new();
        if alt {
            let shown = read_screen(self.term.grid(), &mut table);
            // The exact rows and cursors, to put back after the swaps.
            let rows: Vec<Row<Cell>> = (0..self.term.screen_lines())
                .map(|y| self.term.grid()[Line(y as i32)].clone())
                .collect();
            let cursor = self.term.grid().cursor.clone();
            let saved = self.term.grid().saved_cursor.clone();
            // Leave the alternate screen to read the primary one, and enter it again. Entering
            // resets its grid and gives it the cursor of the primary screen.
            self.term.swap_alt();
            let mut screen = read_screen(self.term.grid(), &mut table);
            if scrollback {
                history_rows = read_scrollback(self.term.grid());
            }
            self.term.grid_mut().truncate();
            // The saved cursor that the terminal holds may be one that an earlier snapshot of
            // this engine already overwrote. The mirror kept the real one.
            let held = self.term.grid().cursor.clone();
            let read_saved = self.term.grid().saved_cursor.clone();
            let true_saved = self.mirror.primary_saved.take().unwrap_or(read_saved);
            screen.saved = Some(snap_cursor(&true_saved, &mut table));
            primary = screen;
            // Entering the screen again sets the saved cursor of the primary screen to its
            // cursor. Keep the value that goes.
            self.mirror.primary_saved = (true_saved != held).then_some(true_saved);
            self.term.swap_alt();
            let grid = self.term.grid_mut();
            for (y, row) in rows.into_iter().enumerate() {
                grid[Line(y as i32)] = row;
            }
            grid.cursor = cursor;
            grid.saved_cursor = saved;
            grid.truncate();
            alternate = shown;
        } else {
            primary = read_screen(self.term.grid(), &mut table);
            if scrollback {
                history_rows = read_scrollback(self.term.grid());
            }
            self.term.grid_mut().truncate();
            let saved = self.term.grid().saved_cursor.clone();
            // The saved cursor of the alternate screen lives until the screen opens again. Open
            // it to read the cursor, and close it. That sets the saved cursor of the primary
            // screen to its cursor, so put it back.
            self.term.swap_alt();
            let alt_saved = snap_cursor(&self.term.grid().saved_cursor, &mut table);
            self.term.grid_mut().truncate();
            self.term.swap_alt();
            self.term.grid_mut().saved_cursor = saved;
            alternate = Screen {
                rows: Vec::new(),
                cursor: None,
                saved: Some(alt_saved),
            };
        }
        // `swap_alt` loads the flags from the keyboard stack. They can differ from its top.
        Handler::set_keyboard_mode(
            &mut self.term,
            kitty_flags(kitty),
            KeyboardModesApplyBehavior::Replace,
        );
        self.term.reset_damage();

        let stack = |s: &[KeyboardModes]| s.iter().map(|m| m.bits()).collect::<Vec<_>>();
        let colors = (0..COLOR_SLOTS)
            .filter_map(|slot| {
                self.term.colors()[slot].map(|c| {
                    (
                        slot as u16,
                        u32::from(c.r) << 16 | u32::from(c.g) << 8 | u32::from(c.b),
                    )
                })
            })
            .collect();
        let style: CursorStyle = self.term.cursor_style();
        let mirror = &self.mirror;
        let snapshot = Snapshot {
            version: SNAPSHOT_VERSION,
            journal_offset,
            cols: self.term.columns() as u16,
            rows: self.term.screen_lines() as u16,
            cells: table.into_cells(),
            primary,
            alternate,
            alt_active: alt,
            term_mode: self.term.mode().bits(),
            kitty: stack(&mirror.kitty),
            kitty_inactive: stack(&mirror.kitty_inactive),
            mouse_x10: mirror.mouse_x10,
            mouse_urxvt: mirror.mouse_urxvt,
            modify_other_keys: mirror.modify_other_keys as u8,
            tabs: mirror
                .tabs
                .iter()
                .enumerate()
                .filter(|(_, stop)| **stop)
                .map(|(column, _)| column as u16)
                .collect(),
            region: (
                mirror.region.0.clamp(0, i32::from(u16::MAX)) as u16,
                mirror.region.1.clamp(0, i32::from(u16::MAX)) as u16,
            ),
            active_charset: mirror.active_charset as u8,
            title: mirror.title.clone(),
            title_stack: mirror.title_stack.clone(),
            cursor_shape: shape_number(style.shape),
            cursor_blinking: style.blinking,
            colors,
            last_char: mirror.last_char,
            history: HistoryState {
                scrolled: self.history.scrolled(),
                gaps: self.history.gaps(),
                pending: self.history.pending().to_vec(),
                stale: self.history.stale(),
            },
            bells: self.bells,
        };
        Ok((snapshot, history_rows))
    }

    /// The implementation of [`crate::VtEngine::restore`].
    pub(crate) fn from_snapshot(
        snapshot: &Snapshot,
        config: &EngineConfig,
    ) -> Result<Self, RestoreError> {
        Self::from_snapshot_with(snapshot, config, &[])
    }

    /// Restores a snapshot and fills the scrollback in memory with `scrollback`, the oldest row
    /// first. The scrollback decides what a resize does with the rows above the screen.
    fn from_snapshot_with(
        snapshot: &Snapshot,
        config: &EngineConfig,
        scrollback: &[Row<Cell>],
    ) -> Result<Self, RestoreError> {
        snapshot.validate()?;
        let cells = snapshot
            .cells
            .iter()
            .map(build_cell)
            .collect::<Result<Vec<_>, _>>()?;
        let mut engine = Self::new(
            snapshot.cols,
            snapshot.rows,
            config.history,
            config.theme.clone(),
        );
        engine.cell_px = config.cell_px;
        engine.apply(snapshot, &cells, scrollback);
        engine.verify(snapshot)?;
        Ok(engine)
    }

    /// Builds the state of `s` in a new engine.
    fn apply(&mut self, s: &Snapshot, cells: &[Cell], scrollback: &[Row<Cell>]) {
        // The tee counts lines that leave the screen into a tracker. Here nothing is counted, and
        // the grid is not touched: the history is built below, and the tracker of the snapshot is
        // set at the end.
        let mut unused = HistoryTracker::inert();
        let mut tee = Tee {
            term: &mut self.term,
            mirror: &mut self.mirror,
            history: &mut unused,
        };

        // The scrollback goes in first, while the screen is empty. Each row is written at the top
        // of the screen, and the screen scrolls up to put it into the history.
        let grid = tee.term.grid_mut();
        let (rows, cols) = (grid.screen_lines() as i32, grid.columns());
        for row in scrollback.iter().filter(|r| r.len() == cols) {
            grid[Line(0)] = row.clone();
            grid.scroll_up::<Color>(&(Line(0)..Line(rows)), 1);
        }
        let held = grid.history_size();

        // The last printed character goes through the processor, because only the processor
        // keeps it. It lands in a cell that the screen below overwrites.
        if let Some(c) = s.last_char {
            let mut buf = [0u8; 4];
            self.processor
                .advance(&mut tee, c.encode_utf8(&mut buf).as_bytes());
        }

        for &(slot, rgb) in &s.colors {
            let color = Rgb {
                r: (rgb >> 16) as u8,
                g: (rgb >> 8) as u8,
                b: rgb as u8,
            };
            Handler::set_color(&mut tee, usize::from(slot), color);
        }
        let style = CursorStyle {
            shape: shape_of(s.cursor_shape),
            blinking: s.cursor_blinking,
        };
        Handler::set_cursor_style(&mut tee, Some(style));

        if s.tabs != stops(&default_tabs(usize::from(s.cols))) {
            Handler::clear_tabs(&mut tee, TabulationClearMode::All);
            for &column in &s.tabs {
                Handler::goto_col(&mut tee, usize::from(column));
                Handler::set_horizontal_tabstop(&mut tee);
            }
        }

        let want = TermMode::from_bits_retain(s.term_mode);
        let have = TermMode::default();
        for (flag, way) in MODES {
            let on = want.contains(flag);
            if on == have.contains(flag) {
                continue;
            }
            match (way, on) {
                (Way::Private(mode), true) => {
                    Handler::set_private_mode(&mut tee, PrivateMode::Named(mode));
                }
                (Way::Private(mode), false) => {
                    Handler::unset_private_mode(&mut tee, PrivateMode::Named(mode));
                }
                (Way::Ansi(mode), true) => Handler::set_mode(&mut tee, Mode::Named(mode)),
                (Way::Ansi(mode), false) => Handler::unset_mode(&mut tee, Mode::Named(mode)),
                (Way::Keypad, true) => Handler::set_keypad_application_mode(&mut tee),
                (Way::Keypad, false) => Handler::unset_keypad_application_mode(&mut tee),
            }
        }
        if s.mouse_x10 {
            Handler::set_private_mode(&mut tee, PrivateMode::Unknown(9));
        }
        if s.mouse_urxvt {
            Handler::set_private_mode(&mut tee, PrivateMode::Unknown(1015));
        }
        Handler::set_modify_other_keys(&mut tee, modify_other_keys_of(s.modify_other_keys));

        // The region. `CSI top ; bottom r` clamps both numbers to the height, so a region that
        // starts at the last row has an `end` that no `bottom` above it reaches directly.
        if s.region != (0, s.rows) {
            let top = usize::from(s.region.0) + 1;
            let bottom = if s.region.1 > s.region.0 {
                usize::from(s.region.1)
            } else {
                top + 1
            };
            Handler::set_scrolling_region(&mut tee, top, Some(bottom));
        }
        Handler::set_active_charset(&mut tee, charset_slot(s.active_charset));

        // The screens. The primary screen goes first, with its keyboard stack, because opening the
        // alternate screen swaps the stacks.
        let (primary_stack, alt_stack) = if s.alt_active {
            (&s.kitty_inactive, &s.kitty)
        } else {
            (&s.kitty, &s.kitty_inactive)
        };
        let cursors = |screen: &Screen| (screen.cursor.clone(), screen.saved.clone());
        let (primary_cursor, primary_saved) = cursors(&s.primary);
        let (alt_cursor, alt_saved) = cursors(&s.alternate);
        let grid = tee.term.grid_mut();
        write_rows(grid, &s.primary.rows, cells);
        if let Some(c) = &primary_cursor {
            grid.cursor = cursor_of(c, cells);
        }
        if let Some(c) = &primary_saved {
            grid.saved_cursor = cursor_of(c, cells);
        }
        for &bits in primary_stack {
            Handler::push_keyboard_mode(&mut tee, KeyboardModes::from_bits_truncate(bits));
        }
        Handler::set_private_mode(
            &mut tee,
            PrivateMode::Named(NamedPrivateMode::SwapScreenAndSetRestoreCursor),
        );
        let grid = tee.term.grid_mut();
        if s.alt_active {
            write_rows(grid, &s.alternate.rows, cells);
            if let Some(c) = &alt_cursor {
                grid.cursor = cursor_of(c, cells);
            }
            // Opening the alternate screen set the saved cursor of the primary screen to its
            // cursor, and it cannot be written now. If the snapshot has another one, the mirror
            // writes it when the screen goes off.
            if let (Some(c), Some(saved)) = (&primary_cursor, &primary_saved)
                && c != saved
            {
                tee.mirror.primary_saved = Some(cursor_of(saved, cells));
            }
        }
        if let Some(c) = &alt_saved {
            grid.saved_cursor = cursor_of(c, cells);
        }
        for &bits in alt_stack {
            Handler::push_keyboard_mode(&mut tee, KeyboardModes::from_bits_truncate(bits));
        }
        if !s.alt_active {
            Handler::unset_private_mode(
                &mut tee,
                PrivateMode::Named(NamedPrivateMode::SwapScreenAndSetRestoreCursor),
            );
            // Opening the alternate screen set this cursor to the cursor of the primary screen.
            if let Some(c) = &primary_saved {
                tee.term.grid_mut().saved_cursor = cursor_of(c, cells);
            }
        }
        Handler::set_keyboard_mode(
            &mut tee,
            kitty_flags(want),
            KeyboardModesApplyBehavior::Replace,
        );

        // The titles go last. A keyboard stack that is deeper than the terminal allows (4096
        // entries) makes each push remove the oldest title that is saved, and the pushes above
        // must not remove the titles of the snapshot. Each push saves the title that is set at
        // that moment.
        for entry in &s.title_stack {
            Handler::set_title(&mut tee, entry.clone());
            Handler::push_title(&mut tee);
        }
        Handler::set_title(&mut tee, s.title.clone());

        self.history.restore(
            s.history.scrolled,
            s.history.gaps,
            s.history.pending.clone(),
            held,
            s.history.stale,
        );
        self.bells = s.bells;
        // The terminal sent events while the state was set. They describe the restore, not the
        // output of the child.
        self.events
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
        self.last_cursor_row = self.cursor().y;
    }

    /// Checks that the engine holds what the snapshot says. A difference is a bug in the restore,
    /// and the caller gets an error instead of an engine in a wrong state.
    fn verify(&self, s: &Snapshot) -> Result<(), RestoreError> {
        let mirror = &self.mirror;
        let checks = [
            (self.term.mode().bits() == s.term_mode, "the modes differ"),
            (stops(&mirror.tabs) == s.tabs, "the tab stops differ"),
            (
                (i32::from(s.region.0), i32::from(s.region.1)) == mirror.region,
                "the scroll region differs",
            ),
            (
                mirror.active_charset as u8 == s.active_charset,
                "the active charset differs",
            ),
            (mirror.title == s.title, "the title differs"),
            (
                mirror.title_stack == s.title_stack,
                "the title stack differs",
            ),
            (
                mirror
                    .kitty
                    .iter()
                    .map(|m| m.bits())
                    .eq(s.kitty.iter().copied())
                    && mirror
                        .kitty_inactive
                        .iter()
                        .map(|m| m.bits())
                        .eq(s.kitty_inactive.iter().copied()),
                "a keyboard stack differs",
            ),
            (
                mirror.mouse_x10 == s.mouse_x10
                    && mirror.mouse_urxvt == s.mouse_urxvt
                    && mirror.modify_other_keys as u8 == s.modify_other_keys,
                "an extra mode differs",
            ),
            (
                mirror.last_char == s.last_char,
                "the last character differs",
            ),
        ];
        match checks.iter().find(|(ok, _)| !ok) {
            Some((_, what)) => Err(RestoreError::Invalid(what)),
            None => Ok(()),
        }
    }
}

/// The columns that have a tab stop.
fn stops(tabs: &[bool]) -> Vec<u16> {
    tabs.iter()
        .enumerate()
        .filter(|(_, stop)| **stop)
        .map(|(column, _)| column as u16)
        .collect()
}
