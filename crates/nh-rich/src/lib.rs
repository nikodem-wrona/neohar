//! Rich content for the cards and the plan view.
//!
//! The core renders a payload once, on open, and every client shows the same result. This crate has
//! no workspace dependency and no async code: each function takes text and gives a value. The core
//! maps the types to the types of the API.
//!
//! All offsets are UTF-16 code units, because JavaScript slices strings that way.

pub mod command;
pub mod diff;
pub mod highlight;
pub mod markdown;
pub mod project;
pub mod text;

pub use highlight::{Span, Spans, class};
pub use markdown::{Markdown, markdown};
pub use text::{Capped, OUTPUT_CAP, cap, cap_output, strip_terminal_controls};
