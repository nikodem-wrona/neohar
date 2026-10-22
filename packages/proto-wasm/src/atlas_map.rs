//! The map from glyphs to slots of the glyph atlas texture.
//!
//! The UI rasterizes each glyph once and puts it in a slot. The decoder decides which slot a glyph
//! gets, so that the cells it writes already hold slots. A glyph is a code point, or a grapheme id
//! with the high bit set. A variant is the bold and italic bits, because the browser draws them
//! with other fonts. A wide glyph takes two neighbouring slots. Slot 0 is empty: it draws nothing.
//!
//! The atlas serves every pane, but a grapheme id of the wire belongs to one pane. So the map
//! interns the text of a cluster and gives it an id of its own (also with the high bit set). Two
//! panes that show the same cluster share one slot, and two panes whose clusters have the same
//! wire id do not.

use std::collections::HashMap;

use nh_proto::cells::GRAPHEME_FLAG;

/// A glyph the UI must rasterize, as the decoder reports it.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pending {
    pub slot: u32,
    pub glyph: u32,
    pub variant: u32,
    /// 1 when the glyph is two cells wide and takes slots `slot` and `slot + 1`.
    pub wide: u32,
}

/// The slots of the atlas and what is in them.
pub struct AtlasMap {
    slots: u32,
    next: u32,
    map: HashMap<(u32, u8), u32>,
    /// For each slot, the glyph and variant it holds.
    held: Vec<(u32, u8)>,
    pending: Vec<Pending>,
    full: bool,
    /// The id of each cluster text that the map knows.
    cluster_ids: HashMap<Box<[u8]>, u32>,
    /// The text of each cluster, by id.
    clusters: Vec<Box<[u8]>>,
}

impl AtlasMap {
    /// A map for an atlas of `slots` slots. The map allocates its tables now, so that it does not
    /// allocate when it decodes.
    pub fn new(slots: u32) -> Self {
        let capacity = slots as usize;
        Self {
            slots,
            next: 1,
            map: HashMap::with_capacity(capacity),
            held: vec![(0, 0); capacity],
            pending: Vec::with_capacity(1024),
            full: false,
            cluster_ids: HashMap::new(),
            clusters: Vec::new(),
        }
    }

    /// The glyph of a cluster of several code points: its own id, with the high bit set. The same
    /// text always gets the same glyph, whatever the pane. Only a text that the map has not seen
    /// allocates.
    pub fn cluster(&mut self, text: &[u8]) -> u32 {
        if let Some(&id) = self.cluster_ids.get(text) {
            return id | GRAPHEME_FLAG;
        }
        let id = self.clusters.len() as u32;
        self.cluster_ids.insert(text.into(), id);
        self.clusters.push(text.into());
        id | GRAPHEME_FLAG
    }

    /// The text of a glyph that [`AtlasMap::cluster`] made, as UTF-8.
    pub fn cluster_text(&self, glyph: u32) -> Option<&[u8]> {
        if glyph & GRAPHEME_FLAG == 0 {
            return None;
        }
        self.clusters
            .get((glyph & !GRAPHEME_FLAG) as usize)
            .map(|text| &**text)
    }

    /// The slot of a glyph. A new glyph gets the next free slot and joins the pending list. When no
    /// slot is free, the result is slot 0 and [`AtlasMap::is_full`] is true.
    pub fn slot(&mut self, glyph: u32, variant: u8, wide: bool) -> u32 {
        if let Some(&slot) = self.map.get(&(glyph, variant)) {
            return slot;
        }
        let needed = if wide { 2 } else { 1 };
        if self.next + needed > self.slots {
            self.full = true;
            return 0;
        }
        let slot = self.next;
        self.next += needed;
        self.map.insert((glyph, variant), slot);
        self.held[slot as usize] = (glyph, variant);
        self.pending.push(Pending {
            slot,
            glyph,
            variant: u32::from(variant),
            wide: u32::from(wide),
        });
        slot
    }

    /// The glyph and variant in a slot, if any.
    pub fn glyph_of(&self, slot: u32) -> Option<(u32, u8)> {
        (slot != 0 && slot < self.next).then(|| self.held[slot as usize])
    }

    /// The glyphs the UI has not rasterized yet.
    pub fn pending(&self) -> &[Pending] {
        &self.pending
    }

    pub fn clear_pending(&mut self) {
        self.pending.clear();
    }

    /// Whether a glyph had no slot. The UI then calls [`AtlasMap::reset`] through the decoder.
    pub fn is_full(&self) -> bool {
        self.full
    }

    /// Forgets every slot and every cluster. The decoder gives the glyphs on screen new slots.
    pub fn reset(&mut self) {
        self.map.clear();
        self.cluster_ids.clear();
        self.clusters.clear();
        self.pending.clear();
        self.next = 1;
        self.full = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_glyph_keeps_its_slot_and_a_variant_has_its_own() {
        let mut a = AtlasMap::new(16);
        let plain = a.slot('a' as u32, 0, false);
        assert_eq!(plain, 1);
        assert_eq!(a.slot('a' as u32, 0, false), 1);
        assert_eq!(a.slot('a' as u32, 1, false), 2);
        assert_eq!(a.pending().len(), 2);
        assert_eq!(a.glyph_of(2), Some(('a' as u32, 1)));
        assert_eq!(a.glyph_of(0), None);
    }

    #[test]
    fn a_cluster_has_one_glyph_for_its_text_and_a_reset_forgets_it() {
        let mut a = AtlasMap::new(16);
        let e = a.cluster("e\u{301}".as_bytes());
        assert_ne!(e & GRAPHEME_FLAG, 0);
        assert_eq!(a.cluster("e\u{301}".as_bytes()), e);
        let other = a.cluster("a\u{301}".as_bytes());
        assert_ne!(other, e);
        assert_eq!(a.cluster_text(e), Some("e\u{301}".as_bytes()));
        assert_eq!(a.cluster_text('x' as u32), None);
        a.reset();
        assert_eq!(a.cluster_text(e), None);
    }

    #[test]
    fn a_wide_glyph_takes_two_slots() {
        let mut a = AtlasMap::new(16);
        assert_eq!(a.slot('日' as u32, 0, true), 1);
        assert_eq!(a.slot('b' as u32, 0, false), 3);
        assert_eq!(
            a.pending()[0],
            Pending {
                slot: 1,
                glyph: '日' as u32,
                variant: 0,
                wide: 1
            }
        );
    }

    #[test]
    fn a_full_atlas_answers_slot_0_and_a_reset_frees_it() {
        let mut a = AtlasMap::new(4);
        assert_eq!(a.slot(1, 0, false), 1);
        assert_eq!(a.slot(2, 0, false), 2);
        assert_eq!(a.slot(3, 0, false), 3);
        assert!(!a.is_full());
        assert_eq!(a.slot(4, 0, false), 0);
        assert!(a.is_full());
        // A wide glyph needs two slots, and only one is left in this atlas.
        a.reset();
        assert_eq!(a.slot(9, 0, false), 1);
        assert_eq!(a.slot(8, 0, false), 2);
        assert_eq!(a.slot(7, 0, true), 0);
        assert!(a.is_full());
    }
}
