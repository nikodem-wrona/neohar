//! Helpers for the tests of the layout: counters in memory instead of the database.

use std::collections::HashMap;

use nh_proto::api::ids::Ref;
use ulid::Ulid;

use super::model::{Alloc, LayoutError};

/// Ids and numbers from counters. The ids are 1, 2, 3 and so on, so they sort in the order of
/// their creation, as ULIDs do.
#[derive(Default)]
pub struct Counters {
    next_id: u128,
    numbers: HashMap<String, u32>,
}

impl Counters {
    pub fn new() -> Self {
        Self {
            next_id: 1_000,
            numbers: HashMap::new(),
        }
    }
}

impl Alloc for Counters {
    fn ulid(&mut self) -> Result<Ulid, LayoutError> {
        self.next_id += 1;
        Ok(Ulid::from(self.next_id))
    }

    fn number(&mut self, scope: &str) -> Result<u32, LayoutError> {
        let next = self.numbers.entry(scope.to_owned()).or_insert(0);
        *next += 1;
        Ok(*next)
    }
}

/// A reference from text: a handle or an id.
pub fn r<K: nh_proto::api::ids::RefKind + std::str::FromStr<Err = nh_proto::api::ids::IdError>>(
    text: &str,
) -> Ref<K> {
    text.parse().unwrap_or_else(|e| panic!("{text}: {e}"))
}
