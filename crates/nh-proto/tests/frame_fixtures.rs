//! Frozen fixtures of the frame wire format. See `tests/common/mod.rs`.

mod common;

use nh_proto::{
    cells::pack_cell,
    frame::{
        AckState, Cursor, CursorShape, Frame, FrameKind, Palette, Placement, RowUpdate, Selection,
        SelectionMode, SelectionPoint, Viewport,
    },
    modes::ModeFlags,
    style::{Attrs, CellWidth, Color, Style, UnderlineKind},
};

fn row(y: u16, text: &str) -> RowUpdate {
    let cells = text.chars().flat_map(|c| pack_cell(c as u32, 1)).collect();
    RowUpdate { y, cells }
}

fn keyframe() -> Frame {
    let mut colors = [0u32; 256];
    for (i, c) in colors.iter_mut().enumerate() {
        *c = 0x0001_0203 * (i as u32 % 5);
    }
    Frame {
        pane: 7,
        reset_epoch: 2,
        base_rev: 0,
        rev: 31,
        kind: FrameKind::Keyframe,
        cols: 5,
        rows: 2,
        lines: vec![row(0, "hello"), row(1, "world")],
        new_styles: vec![(
            1,
            Style {
                fg: Color::Palette(2),
                bg: Color::Rgb(10, 20, 30),
                underline_color: Color::Default,
                attrs: Attrs::BOLD | Attrs::ITALIC,
                underline: UnderlineKind::Curly,
                width: CellWidth::Wide,
            },
        )],
        new_graphemes: vec![(3, "e\u{301}".to_owned())],
        cursor: Cursor {
            x: 2,
            y: 1,
            shape: CursorShape::Bar,
            visible: true,
            blink: true,
        },
        viewport: Viewport {
            top_line: 120,
            total_lines: 4_000,
        },
        modes: ModeFlags::BRACKETED_PASTE | ModeFlags::MOUSE_SGR | ModeFlags::AUTOWRAP,
        title: Some("zsh".to_owned()),
        bell: true,
        images: vec![Placement {
            id: 1,
            blob_hash: [9; 32],
            col: -1,
            row: 2,
            cols: 4,
            rows: 3,
            z: -1,
            offset_px: (3, -4),
        }],
        selection: Some(Selection {
            start: SelectionPoint { col: 0, line: 120 },
            end: SelectionPoint { col: 4, line: 121 },
            mode: SelectionMode::Block,
        }),
        ack_status: vec![(5, AckState::Acked), (6, AckState::Unconfirmed)],
        palette: Some(Palette {
            colors,
            fg: 0x00ee_eeee,
            bg: 0x0011_1111,
            cursor: 0x00ff_ffff,
        }),
    }
}

fn delta() -> Frame {
    Frame {
        kind: FrameKind::Delta,
        base_rev: 31,
        rev: 32,
        lines: vec![row(1, "WORLD")],
        new_styles: Vec::new(),
        new_graphemes: Vec::new(),
        title: None,
        bell: false,
        images: Vec::new(),
        selection: None,
        ack_status: Vec::new(),
        palette: None,
        ..keyframe()
    }
}

#[test]
fn frame_fixtures_are_frozen() {
    common::check_fixtures(
        "frame",
        "frame",
        &[("keyframe_full", keyframe()), ("delta_minimal", delta())],
    );
}
