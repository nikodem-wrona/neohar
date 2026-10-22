//! A reference implementation of the client side of frames, for tests.
//!
//! The real client is the WASM decoder and the renderer. This one is plain and slow. It applies
//! frames, and it can show the screen it has built, to compare with the engine's screen.

use std::collections::HashMap;

use nh_proto::{
    cells::{GRAPHEME_FLAG, unpack_row},
    frame::{Cursor, Frame, FrameKind, Palette},
    modes::ModeFlags,
    style::Style,
};

use crate::engine::VtEngine;

/// Why a frame does not apply.
#[derive(Debug, PartialEq, Eq)]
pub enum ApplyError {
    /// The delta is for another revision or another epoch. The core must send a keyframe.
    Stale {
        have_epoch: u32,
        have_rev: u64,
        base_rev: u64,
        epoch: u32,
    },
    /// A keyframe lacks a row.
    MissingRow(u16),
    /// A row has the wrong number of cells.
    BadRow(u16),
    UnknownStyle(u16),
    UnknownGrapheme(u32),
}

/// The visible screen, resolved to text and styles, so that two screens compare by value.
#[derive(Debug, PartialEq, Eq)]
pub struct ScreenDump {
    pub rows: Vec<Vec<(String, Style)>>,
    pub cursor: Cursor,
    pub modes: ModeFlags,
    pub title: Option<String>,
    pub palette: Palette,
}

impl ScreenDump {
    /// Describes the first difference from `other`, or `None` when the screens are equal.
    pub fn diff(&self, other: &Self) -> Option<String> {
        if self.rows.len() != other.rows.len() {
            return Some(format!(
                "{} rows against {}",
                self.rows.len(),
                other.rows.len()
            ));
        }
        for (y, (a, b)) in self.rows.iter().zip(&other.rows).enumerate() {
            if a.len() != b.len() {
                return Some(format!("row {y}: {} cells against {}", a.len(), b.len()));
            }
            if let Some(x) = (0..a.len()).find(|&x| a[x] != b[x]) {
                return Some(format!("cell ({x},{y}): {:?} against {:?}", a[x], b[x]));
            }
        }
        if self.cursor != other.cursor {
            return Some(format!(
                "cursor {:?} against {:?}",
                self.cursor, other.cursor
            ));
        }
        if self.modes != other.modes {
            return Some(format!("modes {:?} against {:?}", self.modes, other.modes));
        }
        if self.title != other.title {
            return Some(format!("title {:?} against {:?}", self.title, other.title));
        }
        if self.palette != other.palette {
            let i = (0..256).find(|&i| self.palette.colors[i] != other.palette.colors[i]);
            return Some(format!("palette differs at {i:?}"));
        }
        None
    }
}

/// The screen of a client.
#[derive(Default)]
pub struct ClientGrid {
    epoch: u32,
    rev: u64,
    synced: bool,
    size: (u16, u16),
    /// `(glyph, style id)` for each cell, row by row.
    cells: Vec<(u32, u16)>,
    styles: HashMap<u16, Style>,
    graphemes: HashMap<u32, String>,
    cursor: Cursor,
    modes: ModeFlags,
    title: Option<String>,
    palette: Option<Palette>,
}

impl ClientGrid {
    pub fn new() -> Self {
        Self::default()
    }

    /// The epoch of the last frame.
    pub fn epoch(&self) -> u32 {
        self.epoch
    }

    /// Applies a frame.
    pub fn apply(&mut self, frame: &Frame) -> Result<(), ApplyError> {
        let keyframe = frame.kind == FrameKind::Keyframe;
        if !keyframe
            && (!self.synced || self.epoch != frame.reset_epoch || self.rev != frame.base_rev)
        {
            return Err(ApplyError::Stale {
                have_epoch: self.epoch,
                have_rev: self.rev,
                base_rev: frame.base_rev,
                epoch: frame.reset_epoch,
            });
        }
        if keyframe && (!self.synced || self.epoch != frame.reset_epoch) {
            self.styles.clear();
            self.graphemes.clear();
        }
        let (cols, rows) = (usize::from(frame.cols), usize::from(frame.rows));
        if keyframe {
            self.size = (frame.cols, frame.rows);
            self.cells = vec![(u32::from(b' '), 0); cols * rows];
            if let Some(missing) = (0..frame.rows).find(|y| !frame.lines.iter().any(|l| l.y == *y))
            {
                return Err(ApplyError::MissingRow(missing));
            }
        }
        for (id, style) in &frame.new_styles {
            self.styles.insert(*id, *style);
        }
        for (id, text) in &frame.new_graphemes {
            self.graphemes.insert(*id, text.clone());
        }
        for line in &frame.lines {
            let y = usize::from(line.y);
            let row = unpack_row(&line.cells).collect::<Vec<_>>();
            if y >= rows || row.len() != cols {
                return Err(ApplyError::BadRow(line.y));
            }
            self.cells[y * cols..(y + 1) * cols].copy_from_slice(&row);
        }
        self.cursor = frame.cursor;
        self.modes = frame.modes;
        match (&frame.title, keyframe) {
            (title, true) => self.title = title.clone(),
            (Some(title), false) => self.title = (!title.is_empty()).then(|| title.clone()),
            (None, false) => {}
        }
        if frame.palette.is_some() {
            self.palette.clone_from(&frame.palette);
        }
        self.epoch = frame.reset_epoch;
        self.rev = frame.rev;
        self.synced = true;
        Ok(())
    }

    /// The screen that this client shows.
    pub fn dump(&self) -> Result<ScreenDump, ApplyError> {
        let cols = usize::from(self.size.0);
        let mut rows = Vec::new();
        for chunk in self.cells.chunks(cols.max(1)) {
            let mut row = Vec::with_capacity(cols);
            for &(glyph, style_id) in chunk {
                let text = if glyph & GRAPHEME_FLAG != 0 {
                    let id = glyph & !GRAPHEME_FLAG;
                    self.graphemes
                        .get(&id)
                        .cloned()
                        .ok_or(ApplyError::UnknownGrapheme(id))?
                } else {
                    char::from_u32(glyph).map(String::from).unwrap_or_default()
                };
                let style = if style_id == 0 {
                    Style::default()
                } else {
                    *self
                        .styles
                        .get(&style_id)
                        .ok_or(ApplyError::UnknownStyle(style_id))?
                };
                row.push((text, style));
            }
            rows.push(row);
        }
        Ok(ScreenDump {
            rows,
            cursor: self.cursor,
            modes: self.modes,
            title: self.title.clone(),
            palette: self.palette.clone().unwrap_or_else(crate::xterm_palette),
        })
    }
}

/// The screen that an engine shows, resolved the same way.
pub fn dump_engine(engine: &dyn VtEngine) -> ScreenDump {
    let (_, rows) = engine.size();
    let mut out = Vec::new();
    for y in 0..rows {
        let mut row = Vec::new();
        engine.visit_row(y, &mut |cell| {
            let mut text = String::from(cell.ch);
            text.extend(cell.zerowidth);
            row.push((text, cell.style));
        });
        out.push(row);
    }
    ScreenDump {
        rows: out,
        cursor: engine.cursor(),
        modes: engine.modes(),
        title: engine.title().map(str::to_owned),
        palette: engine.palette(),
    }
}
