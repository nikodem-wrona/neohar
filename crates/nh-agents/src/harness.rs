//! Which of the two harnesses a manifest describes.
//!
//! The manifest of a wrapper has its own id (`cc`) and `extends` another manifest. The harness of
//! the wrapper is the harness at the end of that chain. A manifest that does not end at `claude`
//! or `codex` describes no harness that neohar knows, and the matcher leaves it out.

use nh_proto::api::common::Harness;

use crate::manifest::Loaded;

/// How many `extends` the search follows. The loader refuses a loop, so this is only a bound.
const MAX_CHAIN: usize = 8;

/// The text of a harness in a manifest id, in a path and in a log.
pub fn name(harness: Harness) -> &'static str {
    match harness {
        Harness::Claude => "claude",
        Harness::Codex => "codex",
    }
}

/// The harness of the manifest `id`, found through `extends`.
pub fn harness_of(loaded: &Loaded, id: &str) -> Option<Harness> {
    let mut id = id;
    for _ in 0..MAX_CHAIN {
        match id {
            "claude" => return Some(Harness::Claude),
            "codex" => return Some(Harness::Codex),
            _ => {}
        }
        id = loaded.get(id)?.extends.as_deref()?;
    }
    None
}
