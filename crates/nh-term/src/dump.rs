//! A dump of what an engine shows (`debug.screen`).
//!
//! The chaos test compares the screen of a restored pane with the screen of an engine that read the
//! whole journal. Both sides make a [`ScreenDump`] and compare the two values. The dump holds what
//! a person can see and what the next byte of output depends on: the size, the cells with their
//! styles, the cursor, the modes, the title and the colours that a program changed.

use nh_proto::{
    frame::{Cursor, Palette},
    modes::ModeFlags,
    style::{CellWidth, Style},
};
use serde::{Deserialize, Serialize};

use crate::VtEngine;

/// One cell of a row.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DumpCell {
    pub ch: char,
    /// The zero-width characters that combine with `ch`.
    pub zerowidth: Vec<char>,
    pub style: Style,
}

/// Equal cells next to each other in a row.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DumpRun {
    pub count: u16,
    pub cell: DumpCell,
}

/// The visible screen of an engine.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScreenDump {
    pub cols: u16,
    pub rows: u16,
    pub cursor: Cursor,
    pub modes: ModeFlags,
    pub title: Option<String>,
    /// The palette entries that differ from the theme: the index (256 is the foreground colour, 257
    /// the background colour, 258 the cursor colour) and the colour.
    pub palette_overrides: Vec<(u16, u32)>,
    /// One list of runs for each row, top to bottom.
    pub lines: Vec<Vec<DumpRun>>,
}

/// What differs between two dumps.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DumpDiff {
    /// How many items differ: one for the size, the cursor, and so on, and one for each cell.
    pub count: usize,
    /// A description of the first differences.
    pub samples: Vec<String>,
}

impl DumpDiff {
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    fn add(&mut self, limit: usize, what: impl FnOnce() -> String) {
        self.count += 1;
        if self.samples.len() < limit {
            self.samples.push(what());
        }
    }
}

impl ScreenDump {
    /// Reads the screen of `engine`. `theme` is the palette that the engine started from.
    pub fn of(engine: &dyn VtEngine, theme: &Palette) -> Self {
        let (cols, rows) = engine.size();
        let lines = (0..rows)
            .map(|y| {
                let mut runs: Vec<DumpRun> = Vec::new();
                engine.visit_row(y, &mut |view| {
                    let cell = DumpCell {
                        ch: view.ch,
                        zerowidth: view.zerowidth.to_vec(),
                        style: view.style,
                    };
                    match runs.last_mut() {
                        Some(run) if run.cell == cell => run.count += 1,
                        _ => runs.push(DumpRun { count: 1, cell }),
                    }
                });
                runs
            })
            .collect();
        let palette = engine.palette();
        let mut palette_overrides: Vec<(u16, u32)> = Vec::new();
        for (i, (now, base)) in palette.colors.iter().zip(&theme.colors).enumerate() {
            if now != base {
                palette_overrides.push((i as u16, *now));
            }
        }
        for (i, now, base) in [
            (256, palette.fg, theme.fg),
            (257, palette.bg, theme.bg),
            (258, palette.cursor, theme.cursor),
        ] {
            if now != base {
                palette_overrides.push((i, now));
            }
        }
        Self {
            cols,
            rows,
            cursor: engine.cursor(),
            modes: engine.modes(),
            title: engine.title().map(str::to_owned),
            palette_overrides,
            lines,
        }
    }

    /// The text of each row, without the blank cells at the end. The second cell of a wide
    /// character has no text of its own.
    pub fn text(&self) -> Vec<String> {
        self.lines
            .iter()
            .map(|runs| {
                let mut line = String::new();
                for run in runs {
                    if run.cell.style.width == CellWidth::Spacer {
                        continue;
                    }
                    for _ in 0..run.count {
                        line.push(run.cell.ch);
                        line.extend(&run.cell.zerowidth);
                    }
                }
                line.trim_end().to_owned()
            })
            .collect()
    }

    /// The cell at column `x` of row `y`.
    pub fn cell(&self, x: u16, y: u16) -> Option<&DumpCell> {
        let mut left = x;
        for run in self.lines.get(usize::from(y))? {
            if left < run.count {
                return Some(&run.cell);
            }
            left -= run.count;
        }
        None
    }

    /// Compares this dump with `other`. `samples` holds at most `limit` descriptions.
    pub fn differences(&self, other: &Self, limit: usize) -> DumpDiff {
        let mut diff = DumpDiff::default();
        if (self.cols, self.rows) != (other.cols, other.rows) {
            diff.add(limit, || {
                format!(
                    "size {}x{} against {}x{}",
                    self.cols, self.rows, other.cols, other.rows
                )
            });
            // Cells of grids with another size do not line up.
            return diff;
        }
        if self.cursor != other.cursor {
            diff.add(limit, || {
                format!("cursor {:?} against {:?}", self.cursor, other.cursor)
            });
        }
        if self.modes != other.modes {
            diff.add(limit, || {
                format!("modes {:?} against {:?}", self.modes, other.modes)
            });
        }
        if self.title != other.title {
            diff.add(limit, || {
                format!("title {:?} against {:?}", self.title, other.title)
            });
        }
        if self.palette_overrides != other.palette_overrides {
            diff.add(limit, || "palette overrides".to_owned());
        }
        for y in 0..self.rows {
            if self.lines.get(usize::from(y)) == other.lines.get(usize::from(y)) {
                continue;
            }
            for x in 0..self.cols {
                let (a, b) = (self.cell(x, y), other.cell(x, y));
                if a != b {
                    diff.add(limit, || format!("cell ({x}, {y}): {a:?} against {b:?}"));
                }
            }
        }
        diff
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AlacrittyEngine, MIN_HISTORY, Replaying, xterm_palette};

    fn engine(bytes: &[u8]) -> AlacrittyEngine {
        let mut engine = AlacrittyEngine::new(20, 4, MIN_HISTORY, xterm_palette());
        engine.advance(bytes, &mut Replaying::default());
        engine
    }

    fn dump(bytes: &[u8]) -> ScreenDump {
        ScreenDump::of(&engine(bytes), &xterm_palette())
    }

    #[test]
    fn the_text_of_a_dump_is_the_text_of_the_screen() {
        let dump = dump(b"one\r\ntwo");
        assert_eq!(dump.text(), ["one", "two", "", ""]);
    }

    #[test]
    fn a_wide_character_is_one_character_of_the_text() {
        let dump = dump("a日本é\r\nb".as_bytes());
        assert_eq!(dump.text(), ["a日本é", "b", "", ""]);
    }

    #[test]
    fn equal_engines_give_equal_dumps_and_no_differences() {
        let a = dump(b"\x1b[31mred\x1b[m plain\x1b]0;title\x07");
        let b = dump(b"\x1b[31mred\x1b[m plain\x1b]0;title\x07");
        assert_eq!(a, b);
        assert!(a.differences(&b, 8).is_empty());
        assert_eq!(a.title.as_deref(), Some("title"));
    }

    #[test]
    fn a_changed_style_is_a_difference_at_that_cell() {
        let a = dump(b"ab");
        let b = dump(b"a\x1b[1mb");
        let diff = a.differences(&b, 8);
        assert_eq!(diff.count, 1);
        assert!(diff.samples[0].starts_with("cell (1, 0)"), "{diff:?}");
    }

    #[test]
    fn the_samples_are_limited_and_the_count_is_not() {
        let a = dump(b"aaaaaaaaaa");
        let b = dump(b"bbbbbbbbbb");
        let diff = a.differences(&b, 3);
        assert_eq!(diff.count, 10);
        assert_eq!(diff.samples.len(), 3);
    }

    #[test]
    fn a_cursor_move_a_mode_and_a_color_change_show() {
        let base = dump(b"x");
        assert_eq!(base.differences(&dump(b"x\x1b[2;3H"), 4).count, 1);
        assert_eq!(base.differences(&dump(b"x\x1b[?2004h"), 4).count, 1);
        let recolored = dump(b"x\x1b]4;1;rgb:12/34/56\x07");
        assert_eq!(recolored.palette_overrides.len(), 1);
        assert_eq!(base.differences(&recolored, 4).count, 1);
    }

    #[test]
    fn grids_of_another_size_are_one_difference() {
        let a = dump(b"x");
        let mut small = AlacrittyEngine::new(10, 4, MIN_HISTORY, xterm_palette());
        small.advance(b"x", &mut Replaying::default());
        let b = ScreenDump::of(&small, &xterm_palette());
        assert_eq!(a.differences(&b, 4).count, 1);
    }

    #[test]
    fn a_dump_survives_json() {
        let a = dump(b"\x1b[38;2;1;2;3mrgb \x1b[4:3mcurly\x1b[m \xc3\xa9\xcc\x81");
        let json = serde_json::to_string(&a).expect("the dump serializes");
        let back: ScreenDump = serde_json::from_str(&json).expect("the dump reads back");
        assert_eq!(a, back);
    }
}
