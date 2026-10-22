//! Terminal queries that a program sends to its terminal, and who answers them.
//!
//! While no core is attached, the holder answers only the queries whose answer does not depend on
//! the screen. The core answers the rest when it attaches again. The reply bytes are
//! here, so that the holder and the VT engine of the core give the same answer: a program must see
//! one terminal, whatever process answers.

use serde::{Deserialize, Serialize};

/// The kind of a terminal query. The holder queues the kinds that need the screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(test, derive(proptest_derive::Arbitrary))]
pub enum QueryKind {
    /// Primary device attributes (`CSI c`).
    Da1,
    /// Secondary device attributes (`CSI > c`).
    Da2,
    /// Tertiary device attributes (`CSI = c`).
    Da3,
    /// Terminal name and version (`CSI > q`).
    XtVersion,
    /// Report of a mode setting (`CSI ? Ps $ p` when `private` is true, else `CSI Ps $ p`).
    Decrqm { private: bool, mode: u16 },
    /// Cursor position report (`CSI 6 n`). It needs the screen.
    CursorPosition,
    /// Colour query of OSC `code` (10, 11 or 12). It needs the palette.
    OscColor { code: u8 },
    /// Window size report (`CSI Ps t`). It needs the size and the pixel metrics.
    WindowSize { op: u16 },
}

impl QueryKind {
    /// Whether the answer depends on the screen, so that only a core can give it.
    pub fn needs_screen(self) -> bool {
        matches!(
            self,
            Self::CursorPosition | Self::OscColor { .. } | Self::WindowSize { .. }
        )
    }
}

/// The reply to DA1: a VT220-level terminal with ANSI colour.
pub const DA1: &[u8] = b"\x1b[?62;22c";

/// The reply to DA2: a VT100-class terminal, firmware 100, no ROM cartridge.
pub const DA2: &[u8] = b"\x1b[>0;100;1c";

/// The reply to DA3: the unit id `00000000`, as `xterm` sends.
pub const DA3: &[u8] = b"\x1bP!|00000000\x1b\\";

/// The reply to XTVERSION: the terminal name and its version. The version is the version of the
/// workspace, so it changes with each release.
pub const XTVERSION: &[u8] =
    concat!("\x1bP>|neohar ", env!("CARGO_PKG_VERSION"), "\x1b\\").as_bytes();

/// The reply to DECRQM for `mode`. `state` is `Some(true)` for a mode that is set, `Some(false)`
/// for one that is reset, and `None` for a mode that the terminal does not know.
pub fn decrqm(private: bool, mode: u16, state: Option<bool>) -> Vec<u8> {
    let value = match state {
        Some(true) => 1,
        Some(false) => 2,
        None => 0,
    };
    let marker = if private { "?" } else { "" };
    format!("\x1b[{marker}{mode};{value}$y").into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fixed_replies_are_complete_sequences() {
        for csi in [DA1, DA2] {
            assert!(csi.starts_with(b"\x1b[") && csi.ends_with(b"c"));
        }
        for dcs in [DA3, XTVERSION] {
            assert!(dcs.starts_with(b"\x1bP") && dcs.ends_with(b"\x1b\\"));
        }
    }

    #[test]
    fn xtversion_names_the_version_of_the_crate() {
        let text = std::str::from_utf8(XTVERSION).expect("UTF-8");
        assert_eq!(
            text,
            format!("\x1bP>|neohar {}\x1b\\", env!("CARGO_PKG_VERSION"))
        );
    }

    #[test]
    fn decrqm_reports_set_reset_and_unknown() {
        assert_eq!(decrqm(true, 2004, Some(true)), b"\x1b[?2004;1$y");
        assert_eq!(decrqm(true, 1004, Some(false)), b"\x1b[?1004;2$y");
        assert_eq!(decrqm(false, 4, None), b"\x1b[4;0$y");
    }

    #[test]
    fn only_screen_dependent_queries_need_the_core() {
        assert!(!QueryKind::Da1.needs_screen());
        assert!(
            !QueryKind::Decrqm {
                private: true,
                mode: 2004
            }
            .needs_screen()
        );
        assert!(QueryKind::CursorPosition.needs_screen());
        assert!(QueryKind::OscColor { code: 11 }.needs_screen());
        assert!(QueryKind::WindowSize { op: 14 }.needs_screen());
    }
}
