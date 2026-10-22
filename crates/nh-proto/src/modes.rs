//! Terminal mode flags that the UI needs to encode input and to draw.

use bitflags::bitflags;
use serde::{Deserialize, Serialize};

bitflags! {
    /// The modes of a pane's screen. The core sends them in every frame.
    ///
    /// The kitty keyboard flags (bits 17 to 21) and the `modifyOtherKeys` level (bits 22 and 23)
    /// are multi-bit fields. Use [`ModeFlags::kitty_flags`] and [`ModeFlags::modify_other_keys`].
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
    #[serde(transparent)]
    pub struct ModeFlags: u32 {
        const ALT_SCREEN = 1 << 0;
        /// DECCKM, application cursor keys.
        const APP_CURSOR = 1 << 1;
        /// DECKPAM, application keypad.
        const APP_KEYPAD = 1 << 2;
        /// Mouse X10 reporting (mode 9).
        const MOUSE_X10 = 1 << 3;
        /// Mouse button reporting (mode 1000).
        const MOUSE_1000 = 1 << 4;
        /// Mouse drag reporting (mode 1002).
        const MOUSE_1002 = 1 << 5;
        /// Mouse motion reporting (mode 1003).
        const MOUSE_1003 = 1 << 6;
        /// SGR mouse encoding (mode 1006).
        const MOUSE_SGR = 1 << 7;
        /// UTF-8 mouse encoding (mode 1005).
        const MOUSE_UTF8 = 1 << 8;
        /// urxvt mouse encoding (mode 1015).
        const MOUSE_URXVT = 1 << 9;
        /// Alternate scroll (mode 1007).
        const ALTERNATE_SCROLL = 1 << 10;
        const BRACKETED_PASTE = 1 << 11;
        const FOCUS_EVENTS = 1 << 12;
        /// DECOM, origin mode.
        const ORIGIN = 1 << 13;
        /// DECAWM, autowrap.
        const AUTOWRAP = 1 << 14;
        /// IRM, insert mode.
        const INSERT = 1 << 15;
        /// LNM, line feed and new line mode.
        const LNM = 1 << 16;
        /// Kitty keyboard flag 1: disambiguate escape codes.
        const KITTY_DISAMBIGUATE = 1 << 17;
        /// Kitty keyboard flag 2: report event types.
        const KITTY_REPORT_EVENT_TYPES = 1 << 18;
        /// Kitty keyboard flag 4: report alternate keys.
        const KITTY_REPORT_ALTERNATE_KEYS = 1 << 19;
        /// Kitty keyboard flag 8: report all keys as escape codes.
        const KITTY_REPORT_ALL_KEYS = 1 << 20;
        /// Kitty keyboard flag 16: report associated text.
        const KITTY_REPORT_TEXT = 1 << 21;
    }
}

/// The first bit of the kitty keyboard flag field.
const KITTY_SHIFT: u32 = 17;
/// The mask of the five kitty keyboard flag bits, after the shift.
const KITTY_MASK: u32 = 0b1_1111;
/// The first bit of the `modifyOtherKeys` field.
const MOK_SHIFT: u32 = 22;
/// The mask of the `modifyOtherKeys` level, after the shift.
const MOK_MASK: u32 = 0b11;

impl ModeFlags {
    /// The kitty keyboard flags as the protocol numbers them (1, 2, 4, 8, 16).
    pub fn kitty_flags(self) -> u8 {
        ((self.bits() >> KITTY_SHIFT) & KITTY_MASK) as u8
    }

    /// Returns these flags with the kitty keyboard flags set to `flags`. Bits above 16 are ignored.
    pub fn with_kitty_flags(self, flags: u8) -> Self {
        let cleared = self.bits() & !(KITTY_MASK << KITTY_SHIFT);
        Self::from_bits_retain(cleared | ((u32::from(flags) & KITTY_MASK) << KITTY_SHIFT))
    }

    /// The `modifyOtherKeys` level, 0 to 2.
    pub fn modify_other_keys(self) -> u8 {
        ((self.bits() >> MOK_SHIFT) & MOK_MASK) as u8
    }

    /// Returns these flags with the `modifyOtherKeys` level set. A level above 2 is clamped to 2.
    pub fn with_modify_other_keys(self, level: u8) -> Self {
        let cleared = self.bits() & !(MOK_MASK << MOK_SHIFT);
        Self::from_bits_retain(cleared | (u32::from(level.min(2)) << MOK_SHIFT))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kitty_flags_use_their_own_five_bits() {
        let modes = ModeFlags::ALT_SCREEN.with_kitty_flags(0b1_0101);
        assert_eq!(modes.kitty_flags(), 0b1_0101);
        assert!(modes.contains(ModeFlags::ALT_SCREEN));
        assert!(
            modes.contains(ModeFlags::KITTY_DISAMBIGUATE | ModeFlags::KITTY_REPORT_ALTERNATE_KEYS)
        );
        assert!(!modes.contains(ModeFlags::KITTY_REPORT_EVENT_TYPES));
        assert_eq!(modes.with_kitty_flags(0).kitty_flags(), 0);
        assert_eq!(modes.with_kitty_flags(0xff).kitty_flags(), 0b1_1111);
    }

    #[test]
    fn modify_other_keys_is_two_bits_and_clamped() {
        let modes = ModeFlags::BRACKETED_PASTE.with_modify_other_keys(2);
        assert_eq!(modes.modify_other_keys(), 2);
        assert!(modes.contains(ModeFlags::BRACKETED_PASTE));
        assert_eq!(modes.with_modify_other_keys(9).modify_other_keys(), 2);
        assert_eq!(modes.with_modify_other_keys(0).modify_other_keys(), 0);
    }

    #[test]
    fn the_wire_form_is_the_bits_and_keeps_unknown_bits() {
        let modes = ModeFlags::from_bits_retain(0x8000_0001);
        let bytes = postcard::to_stdvec(&modes).expect("the flags serialize");
        assert_eq!(
            bytes,
            postcard::to_stdvec(&0x8000_0001u32).expect("a u32 serializes")
        );
        let back: ModeFlags = postcard::from_bytes(&bytes).expect("the bytes decode");
        assert_eq!(back.bits(), 0x8000_0001);
    }

    #[test]
    fn attrs_are_one_byte_on_the_wire() {
        let bytes = postcard::to_stdvec(&(crate::style::Attrs::BOLD | crate::style::Attrs::BLINK))
            .expect("the attributes serialize");
        assert_eq!(bytes, [0b100_0001]);
    }
}
