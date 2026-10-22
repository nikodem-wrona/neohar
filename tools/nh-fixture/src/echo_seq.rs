//! `nh-fixture echo-seq`: a program that logs every byte it reads, for the chaos test.
//!
//! The terminal is in raw mode. Each input byte gets a sequence number and goes to the log file as
//! one line, `<seq> <hex>`. The test reads the file, joins the bytes in the order of the numbers,
//! and looks for the payload of every input id. An input that was applied twice shows twice.
//!
//! By default the program shows a status screen that it draws again when the size changes, like a
//! full-screen application. `--plain` shows the bytes as hex in a stream that scrolls, like a shell
//! or a build log.

use std::{
    fs::OpenOptions,
    io::{self, Read, Write},
    path::Path,
    sync::mpsc,
    time::Duration,
};

use crate::tui::{RawMode, size_now};

/// How often the program reads the size of the terminal.
const POLL: Duration = Duration::from_millis(10);

/// The lines of the log for `bytes`, which are the bytes with the numbers `first` and up.
pub fn log_lines(first: u64, bytes: &[u8]) -> String {
    bytes
        .iter()
        .enumerate()
        .map(|(i, b)| format!("{} {b:02x}\n", first + i as u64))
        .collect()
}

/// Reads the bytes of a log that [`log_lines`] wrote, in the order of their numbers.
pub fn read_log(text: &str) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    for (expected, line) in text.lines().enumerate() {
        let (seq, hex) = line
            .split_once(' ')
            .ok_or_else(|| format!("line {expected}: no space"))?;
        if seq.parse::<usize>() != Ok(expected) {
            return Err(format!("line {expected}: the number is {seq}"));
        }
        bytes.push(u8::from_str_radix(hex, 16).map_err(|e| format!("line {expected}: {e}"))?);
    }
    Ok(bytes)
}

/// What the status screen shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Status {
    /// The size of the terminal, as `(cols, rows)`.
    pub size: (u16, u16),
    /// How many bytes the program has read.
    pub bytes: u64,
    /// The last bytes that it read, at most [`LAST`].
    pub last: Vec<u8>,
    /// The path of the log, for the title lines.
    pub log: String,
}

/// How many of the last bytes the status screen shows.
pub const LAST: usize = 24;

/// The bytes that draw the whole status screen: clear, title, and four lines. Used at the start,
/// after each input and after each change of size. The program redraws all of it each time, as an
/// application that handles `SIGWINCH` does.
pub fn draw(status: &Status) -> Vec<u8> {
    let (cols, _) = status.size;
    let width = usize::from(cols.max(1));
    let fit = |text: String| -> String { text.chars().take(width).collect() };
    let hex: Vec<String> = status.last.iter().map(|b| format!("{b:02x}")).collect();
    let mut out = Vec::new();
    out.extend_from_slice(b"\x1b[0m\x1b[2J\x1b[H\x1b]0;nh-fixture echo-seq\x07");
    out.extend_from_slice(
        format!("\x1b[1;7m{:<width$}\x1b[0m", fit(" echo-seq ".into())).as_bytes(),
    );
    for (row, text) in [
        (3, format!("log: {}", status.log)),
        (4, format!("bytes read: {}", status.bytes)),
        (5, format!("last input: {}", hex.join(" "))),
        (6, format!("size: {}x{}", status.size.0, status.size.1)),
    ] {
        out.extend_from_slice(format!("\x1b[{row};1H{}", fit(text)).as_bytes());
    }
    out.extend_from_slice(b"\x1b[8;1H");
    out
}

/// Runs until the input ends.
///
/// Without `plain`, the program draws a status screen on the alternate screen and draws it again
/// when the size changes. With `plain`, it only writes the bytes as hex to the screen, which
/// scrolls and wraps. A program like that does not redraw, and a restart cannot bring it back
/// exactly.
pub fn run(log: &Path, plain: bool) -> io::Result<()> {
    let _raw = RawMode::enter()?;
    let mut file = OpenOptions::new().create(true).append(true).open(log)?;
    let mut out = io::stdout().lock();
    if plain {
        out.write_all(b"echo-seq ready\r\n")?;
    } else {
        out.write_all(b"\x1b[?1049h")?;
    }
    out.flush()?;
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    std::thread::spawn(move || {
        let mut stdin = io::stdin().lock();
        let mut buf = [0u8; 1024];
        while let Ok(n) = stdin.read(&mut buf) {
            if n == 0 || tx.send(buf[..n].to_vec()).is_err() {
                return;
            }
        }
    });
    let mut status = Status {
        size: size_now()?,
        bytes: 0,
        last: Vec::new(),
        log: log.display().to_string(),
    };
    if !plain {
        out.write_all(&draw(&status))?;
        out.flush()?;
    }
    loop {
        match rx.recv_timeout(POLL) {
            Ok(chunk) => {
                // One write for the lines of a read: the file never holds half a line of a chunk
                // that was read whole.
                file.write_all(log_lines(status.bytes, &chunk).as_bytes())?;
                status.bytes += chunk.len() as u64;
                if plain {
                    for byte in &chunk {
                        write!(out, "{byte:02x} ")?;
                    }
                } else {
                    status.last.extend_from_slice(&chunk);
                    let excess = status.last.len().saturating_sub(LAST);
                    status.last.drain(..excess);
                    out.write_all(&draw(&status))?;
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                let size = size_now()?;
                if !plain && size != status.size {
                    status.size = size;
                    out.write_all(&draw(&status))?;
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(()),
        }
        out.flush()?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_lines_have_a_number_and_a_hex_byte() {
        assert_eq!(log_lines(0, b"a\n"), "0 61\n1 0a\n");
        assert_eq!(log_lines(10, &[0xff]), "10 ff\n");
    }

    #[test]
    fn a_log_reads_back_as_the_bytes_that_were_written() {
        let mut text = log_lines(0, b"<0000001>");
        text.push_str(&log_lines(9, b"<0000002>"));
        assert_eq!(read_log(&text).expect("a good log"), b"<0000001><0000002>");
    }

    fn status(cols: u16, bytes: u64, last: &[u8]) -> Status {
        Status {
            size: (cols, 24),
            bytes,
            last: last.to_vec(),
            log: "/tmp/echo.log".to_owned(),
        }
    }

    #[test]
    fn the_status_screen_shows_the_counts_and_the_last_bytes() {
        let text = String::from_utf8(draw(&status(80, 12, b"<1>"))).expect("text");
        assert!(text.starts_with("\x1b[0m\x1b[2J\x1b[H"));
        assert!(text.contains("bytes read: 12"));
        assert!(text.contains("last input: 3c 31 3e"));
        assert!(text.contains("size: 80x24") && text.contains("log: /tmp/echo.log"));
    }

    #[test]
    fn the_status_screen_fits_a_narrow_terminal() {
        let text = String::from_utf8(draw(&status(10, 12, &[0xab; 24]))).expect("text");
        for line in text.split("\x1b[").filter(|l| l.contains("last input")) {
            let shown = line.split_once('H').map_or("", |(_, t)| t);
            assert!(shown.chars().count() <= 10, "{shown:?}");
        }
    }

    #[test]
    fn a_log_with_a_gap_or_a_repeat_is_refused() {
        assert!(read_log("0 61\n2 62\n").is_err());
        assert!(read_log("0 61\n0 61\n").is_err());
        assert!(read_log("0 zz\n").is_err());
        assert!(read_log("0\n").is_err());
    }
}
