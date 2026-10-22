//! The exactness check of the VT engine snapshot.
//!
//! A stream of output (a seed of the synthetic corpus, or a journal that a holder recorded) goes
//! through three kinds of engine:
//!
//! - **A** reads the whole stream, and takes a snapshot at the cut points that the check selects.
//! - **Q** reads the same stream and never takes a snapshot. It is the reference for what the
//!   output means.
//! - **B** is a copy that a snapshot of A restores at a cut point. It reads the next `window`
//!   bytes with A.
//!
//! After every step (a chunk of output, or a resize) each B must agree with A on the screen, the
//! cursor, the modes, the title, the palette and the viewport, and on the replies that the engine
//! writes to the child. From time to time they must also agree on the whole snapshot, which holds
//! the state that the screen does not show (tab stops, the scroll region, the saved cursors, the
//! keyboard stacks...). A must agree with Q: taking a snapshot must not change what the output
//! means. The check reports the first difference of each B with the bytes around it.

use std::{
    fmt::Write as _,
    fs::File,
    io::BufReader,
    path::Path,
    time::{Duration, Instant},
};

use nh_fixture::Op;
use nh_hold::CutScanner;
use nh_proto::{
    framing::{self, FramingError},
    holder::{HolderEvent, Marker},
};
use nh_term::{
    AlacrittyEngine, Effect, EngineConfig, HistoryLine, MIN_HISTORY, Recorder, Snapshot,
    SnapshotError, VtEngine, client_grid::dump_engine, xterm_palette,
};

/// One step of a stream.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    /// Output of the child.
    Bytes(Vec<u8>),
    /// The pane changes size.
    Resize { cols: u16, rows: u16 },
}

/// A stream of output with the places where the holder would put a cut point.
#[derive(Clone, Debug)]
pub struct Stream {
    pub name: String,
    /// The size of the pane at the start.
    pub initial: (u16, u16),
    pub steps: Vec<Step>,
    /// `cuts[i]` is true when a cut point follows step `i`.
    pub cuts: Vec<bool>,
    /// How many chunk ends of a generated stream were not at a cut point of the holder's scanner.
    pub unsafe_ends: usize,
}

impl Stream {
    /// A stream from generated operations. A cut point follows each chunk of output that ends
    /// where the holder's scanner says that a cut point can be, as the holder puts one at the end
    /// of each read. A resize is also a safe place.
    pub fn from_ops(name: impl Into<String>, ops: Vec<Op>) -> Self {
        let mut scanner = CutScanner::new();
        let mut steps = Vec::with_capacity(ops.len());
        let mut cuts = Vec::with_capacity(ops.len());
        let mut unsafe_ends = 0;
        for op in ops {
            match op {
                Op::Bytes(bytes) => {
                    scanner.scan(&bytes, &mut Vec::new());
                    let safe = scanner.at_cut_point();
                    unsafe_ends += usize::from(!safe);
                    cuts.push(safe);
                    steps.push(Step::Bytes(bytes));
                }
                Op::Resize { cols, rows } => {
                    cuts.push(scanner.at_cut_point());
                    steps.push(Step::Resize { cols, rows });
                }
            }
        }
        Self {
            name: name.into(),
            initial: (80, 24),
            steps,
            cuts,
            unsafe_ends,
        }
    }

    /// A stream of the synthetic corpus: `bytes` bytes of output made from `seed`.
    pub fn from_seed(seed: u64, bytes: usize) -> Self {
        Self::from_ops(format!("seed {seed}"), nh_fixture::vt::corpus(seed, bytes))
    }

    /// A stream from a journal that `nh-record` saved: framed holder events. The cut points are
    /// the `CutPoint` markers that the holder wrote, and the resizes are its `Resize` markers.
    pub fn from_journal(path: &Path) -> Result<Self, String> {
        let file = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut reader = BufReader::new(file);
        let mut bytes = Vec::new();
        let mut first = None;
        let mut markers: Vec<(u64, Marker)> = Vec::new();
        loop {
            match framing::read_msg::<_, HolderEvent>(&mut reader) {
                Ok(HolderEvent::Output {
                    offset, bytes: b, ..
                }) => {
                    first.get_or_insert(offset);
                    bytes.extend_from_slice(&b);
                }
                Ok(HolderEvent::Marker { offset, marker }) => markers.push((offset, marker)),
                Ok(_) => {}
                Err(FramingError::Eof) => break,
                Err(e) => return Err(format!("{}: {e}", path.display())),
            }
        }
        let base = first.unwrap_or(0);
        let mut initial = (80, 24);
        let (mut steps, mut cuts) = (Vec::new(), Vec::new());
        let mut at = 0usize;
        let push_bytes =
            |upto: usize, at: &mut usize, steps: &mut Vec<Step>, cuts: &mut Vec<bool>| {
                if upto > *at {
                    steps.push(Step::Bytes(bytes[*at..upto].to_vec()));
                    cuts.push(false);
                    *at = upto;
                }
            };
        for (offset, marker) in markers {
            let upto = usize::try_from(offset.saturating_sub(base))
                .unwrap_or(usize::MAX)
                .min(bytes.len());
            match marker {
                Marker::Resize { cols, rows } => {
                    if steps.is_empty() && upto == 0 {
                        initial = (cols, rows);
                    } else {
                        push_bytes(upto, &mut at, &mut steps, &mut cuts);
                        steps.push(Step::Resize { cols, rows });
                        cuts.push(true);
                    }
                }
                Marker::CutPoint => {
                    push_bytes(upto, &mut at, &mut steps, &mut cuts);
                    if let Some(last) = cuts.last_mut() {
                        *last = true;
                    }
                }
                Marker::InputAck { .. } | Marker::Stream { .. } => {}
            }
        }
        push_bytes(bytes.len(), &mut at, &mut steps, &mut cuts);
        Ok(Self {
            name: path.file_name().map_or_else(
                || path.display().to_string(),
                |n| n.to_string_lossy().into_owned(),
            ),
            initial,
            steps,
            cuts,
            unsafe_ends: 0,
        })
    }

    /// The bytes of all steps.
    pub fn total_bytes(&self) -> u64 {
        self.steps
            .iter()
            .map(|s| {
                if let Step::Bytes(b) = s {
                    b.len() as u64
                } else {
                    0
                }
            })
            .sum()
    }
}

/// How much to check.
#[derive(Clone, Debug)]
pub struct Options {
    /// Take every `every`th cut point. 1 takes all of them.
    pub every: usize,
    /// How many bytes of output each restored engine reads with A.
    pub window: usize,
    /// How many restored engines live at the same time. A cut point that comes when the limit is
    /// reached is skipped and counted.
    pub max_live: usize,
    /// Compare the whole snapshots every this many steps of a restored engine (and at its end).
    pub snapshot_every: usize,
    /// Stop after this many failures.
    pub max_failures: usize,
    /// Rebuild A (and every restored engine) from its snapshot at each checkpoint, as
    /// `AlacrittyEngine::checkpoint` does, so that the engine that continues is built the way a
    /// restored one is. Without it, A is the engine as `alacritty_terminal` left it.
    pub normalize: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            every: 10,
            window: 256 * 1024,
            max_live: 128,
            snapshot_every: 8,
            max_failures: 5,
            normalize: false,
        }
    }
}

/// Where in the life of a restored engine a difference showed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureKind {
    /// Right after the restore: the snapshot did not hold something, or the restore did not apply it.
    Instant,
    /// During the window, in what the engines showed or did.
    Continuation,
    /// During the window, in the state that a snapshot holds and the screen does not show.
    Checkpoint,
    /// The engine could not snapshot or restore.
    Restore,
}

/// What the check found at one place.
#[derive(Clone, Debug)]
pub struct Failure {
    pub kind: FailureKind,
    /// The engines differ only in the style of blank cells (see `screens_blank_only`).
    pub blank_only: bool,
    pub stream: String,
    /// The offset of the cut point where the snapshot was taken (for a restore failure), or the
    /// offset of the step where A and Q first differ.
    pub cut_offset: u64,
    /// The offset after the last byte of the step where the engines differ.
    pub offset: u64,
    pub what: String,
    /// The bytes of the step, escaped.
    pub context: String,
}

/// The result of one run.
#[derive(Debug, Default)]
pub struct Report {
    pub stream: String,
    pub steps: usize,
    pub bytes: u64,
    /// The cut points in the stream.
    pub cut_points: usize,
    /// The cut points that got a snapshot and a restore.
    pub checked: usize,
    /// Cut points that were inside a synchronized update.
    pub skipped_in_sync: usize,
    /// Cut points that came while `max_live` restored engines were alive.
    pub skipped_busy: usize,
    /// Chunk ends of a generated stream that are not cut points. They get no snapshot.
    pub unsafe_ends: usize,
    pub screen_compares: u64,
    pub snapshot_compares: u64,
    pub snapshot_bytes_total: u64,
    pub snapshot_bytes_max: usize,
    pub snapshot_time: Duration,
    pub restore_time: Duration,
    pub restore_time_max: Duration,
    /// A restored engine differs from A.
    pub failures: Vec<Failure>,
    /// A differs from Q: taking snapshots changed what the output means.
    pub purity_failures: Vec<Failure>,
    /// How many restores returned an error.
    pub restore_errors: usize,
}

impl Report {
    /// Whether the restored engines never differed from A.
    pub fn exact(&self) -> bool {
        self.failures.is_empty() && self.restore_errors == 0
    }

    /// How many failures of this kind.
    pub fn count(&self, kind: FailureKind) -> usize {
        self.failures.iter().filter(|f| f.kind == kind).count()
    }

    /// How many failures of this kind differ in more than the style of blank cells.
    pub fn count_not_blank(&self, kind: FailureKind) -> usize {
        self.failures
            .iter()
            .filter(|f| f.kind == kind && !f.blank_only)
            .count()
    }

    /// Whether every difference that showed later is a stale blank cell (see
    /// `screens_blank_only`) and nothing else.
    pub fn only_blank_cells_differ(&self) -> bool {
        self.failures
            .iter()
            .chain(&self.purity_failures)
            .all(|f| f.blank_only)
    }

    /// Whether every restore was exact at the moment of the restore: the snapshot held all that
    /// the engine shows and keeps, and no snapshot or restore failed.
    pub fn instant_exact(&self) -> bool {
        self.count(FailureKind::Instant) == 0
            && self.count(FailureKind::Restore) == 0
            && self.restore_errors == 0
    }

    /// Adds the numbers of `other` to this report.
    pub fn absorb(&mut self, other: Report) {
        self.steps += other.steps;
        self.bytes += other.bytes;
        self.cut_points += other.cut_points;
        self.checked += other.checked;
        self.skipped_in_sync += other.skipped_in_sync;
        self.skipped_busy += other.skipped_busy;
        self.unsafe_ends += other.unsafe_ends;
        self.screen_compares += other.screen_compares;
        self.snapshot_compares += other.snapshot_compares;
        self.snapshot_bytes_total += other.snapshot_bytes_total;
        self.snapshot_bytes_max = self.snapshot_bytes_max.max(other.snapshot_bytes_max);
        self.snapshot_time += other.snapshot_time;
        self.restore_time += other.restore_time;
        self.restore_time_max = self.restore_time_max.max(other.restore_time_max);
        self.restore_errors += other.restore_errors;
        self.failures.extend(other.failures);
        self.purity_failures.extend(other.purity_failures);
    }

    /// A few lines for a person.
    pub fn summary(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(
            out,
            "{}: {} steps, {} bytes, {} cut points; {} checked, {} in a synchronized update, {} busy, {} generated chunk ends not at a cut point",
            self.stream,
            self.steps,
            self.bytes,
            self.cut_points,
            self.checked,
            self.skipped_in_sync,
            self.skipped_busy,
            self.unsafe_ends,
        );
        let _ = writeln!(
            out,
            "  {} screen comparisons, {} snapshot comparisons; snapshot {:.0} bytes on average (max {}), {} snapshots in {:.1?}, restores {:.1?} (max {:.1?})",
            self.screen_compares,
            self.snapshot_compares,
            self.snapshot_bytes_total as f64 / self.checked.max(1) as f64,
            self.snapshot_bytes_max,
            self.checked,
            self.snapshot_time,
            self.restore_time,
            self.restore_time_max,
        );
        let _ = writeln!(
            out,
            "  restored engine differs from A: {} (right after the restore {}, in the window {}, in the checkpoint state {}, snapshot or restore failed {}); of these {} are more than the style of a blank cell; A differs from Q: {} ({} more than the style of a blank cell)",
            self.failures.len(),
            self.count(FailureKind::Instant),
            self.count(FailureKind::Continuation),
            self.count(FailureKind::Checkpoint),
            self.count(FailureKind::Restore),
            self.failures.iter().filter(|f| !f.blank_only).count(),
            self.purity_failures.len(),
            self.purity_failures
                .iter()
                .filter(|f| !f.blank_only)
                .count(),
        );
        for failure in self.failures.iter().chain(&self.purity_failures) {
            let _ = writeln!(
                out,
                "  - {} cut at {}, differs at {}: {}\n    bytes: {}",
                failure.stream, failure.cut_offset, failure.offset, failure.what, failure.context
            );
        }
        out
    }
}

fn new_engine(size: (u16, u16)) -> AlacrittyEngine {
    AlacrittyEngine::new(size.0, size.1, MIN_HISTORY, xterm_palette())
}

fn config() -> EngineConfig {
    EngineConfig {
        history: MIN_HISTORY,
        theme: xterm_palette(),
        cell_px: (0, 0),
    }
}

/// What a step did: the effects on the world, and the lines that left the screen.
#[derive(PartialEq, Debug)]
struct Outcome {
    effects: Vec<Effect>,
    history: Vec<HistoryLine>,
}

/// Describes the first difference between what two engines did, or `None`.
fn outcome_diff(b: &Outcome, a: &Outcome) -> Option<String> {
    if let Some(i) =
        (0..b.effects.len().max(a.effects.len())).find(|&i| b.effects.get(i) != a.effects.get(i))
    {
        let show = |e: Option<&Effect>| match e {
            Some(Effect::PtyWrite(bytes)) => format!("reply {}", show_bytes(bytes, 60)),
            Some(other) => format!("{other:?}"),
            None => "nothing".to_owned(),
        };
        return Some(format!(
            "effect {i} of {} against {}: {} against {}",
            b.effects.len(),
            a.effects.len(),
            show(b.effects.get(i)),
            show(a.effects.get(i))
        ));
    }
    if b.history != a.history {
        if b.history.len() == a.history.len()
            && let Some(i) = (0..b.history.len()).find(|&i| b.history[i] != a.history[i])
        {
            {
                let (x, y) = (&b.history[i], &a.history[i]);
                let what = if x.text != y.text {
                    format!("text {:?} against {:?}", x.text, y.text)
                } else if x.wrapped != y.wrapped {
                    format!("wrapped {} against {}", x.wrapped, y.wrapped)
                } else if x.line_no != y.line_no {
                    format!("line number {} against {}", x.line_no, y.line_no)
                } else {
                    let r = (0..x.runs.len().max(y.runs.len()))
                        .find(|&r| x.runs.get(r) != y.runs.get(r));
                    format!(
                        "style run {r:?}: {:?} against {:?}",
                        r.and_then(|r| x.runs.get(r)),
                        r.and_then(|r| y.runs.get(r))
                    )
                };
                return Some(format!(
                    "history line {i} of {} (#{}): {what}",
                    b.history.len(),
                    y.line_no
                ));
            }
        }
        let line = |h: &[HistoryLine]| {
            h.first().map_or_else(
                || "none".to_owned(),
                |l| {
                    format!(
                        "#{} {:?}",
                        l.line_no,
                        l.text.chars().take(40).collect::<String>()
                    )
                },
            )
        };
        return Some(format!(
            "history lines {} against {}; first {} against {}",
            b.history.len(),
            a.history.len(),
            line(&b.history),
            line(&a.history)
        ));
    }
    None
}

/// Applies a step and returns what it did.
fn apply(engine: &mut AlacrittyEngine, step: &Step) -> Outcome {
    let mut effects = Vec::new();
    match step {
        Step::Bytes(bytes) => {
            let mut recorder = Recorder::default();
            engine.advance(bytes, &mut recorder);
            effects = recorder.0;
        }
        Step::Resize { cols, rows } => engine.resize(*cols, *rows),
    }
    let mut history = Vec::new();
    engine.take_new_history(&mut history);
    Outcome { effects, history }
}

/// The first difference between two engines that a person can see, or `None`.
fn screens_differ(a: &AlacrittyEngine, b: &AlacrittyEngine) -> Option<String> {
    if let Some(difference) = dump_engine(a).diff(&dump_engine(b)) {
        return Some(difference);
    }
    if a.viewport() != b.viewport() {
        return Some(format!(
            "viewport {:?} against {:?}",
            a.viewport(),
            b.viewport()
        ));
    }
    None
}

/// Whether two engines differ only in the style of blank cells: every glyph, the cursor, the modes,
/// the title, the palette and the viewport are equal.
///
/// `alacritty_terminal` keeps rows in its grid that it reuses without clearing them, and a counter
/// per row that lets a reset skip the cells it thinks are clean. Some output leaves a stale
/// background colour on a blank cell. A restored engine has none of that state.
fn screens_blank_only(a: &AlacrittyEngine, b: &AlacrittyEngine) -> bool {
    let (mut da, mut db) = (dump_engine(a), dump_engine(b));
    for dump in [&mut da, &mut db] {
        for (text, style) in dump.rows.iter_mut().flatten() {
            if text.trim().is_empty() {
                *style = nh_proto::style::Style::default();
            }
        }
    }
    // The viewport follows the number of lines that left the screen. A blank line more or less
    // in the history changes it, and the history is compared with `outcome_blank_only`.
    da.diff(&db).is_none()
}

/// Whether two outcomes differ only in blank history lines and in the blanks at the end of
/// history lines.
///
/// `ED 2` moves the lines down to the last non-empty cell into the history. A stale blank cell
/// with a background color is a non-empty cell, so an engine that has one moves some blank lines
/// more than an engine that has none.
fn outcome_blank_only(b: &Outcome, a: &Outcome) -> bool {
    let content = |o: &Outcome| -> Vec<(String, bool)> {
        o.history
            .iter()
            .filter(|l| !l.text.trim().is_empty())
            .map(|l| (l.text.trim_end().to_owned(), l.wrapped))
            .collect()
    };
    b.effects == a.effects && content(b) == content(a)
}

/// Whether two snapshots differ only in the style of blank cells of the screens.
fn snapshot_blank_only(a: &Snapshot, b: &Snapshot) -> bool {
    let plain = |s: &Snapshot| {
        let mut s = s.clone();
        let blank: Vec<bool> = s
            .cells
            .iter()
            .map(|c| c.ch == ' ' && c.zerowidth.is_empty())
            .collect();
        for screen in [&mut s.primary, &mut s.alternate] {
            for run in screen.rows.iter_mut().flatten() {
                if blank[run.cell as usize] {
                    run.cell = 0;
                }
            }
        }
        s
    };
    snapshot_diff(&plain(a), &plain(b)).is_none()
}

/// The cell table of a snapshot, with the identifiers that `alacritty_terminal` made up for
/// hyperlinks without one replaced by their order of appearance. It numbers them from a counter
/// that all engines of a process share, so two engines never agree on them. Which cells belong to
/// one link is what counts.
fn canonical_cells(cells: &[nh_term::SnapCell]) -> Vec<nh_term::SnapCell> {
    let mut seen: Vec<String> = Vec::new();
    cells
        .iter()
        .map(|cell| {
            let mut cell = cell.clone();
            if let Some((id, _)) = &mut cell.hyperlink
                && id.ends_with("_alacritty")
            {
                let n = seen.iter().position(|s| s == id).unwrap_or_else(|| {
                    seen.push(id.clone());
                    seen.len() - 1
                });
                *id = format!("auto{n}");
            }
            cell
        })
        .collect()
}

/// The first field in which two snapshots differ, or `None`. The journal offset does not count.
pub fn snapshot_diff(a: &Snapshot, b: &Snapshot) -> Option<String> {
    let (cells_a, cells_b) = (canonical_cells(&a.cells), canonical_cells(&b.cells));
    macro_rules! fields {
        ($($f:ident),*) => {
            $(if a.$f != b.$f {
                return Some(format!("snapshot field `{}`: {:?} against {:?}", stringify!($f), a.$f, b.$f));
            })*
        };
    }
    fields!(
        cols,
        rows,
        alt_active,
        term_mode,
        kitty,
        kitty_inactive,
        mouse_x10,
        mouse_urxvt,
        modify_other_keys,
        tabs,
        region,
        active_charset,
        title,
        title_stack,
        cursor_shape,
        cursor_blinking,
        colors,
        last_char,
        history,
        bells
    );
    for (name, sa, sb) in [
        ("primary", &a.primary, &b.primary),
        ("alternate", &a.alternate, &b.alternate),
    ] {
        if sa.rows.len() != sb.rows.len() {
            return Some(format!(
                "{name} screen: {} rows against {}",
                sa.rows.len(),
                sb.rows.len()
            ));
        }
        for (y, (ra, rb)) in sa.rows.iter().zip(&sb.rows).enumerate() {
            // The cell numbers can differ when the tables differ, so compare the cells.
            let cells =
                |runs: &[nh_term::Run], table: &[nh_term::SnapCell]| -> Vec<nh_term::SnapCell> {
                    runs.iter()
                        .flat_map(|r| {
                            std::iter::repeat_n(table[r.cell as usize].clone(), usize::from(r.len))
                        })
                        .collect()
                };
            let (ca, cb) = (cells(ra, &cells_a), cells(rb, &cells_b));
            if let Some(x) = (0..ca.len().min(cb.len())).find(|&x| ca[x] != cb[x]) {
                return Some(format!(
                    "{name} screen cell ({x},{y}): {:?} against {:?}",
                    ca[x], cb[x]
                ));
            }
        }
        let cursor = |c: &Option<nh_term::SnapCursor>, table: &[nh_term::SnapCell]| {
            c.as_ref().map(|c| {
                (
                    c.line,
                    c.column,
                    table[c.template as usize].clone(),
                    c.charsets,
                    c.pending_wrap,
                )
            })
        };
        if cursor(&sa.cursor, &cells_a) != cursor(&sb.cursor, &cells_b) {
            return Some(format!(
                "{name} cursor {:?} against {:?}",
                cursor(&sa.cursor, &cells_a),
                cursor(&sb.cursor, &cells_b)
            ));
        }
        if cursor(&sa.saved, &cells_a) != cursor(&sb.saved, &cells_b) {
            return Some(format!(
                "{name} saved cursor {:?} against {:?}",
                cursor(&sa.saved, &cells_a),
                cursor(&sb.saved, &cells_b)
            ));
        }
    }
    None
}

/// Escapes bytes for a message.
pub fn show_bytes(bytes: &[u8], limit: usize) -> String {
    let mut out = String::new();
    for &b in bytes.iter().take(limit) {
        match b {
            0x1b => out.push_str("\\e"),
            b'\r' => out.push_str("\\r"),
            b'\n' => out.push_str("\\n"),
            b'\t' => out.push_str("\\t"),
            0x20..=0x7e => out.push(b as char),
            _ => {
                let _ = write!(out, "\\x{b:02x}");
            }
        }
    }
    if bytes.len() > limit {
        let _ = write!(out, "… ({} bytes)", bytes.len());
    }
    out
}

/// A restored engine that follows A for a while.
struct Branch {
    engine: AlacrittyEngine,
    /// The offset of the cut point where it was restored.
    cut_offset: u64,
    /// How many bytes it still reads.
    left: usize,
    steps: usize,
}

/// Runs the check on a stream.
///
/// Every snapshot goes through [`checkpoint`], which takes one of A and one of each live restored
/// engine at the same step. That matters in one case. A snapshot taken while the alternate screen
/// is on show sets the saved cursor of the primary screen to its cursor (see `VtEngine::snapshot`),
/// so engines that are snapshotted at different steps would differ by what the check does to them.
/// In a real pane the engine that continues and the engine that a restart restores share their
/// checkpoints in the same way.
pub fn run(stream: &Stream, options: &Options) -> Report {
    let mut report = Report {
        stream: stream.name.clone(),
        steps: stream.steps.len(),
        unsafe_ends: stream.unsafe_ends,
        cut_points: stream.cuts.iter().filter(|c| **c).count(),
        bytes: stream.total_bytes(),
        ..Report::default()
    };
    let mut a = new_engine(stream.initial);
    let mut q = new_engine(stream.initial);
    let mut branches: Vec<Branch> = Vec::new();
    let (mut offset, mut cut_index) = (0u64, 0usize);
    let mut purity_reported = false;

    for (i, step) in stream.steps.iter().enumerate() {
        let before = offset;
        if let Step::Bytes(b) = step {
            offset += b.len() as u64;
        }
        let fx_q = apply(&mut q, step);
        let fx_a = apply(&mut a, step);

        // A against Q: the snapshots that A took must not change the meaning of the output.
        if !purity_reported {
            let what = outcome_diff(&fx_a, &fx_q).or_else(|| screens_differ(&a, &q));
            if let Some(what) = what {
                purity_reported = true;
                let blank = outcome_blank_only(&fx_a, &fx_q) && screens_blank_only(&a, &q);
                report
                    .purity_failures
                    .push(failure(stream, step, before, offset, before, what, blank));
            }
        }

        // Each restored engine reads the step and is compared with A.
        let len = if let Step::Bytes(b) = step {
            b.len()
        } else {
            0
        };
        let mut ending = false;
        let mut index = 0;
        while index < branches.len() {
            let branch = &mut branches[index];
            let fx_b = apply(&mut branch.engine, step);
            branch.steps += 1;
            branch.left = branch.left.saturating_sub(len);
            report.screen_compares += 1;
            let what = outcome_diff(&fx_b, &fx_a).or_else(|| screens_differ(&branch.engine, &a));
            if let Some(what) = what {
                let blank =
                    outcome_blank_only(&fx_b, &fx_a) && screens_blank_only(&branch.engine, &a);
                report.failures.push(failure(
                    stream,
                    step,
                    before,
                    offset,
                    branch.cut_offset,
                    what,
                    blank,
                ));
                branches.swap_remove(index);
            } else {
                ending |= branch.left == 0;
                index += 1;
            }
        }
        if report.failures.len() >= options.max_failures {
            break;
        }

        // A cut point where the check takes a snapshot of A and restores it.
        let spawn = stream.cuts[i] && {
            let selected = cut_index % options.every.max(1) == 0;
            cut_index += 1;
            selected && branches.len() < options.max_live
        };
        if stream.cuts[i] && !spawn && (cut_index - 1) % options.every.max(1) == 0 {
            report.skipped_busy += 1;
        }
        if spawn {
            spawn_branch(stream, options, &mut a, &mut branches, &mut report, offset);
        } else if !branches.is_empty() && (ending || i % options.snapshot_every.max(1) == 0) {
            checkpoint(
                stream,
                &mut a,
                &mut branches,
                &mut report,
                offset,
                options.normalize,
            );
        }
        branches.retain(|b| b.left > 0);
        if report.failures.len() >= options.max_failures {
            break;
        }
    }
    report
}

/// Takes a snapshot of A and of every restored engine, and compares each of them with A's.
/// An engine that differs leaves the check.
fn checkpoint(
    stream: &Stream,
    a: &mut AlacrittyEngine,
    branches: &mut Vec<Branch>,
    report: &mut Report,
    offset: u64,
    normalize: bool,
) {
    let reference = if normalize {
        a.checkpoint(offset)
    } else {
        a.snapshot(offset)
    };
    compare_all(stream, &reference, branches, report, offset, normalize);
}

fn compare_all(
    stream: &Stream,
    reference: &Result<Snapshot, SnapshotError>,
    branches: &mut Vec<Branch>,
    report: &mut Report,
    offset: u64,
    normalize: bool,
) {
    let mut index = 0;
    while index < branches.len() {
        report.snapshot_compares += 1;
        match compare_snapshots(&mut branches[index].engine, reference, offset, normalize) {
            Some(what) => {
                report.failures.push(Failure {
                    kind: FailureKind::Checkpoint,
                    blank_only: what.1,
                    stream: stream.name.clone(),
                    cut_offset: branches[index].cut_offset,
                    offset,
                    what: format!("at a checkpoint: {}", what.0),
                    context: String::new(),
                });
                branches.swap_remove(index);
            }
            None => index += 1,
        }
    }
}

/// Takes a snapshot of A at a cut point, restores it into a new engine, and compares them.
fn spawn_branch(
    stream: &Stream,
    options: &Options,
    a: &mut AlacrittyEngine,
    branches: &mut Vec<Branch>,
    report: &mut Report,
    offset: u64,
) {
    let started = Instant::now();
    let taken = if options.normalize {
        a.checkpoint_with_scrollback(offset)
    } else {
        a.snapshot_with_scrollback(offset)
    };
    let (snapshot, scrollback) = match taken {
        Ok(s) => s,
        Err(SnapshotError::SyncOpen) => {
            report.skipped_in_sync += 1;
            return;
        }
        Err(error) => {
            report.restore_errors += 1;
            report.failures.push(Failure {
                kind: FailureKind::Restore,
                blank_only: false,
                stream: stream.name.clone(),
                cut_offset: offset,
                offset,
                what: format!("no snapshot: {error}"),
                context: String::new(),
            });
            return;
        }
    };
    report.snapshot_time += started.elapsed();
    // The live engines take their snapshot at the same moment as A.
    compare_all(
        stream,
        &Ok(snapshot.clone()),
        branches,
        report,
        offset,
        options.normalize,
    );

    let bytes = snapshot.encode();
    report.snapshot_bytes_total += bytes.len() as u64;
    report.snapshot_bytes_max = report.snapshot_bytes_max.max(bytes.len());
    let started = Instant::now();
    let restored = Snapshot::decode(&bytes)
        .and_then(|s| AlacrittyEngine::restore_with_scrollback(&s, &config(), &scrollback));
    let took = started.elapsed();
    report.restore_time += took;
    report.restore_time_max = report.restore_time_max.max(took);
    report.checked += 1;
    let fail = |kind: FailureKind, what: String| Failure {
        kind,
        blank_only: false,
        stream: stream.name.clone(),
        cut_offset: offset,
        offset,
        what,
        context: String::new(),
    };
    match restored {
        Ok(engine) => {
            let mut branch = Branch {
                engine,
                cut_offset: offset,
                left: options.window,
                steps: 0,
            };
            report.screen_compares += 1;
            report.snapshot_compares += 1;
            let what = screens_differ(&branch.engine, a).or_else(|| {
                compare_snapshots(&mut branch.engine, &Ok(snapshot), offset, options.normalize)
                    .map(|(what, _)| what)
            });
            match what {
                Some(what) => report.failures.push(fail(
                    FailureKind::Instant,
                    format!("right after the restore: {what}"),
                )),
                None => branches.push(branch),
            }
        }
        Err(error) => {
            report.restore_errors += 1;
            report.failures.push(fail(
                FailureKind::Restore,
                format!("the snapshot does not restore: {error}"),
            ));
        }
    }
}

/// Takes a snapshot of the restored engine and compares it with the one of A.
fn compare_snapshots(
    b: &mut AlacrittyEngine,
    reference: &Result<Snapshot, SnapshotError>,
    offset: u64,
    normalize: bool,
) -> Option<(String, bool)> {
    let taken = if normalize {
        b.checkpoint(offset)
    } else {
        b.snapshot(offset)
    };
    match (reference, taken) {
        (Ok(sa), Ok(sb)) => snapshot_diff(&sb, sa).map(|d| {
            (
                format!("{d} (restored against continued)"),
                snapshot_blank_only(&sb, sa),
            )
        }),
        (Err(x), Err(y)) if *x == y => None,
        (Err(x), Ok(_)) => Some((
            format!("A cannot snapshot ({x}), the restored engine can"),
            false,
        )),
        (Ok(_), Err(y)) => Some((
            format!("the restored engine cannot snapshot ({y}), A can"),
            false,
        )),
        (Err(_), Err(_)) => None,
    }
}

fn failure(
    stream: &Stream,
    step: &Step,
    before: u64,
    after: u64,
    cut: u64,
    what: String,
    blank_only: bool,
) -> Failure {
    let context = match step {
        Step::Bytes(b) => show_bytes(b, 200),
        Step::Resize { cols, rows } => format!("resize to {cols}x{rows}"),
    };
    Failure {
        kind: FailureKind::Continuation,
        blank_only,
        stream: stream.name.clone(),
        cut_offset: cut,
        offset: after,
        what: format!("{what} (step at {before})"),
        context,
    }
}
