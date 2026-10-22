//! Screen detection: the fallback source of the state and of the dialogs of a harness.
//!
//! A screen manifest is data: rules over the text of some rows of the screen. [`dsl`] reads and
//! compiles a manifest, [`engine`] runs it on a [`view::Screen`], and the built-in manifests
//! ([`builtin`]) are written from the recordings of the harnesses. A screen match never overwrites
//! structured state: the arbiter decides what to do with it.

pub mod dsl;
pub mod engine;
pub mod view;

pub use dsl::{ScreenError, ScreenManifest, ScreenState};
pub use engine::{Detector, Dialog, DialogOption, Evaluation, ScreenMatch};
pub use view::{Cell, Screen};

/// The built-in screen manifest with the id, such as `claude` or `codex`. A test makes sure that
/// each one compiles.
pub fn builtin(id: &str) -> Option<ScreenManifest> {
    let text = match id {
        "claude" => include_str!("../../screens/claude.toml"),
        "codex" => include_str!("../../screens/codex.toml"),
        _ => return None,
    };
    ScreenManifest::parse(text).ok()
}

#[cfg(test)]
mod tests;
