//! The VT engine on `alacritty_terminal`.

use std::{
    borrow::Cow,
    sync::{Arc, Mutex, PoisonError},
    time::Instant,
};

use alacritty_terminal::{
    event::{Event, EventListener, WindowSize},
    grid::{Dimensions, Scroll},
    index::{Column, Line},
    term::{
        ClipboardType, Config, Osc52, Term, TermDamage, TermMode,
        cell::{Cell, Flags},
    },
    vte::ansi::{Color, CursorShape, Processor, Rgb},
};
use nh_proto::{
    answers,
    frame::{Cursor, CursorShape as WireShape, Palette, Selection, Viewport},
    modes::ModeFlags,
    style::{Attrs, CellWidth, Color as WireColor, Style, UnderlineKind},
};

use crate::{
    effects::EffectSink,
    engine::{CellView, Damage, EngineConfig, HistoryLine, VtEngine},
    history::{CHUNK, HistoryTracker},
    intercept::{Interceptor, Item, What},
    mirror::{Mirror, Tee},
    selection::{Lines, normalize, text},
    snapshot::{RestoreError, Snapshot, SnapshotError},
};

/// Collects the events that the terminal sends while it reads bytes.
#[derive(Clone, Default)]
pub(crate) struct Listener(Arc<Mutex<Vec<Event>>>);

impl EventListener for Listener {
    fn send_event(&self, event: Event) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(event);
    }
}

/// The size of the terminal, for `alacritty_terminal`.
pub(crate) struct Size {
    pub cols: usize,
    pub rows: usize,
}

impl Dimensions for Size {
    fn total_lines(&self) -> usize {
        self.rows
    }
    fn screen_lines(&self) -> usize {
        self.rows
    }
    fn columns(&self) -> usize {
        self.cols
    }
}

/// Damage that the engine took from the terminal and the pane did not take yet.
#[derive(Default)]
pub(crate) struct RawDamage {
    pub full: bool,
    pub rows: Vec<u16>,
    pub cells: usize,
}

impl RawDamage {
    /// Adds `other` to this damage.
    pub(crate) fn merge(&mut self, other: Self) {
        self.full |= other.full;
        self.cells += other.cells;
        self.rows.extend(other.rows);
        self.rows.sort_unstable();
        self.rows.dedup();
    }
}

/// The screen model of one pane, on `alacritty_terminal`.
pub struct AlacrittyEngine {
    pub(crate) term: Term<Listener>,
    pub(crate) processor: Processor,
    pub(crate) events: Arc<Mutex<Vec<Event>>>,
    interceptor: Interceptor,
    found: Vec<Item>,
    pub(crate) history: HistoryTracker,
    pub(crate) theme: Palette,
    pub(crate) bells: u64,
    pub(crate) cell_px: (u16, u16),
    pub(crate) last_cursor_row: u16,
    /// What the terminal does not show (`crate::mirror`).
    pub(crate) mirror: Mirror,
    /// Damage that a snapshot read from the terminal and kept for the next `take_damage`.
    pub(crate) carried: Option<RawDamage>,
    /// What the user selected, as the two points they gave (`set_selection`).
    selection: Option<Selection>,
}

impl AlacrittyEngine {
    /// An engine of the given size. `history` is the capacity of the scrollback in memory, in
    /// lines (at least [`crate::MIN_HISTORY`]). `theme` holds the colours before a program changes
    /// them.
    pub fn new(cols: u16, rows: u16, history: usize, theme: Palette) -> Self {
        let listener = Listener::default();
        let events = Arc::clone(&listener.0);
        let config = Config {
            scrolling_history: history,
            kitty_keyboard: true,
            osc52: Osc52::CopyPaste,
            ..Config::default()
        };
        let size = Size {
            cols: usize::from(cols.max(2)),
            rows: usize::from(rows.max(1)),
        };
        let mirror = Mirror::new(size.cols, size.rows);
        Self {
            term: Term::new(config, &size, listener),
            processor: Processor::new(),
            events,
            interceptor: Interceptor::default(),
            found: Vec::new(),
            history: HistoryTracker::new(history),
            theme,
            bells: 0,
            cell_px: (0, 0),
            last_cursor_row: 0,
            mirror,
            carried: None,
            selection: None,
        }
    }

    /// Feeds bytes to the terminal in chunks, and counts the new history lines after each one.
    fn feed(&mut self, bytes: &[u8], fx: &mut dyn EffectSink) {
        for chunk in bytes.chunks(CHUNK) {
            let title = self.mirror.title.clone();
            let mut tee = Tee {
                term: &mut self.term,
                mirror: &mut self.mirror,
                history: &mut self.history,
            };
            self.processor.advance(&mut tee, chunk);
            let announced = self.drain(fx);
            // A full reset clears the title and sends no event for it.
            if !announced && self.mirror.title != title {
                fx.title_changed();
            }
            self.history.account(&mut self.term);
        }
    }

    /// Takes the damage from the terminal and resets it there.
    pub(crate) fn drain_term_damage(&mut self) -> RawDamage {
        let (full, rows, cells) = match self.term.damage() {
            TermDamage::Full => (true, Vec::new(), 0),
            TermDamage::Partial(lines) => {
                let mut rows = Vec::new();
                let mut cells = 0;
                for line in lines {
                    rows.push(line.line as u16);
                    // A change in the first two columns can clear the spacer that a wrapped wide
                    // character left at the end of the row above, and `alacritty_terminal` does
                    // not report that row (found by the exactness test).
                    if line.left <= 1 && line.line > 0 {
                        rows.push(line.line as u16 - 1);
                    }
                    cells += line.right + 1 - line.left;
                }
                rows.sort_unstable();
                rows.dedup();
                (false, rows, cells)
            }
        };
        self.term.reset_damage();
        RawDamage { full, rows, cells }
    }

    /// Passes the events of the terminal to the sink. Returns whether a title event came.
    fn drain(&mut self, fx: &mut dyn EffectSink) -> bool {
        let events =
            std::mem::take(&mut *self.events.lock().unwrap_or_else(PoisonError::into_inner));
        let mut announced = false;
        for event in events {
            match event {
                Event::PtyWrite(text) => fx.pty_write(&reply(&text)),
                Event::Title(_) | Event::ResetTitle => {
                    announced = true;
                    fx.title_changed();
                }
                Event::Bell => {
                    self.bells += 1;
                    fx.bell();
                }
                Event::ClipboardStore(kind, text) => {
                    let clipboard = if matches!(kind, ClipboardType::Selection) {
                        b'p'
                    } else {
                        b'c'
                    };
                    fx.clipboard_store(clipboard, &text);
                }
                Event::ClipboardLoad(..) => fx.clipboard_load_requested(),
                Event::ColorRequest(index, format) => {
                    let color = self.color(index);
                    let rgb = Rgb {
                        r: (color >> 16) as u8,
                        g: (color >> 8) as u8,
                        b: color as u8,
                    };
                    fx.pty_write(format(rgb).as_bytes());
                }
                Event::TextAreaSizeRequest(format) => {
                    let (cell_width, cell_height) = self.cell_px;
                    let size = WindowSize {
                        num_lines: self.term.screen_lines() as u16,
                        num_cols: self.term.columns() as u16,
                        cell_width,
                        cell_height,
                    };
                    fx.pty_write(format(size).as_bytes());
                }
                Event::MouseCursorDirty
                | Event::CursorBlinkingChange
                | Event::Wakeup
                | Event::Exit
                | Event::ChildExit(_) => {}
            }
        }
        announced
    }

    /// The colour in a slot: 0 to 255 for the palette, then the foreground, the background and
    /// the cursor.
    fn color(&self, index: usize) -> u32 {
        let theme = match index {
            256 => self.theme.fg,
            257 => self.theme.bg,
            258 => self.theme.cursor,
            i => self.theme.colors.get(i).copied().unwrap_or(0),
        };
        self.term.colors()[index].map_or(theme, |c| {
            u32::from(c.r) << 16 | u32::from(c.g) << 8 | u32::from(c.b)
        })
    }
}

/// The reply of the engine to a query, or the shared bytes for the same query.
fn reply(text: &str) -> Cow<'_, [u8]> {
    if text == "\x1b[?6c" {
        Cow::Borrowed(answers::DA1)
    } else if text.starts_with("\x1b[>0;") && text.ends_with(";1c") {
        Cow::Borrowed(answers::DA2)
    } else {
        Cow::Borrowed(text.as_bytes())
    }
}

fn wire_color(color: Color) -> WireColor {
    match color {
        Color::Spec(Rgb { r, g, b }) => WireColor::Rgb(r, g, b),
        Color::Indexed(i) => WireColor::Palette(i),
        Color::Named(name) => match name as usize {
            n @ 0..=15 => WireColor::Palette(n as u8),
            // The dim variants of the eight base colours.
            n @ 259..=266 => WireColor::Palette((n - 259) as u8),
            _ => WireColor::Default,
        },
    }
}

/// The style of a cell, in the terms of the wire format.
pub(crate) fn style_of(cell: &Cell) -> Style {
    let flags = cell.flags;
    let mut attrs = Attrs::empty();
    attrs.set(Attrs::BOLD, flags.contains(Flags::BOLD));
    attrs.set(Attrs::DIM, flags.contains(Flags::DIM));
    attrs.set(Attrs::ITALIC, flags.contains(Flags::ITALIC));
    attrs.set(Attrs::INVERSE, flags.contains(Flags::INVERSE));
    attrs.set(Attrs::STRIKETHROUGH, flags.contains(Flags::STRIKEOUT));
    attrs.set(Attrs::HIDDEN, flags.contains(Flags::HIDDEN));
    let underline = if flags.contains(Flags::UNDERCURL) {
        UnderlineKind::Curly
    } else if flags.contains(Flags::DOUBLE_UNDERLINE) {
        UnderlineKind::Double
    } else if flags.contains(Flags::DOTTED_UNDERLINE) {
        UnderlineKind::Dotted
    } else if flags.contains(Flags::DASHED_UNDERLINE) {
        UnderlineKind::Dashed
    } else if flags.contains(Flags::UNDERLINE) {
        UnderlineKind::Single
    } else {
        UnderlineKind::None
    };
    let width = if flags.contains(Flags::WIDE_CHAR) {
        CellWidth::Wide
    } else if flags.intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER) {
        CellWidth::Spacer
    } else {
        CellWidth::Single
    };
    Style {
        fg: wire_color(cell.fg),
        bg: wire_color(cell.bg),
        underline_color: cell
            .underline_color()
            .map_or(WireColor::Default, wire_color),
        attrs,
        underline,
        width,
    }
}

impl VtEngine for AlacrittyEngine {
    fn advance(&mut self, bytes: &[u8], fx: &mut dyn EffectSink) {
        let alternate = self.term.mode().contains(TermMode::ALT_SCREEN);
        let mut found = std::mem::take(&mut self.found);
        found.clear();
        self.interceptor.scan(bytes, &mut found);
        let mut pos = 0;
        for item in found.drain(..) {
            match item.what {
                What::Query(kind) => {
                    self.feed(&bytes[pos..item.end], fx);
                    pos = item.end;
                    fx.pty_write(match kind {
                        nh_proto::answers::QueryKind::Da3 => answers::DA3,
                        _ => answers::XTVERSION,
                    });
                }
                What::Notify { title, body } => {
                    self.feed(&bytes[pos..item.end], fx);
                    pos = item.end;
                    fx.notify(&title, &body);
                }
            }
        }
        self.feed(&bytes[pos..], fx);
        self.found = found;
        // A switch between the screens makes the points of a selection mean other lines.
        if self.term.mode().contains(TermMode::ALT_SCREEN) != alternate {
            self.selection = None;
        }
    }

    fn resize(&mut self, cols: u16, rows: u16) {
        // The lines move when the rows reflow, and the points mean something else.
        self.selection = None;
        let size = Size {
            cols: usize::from(cols.max(2)),
            rows: usize::from(rows.max(1)),
        };
        let from = (self.term.columns(), self.term.screen_lines());
        self.term.resize(size);
        self.mirror
            .resize(from, (self.term.columns(), self.term.screen_lines()));
        self.history.rebaseline(&self.term);
    }

    fn size(&self) -> (u16, u16) {
        (self.term.columns() as u16, self.term.screen_lines() as u16)
    }

    fn modes(&self) -> ModeFlags {
        let mode = *self.term.mode();
        let map = [
            (TermMode::ALT_SCREEN, ModeFlags::ALT_SCREEN),
            (TermMode::APP_CURSOR, ModeFlags::APP_CURSOR),
            (TermMode::APP_KEYPAD, ModeFlags::APP_KEYPAD),
            (TermMode::MOUSE_REPORT_CLICK, ModeFlags::MOUSE_1000),
            (TermMode::MOUSE_DRAG, ModeFlags::MOUSE_1002),
            (TermMode::MOUSE_MOTION, ModeFlags::MOUSE_1003),
            (TermMode::SGR_MOUSE, ModeFlags::MOUSE_SGR),
            (TermMode::UTF8_MOUSE, ModeFlags::MOUSE_UTF8),
            (TermMode::ALTERNATE_SCROLL, ModeFlags::ALTERNATE_SCROLL),
            (TermMode::BRACKETED_PASTE, ModeFlags::BRACKETED_PASTE),
            (TermMode::FOCUS_IN_OUT, ModeFlags::FOCUS_EVENTS),
            (TermMode::ORIGIN, ModeFlags::ORIGIN),
            (TermMode::LINE_WRAP, ModeFlags::AUTOWRAP),
            (TermMode::INSERT, ModeFlags::INSERT),
            (TermMode::LINE_FEED_NEW_LINE, ModeFlags::LNM),
            (
                TermMode::DISAMBIGUATE_ESC_CODES,
                ModeFlags::KITTY_DISAMBIGUATE,
            ),
            (
                TermMode::REPORT_EVENT_TYPES,
                ModeFlags::KITTY_REPORT_EVENT_TYPES,
            ),
            (
                TermMode::REPORT_ALTERNATE_KEYS,
                ModeFlags::KITTY_REPORT_ALTERNATE_KEYS,
            ),
            (
                TermMode::REPORT_ALL_KEYS_AS_ESC,
                ModeFlags::KITTY_REPORT_ALL_KEYS,
            ),
            (
                TermMode::REPORT_ASSOCIATED_TEXT,
                ModeFlags::KITTY_REPORT_TEXT,
            ),
        ];
        let mut flags = map
            .iter()
            .filter(|(from, _)| mode.contains(*from))
            .fold(ModeFlags::empty(), |acc, (_, to)| acc | *to);
        // The terminal ignores these. The mirror reads them from the parser's events.
        flags.set(ModeFlags::MOUSE_X10, self.mirror.mouse_x10);
        flags.set(ModeFlags::MOUSE_URXVT, self.mirror.mouse_urxvt);
        flags.with_modify_other_keys(self.mirror.modify_other_keys as u8)
    }

    fn cursor(&self) -> Cursor {
        let (cols, rows) = self.size();
        let grid = self.term.grid();
        let point = grid.cursor.point;
        let style = self.term.cursor_style();
        let y = point.line.0 as usize + grid.display_offset();
        let on_screen = y < usize::from(rows);
        Cursor {
            x: (point.column.0 as u16).min(cols.saturating_sub(1)),
            y: (y.min(usize::from(rows) - 1)) as u16,
            shape: match style.shape {
                CursorShape::Block | CursorShape::Hidden => WireShape::Block,
                CursorShape::Underline => WireShape::Underline,
                CursorShape::Beam => WireShape::Bar,
                CursorShape::HollowBlock => WireShape::HollowBlock,
            },
            visible: on_screen
                && self.term.mode().contains(TermMode::SHOW_CURSOR)
                && style.shape != CursorShape::Hidden,
            blink: style.blinking,
        }
    }

    fn title(&self) -> Option<&str> {
        // An empty title is no title. A delta frame cannot tell them apart.
        self.mirror.title.as_deref().filter(|t| !t.is_empty())
    }

    fn palette(&self) -> Palette {
        let mut palette = self.theme.clone();
        for (i, color) in palette.colors.iter_mut().enumerate() {
            *color = self.color(i);
        }
        palette.fg = self.color(256);
        palette.bg = self.color(257);
        palette.cursor = self.color(258);
        palette
    }

    fn visit_row(&self, y: u16, f: &mut dyn FnMut(CellView<'_>)) {
        let grid = self.term.grid();
        let row = &grid[Line(i32::from(y) - grid.display_offset() as i32)];
        for x in 0..grid.columns() {
            let cell = &row[Column(x)];
            f(CellView {
                ch: cell.c,
                zerowidth: cell.zerowidth().unwrap_or(&[]),
                style: style_of(cell),
            });
        }
    }

    fn take_damage(&mut self) -> Damage {
        let mut damage = self.drain_term_damage();
        if let Some(carried) = self.carried.take() {
            damage.merge(carried);
        }
        let RawDamage { full, rows, cells } = damage;
        let cursor_row = self.cursor().y;
        let moved = cursor_row != self.last_cursor_row;
        self.last_cursor_row = cursor_row;
        let minor = !full && rows.len() <= 2 && cells <= 16 && !moved;
        Damage { full, rows, minor }
    }

    fn take_new_history(&mut self, out: &mut Vec<HistoryLine>) {
        self.history.take(out);
    }

    fn start_history_at(&mut self, first_line: u64) {
        self.history.start_at(first_line);
    }

    fn history_gaps(&self) -> u64 {
        self.history.gaps()
    }

    fn scroll_display(&mut self, top_line: Option<u64>) -> Viewport {
        let grid = self.term.grid();
        let (offset, held) = (grid.display_offset(), grid.history_size());
        let wanted = top_line.map_or(0, |top| self.history.scrolled().saturating_sub(top));
        let target = usize::try_from(wanted).map_or(held, |w| w.min(held));
        self.term
            .scroll_display(Scroll::Delta(target as i32 - offset as i32));
        self.viewport()
    }

    fn set_selection(&mut self, selection: Option<Selection>) {
        let alternate = self.term.mode().contains(TermMode::ALT_SCREEN);
        self.selection = selection.filter(|_| !alternate);
    }

    fn selection(&self) -> Option<Selection> {
        if self.term.mode().contains(TermMode::ALT_SCREEN) {
            return None;
        }
        let lines = Lines::new(self.term.grid(), self.history.scrolled());
        normalize(&lines, self.selection?)
    }

    fn selection_text(&self) -> Option<String> {
        let lines = Lines::new(self.term.grid(), self.history.scrolled());
        let selection = self.selection()?;
        Some(text(&lines, &selection))
    }

    fn viewport(&self) -> Viewport {
        let scrolled = self.history.scrolled();
        Viewport {
            top_line: scrolled - self.term.grid().display_offset() as u64,
            total_lines: scrolled + self.term.screen_lines() as u64,
        }
    }

    fn bell_count(&self) -> u64 {
        self.bells
    }

    fn set_cell_size(&mut self, width: u16, height: u16) {
        self.cell_px = (width, height);
    }

    fn sync_deadline(&self) -> Option<Instant> {
        self.processor.sync_timeout().sync_timeout()
    }

    fn flush_sync(&mut self, fx: &mut dyn EffectSink) {
        let title = self.mirror.title.clone();
        let mut tee = Tee {
            term: &mut self.term,
            mirror: &mut self.mirror,
            history: &mut self.history,
        };
        self.processor.stop_sync(&mut tee);
        if !self.drain(fx) && self.mirror.title != title {
            fx.title_changed();
        }
        self.history.account(&mut self.term);
    }

    fn snapshot(&mut self, journal_offset: u64) -> Result<Snapshot, SnapshotError> {
        self.take_snapshot(journal_offset)
    }

    fn restore(snapshot: &Snapshot, config: &EngineConfig) -> Result<Self, RestoreError> {
        Self::from_snapshot(snapshot, config)
    }
}
