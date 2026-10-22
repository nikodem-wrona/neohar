//! The glyph atlas is shared by every pane, but a grapheme id belongs to one pane.

mod common;

use nh_proto::frame::FrameKind;
use proto_wasm::{Decoder, SLOT_MASK, STATUS_APPLIED};

use common::{Source, encode};

/// Two panes whose first cluster has the same id on the wire and different text.
fn two_panes(decoder: &mut Decoder) {
    for (pane, text) in [(1, "e\u{301}"), (2, "a\u{301}")] {
        let mut source = Source::with_pane(pane, 10, 2);
        source.write(text.as_bytes());
        assert_eq!(
            decoder.decode(&encode(&source.frame(FrameKind::Keyframe))),
            STATUS_APPLIED
        );
    }
}

#[test]
fn clusters_with_the_same_id_in_two_panes_get_two_slots() {
    let mut decoder = Decoder::new(64);
    two_panes(&mut decoder);
    let (a, b) = (
        decoder.pane(1).expect("pane 1"),
        decoder.pane(2).expect("pane 2"),
    );
    assert_eq!(a.text_at(0, 0), "e\u{301}");
    assert_eq!(b.text_at(0, 0), "a\u{301}");
    assert_ne!(
        a.cells[0] & SLOT_MASK,
        b.cells[0] & SLOT_MASK,
        "two different clusters share one atlas slot"
    );
}

#[test]
fn a_pending_cluster_can_be_read_without_its_pane() {
    let mut decoder = Decoder::new(64);
    two_panes(&mut decoder);
    // The pending list holds the glyphs of the last frame only (pane 2). It must give the text
    // of its cluster without the pane, because the atlas serves every pane.
    let pending = decoder.pending().to_vec();
    assert_eq!(pending.len(), 1);
    assert_eq!(
        decoder.atlas_text(pending[0].glyph),
        Some("a\u{301}".as_bytes())
    );
}

#[test]
fn the_same_cluster_in_two_panes_shares_a_slot() {
    let mut decoder = Decoder::new(64);
    for pane in [1, 2] {
        let mut source = Source::with_pane(pane, 10, 2);
        source.write("e\u{301}".as_bytes());
        decoder.decode(&encode(&source.frame(FrameKind::Keyframe)));
    }
    let (a, b) = (
        decoder.pane(1).expect("pane 1"),
        decoder.pane(2).expect("pane 2"),
    );
    assert_eq!(a.cells[0] & SLOT_MASK, b.cells[0] & SLOT_MASK);
}

#[test]
fn a_reset_keeps_each_cluster_with_its_text() {
    let mut decoder = Decoder::new(64);
    two_panes(&mut decoder);
    decoder.atlas_reset();
    let pending = decoder.pending().to_vec();
    let mut texts: Vec<String> = pending
        .iter()
        .map(|p| {
            String::from_utf8(decoder.atlas_text(p.glyph).expect("text").to_vec()).expect("utf-8")
        })
        .collect();
    texts.sort();
    assert_eq!(texts, ["a\u{301}", "e\u{301}"]);
    let (a, b) = (
        decoder.pane(1).expect("pane 1"),
        decoder.pane(2).expect("pane 2"),
    );
    assert_ne!(a.cells[0] & SLOT_MASK, b.cells[0] & SLOT_MASK);
}
