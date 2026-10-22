//! A frame that does not follow the state of its pane changes nothing.

mod common;

use nh_proto::frame::FrameKind;
use proto_wasm::{Decoder, STATUS_APPLIED, STATUS_ERROR, STATUS_NEEDS_KEYFRAME};

use common::{PANE, Source, encode};

/// A decoder that has applied a keyframe and two deltas, and the source behind it.
fn running() -> (Source, Decoder) {
    let mut source = Source::new(40, 10);
    let mut decoder = Decoder::new(4096);
    source.write(b"\x1b[31mred\x1b[0m text\r\nsecond");
    assert_eq!(
        decoder.decode(&encode(&source.frame(FrameKind::Delta))),
        STATUS_APPLIED
    );
    for text in ["\x1b[2;1Hmore", "\x1b[3;1Hand more"] {
        source.write(text.as_bytes());
        assert_eq!(
            decoder.decode(&encode(&source.frame(FrameKind::Delta))),
            STATUS_APPLIED
        );
    }
    (source, decoder)
}

/// Everything about a pane that a frame could change.
#[derive(Debug, PartialEq)]
struct Snapshot {
    rev: u64,
    cells: Vec<u32>,
    styles: Vec<u16>,
    style_table: Vec<[u32; 4]>,
    dirty_rows: Vec<u16>,
}

fn state(decoder: &Decoder) -> Snapshot {
    let pane = decoder.pane(PANE).expect("the pane exists");
    Snapshot {
        rev: pane.rev,
        cells: pane.cells.clone(),
        styles: pane.styles.clone(),
        style_table: pane.style_table.clone(),
        dirty_rows: decoder.dirty_rows().to_vec(),
    }
}

#[test]
fn a_delta_for_another_revision_needs_a_keyframe_and_changes_nothing() {
    let (mut source, mut decoder) = running();
    source.write(b"missed");
    let missed = source.frame(FrameKind::Delta);
    source.write(b"next");
    let next = source.frame(FrameKind::Delta);
    let before = state(&decoder);
    assert_eq!(decoder.decode(&encode(&next)), STATUS_NEEDS_KEYFRAME);
    assert_eq!(state(&decoder), before);
    // The UI parses no bytes. It learns from the decoder which pane to name in `need_keyframe`.
    assert_eq!(decoder.last_pane(), PANE);
    assert_eq!(decoder.last_rev(), next.rev);
    // The pane does not accept the frame it missed in the wrong order either.
    assert_eq!(decoder.decode(&encode(&missed)), STATUS_APPLIED);
    assert_eq!(decoder.decode(&encode(&next)), STATUS_APPLIED);
}

#[test]
fn a_delta_of_another_epoch_or_size_needs_a_keyframe() {
    let (mut source, mut decoder) = running();
    source.resize(50, 12);
    let delta = source.frame(FrameKind::Delta);
    // The builder sends a keyframe after a resize. Pretend that it sent a delta.
    assert_eq!(delta.kind, FrameKind::Keyframe);
    let mut forged = delta.clone();
    forged.kind = FrameKind::Delta;
    forged.base_rev = decoder.pane(PANE).expect("pane").rev;
    let before = state(&decoder);
    assert_eq!(decoder.decode(&encode(&forged)), STATUS_NEEDS_KEYFRAME);
    assert_eq!(state(&decoder), before);
    assert_eq!(decoder.decode(&encode(&delta)), STATUS_APPLIED);
    assert_eq!(
        (
            decoder.pane(PANE).expect("pane").cols,
            decoder.pane(PANE).expect("pane").rows
        ),
        (50, 12)
    );
}

/// The default style as the renderer reads it: the colours of the palette (`xterm_palette`: light
/// grey on black), no attributes. Every cell that no SGR touched has this style.
const DEFAULT_STYLE: [u32; 4] = [0xe5e5_e5ff, 0x0000_00ff, 0xe5e5_e5ff, 0];

#[test]
fn the_default_style_has_the_colours_of_the_palette_in_every_epoch() {
    let mut source = Source::new(40, 10);
    let mut decoder = Decoder::new(256);
    source.write(b"plain");
    assert_eq!(
        decoder.decode(&encode(&source.frame(FrameKind::Delta))),
        STATUS_APPLIED
    );
    let pane = decoder.pane(PANE).expect("pane");
    assert_eq!(pane.styles[0], 0, "the first cell has the default style");
    assert_eq!(pane.style_table[0], DEFAULT_STYLE, "first keyframe");

    // A resize starts a new epoch. Its keyframe clears the style table, and the palette is equal
    // to the one before, so nothing else makes the decoder resolve the default style again.
    source.resize(50, 12);
    source.write(b"plain");
    let keyframe = source.frame(FrameKind::Delta);
    assert_eq!(keyframe.kind, FrameKind::Keyframe);
    assert_eq!(decoder.decode(&encode(&keyframe)), STATUS_APPLIED);
    let pane = decoder.pane(PANE).expect("pane");
    assert_eq!(
        pane.style_table[0], DEFAULT_STYLE,
        "keyframe of a new epoch"
    );
    assert!(pane.style_table_dirty, "the renderer uploads the new table");
}

#[test]
fn a_delta_for_a_pane_that_the_decoder_does_not_know_needs_a_keyframe() {
    let mut source = Source::new(20, 5);
    source.write(b"a");
    let _first = source.frame(FrameKind::Delta);
    source.write(b"b");
    let delta = source.frame(FrameKind::Delta);
    let mut decoder = Decoder::new(256);
    assert_eq!(decoder.decode(&encode(&delta)), STATUS_NEEDS_KEYFRAME);
    assert!(decoder.pane(PANE).is_none());
}

#[test]
fn a_keyframe_always_applies() {
    let (mut source, mut decoder) = running();
    source.write(b"later");
    let _lost = source.frame(FrameKind::Delta);
    source.write(b"even later");
    let keyframe = source.frame(FrameKind::Keyframe);
    assert_eq!(decoder.decode(&encode(&keyframe)), STATUS_APPLIED);
    let pane = decoder.pane(PANE).expect("pane");
    assert_eq!(pane.rev, keyframe.rev);
    assert_eq!(decoder.dirty_rows().len(), usize::from(pane.rows));
}

#[test]
fn bytes_that_are_not_a_frame_give_an_error_and_change_nothing() {
    let (mut source, mut decoder) = running();
    source.write(b"x");
    let frame = encode(&source.frame(FrameKind::Delta));
    let before = state(&decoder);
    let garbage: [&[u8]; 4] = [
        b"",
        b"\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff",
        &frame[..frame.len() / 2],
        &frame[..frame.len() - 1],
    ];
    for bytes in garbage {
        assert_eq!(decoder.decode(bytes), STATUS_ERROR, "{} bytes", bytes.len());
        assert_eq!(state(&decoder), before);
    }
    // The frame itself still applies afterwards.
    assert_eq!(decoder.decode(&frame), STATUS_APPLIED);
}

#[test]
fn a_row_of_the_wrong_size_is_an_error() {
    let (mut source, mut decoder) = running();
    source.write(b"x");
    let mut frame = source.frame(FrameKind::Delta);
    frame.lines[0].cells.truncate(12);
    let before = state(&decoder);
    assert_eq!(decoder.decode(&encode(&frame)), STATUS_ERROR);
    assert_eq!(state(&decoder), before);
}

#[test]
fn a_full_atlas_is_reported_and_a_reset_gives_the_glyphs_on_screen_new_slots() {
    let mut source = Source::new(40, 5);
    // Only 8 slots: 7 usable. The screen shows more than 7 distinct glyphs.
    let mut decoder = Decoder::new(8);
    source.write("abcdefghijklmnop wide: 日本".as_bytes());
    assert_eq!(
        decoder.decode(&encode(&source.frame(FrameKind::Delta))),
        STATUS_APPLIED
    );
    assert!(decoder.atlas_full());
    decoder.atlas_reset();
    assert!(decoder.pending().len() <= 7);
    // The decoder cannot show more than the atlas holds, and it says so again.
    assert!(decoder.atlas_full());

    // With room for the screen, a reset puts every visible glyph in a slot again.
    let mut roomy = Decoder::new(256);
    assert_eq!(
        roomy.decode(&encode(&source.frame(FrameKind::Keyframe))),
        STATUS_APPLIED
    );
    let before: Vec<u32> = roomy.pane(PANE).expect("pane").cells.clone();
    roomy.atlas_reset();
    assert!(!roomy.atlas_full());
    let pane = roomy.pane(PANE).expect("pane");
    let mut slots = std::collections::BTreeSet::new();
    for (i, cell) in pane.cells.iter().enumerate() {
        let slot = cell & proto_wasm::SLOT_MASK;
        assert_eq!(
            slot != 0,
            before[i] & proto_wasm::SLOT_MASK != 0,
            "cell {i} keeps its blankness"
        );
        if slot != 0 {
            assert_eq!(
                roomy.atlas().glyph_of(slot).map(|(g, _)| g),
                Some(pane.glyphs[i])
            );
            slots.insert(slot);
        }
    }
    let queued: std::collections::BTreeSet<u32> = roomy.pending().iter().map(|p| p.slot).collect();
    assert_eq!(
        queued, slots,
        "every glyph on screen is queued for the atlas again"
    );
}
