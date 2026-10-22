//! The decoder shows what the reference client shows: glyphs, styles, cursor, modes, title
//! and palette, and the atlas slots and the dirty rows that go with them.

mod common;

use nh_fixture::{Op, ops};
use nh_proto::{
    cells::GRAPHEME_FLAG,
    frame::FrameKind,
    style::{Attrs, CellWidth, Color, Style},
};
use nh_term::client_grid::ClientGrid;
use proptest::prelude::*;
use proto_wasm::{Decoder, FLAG_CURSOR, FLAG_SPACER, FLAG_WIDE, Pane, SLOT_MASK, STATUS_APPLIED};

use common::{PANE, Source, encode};

/// The style table entry that the decoder must have made, written again from the rules.
fn expected_entry(style: &Style, pane: &Pane) -> [u32; 4] {
    let rgba = |color: Color, default: u32| -> u32 {
        let rgb = match color {
            Color::Default => default,
            Color::Palette(i) => pane.palette.colors[usize::from(i)],
            Color::Rgb(r, g, b) => u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b),
        };
        rgb << 8 | 0xff
    };
    let (mut fg, mut bg) = (
        rgba(style.fg, pane.palette.fg),
        rgba(style.bg, pane.palette.bg),
    );
    let underline = if style.underline_color == Color::Default {
        fg
    } else {
        rgba(style.underline_color, pane.palette.fg)
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

/// Checks the decoded pane against the reference client. Returns a description of the first
/// difference.
fn compare(decoder: &Decoder, client: &ClientGrid, wanted_dirty: &[u16]) -> Result<(), String> {
    let pane = decoder.pane(PANE).ok_or("no pane")?;
    let dump = client.dump().map_err(|e| format!("{e:?}"))?;
    if usize::from(pane.rows) != dump.rows.len() {
        return Err(format!("{} rows against {}", pane.rows, dump.rows.len()));
    }
    for y in 0..pane.rows {
        for x in 0..pane.cols {
            let (text, style) = &dump.rows[usize::from(y)][usize::from(x)];
            if &pane.text_at(x, y) != text || &pane.style_at(x, y) != style {
                return Err(format!(
                    "cell ({x},{y}): {:?}/{:?} against {text:?}/{style:?}",
                    pane.text_at(x, y),
                    pane.style_at(x, y)
                ));
            }
            let at = usize::from(y) * usize::from(pane.cols) + usize::from(x);
            let cell = pane.cells[at];
            let slot = cell & SLOT_MASK;
            match style.width {
                CellWidth::Spacer => {
                    if cell & FLAG_SPACER == 0 || slot != 0 {
                        return Err(format!("spacer at ({x},{y}) is {cell:#x}"));
                    }
                }
                width => {
                    let glyph = pane.glyphs[at];
                    let blank = glyph == u32::from(b' ') || glyph == 0;
                    if blank != (slot == 0) {
                        return Err(format!("slot {slot} for glyph {glyph:#x} at ({x},{y})"));
                    }
                    if (width == CellWidth::Wide) != (cell & FLAG_WIDE != 0) {
                        return Err(format!("wide flag at ({x},{y})"));
                    }
                    if !blank {
                        let variant = u8::from(style.attrs.contains(Attrs::BOLD))
                            | u8::from(style.attrs.contains(Attrs::ITALIC)) << 1;
                        // The atlas has its own glyph for a cluster, so compare the text.
                        let held = decoder.atlas().glyph_of(slot).map(|(held, variant)| {
                            let text = match decoder.atlas_text(held) {
                                Some(bytes) => String::from_utf8_lossy(bytes).into_owned(),
                                None => char::from_u32(held).map(String::from).unwrap_or_default(),
                            };
                            (text, variant)
                        });
                        if held != Some((pane.text_at(x, y), variant)) {
                            return Err(format!(
                                "slot {slot} holds {held:?}, not {:?}/{variant}",
                                pane.text_at(x, y)
                            ));
                        }
                    }
                }
            }
            if glyph_is_grapheme(pane.glyphs[at])
                && pane.grapheme(pane.glyphs[at] & !GRAPHEME_FLAG).is_none()
            {
                return Err(format!("grapheme at ({x},{y}) has no text"));
            }
        }
    }
    let cursor = dump.cursor;
    let flagged: Vec<(u16, u16)> = (0..pane.rows)
        .flat_map(|y| (0..pane.cols).map(move |x| (x, y)))
        .filter(|&(x, y)| {
            pane.cells[usize::from(y) * usize::from(pane.cols) + usize::from(x)] & FLAG_CURSOR != 0
        })
        .collect();
    let expected: Vec<(u16, u16)> =
        if cursor.visible && cursor.x < pane.cols && cursor.y < pane.rows {
            vec![(cursor.x, cursor.y)]
        } else {
            vec![]
        };
    if flagged != expected {
        return Err(format!("cursor flags {flagged:?} against {expected:?}"));
    }
    if pane.cursor != cursor || pane.modes != dump.modes {
        return Err("cursor or modes".into());
    }
    let title = (!pane.title.is_empty()).then(|| String::from_utf8_lossy(&pane.title).into_owned());
    if title != dump.title {
        return Err(format!("title {title:?} against {:?}", dump.title));
    }
    if pane.palette != dump.palette {
        return Err("palette".into());
    }
    for (id, entry) in pane.style_table.iter().enumerate().skip(1) {
        let style = (0..pane.rows)
            .flat_map(|y| (0..pane.cols).map(move |x| (x, y)))
            .find(|&(x, y)| {
                pane.styles[usize::from(y) * usize::from(pane.cols) + usize::from(x)] as usize == id
            })
            .map(|(x, y)| pane.style_at(x, y));
        if let Some(style) = style
            && *entry != expected_entry(&style, pane)
        {
            return Err(format!("style table entry {id}"));
        }
    }
    for y in wanted_dirty {
        if !decoder.dirty_rows().contains(y) {
            return Err(format!("row {y} is not dirty"));
        }
    }
    Ok(())
}

fn glyph_is_grapheme(glyph: u32) -> bool {
    glyph & GRAPHEME_FLAG != 0
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(150))]

    #[test]
    fn the_decoder_shows_what_the_reference_client_shows(stream in ops(30)) {
        let mut source = Source::new(80, 24);
        let mut client = ClientGrid::new();
        let mut decoder = Decoder::new(1 << 16);
        for (n, op) in stream.iter().enumerate() {
            match op {
                Op::Bytes(bytes) => source.write(bytes),
                Op::Resize { cols, rows } => source.resize(*cols, *rows),
            }
            let frame = source.frame(FrameKind::Delta);
            client.apply(&frame).expect("the reference client applies the frame");
            prop_assert_eq!(decoder.decode(&encode(&frame)), STATUS_APPLIED, "op {}", n);
            let wanted: Vec<u16> = frame.lines.iter().map(|l| l.y).collect();
            if let Err(difference) = compare(&decoder, &client, &wanted) {
                return Err(TestCaseError::fail(format!("after op {n} {op:?}: {difference}")));
            }
        }
    }
}
