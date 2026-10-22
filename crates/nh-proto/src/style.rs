//! Cell styles. A style is interned per pane epoch and sent once in `new_styles`.

use bitflags::bitflags;
use serde::{Deserialize, Serialize};

/// A foreground, background or underline colour.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(test, derive(proptest_derive::Arbitrary))]
pub enum Color {
    /// The theme's default colour for this role.
    #[default]
    Default,
    /// An index into the pane's 256-colour palette.
    Palette(u8),
    /// A 24-bit colour.
    Rgb(u8, u8, u8),
}

bitflags! {
    /// Text attributes of a cell.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[serde(transparent)]
    pub struct Attrs: u8 {
        const BOLD = 1 << 0;
        const DIM = 1 << 1;
        const ITALIC = 1 << 2;
        const INVERSE = 1 << 3;
        const STRIKETHROUGH = 1 << 4;
        const HIDDEN = 1 << 5;
        const BLINK = 1 << 6;
    }
}

/// The shape of the underline of a cell.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(test, derive(proptest_derive::Arbitrary))]
pub enum UnderlineKind {
    #[default]
    None,
    Single,
    Double,
    Curly,
    Dotted,
    Dashed,
}

/// How many columns a cell covers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(test, derive(proptest_derive::Arbitrary))]
pub enum CellWidth {
    /// A normal one-column cell.
    #[default]
    Single,
    /// The first column of a wide glyph.
    Wide,
    /// The second column of a wide glyph. It draws nothing.
    Spacer,
}

/// The full style of a cell.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(test, derive(proptest_derive::Arbitrary))]
pub struct Style {
    pub fg: Color,
    pub bg: Color,
    pub underline_color: Color,
    pub attrs: Attrs,
    pub underline: UnderlineKind,
    pub width: CellWidth,
}
