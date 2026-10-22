//! Decoding frames into per-pane typed arrays.
//!
//! `decode` reads a frame once, with `serde` visitors that copy nothing: they note where the cells
//! and strings of the frame lie in the input, and collect the small parts in buffers that are
//! reused. The frame lists its rows before its styles, but the cells need the styles, so the cells
//! are written after the whole frame is read. Nothing in the decoder changes until the whole frame
//! is read and checked. A frame that does not decode, or does not follow the state of its pane,
//! leaves everything as it was.
//!
//! After the first frames of a pane, `decode` allocates nothing for frames of the same size that
//! bring no new styles or graphemes.

use std::{collections::HashMap, fmt};

use nh_proto::{
    cells::{GRAPHEME_FLAG, unpack_row},
    frame::{AckState, Cursor, FrameKind, Palette, Placement, Selection, Viewport},
    modes::ModeFlags,
    style::{Attrs, CellWidth, Color, Style},
};
use serde::{
    Deserialize,
    de::{DeserializeSeed, Deserializer, Error as DeError, SeqAccess, Visitor},
};

use crate::atlas_map::{AtlasMap, Pending};

/// `decode` applied the frame.
pub const STATUS_APPLIED: u32 = 0;
/// The frame does not follow the state of its pane. The UI asks for a keyframe.
pub const STATUS_NEEDS_KEYFRAME: u32 = 1;
/// The bytes are not a frame.
pub const STATUS_ERROR: u32 = 2;

/// In a cell: the cell is the first column of a wide glyph.
pub const FLAG_WIDE: u32 = 1 << 24;
/// In a cell: the cell is the second column of a wide glyph. It draws nothing.
pub const FLAG_SPACER: u32 = 1 << 25;
/// In a cell: the cursor is on this cell.
pub const FLAG_CURSOR: u32 = 1 << 26;
/// The atlas slot is in the low 24 bits of a cell.
pub const SLOT_MASK: u32 = 0x00ff_ffff;

/// The number of fields of a frame on the wire.
const FRAME_FIELDS: &[&str] = &[
    "pane",
    "reset_epoch",
    "base_rev",
    "rev",
    "kind",
    "cols",
    "rows",
    "lines",
    "new_styles",
    "new_graphemes",
    "cursor",
    "viewport",
    "modes",
    "title",
    "bell",
    "images",
    "selection",
    "ack_status",
    "palette",
];

/// The first fields of a frame.
#[derive(Clone, Copy, Default)]
struct Header {
    pane: u32,
    epoch: u32,
    rev: u64,
    keyframe: bool,
    cols: u16,
    rows: u16,
}

/// A place in the input: the offset of a byte string, and its length.
#[derive(Clone, Copy, Default)]
struct Span {
    start: usize,
    len: usize,
}

impl Span {
    fn of(bytes: &[u8], base: usize) -> Self {
        Self {
            start: (bytes.as_ptr() as usize).wrapping_sub(base),
            len: bytes.len(),
        }
    }

    fn slice<'a>(&self, input: &'a [u8]) -> &'a [u8] {
        &input[self.start..self.start + self.len]
    }
}

/// What the visitors collect. Its buffers keep their capacity between frames.
#[derive(Default)]
struct Scratch {
    header: Header,
    rows: Vec<(u16, Span)>,
    styles: Vec<(u16, Style)>,
    graphemes: Vec<(u32, Span)>,
    cursor: Cursor,
    viewport: Viewport,
    modes: ModeFlags,
    title: Option<Span>,
    bell: bool,
    images: Vec<Placement>,
    selection: Option<Selection>,
    acks: Vec<(u64, AckState)>,
    palette: Option<Palette>,
    needs_keyframe: bool,
}

/// The decoded state of one pane.
pub struct Pane {
    pub epoch: u32,
    pub rev: u64,
    synced: bool,
    pub cols: u16,
    pub rows: u16,
    /// For each cell: the atlas slot in the low 24 bits, and the flags above them.
    pub cells: Vec<u32>,
    /// For each cell: the glyph, as a code point or as a grapheme id with the high bit set.
    pub glyphs: Vec<u32>,
    /// For each cell: the style id.
    pub styles: Vec<u16>,
    /// For each style id: foreground, background and underline colour as `0xRRGGBBAA`, and the
    /// attribute bits. A style with inverse video already has its colours swapped.
    pub style_table: Vec<[u32; 4]>,
    raw_styles: Vec<Style>,
    pub style_table_dirty: bool,
    pub palette: Palette,
    pub cursor: Cursor,
    pub modes: ModeFlags,
    pub viewport: Viewport,
    pub selection: Option<Selection>,
    pub title: Vec<u8>,
    pub bell: bool,
    pub acks: Vec<[u32; 3]>,
    graphemes: HashMap<u32, Box<[u8]>>,
}

impl Pane {
    fn new() -> Self {
        Self {
            epoch: 0,
            rev: 0,
            synced: false,
            cols: 0,
            rows: 0,
            cells: Vec::new(),
            glyphs: Vec::new(),
            styles: Vec::new(),
            style_table: Vec::new(),
            raw_styles: Vec::new(),
            style_table_dirty: false,
            palette: Palette {
                colors: [0; 256],
                fg: 0x00e5_e5e5,
                bg: 0,
                cursor: 0x00e5_e5e5,
            },
            cursor: Cursor::default(),
            modes: ModeFlags::empty(),
            viewport: Viewport::default(),
            selection: None,
            title: Vec::new(),
            bell: false,
            acks: Vec::new(),
            graphemes: HashMap::new(),
        }
    }

    /// The text of a grapheme id, as UTF-8.
    pub fn grapheme(&self, id: u32) -> Option<&[u8]> {
        self.graphemes.get(&id).map(|b| &**b)
    }

    /// The text of the cell at `(x, y)`, for tests and tools.
    pub fn text_at(&self, x: u16, y: u16) -> String {
        let glyph = self.glyphs[usize::from(y) * usize::from(self.cols) + usize::from(x)];
        if glyph & GRAPHEME_FLAG != 0 {
            let id = glyph & !GRAPHEME_FLAG;
            self.grapheme(id)
                .map(|b| String::from_utf8_lossy(b).into_owned())
                .unwrap_or_default()
        } else {
            char::from_u32(glyph).map(String::from).unwrap_or_default()
        }
    }

    /// The style of the cell at `(x, y)`.
    pub fn style_at(&self, x: u16, y: u16) -> Style {
        let id = self.styles[usize::from(y) * usize::from(self.cols) + usize::from(x)];
        self.raw_styles
            .get(usize::from(id))
            .copied()
            .unwrap_or_default()
    }

    fn resize(&mut self, cols: u16, rows: u16) {
        self.cols = cols;
        self.rows = rows;
        let n = usize::from(cols) * usize::from(rows);
        self.cells.clear();
        self.cells.resize(n, 0);
        self.glyphs.clear();
        self.glyphs.resize(n, u32::from(b' '));
        self.styles.clear();
        self.styles.resize(n, 0);
    }

    fn set_style(&mut self, id: u16, style: Style) {
        let id = usize::from(id);
        if self.raw_styles.len() <= id {
            self.raw_styles.resize(id + 1, Style::default());
            self.style_table.resize(id + 1, [0; 4]);
        }
        self.raw_styles[id] = style;
        self.style_table[id] = resolve(&style, &self.palette);
        self.style_table_dirty = true;
    }

    /// Resolves every style again, after the palette changed.
    fn resolve_all(&mut self) {
        if self.raw_styles.is_empty() {
            self.raw_styles.push(Style::default());
            self.style_table.push([0; 4]);
        }
        for (raw, entry) in self.raw_styles.iter().zip(&mut self.style_table) {
            *entry = resolve(raw, &self.palette);
        }
        self.style_table_dirty = true;
    }

    /// The font variant of a style: bold in bit 0, italic in bit 1.
    fn variant(&self, id: u16) -> u8 {
        self.raw_styles.get(usize::from(id)).map_or(0, |s| {
            u8::from(s.attrs.contains(Attrs::BOLD)) | u8::from(s.attrs.contains(Attrs::ITALIC)) << 1
        })
    }
}

fn rgba(color: Color, default: u32, palette: &Palette) -> u32 {
    let rgb = match color {
        Color::Default => default,
        Color::Palette(i) => palette.colors[usize::from(i)],
        Color::Rgb(r, g, b) => u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b),
    };
    rgb << 8 | 0xff
}

/// A style as the renderer reads it. Attribute bits: the [`Attrs`] bits in the low 8 bits, and the
/// underline kind in bits 8 to 10.
fn resolve(style: &Style, palette: &Palette) -> [u32; 4] {
    let (mut fg, mut bg) = (
        rgba(style.fg, palette.fg, palette),
        rgba(style.bg, palette.bg, palette),
    );
    let underline = match style.underline_color {
        Color::Default => fg,
        other => rgba(other, palette.fg, palette),
    };
    if style.attrs.contains(Attrs::INVERSE) {
        std::mem::swap(&mut fg, &mut bg);
    }
    [
        fg,
        bg,
        underline,
        u32::from(style.attrs.bits()) | (style.underline as u32) << 8,
    ]
}

/// The decoder: the state of every pane, and the glyph atlas map.
pub struct Decoder {
    panes: HashMap<u32, Pane>,
    atlas: AtlasMap,
    scratch: Scratch,
    last_pane: u32,
    last_rev: u64,
    dirty_rows: Vec<u16>,
}

impl Decoder {
    /// A decoder for an atlas of `atlas_slots` slots.
    pub fn new(atlas_slots: u32) -> Self {
        Self {
            panes: HashMap::new(),
            atlas: AtlasMap::new(atlas_slots),
            scratch: Scratch::default(),
            last_pane: 0,
            last_rev: 0,
            dirty_rows: Vec::with_capacity(512),
        }
    }

    pub fn atlas(&self) -> &AtlasMap {
        &self.atlas
    }

    /// The text of a pending glyph that is a cluster of several code points, as UTF-8. The pending
    /// list does not name a pane, because the atlas serves every pane.
    pub fn atlas_text(&self, glyph: u32) -> Option<&[u8]> {
        self.atlas.cluster_text(glyph)
    }

    /// The glyphs to rasterize after the last frame.
    pub fn pending(&self) -> &[Pending] {
        self.atlas.pending()
    }

    /// The pane of the last frame.
    pub fn last_pane(&self) -> u32 {
        self.last_pane
    }

    /// The revision of the last frame.
    pub fn last_rev(&self) -> u64 {
        self.last_rev
    }

    /// The rows that the last frame changed. The UI uploads these rows to the GPU.
    pub fn dirty_rows(&self) -> &[u16] {
        &self.dirty_rows
    }

    /// The state of a pane.
    pub fn pane(&self, pane: u32) -> Option<&Pane> {
        self.panes.get(&pane)
    }

    /// Records that the UI uploaded the style table of a pane.
    pub fn set_style_table_clean(&mut self, pane: u32) {
        if let Some(p) = self.panes.get_mut(&pane) {
            p.style_table_dirty = false;
        }
    }

    /// Forgets a pane, for example when the UI no longer shows it.
    pub fn remove_pane(&mut self, pane: u32) {
        self.panes.remove(&pane);
    }

    /// Reads a frame and applies it. The result is one of the `STATUS_` constants.
    pub fn decode(&mut self, bytes: &[u8]) -> u32 {
        self.atlas.clear_pending();
        let base = bytes.as_ptr() as usize;
        let mut de = postcard::Deserializer::from_bytes(bytes);
        let Self { panes, scratch, .. } = self;
        scratch.needs_keyframe = false;
        let read = (&mut de).deserialize_struct(
            "Frame",
            FRAME_FIELDS,
            FrameVisitor {
                scratch,
                panes,
                base,
            },
        );
        if self.scratch.needs_keyframe {
            // The UI must name the pane in `render.ack.need_keyframe`, and it parses no bytes.
            self.last_pane = self.scratch.header.pane;
            self.last_rev = self.scratch.header.rev;
            return STATUS_NEEDS_KEYFRAME;
        }
        let h = self.scratch.header;
        if read.is_err() || h.cols == 0 || h.rows == 0 || !self.rows_fit(bytes) {
            return STATUS_ERROR;
        }
        self.apply(bytes);
        STATUS_APPLIED
    }

    /// Whether every row of the frame has the right size and place.
    fn rows_fit(&self, input: &[u8]) -> bool {
        let h = &self.scratch.header;
        let wanted = usize::from(h.cols) * 6;
        self.scratch.rows.iter().all(|(y, span)| {
            *y < h.rows && span.len == wanted && span.start + span.len <= input.len()
        }) && (!h.keyframe || self.scratch.rows.len() == usize::from(h.rows))
    }

    fn apply(&mut self, input: &[u8]) {
        let Self {
            panes,
            scratch,
            atlas,
            dirty_rows,
            ..
        } = self;
        let h = scratch.header;
        let pane = panes.entry(h.pane).or_insert_with(Pane::new);
        dirty_rows.clear();

        let resized = pane.cols != h.cols || pane.rows != h.rows;
        if h.keyframe && (resized || !pane.synced) {
            pane.resize(h.cols, h.rows);
        }
        if h.keyframe && (pane.epoch != h.epoch || !pane.synced) {
            // A new epoch has new style ids. Id 0 is the default style, which no frame sends. It
            // takes its colours from the palette that the pane has now. A palette that arrives
            // with this frame resolves it again. One that is equal to the old one does not.
            pane.raw_styles.clear();
            pane.style_table.clear();
            pane.raw_styles.push(Style::default());
            pane.style_table
                .push(resolve(&Style::default(), &pane.palette));
            pane.style_table_dirty = true;
            pane.graphemes.clear();
        }
        pane.epoch = h.epoch;

        let mut palette_changed = false;
        if let Some(palette) = scratch.palette.take() {
            palette_changed = pane.palette != palette;
            pane.palette = palette;
        }
        if pane.raw_styles.is_empty() {
            pane.raw_styles.push(Style::default());
            pane.style_table.push([0; 4]);
            palette_changed = true;
        }
        if palette_changed {
            pane.resolve_all();
        }
        for &(id, style) in &scratch.styles {
            pane.set_style(id, style);
        }
        for &(id, span) in &scratch.graphemes {
            pane.graphemes.insert(id, span.slice(input).into());
        }

        for &(y, span) in &scratch.rows {
            let first = usize::from(y) * usize::from(pane.cols);
            for (x, (glyph, style)) in unpack_row(span.slice(input)).enumerate() {
                let at = first + x;
                pane.glyphs[at] = glyph;
                pane.styles[at] = style;
                let (slot, flags) = cell_slot(atlas, pane, glyph, style);
                // The cursor flag stays with the cursor, not with the cell.
                pane.cells[at] = slot | flags | (pane.cells[at] & FLAG_CURSOR);
            }
            dirty_rows.push(y);
        }
        if h.keyframe {
            dirty_rows.clear();
            dirty_rows.extend(0..h.rows);
        }

        // The cursor flag moves with the cursor, and both rows need uploading.
        let old = pane.cursor;
        let new = scratch.cursor;
        if let Some(i) = cell_index(pane, old) {
            pane.cells[i] &= !FLAG_CURSOR;
        }
        if new.visible
            && let Some(i) = cell_index(pane, new)
        {
            pane.cells[i] |= FLAG_CURSOR;
        }
        for y in [old.y, new.y] {
            if y < pane.rows && !dirty_rows.contains(&y) {
                dirty_rows.push(y);
            }
        }
        dirty_rows.sort_unstable();

        pane.cursor = new;
        pane.viewport = scratch.viewport;
        pane.modes = scratch.modes;
        pane.selection = scratch.selection;
        pane.bell = scratch.bell;
        if h.keyframe || scratch.title.is_some() {
            pane.title.clear();
            if let Some(span) = scratch.title {
                pane.title.extend_from_slice(span.slice(input));
            }
        }
        pane.acks.clear();
        pane.acks.extend(
            scratch
                .acks
                .iter()
                .map(|&(id, state)| [id as u32, (id >> 32) as u32, state as u32]),
        );
        pane.rev = h.rev;
        pane.synced = true;
        self.last_pane = h.pane;
        self.last_rev = h.rev;
    }

    /// Whether a glyph had no atlas slot. The UI calls [`Decoder::atlas_reset`].
    pub fn atlas_full(&self) -> bool {
        self.atlas.is_full()
    }

    /// Gives every glyph on screen a new slot, after the UI emptied its atlas. All rows of every
    /// pane need uploading again.
    pub fn atlas_reset(&mut self) {
        self.atlas.reset();
        self.dirty_rows.clear();
        let Self { panes, atlas, .. } = self;
        for pane in panes.values_mut() {
            for i in 0..pane.cells.len() {
                let (slot, flags) = cell_slot(atlas, pane, pane.glyphs[i], pane.styles[i]);
                pane.cells[i] = slot | flags | (pane.cells[i] & FLAG_CURSOR);
            }
        }
    }
}

fn cell_index(pane: &Pane, cursor: Cursor) -> Option<usize> {
    (cursor.x < pane.cols && cursor.y < pane.rows)
        .then(|| usize::from(cursor.y) * usize::from(pane.cols) + usize::from(cursor.x))
}

/// The atlas slot of a cell and its flags (without the cursor).
fn cell_slot(atlas: &mut AtlasMap, pane: &Pane, glyph: u32, style: u16) -> (u32, u32) {
    let width = pane
        .raw_styles
        .get(usize::from(style))
        .map_or(CellWidth::Single, |s| s.width);
    if width == CellWidth::Spacer {
        return (0, FLAG_SPACER);
    }
    if glyph == u32::from(b' ') || glyph == 0 {
        return (0, 0);
    }
    // A grapheme id is local to its pane. The atlas has its own glyph for the text.
    let glyph = if glyph & GRAPHEME_FLAG != 0 {
        match pane.grapheme(glyph & !GRAPHEME_FLAG) {
            Some(text) => atlas.cluster(text),
            None => return (0, 0),
        }
    } else {
        glyph
    };
    match width {
        CellWidth::Wide => (atlas.slot(glyph, pane.variant(style), true), FLAG_WIDE),
        _ => (atlas.slot(glyph, pane.variant(style), false), 0),
    }
}

// ---- the visitors ----

/// Reads the next field of the frame.
fn next<'de, A: SeqAccess<'de>, T: Deserialize<'de>>(seq: &mut A) -> Result<T, A::Error> {
    seq.next_element()?
        .ok_or_else(|| <A::Error as DeError>::custom("the frame ends early"))
}

fn next_seed<'de, A: SeqAccess<'de>, S: DeserializeSeed<'de>>(
    seq: &mut A,
    seed: S,
) -> Result<S::Value, A::Error> {
    seq.next_element_seed(seed)?
        .ok_or_else(|| <A::Error as DeError>::custom("the frame ends early"))
}

struct FrameVisitor<'a> {
    scratch: &'a mut Scratch,
    panes: &'a HashMap<u32, Pane>,
    base: usize,
}

impl<'de> Visitor<'de> for FrameVisitor<'_> {
    type Value = ();

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a frame")
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
        let Self {
            scratch,
            panes,
            base,
        } = self;
        let pane: u32 = next(&mut seq)?;
        let epoch: u32 = next(&mut seq)?;
        let base_rev: u64 = next(&mut seq)?;
        let rev: u64 = next(&mut seq)?;
        let kind: FrameKind = next(&mut seq)?;
        let cols: u16 = next(&mut seq)?;
        let rows: u16 = next(&mut seq)?;
        let keyframe = kind == FrameKind::Keyframe;
        scratch.header = Header {
            pane,
            epoch,
            rev,
            keyframe,
            cols,
            rows,
        };
        // A delta applies only to a pane at exactly its base revision, in the same epoch and size.
        let follows = keyframe
            || panes.get(&pane).is_some_and(|p| {
                p.synced
                    && p.epoch == epoch
                    && p.rev == base_rev
                    && (p.cols, p.rows) == (cols, rows)
            });
        if !follows {
            scratch.needs_keyframe = true;
            return Err(<A::Error as DeError>::custom("the pane needs a keyframe"));
        }
        scratch.rows.clear();
        next_seed(
            &mut seq,
            Rows {
                rows: &mut scratch.rows,
                base,
            },
        )?;
        next_seed(&mut seq, Collect(&mut scratch.styles))?;
        next_seed(
            &mut seq,
            Graphemes {
                out: &mut scratch.graphemes,
                base,
            },
        )?;
        scratch.cursor = next(&mut seq)?;
        scratch.viewport = next(&mut seq)?;
        scratch.modes = next(&mut seq)?;
        scratch.title = next_seed(&mut seq, OptionalStr { base })?;
        scratch.bell = next(&mut seq)?;
        next_seed(&mut seq, Collect(&mut scratch.images))?;
        scratch.selection = next(&mut seq)?;
        next_seed(&mut seq, Collect(&mut scratch.acks))?;
        scratch.palette = next(&mut seq)?;
        Ok(())
    }
}

/// A sequence of small values, collected into a buffer that is cleared first.
struct Collect<'a, T>(&'a mut Vec<T>);

impl<'de, T: Deserialize<'de>> DeserializeSeed<'de> for Collect<'_, T> {
    type Value = ();

    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<(), D::Error> {
        struct V<'a, T>(&'a mut Vec<T>);
        impl<'de, T: Deserialize<'de>> Visitor<'de> for V<'_, T> {
            type Value = ();
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a list")
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
                self.0.clear();
                while let Some(item) = seq.next_element::<T>()? {
                    self.0.push(item);
                }
                Ok(())
            }
        }
        d.deserialize_seq(V(self.0))
    }
}

/// A byte string of the input, noted as a span.
struct SpanOfBytes {
    base: usize,
}

impl<'de> DeserializeSeed<'de> for SpanOfBytes {
    type Value = Span;

    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<Span, D::Error> {
        struct V(usize);
        impl<'de> Visitor<'de> for V {
            type Value = Span;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("bytes")
            }
            fn visit_borrowed_bytes<E: DeError>(self, v: &'de [u8]) -> Result<Span, E> {
                Ok(Span::of(v, self.0))
            }
        }
        d.deserialize_bytes(V(self.base))
    }
}

struct Rows<'a> {
    rows: &'a mut Vec<(u16, Span)>,
    base: usize,
}

impl<'de> DeserializeSeed<'de> for Rows<'_> {
    type Value = ();

    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<(), D::Error> {
        struct V<'a>(&'a mut Vec<(u16, Span)>, usize);
        impl<'de> Visitor<'de> for V<'_> {
            type Value = ();
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("rows")
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
                while let Some(row) = seq.next_element_seed(Row(self.1))? {
                    self.0.push(row);
                }
                Ok(())
            }
        }
        d.deserialize_seq(V(self.rows, self.base))
    }
}

/// One `RowUpdate`: its row number and the span of its cells.
struct Row(usize);

impl<'de> DeserializeSeed<'de> for Row {
    type Value = (u16, Span);

    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<(u16, Span), D::Error> {
        struct V(usize);
        impl<'de> Visitor<'de> for V {
            type Value = (u16, Span);
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a row")
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(u16, Span), A::Error> {
                let y: u16 = next(&mut seq)?;
                let cells = next_seed(&mut seq, SpanOfBytes { base: self.0 })?;
                Ok((y, cells))
            }
        }
        d.deserialize_struct("RowUpdate", &["y", "cells"], V(self.0))
    }
}

struct Graphemes<'a> {
    out: &'a mut Vec<(u32, Span)>,
    base: usize,
}

impl<'de> DeserializeSeed<'de> for Graphemes<'_> {
    type Value = ();

    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<(), D::Error> {
        struct V<'a>(&'a mut Vec<(u32, Span)>, usize);
        impl<'de> Visitor<'de> for V<'_> {
            type Value = ();
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("graphemes")
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
                self.0.clear();
                while let Some(item) = seq.next_element_seed(Grapheme(self.1))? {
                    self.0.push(item);
                }
                Ok(())
            }
        }
        d.deserialize_seq(V(self.out, self.base))
    }
}

/// One `(GraphemeId, String)`.
struct Grapheme(usize);

impl<'de> DeserializeSeed<'de> for Grapheme {
    type Value = (u32, Span);

    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<(u32, Span), D::Error> {
        struct V(usize);
        impl<'de> Visitor<'de> for V {
            type Value = (u32, Span);
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a grapheme")
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(u32, Span), A::Error> {
                let id: u32 = next(&mut seq)?;
                let text = next_seed(&mut seq, StrSpan(self.0))?;
                Ok((id, text))
            }
        }
        d.deserialize_tuple(2, V(self.0))
    }
}

/// A string of the input, noted as a span.
struct StrSpan(usize);

impl<'de> DeserializeSeed<'de> for StrSpan {
    type Value = Span;

    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<Span, D::Error> {
        struct V(usize);
        impl<'de> Visitor<'de> for V {
            type Value = Span;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a string")
            }
            fn visit_borrowed_str<E: DeError>(self, v: &'de str) -> Result<Span, E> {
                Ok(Span::of(v.as_bytes(), self.0))
            }
        }
        d.deserialize_str(V(self.0))
    }
}

/// `Option<String>`, as `Option<Span>`.
struct OptionalStr {
    base: usize,
}

impl<'de> DeserializeSeed<'de> for OptionalStr {
    type Value = Option<Span>;

    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<Option<Span>, D::Error> {
        struct V(usize);
        impl<'de> Visitor<'de> for V {
            type Value = Option<Span>;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("an optional string")
            }
            fn visit_none<E: DeError>(self) -> Result<Option<Span>, E> {
                Ok(None)
            }
            fn visit_some<D: Deserializer<'de>>(self, d: D) -> Result<Option<Span>, D::Error> {
                StrSpan(self.0).deserialize(d).map(Some)
            }
        }
        d.deserialize_option(V(self.base))
    }
}
