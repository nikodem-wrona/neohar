//! What the detector reads: some rows of a screen with their styles.
//!
//! The core copies the rows that the rules cover out of the engine of the pane. The detector does
//! not know the engine. A test builds a screen from text.

use nh_proto::style::Style;

/// One cell: its character and its style. The second column of a wide glyph holds a space.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    pub ch: char,
    pub style: Style,
}

/// Some rows of the visible screen of a pane.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Screen {
    /// How many rows the whole screen has.
    pub total_rows: u16,
    /// The number of the first row in `rows`.
    pub first_row: u16,
    pub rows: Vec<Vec<Cell>>,
    /// The cursor as `(row, column)` of the whole screen.
    pub cursor: (u16, u16),
    /// The alternate screen is on show.
    pub alt_screen: bool,
    /// The title that the program set.
    pub title: Option<String>,
}

impl Screen {
    /// A screen of plain text, one string for each row, with the default style. For tests and for
    /// the screens that a recording saved as text.
    pub fn from_text(lines: &[&str]) -> Self {
        let rows = lines
            .iter()
            .map(|line| {
                line.chars()
                    .map(|ch| Cell {
                        ch,
                        style: Style::default(),
                    })
                    .collect()
            })
            .collect();
        Self {
            total_rows: u16::try_from(lines.len()).unwrap_or(u16::MAX),
            first_row: 0,
            rows,
            cursor: (0, 0),
            alt_screen: false,
            title: None,
        }
    }

    /// The same screen on the alternate screen, with a title.
    pub fn with_title(mut self, title: &str) -> Self {
        self.title = Some(title.to_owned());
        self
    }

    /// The same screen with the alternate screen on show.
    pub fn on_alt_screen(mut self) -> Self {
        self.alt_screen = true;
        self
    }

    /// The same screen with the cursor at `(row, column)`.
    pub fn with_cursor(mut self, row: u16, column: u16) -> Self {
        self.cursor = (row, column);
        self
    }

    /// The cells of row `y` of the whole screen, when this screen holds it.
    pub fn row(&self, y: u16) -> Option<&[Cell]> {
        let at = usize::from(y.checked_sub(self.first_row)?);
        self.rows.get(at).map(Vec::as_slice)
    }

    /// The text of row `y`, without the blank cells at the end.
    pub fn row_text(&self, y: u16) -> String {
        let Some(cells) = self.row(y) else {
            return String::new();
        };
        let text: String = cells.iter().map(|c| c.ch).collect();
        text.trim_end().to_owned()
    }
}
