//! The scenarios of `gen-frames`: what each pane shows from one display frame to the next.

use nh_proto::frame::{
    Cursor, CursorShape, Frame, FrameKind, Selection, SelectionMode, SelectionPoint,
};
use nh_term::{
    AlacrittyEngine, ClientState, FrameBuilder, MIN_HISTORY, Replaying, VtEngine, xterm_palette,
};

use crate::container::{Container, Record};

/// What the panes show.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scenario {
    /// One pane with a screen of every kind of cell, then deltas that move the cursor and the
    /// selection. For the tests that compare the renderers.
    Screen,
    /// Every cell of every pane changes in every display frame, like a full-screen TUI that
    /// redraws.
    Redraw,
    /// A screen of text, and in each display frame a spinner and a timer change: two rows.
    Spinner,
    /// A line of output in each display frame, so that the whole screen scrolls.
    Scroll,
    /// A screen of 200 other characters in each display frame, so that a small atlas fills up.
    Glyphs,
}

impl Scenario {
    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "screen" => Self::Screen,
            "redraw" => Self::Redraw,
            "spinner" => Self::Spinner,
            "scroll" => Self::Scroll,
            "glyphs" => Self::Glyphs,
            _ => return None,
        })
    }
}

/// What `generate` needs.
#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub scenario: Scenario,
    pub panes: u32,
    pub cols: u16,
    pub rows: u16,
    /// The number of display frames (the `Screen` scenario has a fixed number).
    pub frames: u32,
    pub seed: u64,
}

/// A small deterministic generator. The scenarios must not change from run to run.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

/// One pane: an engine, the builder of its frames and the one client that receives them.
struct Source {
    engine: AlacrittyEngine,
    builder: FrameBuilder,
    client: ClientState,
}

impl Source {
    fn new(pane: u32, cols: u16, rows: u16) -> Self {
        let engine = AlacrittyEngine::new(cols, rows, MIN_HISTORY, xterm_palette());
        let builder = FrameBuilder::new(pane, &engine);
        Self {
            engine,
            builder,
            client: ClientState::new(),
        }
    }

    /// Feeds bytes to the engine and builds the next frame.
    fn step(&mut self, bytes: &[u8]) -> Frame {
        self.engine.advance(bytes, &mut Replaying::default());
        self.builder.absorb(&mut self.engine);
        self.builder
            .build(&self.engine, &mut self.client, FrameKind::Delta)
    }
}

fn encode(frame: &Frame) -> Vec<u8> {
    postcard::to_stdvec(frame).expect("a frame serializes")
}

/// The frames of a scenario.
pub fn generate(params: &Params) -> Container {
    let records = match params.scenario {
        Scenario::Screen => screen(params),
        _ => animated(params),
    };
    Container {
        panes: if params.scenario == Scenario::Screen {
            1
        } else {
            params.panes
        },
        cols: u32::from(params.cols),
        rows: u32::from(params.rows),
        records,
    }
}

fn animated(params: &Params) -> Vec<Record> {
    let mut sources: Vec<Source> = (1..=params.panes)
        .map(|pane| Source::new(pane, params.cols, params.rows))
        .collect();
    let mut rngs: Vec<Rng> = (1..=u64::from(params.panes))
        .map(|pane| Rng(params.seed ^ pane.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1))
        .collect();
    let mut records = Vec::new();
    for tick in 0..params.frames {
        for (index, source) in sources.iter_mut().enumerate() {
            let rng = &mut rngs[index];
            let bytes = match params.scenario {
                Scenario::Redraw => redraw(params, tick, index as u32),
                Scenario::Spinner => spinner(params, tick, rng),
                Scenario::Glyphs => glyphs(params, tick),
                _ => scroll(params, tick, rng),
            };
            let frame = source.step(&bytes);
            records.push(Record {
                tick,
                pane: index as u32 + 1,
                bytes: encode(&frame),
            });
        }
    }
    records
}

/// Characters of the redraw. Some are wide, some are box drawing and braille.
const GLYPHS: &[&str] = &[
    "a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l", "m", "n", "o", "p", "q", "r", "s",
    "t", "u", "v", "w", "x", "y", "z", "A", "B", "C", "D", "E", "F", "0", "1", "2", "3", "4", "5",
    "6", "7", "8", "9", "(", ")", "[", "]", "{", "}", "<", ">", "=", "+", "-", "*", "/", "#", "_",
    "─", "│", "┌", "┐", "└", "┘", "├", "┤", "█", "▓", "▒", "░", "⠋", "⠙", "⠹", "日", "本", "語",
];

/// The style of a run: bounded, so that the style table stops growing after the first frames.
/// 4 attribute sets × 16 foregrounds × 3 backgrounds = 192 styles.
fn style_for(x: u32, y: u32, tick: u32, pane: u32) -> String {
    let h = x
        .wrapping_mul(2_654_435_761)
        .wrapping_add(y.wrapping_mul(40_503))
        .wrapping_add(tick.wrapping_mul(97))
        .wrapping_add(pane.wrapping_mul(7919));
    let attr = [0u32, 1, 3, 4][(h % 4) as usize];
    let fg = 1 + (h / 4) % 16;
    let bg = [0u32, 17, 236][((h / 64) % 3) as usize];
    format!("\x1b[0;{attr};38;5;{fg};48;5;{bg}m")
}

fn redraw(params: &Params, tick: u32, pane: u32) -> Vec<u8> {
    redraw_bytes(params.cols, params.rows, tick, pane)
}

/// The bytes of one full-screen redraw, as a TUI that repaints everything writes them: every cell
/// of the `cols` × `rows` screen changes between two ticks. `pane` and `tick` choose the content.
fn redraw_bytes(cols: u16, rows: u16, tick: u32, pane: u32) -> Vec<u8> {
    let (cols, rows) = (u32::from(cols), u32::from(rows));
    let mut out = String::from("\x1b[H");
    for y in 0..rows {
        let mut x = 0;
        while x < cols {
            if x % 4 == 0 {
                out.push_str(&style_for(x / 4, y, tick, pane));
            }
            let g = GLYPHS[((x * 7 + y * 3 + tick * 5 + pane) as usize) % GLYPHS.len()];
            let wide = matches!(g, "日" | "本" | "語");
            if wide && x + 2 > cols {
                out.push(' ');
                x += 1;
            } else {
                out.push_str(g);
                x += if wide { 2 } else { 1 };
            }
        }
        if y + 1 < rows {
            out.push_str("\r\n");
        }
    }
    out.into_bytes()
}

const WORDS: &[&str] = &[
    "the",
    "agent",
    "reads",
    "file",
    "src/main.rs",
    "and",
    "writes",
    "tests",
    "for",
    "parser",
    "error",
    "handling",
    "in",
    "module",
    "output",
    "stream",
    "token",
    "cache",
    "result",
    "diff",
];

fn text_line(rng: &mut Rng, cols: usize) -> String {
    let mut line = String::new();
    let mut colored = 0;
    while line.chars().count() + 12 < cols {
        if colored % 5 == 0 {
            let color = [33, 36, 32, 35, 37, 90][rng.below(6) as usize];
            line.push_str(&format!("\x1b[{color}m"));
        }
        line.push_str(WORDS[rng.below(WORDS.len() as u64) as usize]);
        line.push(' ');
        colored += 1;
    }
    line.push_str("\x1b[0m");
    line
}

fn spinner(params: &Params, tick: u32, rng: &mut Rng) -> Vec<u8> {
    const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    let rows = u32::from(params.rows);
    let mut out = String::new();
    if tick == 0 {
        out.push_str("\x1b[2J\x1b[H");
        for y in 0..rows.saturating_sub(2) {
            out.push_str(&format!("\x1b[90m{y:3} │\x1b[0m "));
            out.push_str(&text_line(rng, usize::from(params.cols) - 6));
            out.push_str("\r\n");
        }
    }
    let glyph = SPINNER[(tick % 10) as usize];
    let color = 208 + (tick / 10) % 8;
    out.push_str(&format!(
        "\x1b[{};1H\x1b[2K\x1b[38;5;{color}m{glyph}\x1b[0m Thinking… {}s \x1b[2m(esc to interrupt)\x1b[0m",
        rows - 1,
        tick / 60,
    ));
    out.push_str(&format!("\x1b[{rows};1H> "));
    out.into_bytes()
}

/// Characters of one column width: Latin-1 and Latin Extended, Greek and Cyrillic. The soft
/// hyphen, the combining Cyrillic marks and the one unassigned code point are left out, because
/// they have no width of their own.
fn narrow_chars() -> Vec<char> {
    (0xa1..=0x17f)
        .chain(0x391..=0x3c9)
        .chain(0x400..=0x4ff)
        .filter(|c| !matches!(c, 0xad | 0x3a2 | 0x482..=0x489))
        .filter_map(char::from_u32)
        .collect()
}

/// Every display frame shows 200 characters that the frames before did not, repeated over the
/// screen. A small atlas holds two frames of them, so it must reset to hold the next.
fn glyphs(params: &Params, tick: u32) -> Vec<u8> {
    let chars = narrow_chars();
    let first = (tick as usize * 200) % chars.len();
    let mut out = String::from("\x1b[0m\x1b[H");
    for y in 0..usize::from(params.rows) {
        for x in 0..usize::from(params.cols) {
            let n = (x + y * usize::from(params.cols)) % 200;
            out.push(chars[(first + n) % chars.len()]);
        }
        if y + 1 < usize::from(params.rows) {
            out.push_str("\r\n");
        }
    }
    out.into_bytes()
}

fn scroll(params: &Params, tick: u32, rng: &mut Rng) -> Vec<u8> {
    let rows = usize::from(params.rows);
    let mut out = String::new();
    if tick == 0 {
        // Fill the screen, so that every next line scrolls it.
        out.push_str("\x1b[2J\x1b[H");
        for n in 0..rows - 1 {
            out.push_str(&format!("\x1b[90m{n:6}\x1b[0m "));
            out.push_str(&text_line(rng, usize::from(params.cols) - 7));
            out.push_str("\r\n");
        }
    }
    out.push_str(&format!("\x1b[90m{:6}\x1b[0m ", rows - 1 + tick as usize));
    out.push_str(&text_line(rng, usize::from(params.cols) - 7));
    out.push_str("\r\n");
    out.into_bytes()
}

/// The fixture screen. It needs at least 64 columns and 20 rows.
fn screen_bytes() -> Vec<u8> {
    let mut s = String::from("\x1b[2J\x1b[H");
    s.push_str("plain text 0123456789 abcXYZ ~!@#$%^&*()\r\n");
    s.push_str(
        "\x1b[1mbold\x1b[0m \x1b[2mdim\x1b[0m \x1b[3mitalic\x1b[0m \x1b[7minverse\x1b[0m \
         \x1b[9mstrike\x1b[0m \x1b[8mhidden\x1b[0m \x1b[5mblink\x1b[0m \x1b[1;3mboth\x1b[0m\r\n",
    );
    s.push_str(
        "\x1b[4msingle\x1b[0m \x1b[4:2mdouble\x1b[0m \x1b[4:3m\x1b[58;2;255;90;90mcurly\x1b[0m \
         \x1b[4:4mdotted\x1b[0m \x1b[4:5mdashed\x1b[0m \x1b[4;9mboth\x1b[0m\r\n",
    );
    for i in 0..8 {
        s.push_str(&format!("\x1b[3{i}m██\x1b[4{i}m  \x1b[0m"));
    }
    for i in 0..8 {
        s.push_str(&format!("\x1b[9{i}m██\x1b[10{i}m  \x1b[0m"));
    }
    s.push_str("\r\n");
    for n in 16..52 {
        s.push_str(&format!("\x1b[48;5;{n}m "));
    }
    s.push_str("\x1b[0m\r\n");
    for n in 0..32 {
        let (r, g, b) = (n * 8, 255 - n * 8, 128);
        s.push_str(&format!("\x1b[48;2;{r};{g};{b}m "));
    }
    s.push_str("\x1b[0m\r\n");
    s.push_str("日本語テキスト 한국어 😀 🚀 👨‍👩‍👧 🇩🇪 e\u{301} a\u{308} 中\r\n");
    s.push_str("┌──┬──┐ │ ╳ ╱╲ ⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏ ▀▄█▌▐░▒▓ ✻ ✓ →\r\n");
    s.push_str(
        "\x1b[1;3;4;38;5;208mbold italic underline\x1b[0m \x1b[2;9;38;5;81mdim strike\x1b[0m \
         \x1b[1;48;5;22;38;5;231m status \x1b[0m\r\n",
    );
    s.push_str(&"0123456789".repeat(6));
    s.push_str("0123\r\n");
    s.push_str(
        "\x1b[10;1H\x1b[38;5;196mred\x1b[0m on default \x1b[48;5;238m grey bg \x1b[0m end\r\n",
    );
    s.into_bytes()
}

/// The frames of the fixture: a keyframe, then deltas that only change the cursor and the
/// selection. The builder does not set a selection, so the scenario writes it into the frames.
fn screen(params: &Params) -> Vec<Record> {
    let mut source = Source::new(1, params.cols, params.rows);
    let mut frames = vec![source.step(&screen_bytes())];
    // The cursor sits on column 10 of row 11 in the first frame.
    let base = |shape, visible, x, y| Cursor {
        x,
        y,
        shape,
        visible,
        blink: false,
    };
    let line = |row: u64, col: u16| SelectionPoint { col, line: row };
    let selection = |mode, start, end| Some(Selection { start, end, mode });
    let variants: Vec<(Cursor, Option<Selection>)> = vec![
        (base(CursorShape::Underline, true, 10, 11), None),
        (base(CursorShape::Bar, true, 10, 11), None),
        (base(CursorShape::HollowBlock, true, 10, 11), None),
        (base(CursorShape::Block, false, 10, 11), None),
        // On the wide glyph 日 (column 0 of row 6) and on a colour emoji.
        (base(CursorShape::Block, true, 0, 6), None),
        (base(CursorShape::Underline, true, 15, 6), None),
        (
            base(CursorShape::Block, true, 10, 11),
            selection(SelectionMode::Simple, line(1, 5), line(3, 12)),
        ),
        (
            base(CursorShape::Block, true, 10, 11),
            selection(SelectionMode::Line, line(2, 7), line(4, 3)),
        ),
        (
            base(CursorShape::Block, true, 10, 11),
            selection(SelectionMode::Block, line(5, 14), line(8, 3)),
        ),
        (
            base(CursorShape::Block, true, 10, 11),
            selection(SelectionMode::Simple, line(6, 0), line(7, 20)),
        ),
    ];
    for (cursor, selection) in variants {
        source.builder.absorb(&mut source.engine);
        let mut frame = source
            .builder
            .build(&source.engine, &mut source.client, FrameKind::Delta);
        frame.cursor = cursor;
        frame.selection = selection;
        frames.push(frame);
    }
    frames
        .iter()
        .enumerate()
        .map(|(tick, frame)| Record {
            tick: tick as u32,
            pane: 1,
            bytes: encode(frame),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use nh_proto::frame::FrameKind;

    fn params(scenario: Scenario) -> Params {
        Params {
            scenario,
            panes: 3,
            cols: 80,
            rows: 24,
            frames: 12,
            seed: 7,
        }
    }

    fn frames(container: &Container) -> Vec<Frame> {
        container
            .records
            .iter()
            .map(|r| postcard::from_bytes(&r.bytes).expect("a frame"))
            .collect()
    }

    #[test]
    fn the_same_arguments_give_the_same_bytes() {
        for scenario in [
            Scenario::Screen,
            Scenario::Redraw,
            Scenario::Spinner,
            Scenario::Scroll,
        ] {
            let p = params(scenario);
            assert_eq!(generate(&p).records, generate(&p).records, "{scenario:?}");
        }
        let mut other = params(Scenario::Spinner);
        other.seed = 8;
        assert_ne!(
            generate(&params(Scenario::Spinner)).records,
            generate(&other).records
        );
    }

    #[test]
    fn a_redraw_changes_every_row_of_every_pane_in_every_display_frame() {
        let container = generate(&params(Scenario::Redraw));
        assert_eq!(container.records.len(), 3 * 12);
        for (frame, record) in frames(&container).iter().zip(&container.records) {
            assert_eq!(frame.pane, record.pane);
            assert_eq!(
                frame.lines.len(),
                24,
                "tick {} pane {}",
                record.tick,
                record.pane
            );
        }
        assert_eq!(frames(&container)[0].kind, FrameKind::Keyframe);
        assert_eq!(frames(&container)[3].kind, FrameKind::Delta);
    }

    #[test]
    fn a_spinner_changes_two_rows() {
        let container = generate(&params(Scenario::Spinner));
        let f = frames(&container);
        for frame in &f[3..] {
            assert!(frame.lines.len() <= 3, "{} rows", frame.lines.len());
            assert_eq!(frame.kind, FrameKind::Delta);
        }
    }

    #[test]
    fn a_scroll_changes_the_whole_screen() {
        let container = generate(&params(Scenario::Scroll));
        for frame in &frames(&container)[3..] {
            assert_eq!(frame.lines.len(), 24);
        }
    }

    #[test]
    fn a_glyph_frame_shows_200_characters_that_the_frame_before_did_not() {
        let mut p = params(Scenario::Glyphs);
        p.panes = 1;
        let container = generate(&p);
        let screens: Vec<String> = container
            .records
            .iter()
            .map(|record| {
                let frame: Frame = postcard::from_bytes(&record.bytes).expect("a frame");
                assert_eq!(frame.lines.len(), 24);
                // The cells hold code points or grapheme ids; the distinct values are the glyphs.
                let mut glyphs: Vec<u32> = frame
                    .lines
                    .iter()
                    .flat_map(|line| {
                        line.cells
                            .chunks(6)
                            .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                    })
                    .collect();
                glyphs.sort_unstable();
                glyphs.dedup();
                assert_eq!(glyphs.len(), 200);
                format!("{glyphs:?}")
            })
            .collect();
        assert_ne!(screens[0], screens[1]);
    }

    #[test]
    fn the_fixture_has_a_keyframe_and_ten_variants_of_cursor_and_selection() {
        let mut p = params(Scenario::Screen);
        p.cols = 64;
        p.rows = 20;
        let container = generate(&p);
        assert_eq!(container.panes, 1);
        let f = frames(&container);
        assert_eq!(f.len(), 11);
        assert_eq!(f[0].kind, FrameKind::Keyframe);
        assert!(f[1..].iter().all(|frame| frame.kind == FrameKind::Delta));
        // The text does not change. A delta carries only the rows of the engine's cursor.
        assert!(f[1..].iter().all(|frame| frame.lines.len() <= 2));
        assert_eq!(f[1].cursor.shape, CursorShape::Underline);
        assert!(!f[4].cursor.visible);
        assert!(f[0].selection.is_none());
        assert!(f[7..].iter().all(|frame| frame.selection.is_some()));
    }
}
