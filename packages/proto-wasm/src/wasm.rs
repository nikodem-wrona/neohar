//! The functions that JavaScript calls (`wasm-bindgen`). Numbers and pointers only.
//!
//! JavaScript writes a frame into the input buffer of the decoder (`input_ptr`), so that no copy of
//! the frame is allocated for each call, and then calls `decode_input`. After `decode`, the
//! pointers of a pane point to its arrays in WASM memory. When WASM memory grows, JavaScript must
//! make its views again.

use wasm_bindgen::prelude::*;

use crate::decoder::{Decoder, Pane};

/// The version of the frame format that this decoder reads. The UI compares it with the version
/// that `render.attach` reports.
#[wasm_bindgen]
pub fn frame_proto() -> u32 {
    u32::from(nh_proto::version::FRAME_PROTO)
}

/// The decoder, as JavaScript sees it.
#[wasm_bindgen]
pub struct WasmDecoder {
    inner: Decoder,
    input: Vec<u8>,
}

/// The row of `line` in a viewport that starts at `top`, kept between -1 and `rows`.
fn relative_row(line: u64, top: u64, rows: u16) -> i32 {
    (i128::from(line) - i128::from(top)).clamp(-1, i128::from(rows)) as i32
}

fn ptr<T>(slice: &[T]) -> usize {
    slice.as_ptr() as usize
}

#[wasm_bindgen]
impl WasmDecoder {
    /// A decoder for a glyph atlas of `atlas_slots` slots.
    #[wasm_bindgen(constructor)]
    pub fn new(atlas_slots: u32) -> WasmDecoder {
        WasmDecoder {
            inner: Decoder::new(atlas_slots),
            input: Vec::new(),
        }
    }

    /// The input buffer, at least `min_len` bytes long. The address changes when it grows.
    pub fn input_ptr(&mut self, min_len: usize) -> usize {
        if self.input.len() < min_len {
            self.input.resize(min_len.next_power_of_two(), 0);
        }
        ptr(&self.input)
    }

    /// The length of the input buffer. JavaScript makes one view of it and reuses the view.
    pub fn input_cap(&self) -> usize {
        self.input.len()
    }

    /// Decodes the first `len` bytes of the input buffer. 0: applied. 1: the pane needs a keyframe.
    /// 2: the bytes are not a frame.
    pub fn decode_input(&mut self, len: usize) -> u32 {
        let len = len.min(self.input.len());
        self.inner.decode(&self.input[..len])
    }

    /// The pane of the last frame.
    pub fn pane(&self) -> u32 {
        self.inner.last_pane()
    }

    /// The revision of the last frame.
    pub fn rev(&self) -> f64 {
        self.inner.last_rev() as f64
    }

    /// The rows that the last frame changed: `u16` values.
    pub fn dirty_rows_ptr(&self) -> usize {
        ptr(self.inner.dirty_rows())
    }

    pub fn dirty_rows_len(&self) -> usize {
        self.inner.dirty_rows().len()
    }

    /// One row of the list above. The renderer reads rows one by one, so that it makes no view of
    /// a length that changes with each frame.
    pub fn dirty_row(&self, i: usize) -> u32 {
        self.inner.dirty_rows().get(i).copied().map_or(0, u32::from)
    }

    /// The cells of a pane: `u32` values, the atlas slot in the low 24 bits.
    pub fn cells_ptr(&self, pane: u32) -> usize {
        self.with(pane, |p| ptr(&p.cells))
    }

    /// The style id of each cell: `u16` values.
    pub fn styles_ptr(&self, pane: u32) -> usize {
        self.with(pane, |p| ptr(&p.styles))
    }

    pub fn cols(&self, pane: u32) -> u32 {
        self.with(pane, |p| u32::from(p.cols))
    }

    pub fn rows(&self, pane: u32) -> u32 {
        self.with(pane, |p| u32::from(p.rows))
    }

    /// The styles of a pane: four `u32` values for each style id.
    pub fn style_table_ptr(&self, pane: u32) -> usize {
        self.with(pane, |p| ptr(&p.style_table))
    }

    /// The number of styles in the table.
    pub fn style_table_len(&self, pane: u32) -> usize {
        self.with(pane, |p| p.style_table.len())
    }

    /// Whether the table changed since [`WasmDecoder::style_table_clean`].
    pub fn style_table_dirty(&self, pane: u32) -> bool {
        self.with(pane, |p| p.style_table_dirty)
    }

    /// The UI has uploaded the style table.
    pub fn style_table_clean(&mut self, pane: u32) {
        self.inner.set_style_table_clean(pane);
    }

    /// The default foreground of the pane as `0xRRGGBB`.
    pub fn palette_fg(&self, pane: u32) -> u32 {
        self.with(pane, |p| p.palette.fg)
    }

    /// The default background of the pane as `0xRRGGBB`.
    pub fn palette_bg(&self, pane: u32) -> u32 {
        self.with(pane, |p| p.palette.bg)
    }

    /// The cursor colour of the pane as `0xRRGGBB`.
    pub fn palette_cursor(&self, pane: u32) -> u32 {
        self.with(pane, |p| p.palette.cursor)
    }

    pub fn cursor_x(&self, pane: u32) -> u32 {
        self.with(pane, |p| u32::from(p.cursor.x))
    }

    pub fn cursor_y(&self, pane: u32) -> u32 {
        self.with(pane, |p| u32::from(p.cursor.y))
    }

    /// 0 block, 1 underline, 2 bar, 3 hollow block.
    pub fn cursor_shape(&self, pane: u32) -> u32 {
        self.with(pane, |p| p.cursor.shape as u32)
    }

    pub fn cursor_visible(&self, pane: u32) -> bool {
        self.with(pane, |p| p.cursor.visible)
    }

    pub fn cursor_blink(&self, pane: u32) -> bool {
        self.with(pane, |p| p.cursor.blink)
    }

    /// The `ModeFlags` bits.
    pub fn modes(&self, pane: u32) -> u32 {
        self.with(pane, |p| p.modes.bits())
    }

    /// Whether the application in the pane asked for key release events (the kitty "report event
    /// types" mode). The UI sends a `release` only when this is true. The decoder knows
    /// the bit, so the UI holds no copy of it.
    pub fn wants_key_release(&self, pane: u32) -> bool {
        self.with(pane, |p| {
            p.modes
                .contains(nh_proto::modes::ModeFlags::KITTY_REPORT_EVENT_TYPES)
        })
    }

    pub fn viewport_top(&self, pane: u32) -> f64 {
        self.with(pane, |p| p.viewport.top_line as f64)
    }

    pub fn viewport_total(&self, pane: u32) -> f64 {
        self.with(pane, |p| p.viewport.total_lines as f64)
    }

    /// 0 when there is no selection, else 1 to 4 for the mode (simple, word, line, block).
    pub fn selection_mode(&self, pane: u32) -> u32 {
        self.with(pane, |p| p.selection.map_or(0, |s| s.mode as u32 + 1))
    }

    pub fn selection_start_col(&self, pane: u32) -> u32 {
        self.with(pane, |p| p.selection.map_or(0, |s| u32::from(s.start.col)))
    }

    pub fn selection_end_col(&self, pane: u32) -> u32 {
        self.with(pane, |p| p.selection.map_or(0, |s| u32::from(s.end.col)))
    }

    pub fn selection_start_line(&self, pane: u32) -> f64 {
        self.with(pane, |p| p.selection.map_or(0.0, |s| s.start.line as f64))
    }

    pub fn selection_end_line(&self, pane: u32) -> f64 {
        self.with(pane, |p| p.selection.map_or(0.0, |s| s.end.line as f64))
    }

    /// The first row of the selection, counted from the first row of the viewport. It is kept
    /// between -1 (on or above the row before the first) and the number of rows (on or below the
    /// last), so that it is a small number. The renderer reads it in every frame, and an `i32`
    /// is not boxed on the heap as an `f64` is.
    pub fn selection_start_row(&self, pane: u32) -> i32 {
        self.with(pane, |p| {
            p.selection.map_or(0, |s| {
                relative_row(s.start.line, p.viewport.top_line, p.rows)
            })
        })
    }

    /// The last row of the selection, as [`WasmDecoder::selection_start_row`].
    pub fn selection_end_row(&self, pane: u32) -> i32 {
        self.with(pane, |p| {
            p.selection
                .map_or(0, |s| relative_row(s.end.line, p.viewport.top_line, p.rows))
        })
    }

    /// The title, as UTF-8.
    pub fn title_ptr(&self, pane: u32) -> usize {
        self.with(pane, |p| ptr(&p.title))
    }

    pub fn title_len(&self, pane: u32) -> usize {
        self.with(pane, |p| p.title.len())
    }

    /// Whether the last frame of the pane carried a bell.
    pub fn bell(&self, pane: u32) -> bool {
        self.with(pane, |p| p.bell)
    }

    /// The acknowledged inputs: three `u32` values each (id low, id high, 0 acked or 1 unconfirmed).
    pub fn ack_ptr(&self, pane: u32) -> usize {
        self.with(pane, |p| ptr(&p.acks))
    }

    pub fn ack_len(&self, pane: u32) -> usize {
        self.with(pane, |p| p.acks.len())
    }

    /// The glyphs to rasterize after the last frame: four `u32` values each (slot, glyph, variant,
    /// wide).
    pub fn pending_ptr(&self) -> usize {
        ptr(self.inner.pending())
    }

    pub fn pending_len(&self) -> usize {
        self.inner.pending().len()
    }

    /// One field of one pending glyph: 0 slot, 1 glyph, 2 variant, 3 wide. Like `dirty_row`, it
    /// saves a view per frame.
    pub fn pending_field(&self, i: usize, field: u32) -> u32 {
        self.inner.pending().get(i).map_or(0, |p| match field {
            0 => p.slot,
            1 => p.glyph,
            2 => p.variant,
            _ => p.wide,
        })
    }

    /// The text of a grapheme id of a pane, as UTF-8.
    pub fn grapheme_ptr(&self, pane: u32, id: u32) -> usize {
        self.inner
            .pane(pane)
            .and_then(|p| p.grapheme(id))
            .map_or(0, ptr)
    }

    pub fn grapheme_len(&self, pane: u32, id: u32) -> usize {
        self.inner
            .pane(pane)
            .and_then(|p| p.grapheme(id))
            .map_or(0, <[u8]>::len)
    }

    /// The text of a pending glyph that is a cluster of several code points, as UTF-8. It has the
    /// high bit set. The atlas serves every pane, so the text does not depend on a pane.
    pub fn atlas_text_ptr(&self, glyph: u32) -> usize {
        self.inner.atlas_text(glyph).map_or(0, ptr)
    }

    pub fn atlas_text_len(&self, glyph: u32) -> usize {
        self.inner.atlas_text(glyph).map_or(0, <[u8]>::len)
    }

    /// Whether a glyph found no free slot in the atlas.
    pub fn atlas_full(&self) -> bool {
        self.inner.atlas_full()
    }

    /// Gives the glyphs on screen new slots after the UI emptied the atlas. Every row of every pane
    /// needs uploading again.
    pub fn atlas_reset(&mut self) {
        self.inner.atlas_reset();
    }

    /// Forgets a pane.
    pub fn remove_pane(&mut self, pane: u32) {
        self.inner.remove_pane(pane);
    }

    fn with<T: Default>(&self, pane: u32, f: impl FnOnce(&Pane) -> T) -> T {
        self.inner.pane(pane).map(f).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use nh_proto::frame::FrameKind;
    use nh_term::{
        AlacrittyEngine, ClientState, FrameBuilder, MIN_HISTORY, Replaying, VtEngine, xterm_palette,
    };

    use super::{WasmDecoder, relative_row};

    /// The bytes of a keyframe of pane 3 after the application wrote `output`.
    fn keyframe_after(output: &[u8]) -> Vec<u8> {
        let mut engine = AlacrittyEngine::new(20, 5, MIN_HISTORY, xterm_palette());
        engine.advance(output, &mut Replaying::default());
        let mut builder = FrameBuilder::new(3, &engine);
        builder.absorb(&mut engine);
        let frame = builder.build(&engine, &mut ClientState::new(), FrameKind::Keyframe);
        postcard::to_stdvec(&frame).expect("the frame serializes")
    }

    #[test]
    fn a_pane_wants_key_releases_only_when_the_kitty_mode_asks_for_them() {
        let mut decoder = WasmDecoder::new(256);
        // `CSI > 1 u` pushes the flag "disambiguate". `CSI > 3 u` pushes flags 1 and 2, and flag 2
        // is "report event types".
        for (output, wants) in [(&b"\x1b[>1u"[..], false), (&b"\x1b[>3u"[..], true)] {
            let bytes = keyframe_after(output);
            assert_eq!(decoder.inner.decode(&bytes), crate::STATUS_APPLIED);
            assert_eq!(decoder.wants_key_release(3), wants, "{output:?}");
        }
        assert!(
            !decoder.wants_key_release(99),
            "a pane that does not exist wants nothing"
        );
    }

    #[test]
    fn a_line_is_a_row_of_the_viewport_kept_between_minus_one_and_the_rows() {
        assert_eq!(relative_row(105, 100, 24), 5);
        assert_eq!(relative_row(100, 100, 24), 0);
        assert_eq!(relative_row(99, 100, 24), -1);
        assert_eq!(relative_row(3, 100, 24), -1);
        assert_eq!(relative_row(123, 100, 24), 23);
        assert_eq!(relative_row(124, 100, 24), 24);
        assert_eq!(relative_row(u64::MAX, 0, 24), 24);
        assert_eq!(relative_row(0, u64::MAX, 24), -1);
    }
}
