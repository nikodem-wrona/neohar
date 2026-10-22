//! `nh-fixture tui`: a full-screen program for the chaos test.
//!
//! It stands in for the applications that neohar must bring back after a restart: it uses the
//! alternate screen, a scroll region, truecolor and 256-color backgrounds, box-drawing characters,
//! a title, reverse video and a cursor that stays where the program left it. It animates on a
//! timer, so output never stops. When the size of the terminal changes, it clears the screen and
//! draws everything again, as `vim` and `htop` do on `SIGWINCH`. It does not install a signal
//! handler. It reads the size of the terminal every 10 ms instead, which has the same effect and
//! needs no `unsafe` code.
//!
//! The drawing functions are pure: the same [`State`] gives the same bytes. [`run`] adds the
//! terminal, the timer and the input.

use std::{
    io::{self, Read, Write},
    sync::mpsc,
    time::{Duration, Instant},
};

use rustix::{
    stdio,
    termios::{OptionalActions, tcgetattr, tcgetwinsize, tcsetattr},
};

/// The smallest size that gets the whole layout. A smaller terminal gets a one-line message.
pub const MIN_SIZE: (u16, u16) = (24, 10);

/// How often the program reads the size of the terminal.
const POLL: Duration = Duration::from_millis(10);

/// The height of the log region at the bottom, in rows.
const LOG_ROWS: u16 = 3;

/// What the program shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct State {
    /// The size of the terminal, as `(cols, rows)`.
    pub size: (u16, u16),
    /// The number of timer ticks since the start.
    pub tick: u64,
    /// The bytes of the last input, shown on the status line.
    pub last_input: Vec<u8>,
}

/// The rows of the layout, for a size that is at least [`MIN_SIZE`]. All are 1-based.
struct Layout {
    cols: u16,
    /// The first and the last row of the body (the colored bands).
    body: (u16, u16),
    /// The separator above the log.
    separator: u16,
    /// The first and the last row of the log, which is the scroll region.
    log: (u16, u16),
    bottom_border: u16,
    status: u16,
}

impl Layout {
    fn new((cols, rows): (u16, u16)) -> Self {
        let status = rows;
        let bottom_border = rows - 1;
        let log_last = bottom_border - 1;
        let log_first = log_last + 1 - LOG_ROWS;
        let separator = log_first - 1;
        Self {
            cols,
            body: (3, separator - 1),
            separator,
            log: (log_first, log_last),
            bottom_border,
            status,
        }
    }
}

fn too_small(size: (u16, u16)) -> bool {
    size.0 < MIN_SIZE.0 || size.1 < MIN_SIZE.1
}

/// Moves the cursor to a 1-based row and column.
fn goto(out: &mut Vec<u8>, row: u16, col: u16) {
    out.extend_from_slice(format!("\x1b[{row};{col}H").as_bytes());
}

/// The color of band `band` of body row `y` after `tick` ticks. Every third band uses the 256-color
/// palette, and the others use truecolor.
fn band_sgr(band: u16, y: u16, tick: u64) -> String {
    // The bands move one step to the right every 10 ticks.
    let shift = u16::try_from(tick / 10 % 64).unwrap_or(0);
    let b = band.wrapping_add(shift);
    if band % 3 == 2 {
        format!(
            "\x1b[48;5;{}m",
            16 + ((b % 6) * 36 + y % 6 * 6 + band % 6) % 216
        )
    } else {
        let r = u8::try_from(b.wrapping_mul(23) % 200 + 30).unwrap_or(30);
        let g = u8::try_from(y.wrapping_mul(17) % 200 + 30).unwrap_or(30);
        let bl = u8::try_from((b + y).wrapping_mul(11) % 200 + 30).unwrap_or(30);
        format!("\x1b[48;2;{r};{g};{bl}m")
    }
}

/// Draws one body row between the side borders.
fn body_row(out: &mut Vec<u8>, layout: &Layout, y: u16, tick: u64) {
    goto(out, y, 2);
    let inner = layout.cols - 2;
    let mut x = 0;
    let mut band = 0;
    while x < inner {
        let width = 8.min(inner - x);
        out.extend_from_slice(band_sgr(band, y, tick).as_bytes());
        // A letter on some bands, so that text and color mix.
        if band % 4 == 1 {
            let letter = char::from(b'a' + u8::try_from((band + y) % 26).unwrap_or(0));
            out.extend_from_slice(format!("\x1b[1;97m{letter}").as_bytes());
            out.extend(std::iter::repeat_n(b' ', usize::from(width) - 1));
        } else {
            out.extend(std::iter::repeat_n(b' ', usize::from(width)));
        }
        out.extend_from_slice(b"\x1b[0m");
        x += width;
        band += 1;
    }
}

/// The status line: the tick, the size and the last input as hex, in reverse video, padded to the
/// width minus one column, so that the cursor never waits to wrap. The cursor stays at its end.
fn status_line(out: &mut Vec<u8>, state: &State, layout: &Layout) {
    let hex: Vec<String> = state
        .last_input
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let mut text = format!(
        " tick {:>6}  {}x{}  input [{}]",
        state.tick,
        state.size.0,
        state.size.1,
        hex.join(" ")
    );
    let width = usize::from(layout.cols) - 1;
    text.truncate(width);
    goto(out, layout.status, 1);
    out.extend_from_slice(format!("\x1b[7m{text:<width$}\x1b[0m").as_bytes());
    goto(out, layout.status, layout.cols);
}

/// The bytes that draw the whole screen: it clears it, sets the title, and draws the border, the
/// body, the log region and the status line. Used at the start and after each change of size.
pub fn full(state: &State) -> Vec<u8> {
    let mut out = Vec::new();
    // Reset the scroll region and the attributes, clear, and set the title.
    out.extend_from_slice(b"\x1b[0m\x1b[r\x1b[2J\x1b[H\x1b]0;nh-fixture tui\x07");
    if too_small(state.size) {
        out.extend_from_slice(b"too small");
        return out;
    }
    let layout = Layout::new(state.size);
    let inner = usize::from(layout.cols) - 2;
    let rule = "\u{2500}".repeat(inner);
    goto(&mut out, 1, 1);
    out.extend_from_slice(format!("\u{250c}{rule}\u{2510}").as_bytes());
    goto(&mut out, 2, 1);
    let title = format!("{:^inner$}", " nh-fixture tui ");
    out.extend_from_slice(format!("\u{2502}\x1b[1;7m{title}\x1b[0m\u{2502}").as_bytes());
    for y in layout.body.0..=layout.body.1 {
        goto(&mut out, y, 1);
        out.extend_from_slice("\u{2502}".as_bytes());
        body_row(&mut out, &layout, y, state.tick);
        goto(&mut out, y, layout.cols);
        out.extend_from_slice("\u{2502}".as_bytes());
    }
    goto(&mut out, layout.separator, 1);
    out.extend_from_slice(format!("\u{251c}{rule}\u{2524}").as_bytes());
    goto(&mut out, layout.bottom_border, 1);
    out.extend_from_slice(format!("\u{2514}{rule}\u{2518}").as_bytes());
    // The log region scrolls on its own: lines go in at the bottom of the scroll region.
    out.extend_from_slice(format!("\x1b[{};{}r", layout.log.0, layout.log.1).as_bytes());
    for line in 0..u64::from(LOG_ROWS) {
        goto(&mut out, layout.log.1, 1);
        out.extend_from_slice(
            format!("\r\n log {line:>6} (drawn at tick {})", state.tick).as_bytes(),
        );
    }
    status_line(&mut out, state, &layout);
    out
}

/// The bytes for one timer tick. `state.tick` is the number of the new tick. A tick changes the
/// spinner, the status line and, now and then, one body row and the log.
pub fn animate(state: &State) -> Vec<u8> {
    if too_small(state.size) {
        return Vec::new();
    }
    let layout = Layout::new(state.size);
    let mut out = Vec::new();
    let spinner = ['|', '/', '-', '\\'][usize::try_from(state.tick % 4).unwrap_or(0)];
    goto(&mut out, 2, 2);
    out.extend_from_slice(format!("\x1b[1;7m{spinner}\x1b[0m").as_bytes());
    if state.tick.is_multiple_of(10) {
        let rows = layout.body.1 - layout.body.0 + 1;
        let y = layout.body.0 + u16::try_from(state.tick / 10 % u64::from(rows)).unwrap_or(0);
        body_row(&mut out, &layout, y, state.tick);
    }
    if state.tick.is_multiple_of(5) {
        goto(&mut out, layout.log.1, 1);
        out.extend_from_slice(
            format!("\r\n log {:>6} (tick {})", state.tick / 5 + 3, state.tick).as_bytes(),
        );
    }
    status_line(&mut out, state, &layout);
    out
}

/// The bytes for input: only the status line changes.
pub fn input(state: &State) -> Vec<u8> {
    if too_small(state.size) {
        return Vec::new();
    }
    let layout = Layout::new(state.size);
    let mut out = Vec::new();
    status_line(&mut out, state, &layout);
    out
}

pub(crate) fn size_now() -> io::Result<(u16, u16)> {
    let size = tcgetwinsize(stdio::stdout())?;
    Ok((size.ws_col.max(1), size.ws_row.max(1)))
}

/// Puts the terminal of the program in raw mode and puts it back when dropped.
pub(crate) struct RawMode(rustix::termios::Termios);

impl RawMode {
    pub(crate) fn enter() -> io::Result<Self> {
        let saved = tcgetattr(stdio::stdin())?;
        let mut raw = saved.clone();
        raw.make_raw();
        tcsetattr(stdio::stdin(), OptionalActions::Now, &raw)?;
        Ok(Self(saved))
    }
}

impl Drop for RawMode {
    fn drop(&mut self) {
        let _ = tcsetattr(stdio::stdin(), OptionalActions::Now, &self.0);
    }
}

/// Runs the program until it reads a `q` or its input ends. A tick takes `interval`.
pub fn run(interval: Duration) -> io::Result<()> {
    let _raw = RawMode::enter()?;
    let mut out = io::stdout().lock();
    out.write_all(b"\x1b[?1049h")?;
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    std::thread::spawn(move || {
        let mut stdin = io::stdin().lock();
        let mut buf = [0u8; 256];
        while let Ok(n) = stdin.read(&mut buf) {
            if n == 0 || tx.send(buf[..n].to_vec()).is_err() {
                return;
            }
        }
    });

    let mut state = State {
        size: size_now()?,
        tick: 0,
        last_input: Vec::new(),
    };
    out.write_all(&full(&state))?;
    out.flush()?;
    let mut next_tick = Instant::now() + interval;
    loop {
        let wait = next_tick
            .saturating_duration_since(Instant::now())
            .min(POLL);
        match rx.recv_timeout(wait) {
            Ok(bytes) => {
                if bytes.contains(&b'q') {
                    break;
                }
                state.last_input = bytes;
                out.write_all(&input(&state))?;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
        let size = size_now()?;
        if size != state.size {
            state.size = size;
            out.write_all(&full(&state))?;
        }
        if Instant::now() >= next_tick {
            state.tick += 1;
            next_tick += interval;
            out.write_all(&animate(&state))?;
        }
        out.flush()?;
    }
    out.write_all(b"\x1b[?1049l")?;
    out.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(cols: u16, rows: u16, tick: u64) -> State {
        State {
            size: (cols, rows),
            tick,
            last_input: Vec::new(),
        }
    }

    #[test]
    fn the_same_state_gives_the_same_bytes() {
        let s = state(80, 24, 12);
        assert_eq!(full(&s), full(&s));
        assert_eq!(animate(&s), animate(&s));
    }

    #[test]
    fn a_full_draw_clears_sets_the_title_and_the_scroll_region() {
        let bytes = String::from_utf8(full(&state(80, 24, 0))).expect("the output is text");
        assert!(bytes.starts_with("\x1b[0m\x1b[r\x1b[2J\x1b[H\x1b]0;nh-fixture tui\x07"));
        // Of 24 rows: the status is 24, the border 23, so the log is rows 20 to 22.
        assert!(bytes.contains("\x1b[20;22r"));
        assert!(bytes.contains("\u{250c}") && bytes.contains("\u{2518}"));
        assert!(bytes.contains("38;2;") || bytes.contains("48;2;"));
        assert!(bytes.contains("48;5;"));
    }

    #[test]
    fn a_terminal_that_is_too_small_gets_a_message() {
        let bytes = full(&state(10, 5, 0));
        assert!(bytes.ends_with(b"too small"));
        assert!(animate(&state(10, 5, 1)).is_empty());
    }

    #[test]
    fn the_layout_fits_every_size_from_the_minimum_up() {
        for cols in MIN_SIZE.0..=MIN_SIZE.0 + 40 {
            for rows in MIN_SIZE.1..=MIN_SIZE.1 + 40 {
                let l = Layout::new((cols, rows));
                assert!(l.body.0 <= l.body.1, "{cols}x{rows}");
                assert!(
                    l.body.1 < l.separator && l.separator < l.log.0,
                    "{cols}x{rows}"
                );
                assert!(l.log.1 < l.bottom_border && l.bottom_border < l.status);
                assert_eq!(l.status, rows);
            }
        }
    }

    #[test]
    fn the_status_line_shows_the_tick_the_size_and_the_input() {
        let mut s = state(80, 24, 7);
        s.last_input = vec![0x1b, b'[', b'A'];
        let text = String::from_utf8(input(&s)).expect("the output is text");
        assert!(text.contains("tick      7"), "{text:?}");
        assert!(text.contains("80x24"));
        assert!(text.contains("[1b 5b 41]"));
    }

    #[test]
    fn a_tick_scrolls_the_log_every_fifth_tick_and_moves_a_body_row_every_tenth() {
        let log = |tick| {
            animate(&state(80, 24, tick))
                .windows(4)
                .any(|w| w == b"\r\n l")
        };
        assert!(log(5) && log(10));
        assert!(!log(3));
        let rows_drawn = |tick| {
            animate(&state(80, 24, tick))
                .windows(7)
                .filter(|w| *w == b"\x1b[48;2;")
                .count()
        };
        assert!(rows_drawn(10) > 0);
        assert_eq!(rows_drawn(3), 0);
    }
}
