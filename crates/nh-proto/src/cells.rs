//! The 6-byte cell record.
//!
//! A cell is a little-endian `u32` glyph followed by a little-endian `u16` style id. The glyph is
//! a Unicode scalar value, or a grapheme id when [`GRAPHEME_FLAG`] is set.

/// Bytes in one packed cell.
pub const CELL_BYTES: usize = 6;

/// The high bit of a glyph. When set, the low bits are a grapheme id, not a code point.
pub const GRAPHEME_FLAG: u32 = 0x8000_0000;

/// The largest grapheme id (20 bits).
pub const MAX_GRAPHEME_ID: u32 = (1 << 20) - 1;

/// The style id of a cell with the default style. It is never sent in `new_styles`.
pub const STYLE_DEFAULT: u16 = 0;

/// Packs a glyph and a style id into the 6-byte wire record.
pub fn pack_cell(glyph: u32, style: u16) -> [u8; CELL_BYTES] {
    let g = glyph.to_le_bytes();
    let s = style.to_le_bytes();
    [g[0], g[1], g[2], g[3], s[0], s[1]]
}

/// Unpacks a wire record into `(glyph, style)`. It is the inverse of [`pack_cell`].
pub fn unpack_cell(cell: [u8; CELL_BYTES]) -> (u32, u16) {
    (
        u32::from_le_bytes([cell[0], cell[1], cell[2], cell[3]]),
        u16::from_le_bytes([cell[4], cell[5]]),
    )
}

/// Iterates over the cells of a packed row. A trailing partial record is ignored.
pub fn unpack_row(bytes: &[u8]) -> impl Iterator<Item = (u32, u16)> + '_ {
    bytes
        .as_chunks::<CELL_BYTES>()
        .0
        .iter()
        .map(|&cell| unpack_cell(cell))
}

/// The glyph that stands for grapheme `id`, or `None` when `id` is above [`MAX_GRAPHEME_ID`].
pub fn grapheme_glyph(id: u32) -> Option<u32> {
    (id <= MAX_GRAPHEME_ID).then_some(GRAPHEME_FLAG | id)
}

/// The grapheme id of `glyph`, or `None` when the glyph is a plain code point.
pub fn glyph_grapheme(glyph: u32) -> Option<u32> {
    (glyph & GRAPHEME_FLAG != 0).then_some(glyph & !GRAPHEME_FLAG)
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    proptest! {
        #[test]
        fn pack_and_unpack_are_inverse(glyph in any::<u32>(), style in any::<u16>()) {
            prop_assert_eq!(unpack_cell(pack_cell(glyph, style)), (glyph, style));
        }
    }

    #[test]
    fn the_record_is_little_endian() {
        assert_eq!(pack_cell(0x0102_0304, 0x0506), [4, 3, 2, 1, 6, 5]);
    }

    #[test]
    fn unpack_row_ignores_a_partial_record() {
        let mut row = pack_cell(65, 1).to_vec();
        row.extend_from_slice(&pack_cell(66, 2));
        row.push(9);
        assert_eq!(unpack_row(&row).collect::<Vec<_>>(), vec![(65, 1), (66, 2)]);
    }

    #[test]
    fn grapheme_ids_map_to_glyphs_and_back() {
        assert_eq!(grapheme_glyph(7), Some(GRAPHEME_FLAG | 7));
        assert_eq!(glyph_grapheme(GRAPHEME_FLAG | 7), Some(7));
        assert_eq!(glyph_grapheme(0x1F600), None);
        assert_eq!(
            grapheme_glyph(MAX_GRAPHEME_ID),
            Some(GRAPHEME_FLAG | MAX_GRAPHEME_ID)
        );
        assert_eq!(grapheme_glyph(MAX_GRAPHEME_ID + 1), None);
    }
}
