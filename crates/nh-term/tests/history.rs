//! History accounting: `take_new_history` returns exactly the lines that left the screen,
//! in order, with no gap and no duplicate.

use nh_proto::style::{Color, UnderlineKind};
use nh_term::{
    AlacrittyEngine, HISTORY_HIGH_WATER, HistoryLine, MIN_HISTORY, Replaying, VtEngine,
    xterm_palette,
};

fn engine(cols: u16, rows: u16) -> AlacrittyEngine {
    AlacrittyEngine::new(cols, rows, MIN_HISTORY, xterm_palette())
}

/// A small deterministic random number generator (xorshift), so that a failure repeats.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// Feeds `bytes` in chunks of random sizes up to 4 KiB, and takes the history now and then.
fn feed_randomly(
    engine: &mut AlacrittyEngine,
    bytes: &[u8],
    seed: u64,
    taken: &mut Vec<HistoryLine>,
) {
    let mut rng = Rng(seed);
    let mut pos = 0;
    while pos < bytes.len() {
        let len = (1 + rng.below(4096)).min(bytes.len() - pos);
        engine.advance(&bytes[pos..pos + len], &mut Replaying::default());
        pos += len;
        if rng.below(4) == 0 {
            engine.take_new_history(taken);
        }
    }
    engine.take_new_history(taken);
}

fn numbered(range: std::ops::Range<usize>) -> Vec<u8> {
    range
        .map(|n| format!("line {n}\r\n"))
        .collect::<String>()
        .into_bytes()
}

fn assert_sequence(lines: &[HistoryLine], first_line_no: u64, first_number: usize) {
    for (i, line) in lines.iter().enumerate() {
        assert_eq!(line.line_no, first_line_no + i as u64, "line number at {i}");
        assert_eq!(
            line.text,
            format!("line {}", first_number + i),
            "text at {i}"
        );
    }
}

#[test]
fn a_hundred_thousand_lines_in_random_chunks_leave_the_screen_exactly_once_in_order() {
    let mut engine = engine(80, 24);
    let mut taken = Vec::new();
    feed_randomly(
        &mut engine,
        &numbered(0..100_000),
        0x9e37_79b9_7f4a_7c15,
        &mut taken,
    );
    // The screen shows lines 99,977 to 99,999 and an empty row for the cursor.
    assert_eq!(taken.len(), 99_977);
    assert_sequence(&taken, 0, 0);
    let viewport = engine.viewport();
    assert_eq!(
        (viewport.top_line, viewport.total_lines),
        (99_977, 99_977 + 24)
    );
}

#[test]
fn the_result_does_not_depend_on_the_chunk_sizes() {
    let bytes = numbered(0..30_000);
    for seed in [1, 2, 3, 12345] {
        let mut engine = engine(80, 24);
        let mut taken = Vec::new();
        feed_randomly(&mut engine, &bytes, seed, &mut taken);
        assert_eq!(taken.len(), 30_000 + 1 - 24, "seed {seed}");
        assert_sequence(&taken, 0, 0);
    }
}

#[test]
fn a_clear_of_the_scrollback_loses_no_line_before_it_and_numbering_goes_on() {
    let mut engine = engine(80, 24);
    let mut taken = Vec::new();
    // The lines before the clear, the clear, and the lines after it share chunks.
    let mut bytes = numbered(0..3_000);
    bytes.extend_from_slice(b"\x1b[3J");
    bytes.extend_from_slice(&numbered(3_000..6_000));
    feed_randomly(&mut engine, &bytes, 77, &mut taken);
    // Every line that left the screen is there once, and the numbers have no gap.
    assert_eq!(taken.len(), 6_000 + 1 - 24);
    assert_sequence(&taken, 0, 0);
}

#[test]
fn the_clear_in_the_middle_of_one_chunk_counts_the_lines_before_it() {
    let mut engine = engine(80, 24);
    let mut bytes = numbered(0..100);
    bytes.extend_from_slice(b"\x1b[3J");
    bytes.extend_from_slice(&numbered(100..110));
    engine.advance(&bytes, &mut Replaying::default());
    let mut taken = Vec::new();
    engine.take_new_history(&mut taken);
    assert_eq!(taken.len(), 110 + 1 - 24);
    assert_sequence(&taken, 0, 0);
}

#[test]
fn a_reset_of_the_terminal_keeps_the_archive_and_goes_on() {
    let mut engine = engine(80, 24);
    let mut bytes = numbered(0..500);
    bytes.extend_from_slice(b"\x1bc");
    bytes.extend_from_slice(&numbered(500..1_000));
    let mut taken = Vec::new();
    feed_randomly(&mut engine, &bytes, 5, &mut taken);
    // The reset clears the screen, so the 23 lines that were on it never scrolled off.
    let before: Vec<_> = taken.iter().take_while(|l| l.text != "line 500").collect();
    assert_eq!(before.len(), 500 + 1 - 24);
    assert_eq!(taken.len(), before.len() + 500 + 1 - 24);
    assert!(taken.windows(2).all(|w| w[1].line_no == w[0].line_no + 1));
}

#[test]
fn the_alternate_screen_adds_nothing() {
    let mut engine = engine(80, 24);
    let mut bytes = numbered(0..100);
    bytes.extend_from_slice(b"\x1b[?1049h");
    bytes.extend_from_slice(&numbered(1_000..1_200));
    bytes.extend_from_slice(b"\x1b[?1049l");
    bytes.extend_from_slice(&numbered(100..120));
    let mut taken = Vec::new();
    feed_randomly(&mut engine, &bytes, 9, &mut taken);
    assert_eq!(taken.len(), 120 + 1 - 24);
    assert_sequence(&taken, 0, 0);
}

#[test]
fn a_wrapped_row_says_so_and_a_styled_row_has_runs() {
    let mut engine = engine(10, 3);
    let mut bytes = b"0123456789ABCDEF\r\n".to_vec();
    bytes.extend_from_slice(b"\x1b[1;38;2;1;2;3mred\x1b[0m plain\x1b[4:3m~\x1b[0m\r\n");
    bytes.extend_from_slice(&numbered(0..5));
    engine.advance(&bytes, &mut Replaying::default());
    let mut taken = Vec::new();
    engine.take_new_history(&mut taken);
    assert_eq!(
        (taken[0].text.as_str(), taken[0].wrapped),
        ("0123456789", true)
    );
    assert_eq!(
        (taken[1].text.as_str(), taken[1].wrapped),
        ("ABCDEF", false)
    );
    let styled = &taken[2];
    assert_eq!(styled.text, "red plain~");
    assert_eq!(styled.runs.len(), 2, "{:?}", styled.runs);
    assert_eq!((styled.runs[0].start, styled.runs[0].len), (0, 3));
    assert_eq!(styled.runs[0].style.fg, Color::Rgb(1, 2, 3));
    assert_eq!((styled.runs[1].start, styled.runs[1].len), (9, 1));
    assert_eq!(styled.runs[1].style.underline, UnderlineKind::Curly);
}

#[test]
fn scrolling_back_moves_the_viewport_in_absolute_lines() {
    let mut engine = engine(80, 24);
    engine.advance(&numbered(0..1_000), &mut Replaying::default());
    let live = engine.viewport();
    assert_eq!((live.top_line, live.total_lines), (977, 1_001));
    let scrolled = engine.scroll_display(Some(500));
    assert_eq!(scrolled.top_line, 500);
    let mut first_row = String::new();
    engine.visit_row(0, &mut |cell| first_row.push(cell.ch));
    assert_eq!(first_row.trim_end(), "line 500");
    // A line that is above the history in memory is clamped to the oldest one that is there.
    assert!(engine.scroll_display(Some(0)).top_line <= 500);
    assert_eq!(engine.scroll_display(None).top_line, 977);
}

#[test]
fn history_stays_below_the_high_water_mark() {
    let mut engine = engine(80, 24);
    let mut taken = Vec::new();
    feed_randomly(&mut engine, &numbered(0..60_000), 31, &mut taken);
    // The oldest lines of the scrollback in memory are gone, but the viewport counts them.
    let oldest_in_memory = engine.scroll_display(Some(0)).top_line;
    assert!(oldest_in_memory > 0, "{oldest_in_memory}");
    assert_eq!(engine.viewport().total_lines, 60_000 + 1);
    assert_eq!(taken.len(), 60_000 + 1 - 24);
    // The history in memory is what is between the oldest line and the screen. It is at most the
    // mark plus the lines of one chunk (a line of the test has about 10 bytes), and it is at
    // least half of the mark, because a trim leaves half of it.
    let history = 60_000 + 1 - 24 - oldest_in_memory;
    assert!(
        (HISTORY_HIGH_WATER as u64 / 2..=HISTORY_HIGH_WATER as u64 + 128).contains(&history),
        "{history}"
    );
}

#[test]
fn a_chunk_that_adds_more_lines_than_the_room_is_counted_as_a_gap_and_keeps_the_newest_lines() {
    let mut engine = engine(80, 24);
    // Each `CSI 24 S` scrolls the whole screen into the history: 24 lines in 5 bytes.
    let mut bytes = Vec::new();
    for n in 0..300 {
        bytes.extend_from_slice(format!("mark {n}\x1b[24S").as_bytes());
    }
    engine.advance(&bytes, &mut Replaying::default());
    assert!(engine.history_gaps() >= 1);
    let mut taken = Vec::new();
    engine.take_new_history(&mut taken);
    assert!(!taken.is_empty() && taken.len() < 300 * 24);
    assert!(
        taken.windows(2).all(|w| w[1].line_no == w[0].line_no + 1),
        "the lines have no gap among them"
    );
    // Plain output after it is exact again.
    let before = engine.history_gaps();
    let mut later = Vec::new();
    let mut plain = b"\r\n".to_vec();
    plain.extend_from_slice(&numbered(0..5_000));
    feed_randomly(&mut engine, &plain, 8, &mut later);
    assert_eq!(engine.history_gaps(), before);
    // The numbered lines are all there, once each, in order. Rows of the old screen come before.
    let numbers: Vec<usize> = later
        .iter()
        .filter_map(|l| l.text.strip_prefix("line ")?.parse().ok())
        .collect();
    assert_eq!(numbers, (0..5_000 - 23).collect::<Vec<_>>());
}

#[test]
fn a_resize_in_the_alternate_screen_does_not_archive_the_history_again() {
    // The history of the primary screen is out of reach while the alternate screen is on show.
    // A resize there used to set the baseline to the empty history of the alternate screen, and the
    // whole scrollback came out a second time when the screen went off.
    let mut engine = AlacrittyEngine::new(40, 8, MIN_HISTORY, xterm_palette());
    let lines: String = (0..60).map(|n| format!("row {n}\r\n")).collect();
    engine.advance(lines.as_bytes(), &mut Replaying::default());
    let mut archived = Vec::new();
    engine.take_new_history(&mut archived);
    assert_eq!(archived.len(), 60 - 7);
    engine.advance(b"\x1b[?1049hvim", &mut Replaying::default());
    engine.resize(50, 12);
    engine.resize(30, 6);
    engine.advance(b"\x1b[?1049l", &mut Replaying::default());
    let mut again = Vec::new();
    engine.take_new_history(&mut again);
    assert!(again.is_empty(), "{} lines came out again", again.len());
    // Lines that scroll after the screen went off count as before.
    engine.advance(
        b"a\r\nb\r\nc\r\nd\r\ne\r\nf\r\ng\r\nh\r\ni\r\n",
        &mut Replaying::default(),
    );
    engine.take_new_history(&mut again);
    assert!(!again.is_empty() && again.len() <= 10);
}
