//! Interners for styles and grapheme clusters.
//!
//! An id is stable for the life of an epoch. When an interner is full, the pane bumps its
//! `reset_epoch`, clears the interners and sends keyframes.

use std::{collections::HashMap, hash::Hash};

/// Maps values to small ids. Id 0 is reserved: for styles it is the default style.
pub struct Interner<V: Clone + Eq + Hash> {
    ids: HashMap<V, u32>,
    /// The largest id.
    max_id: u32,
}

impl<V: Clone + Eq + Hash> Interner<V> {
    /// An interner whose ids are `1..=max_id`.
    pub fn new(max_id: u32) -> Self {
        Self {
            ids: HashMap::new(),
            max_id,
        }
    }

    /// The id of `value`, or `None` when it is new and the interner is full.
    pub fn intern(&mut self, value: &V) -> Option<u32> {
        if let Some(&id) = self.ids.get(value) {
            return Some(id);
        }
        let id = u32::try_from(self.ids.len()).ok()? + 1;
        if id > self.max_id {
            return None;
        }
        self.ids.insert(value.clone(), id);
        Some(id)
    }

    pub fn clear(&mut self) {
        self.ids.clear();
    }
}

/// A set of small ids, as a bit set.
#[derive(Clone, Debug, Default)]
pub struct IdSet(Vec<u64>);

impl IdSet {
    /// Adds `id`. Returns whether it was new.
    pub fn insert(&mut self, id: u32) -> bool {
        let word = id as usize / 64;
        if self.0.len() <= word {
            self.0.resize(word + 1, 0);
        }
        let bit = 1u64 << (id % 64);
        let new = self.0[word] & bit == 0;
        self.0[word] |= bit;
        new
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_start_at_one_repeat_for_equal_values_and_stop_at_the_maximum() {
        let mut i = Interner::new(2);
        assert_eq!(i.intern(&"a"), Some(1));
        assert_eq!(i.intern(&"b"), Some(2));
        assert_eq!(i.intern(&"a"), Some(1));
        assert_eq!(i.intern(&"c"), None);
        i.clear();
        assert_eq!(i.intern(&"c"), Some(1));
    }

    #[test]
    fn an_id_set_remembers_ids() {
        let mut s = IdSet::default();
        assert!(s.insert(70_000));
        assert!(!s.insert(70_000));
        assert!(s.insert(69_999));
    }
}
