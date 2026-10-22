//! Round trips of the wire types: property tests, the `FrameRef` twin and the size budget.

use proptest::prelude::*;
use serde::{Serialize, de::DeserializeOwned};

use crate::{
    cells::{STYLE_DEFAULT, pack_cell},
    frame::{Cursor, Frame, FrameKind, FrameRef, Palette, RowUpdate, Viewport},
    holder::{HoldConfig, HolderEvent, HolderRequest},
    modes::ModeFlags,
    style::{Attrs, Color, Style},
};

fn encode<T: Serialize>(value: &T) -> Vec<u8> {
    postcard::to_stdvec(value).expect("the value serializes")
}

fn decode<T: DeserializeOwned>(bytes: &[u8]) -> T {
    postcard::from_bytes(bytes).expect("the bytes decode")
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn frame_round_trips(frame in any::<Frame>()) {
        let bytes = encode(&frame);
        prop_assert_eq!(decode::<Frame>(&bytes), frame);
    }

    #[test]
    fn frame_ref_decodes_the_same_bytes_to_the_same_values(frame in any::<Frame>()) {
        let bytes = encode(&frame);
        let borrowed: FrameRef<'_> = postcard::from_bytes(&bytes).expect("the bytes decode");
        prop_assert_eq!(borrowed.to_frame(), frame);
        // The two types have one wire format: encoding the borrowed twin gives the same bytes.
        prop_assert_eq!(encode(&borrowed), bytes);
    }

    #[test]
    fn holder_request_round_trips(request in any::<HolderRequest>()) {
        let bytes = encode(&request);
        prop_assert_eq!(decode::<HolderRequest>(&bytes), request);
    }

    #[test]
    fn holder_event_round_trips(event in any::<HolderEvent>()) {
        let bytes = encode(&event);
        prop_assert_eq!(decode::<HolderEvent>(&bytes), event);
    }

    #[test]
    fn hold_config_round_trips(config in any::<HoldConfig>()) {
        let bytes = encode(&config);
        prop_assert_eq!(decode::<HoldConfig>(&bytes), config);
    }

    #[test]
    fn a_truncated_message_never_decodes_to_a_value(request in any::<HolderRequest>(), cut in 0usize..64) {
        let bytes = encode(&request);
        let cut = cut.min(bytes.len().saturating_sub(1));
        if cut < bytes.len() {
            prop_assert!(postcard::from_bytes::<HolderRequest>(&bytes[..cut]).is_err());
        }
    }
}

fn sample_frame() -> Frame {
    Frame {
        pane: 3,
        reset_epoch: 1,
        base_rev: 7,
        rev: 8,
        kind: FrameKind::Delta,
        cols: 4,
        rows: 2,
        lines: vec![RowUpdate {
            y: 1,
            cells: pack_cell(0x41, 2).repeat(4),
        }],
        new_styles: vec![(
            2,
            Style {
                fg: Color::Rgb(1, 2, 3),
                ..Style::default()
            },
        )],
        new_graphemes: vec![(5, "e\u{301}".to_owned())],
        cursor: Cursor::default(),
        viewport: Viewport::default(),
        modes: ModeFlags::default(),
        title: Some("zsh".to_owned()),
        bell: false,
        images: Vec::new(),
        selection: None,
        ack_status: Vec::new(),
        palette: None,
    }
}

#[test]
fn frame_ref_borrows_cells_and_strings_from_the_input() {
    let bytes = encode(&sample_frame());
    let borrowed: FrameRef<'_> = postcard::from_bytes(&bytes).expect("the bytes decode");
    let range = bytes.as_ptr_range();
    let cells = borrowed.lines[0].cells;
    assert!(!cells.is_empty() && range.contains(&cells.as_ptr()));
    let title = borrowed.title.expect("the frame has a title");
    assert!(range.contains(&title.as_ptr()));
    assert!(range.contains(&borrowed.new_graphemes[0].1.as_ptr()));
}

#[test]
fn the_default_style_has_the_default_id() {
    assert_eq!(STYLE_DEFAULT, 0);
    assert_eq!(Style::default().attrs, Attrs::empty());
}

/// A 200×60 keyframe is about 72 KB. The budget is 75,000 bytes.
#[test]
fn a_200_by_60_keyframe_with_distinct_cells_fits_the_size_budget() {
    let (cols, rows) = (200u16, 60u16);
    let lines = (0..rows)
        .map(|y| {
            let cells = (0..cols)
                .flat_map(|x| {
                    // Every cell has its own glyph. The 64 styles cycle.
                    let glyph = 0x100 + u32::from(y) * u32::from(cols) + u32::from(x);
                    pack_cell(glyph, 1 + x % 64)
                })
                .collect();
            RowUpdate { y, cells }
        })
        .collect();
    let mut palette = [0u32; 256];
    for (i, color) in palette.iter_mut().enumerate() {
        *color = 0x00ff_ffff - (i as u32) * 0x0101;
    }
    let frame = Frame {
        kind: FrameKind::Keyframe,
        cols,
        rows,
        lines,
        new_styles: (1..=64)
            .map(|id| {
                (
                    id,
                    Style {
                        fg: Color::Palette(id as u8),
                        attrs: Attrs::BOLD,
                        ..Style::default()
                    },
                )
            })
            .collect(),
        palette: Some(Palette {
            colors: palette,
            fg: 0x00ee_eeee,
            bg: 0x0011_1111,
            cursor: 0x00ff_ffff,
        }),
        ..sample_frame()
    };
    let size = encode(&frame).len();
    println!("200x60 keyframe: {size} bytes");
    assert!(size <= 75_000, "the keyframe is {size} bytes");
}
