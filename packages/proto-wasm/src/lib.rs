//! The frame decoder that the webview loads.
//!
//! The decoder reads frames of `nh-proto` into typed arrays that live in WASM memory. The UI makes
//! views of these arrays and uploads the changed rows to the GPU. The API has numbers and
//! pointers only.

pub mod atlas_map;
pub mod decoder;
mod wasm;

pub use atlas_map::{AtlasMap, Pending};
pub use decoder::{
    Decoder, FLAG_CURSOR, FLAG_SPACER, FLAG_WIDE, Pane, SLOT_MASK, STATUS_APPLIED, STATUS_ERROR,
    STATUS_NEEDS_KEYFRAME,
};
