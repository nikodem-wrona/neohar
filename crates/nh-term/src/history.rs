//! Counting the lines that leave the screen.
//!
//! `alacritty_terminal` has no event for a line that enters or leaves its history, and its
//! history length stops changing when it is full. So the engine keeps the history below a
//! high-water mark. After each chunk, the growth of the history is the number of new lines, and
//! they are the newest rows. When the history passes the mark (half the capacity, and at most
//! `HIGH_WATER`), the oldest rows go until half of the mark is left. They are in the archive by
//! then.
//!
//! A chunk can still add more lines than the room that is left, for example with many
//! `CSI 24 S`. Then the history is full, and the count is too low. That is visible, so the
//! tracker counts such events as gaps and does not hide them.

use alacritty_terminal::{
    event::EventListener,
    grid::Dimensions,
    index::{Column, Line},
    term::{
        Term, TermMode,
        cell::{Cell, Flags},
    },
};
use nh_proto::style::Style;

use crate::{
    alacritty::style_of,
    engine::{HistoryLine, StyleRun},
};

/// The engine feeds at most this many bytes between two counts. Plain output adds at most one
/// line per byte, so one chunk adds fewer lines than the room that is left below the capacity.
pub const CHUNK: usize = 1024;

/// The most lines that the history holds after a count, whatever the capacity.
///
/// `alacritty_terminal` grows the rows of its grid in blocks of 1,000 (`Storage::initialize`, the
/// constant `MAX_CACHE_SIZE`). The first time that a pane scrolls, it allocates 1,000 rows at once,
/// and it allocates 1,000 more when the history needs the 1,001st. A row is `columns` cells of 24
/// bytes, so a block of a 200-column pane is 4.8 MB. A history that stays below 1,000 lines minus
/// the screen never needs a second block, and the memory of a pane is about half of what it was
/// with the old limit of 1,024 lines.
pub const HIGH_WATER: usize = 900;

// The rows of the history and the rows of a chunk (at most 100 for plain output) must fit in the
// block of 1,000 rows, or the memory of every pane grows by a block.
const _: () = assert!(HIGH_WATER + 100 <= 1_000);

/// The smallest history capacity. The history holds at most half of the capacity after a count,
/// and a chunk of `CHUNK` bytes of plain output adds at most `CHUNK` lines to it.
pub const MIN_CAPACITY: usize = 2048;

/// Tracks the history of the primary screen.
pub struct HistoryTracker {
    capacity: usize,
    /// The history length at the last count.
    baseline: usize,
    /// How many lines ever went into the history, in all.
    scrolled: u64,
    /// The lines that are not yet taken.
    pending: Vec<HistoryLine>,
    /// How often a chunk filled the history, so that lines may be missing.
    gaps: u64,
    /// A resize changed the history of the primary screen while the alternate screen was on show,
    /// and the old length means nothing. The next count after the screen goes off starts again.
    stale: bool,
    /// An inert tracker counts nothing and never touches the grid.
    inert: bool,
}

impl HistoryTracker {
    pub fn new(capacity: usize) -> Self {
        assert!(
            capacity >= MIN_CAPACITY,
            "the history capacity is at least {MIN_CAPACITY} lines"
        );
        Self {
            capacity,
            baseline: 0,
            scrolled: 0,
            pending: Vec::new(),
            gaps: 0,
            stale: false,
            inert: false,
        }
    }

    /// A tracker that does nothing, for code that must drive the terminal without counting: a
    /// restore builds the history itself.
    pub fn inert() -> Self {
        Self {
            inert: true,
            ..Self::new(MIN_CAPACITY)
        }
    }

    /// The capacity of the scrollback in memory, in lines.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// The number of lines that ever left the screen. The line that is on top of the live screen
    /// has this absolute number.
    pub fn scrolled(&self) -> u64 {
        self.scrolled
    }

    /// Gives the next line that leaves the screen the number `first_line` instead of 0. Call it
    /// before the terminal reads a byte.
    pub fn start_at(&mut self, first_line: u64) {
        debug_assert!(
            self.scrolled == 0 && self.pending.is_empty(),
            "the history has lines already"
        );
        self.scrolled = first_line;
    }

    /// How often a chunk filled the history. Each time, some lines may be missing from the lines
    /// that the tracker reports. The newest ones are there.
    pub fn gaps(&self) -> u64 {
        self.gaps
    }

    /// Takes the lines that went into the history since the last call.
    pub fn take(&mut self, out: &mut Vec<HistoryLine>) {
        out.append(&mut self.pending);
    }

    /// The lines that are not yet taken.
    pub fn pending(&self) -> &[HistoryLine] {
        &self.pending
    }

    /// Whether a resize in the alternate screen made the length unknown.
    pub fn stale(&self) -> bool {
        self.stale
    }

    /// Sets the counters of a snapshot. `baseline` is the length of the scrollback in memory.
    pub fn restore(
        &mut self,
        scrolled: u64,
        gaps: u64,
        pending: Vec<HistoryLine>,
        baseline: usize,
        stale: bool,
    ) {
        self.scrolled = scrolled;
        self.gaps = gaps;
        self.pending = pending;
        self.baseline = baseline;
        self.stale = stale;
    }

    /// Forgets the old length. Call it after a change of size: lines move between the screen and
    /// the history, and the old count means nothing.
    ///
    /// While the alternate screen is on show, the grid that the terminal gives is the alternate
    /// one, and the history of the primary screen is out of reach. The length is then read when
    /// the screen goes off.
    pub fn rebaseline<T: EventListener>(&mut self, term: &Term<T>) {
        if term.mode().contains(TermMode::ALT_SCREEN) {
            self.stale = true;
        } else {
            self.baseline = term.grid().history_size();
        }
    }

    /// Counts the new lines after the engine read a chunk, and keeps the history small.
    pub fn account<T: EventListener>(&mut self, term: &mut Term<T>) {
        // The alternate screen has no history, and the primary screen does not scroll behind it.
        if self.inert || term.mode().contains(TermMode::ALT_SCREEN) {
            return;
        }
        let len = term.grid().history_size();
        if len >= self.capacity {
            self.gaps += 1;
        }
        // A clear leaves only the lines that came after it. After a resize in the alternate
        // screen nothing can be told, so no line counts as new.
        let new = if std::mem::take(&mut self.stale) {
            0
        } else if len >= self.baseline {
            len - self.baseline
        } else {
            len
        };
        for k in (1..=new).rev() {
            let row = &term.grid()[Line(-(k as i32))];
            let cells = (0..row.len()).map(|x| &row[Column(x)]);
            let line_no = self.scrolled + (new - k) as u64;
            self.pending.push(history_line(line_no, cells));
        }
        self.scrolled += new as u64;
        self.baseline = len;
        let high = (self.capacity / 2).min(HIGH_WATER);
        if len > high {
            // `update_history` also sets the limit, so the second call sets it back.
            term.grid_mut().update_history(high / 2);
            term.grid_mut().update_history(self.capacity);
            self.baseline = term.grid().history_size();
        }
    }
}

/// One row as a line of the archive.
fn history_line<'a>(line_no: u64, cells: impl Iterator<Item = &'a Cell>) -> HistoryLine {
    let mut text = String::new();
    let mut runs: Vec<StyleRun> = Vec::new();
    // The text length at the end of the last cell that is not blank.
    let mut keep = 0;
    let mut wrapped = false;
    for (x, cell) in cells.enumerate() {
        wrapped = cell.flags.contains(Flags::WRAPLINE);
        let spacer = cell
            .flags
            .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER);
        if !spacer {
            text.push(cell.c);
            for &z in cell.zerowidth().unwrap_or(&[]) {
                text.push(z);
            }
        }
        let style = style_of(cell);
        let blank =
            !spacer && cell.c == ' ' && style == Style::default() && cell.zerowidth().is_none();
        if !blank {
            keep = text.len();
        }
        let start = u16::try_from(x).unwrap_or(u16::MAX);
        match runs.last_mut() {
            Some(run) if run.style == style && run.start + run.len == start => run.len += 1,
            _ if style == Style::default() => {}
            _ => runs.push(StyleRun {
                start,
                len: 1,
                style,
            }),
        }
    }
    text.truncate(keep);
    HistoryLine {
        line_no,
        text,
        runs,
        wrapped,
    }
}
