//! Mouse reports (`ctlseqs`: "Mouse Tracking", "Mouse Tracking Extensions").
//!
//! Notes on the choices:
//!
//! - The mode that reports the most wins when several are set, and the encodings rank SGR, urxvt,
//!   UTF-8, legacy (see [`InputModes`]).
//! - Coordinates are 1-based on the wire. The legacy form cannot write a coordinate above 223 and
//!   the UTF-8 form cannot write one above 2015. An event beyond the limit is dropped.
//! - A wheel notch is one press of button 64 (up), 65 (down), 66 (left) or 67 (right). One event
//!   with several notches makes several reports, at most [`MAX_NOTCHES`] for each axis.
//! - Alternate scroll (mode 1007): on the alternate screen, when the application has no mouse
//!   mode, a vertical wheel turns into cursor keys, one for each notch.
//! - The encoder has no memory. It does not drop a motion event in the cell of the last one. The
//!   pane runtime does that.

use nh_proto::input::{LogicalMouse, MouseButton, MouseKind};

use super::{Delivery, InputModes, key::mod_bits, push_decimal};

/// The most reports that one event makes for one wheel axis.
pub const MAX_NOTCHES: u16 = 16;

/// What the application asked the mouse to report.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseMode {
    /// Mode 9: button presses only, no modifiers.
    X10,
    /// Mode 1000: presses and releases, and the wheel.
    Click,
    /// Mode 1002: also motion while a button is held.
    Drag,
    /// Mode 1003: also motion with no button.
    Motion,
}

/// How a mouse report is written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseEncoding {
    /// `CSI M` and three bytes, each 32 more than the value (mode 0).
    Legacy,
    /// The same, with coordinates above 95 as UTF-8 (mode 1005).
    Utf8,
    /// `CSI b ; x ; y M` in decimal (mode 1015).
    Urxvt,
    /// `CSI < b ; x ; y M` or `m` (mode 1006).
    Sgr,
}

pub(super) fn encode(mouse: &LogicalMouse, modes: &InputModes, out: &mut Vec<u8>) -> Delivery {
    let Some(mode) = modes.mouse_mode() else {
        return alternate_scroll(mouse, modes, out);
    };
    let encoding = modes.mouse_encoding();
    let modifiers = if mode == MouseMode::X10 {
        0
    } else {
        let bits = mod_bits(&mouse.mods, false);
        // Shift 4, Meta (Alt) 8, Control 16 in the report; the parameter order is Shift, Alt, Ctrl.
        (bits & 1) * 4 + ((bits >> 1) & 1) * 8 + ((bits >> 2) & 1) * 16
    };
    let button = match mouse.button {
        MouseButton::Left => Some(0),
        MouseButton::Middle => Some(1),
        MouseButton::Right => Some(2),
        MouseButton::None => None,
    };
    let (x, y) = (u32::from(mouse.col) + 1, u32::from(mouse.row) + 1);
    match mouse.kind {
        MouseKind::Down => {
            if let Some(button) = button {
                report(out, encoding, button + modifiers, x, y, false);
            }
        }
        MouseKind::Up => {
            if let (Some(button), false) = (button, mode == MouseMode::X10) {
                match encoding {
                    MouseEncoding::Sgr => report(out, encoding, button + modifiers, x, y, true),
                    _ => report(out, encoding, 3 + modifiers, x, y, false),
                }
            }
        }
        MouseKind::Move => {
            let reported = match mode {
                MouseMode::Motion => true,
                MouseMode::Drag => button.is_some(),
                MouseMode::Click | MouseMode::X10 => false,
            };
            if reported {
                report(
                    out,
                    encoding,
                    32 + button.unwrap_or(3) + modifiers,
                    x,
                    y,
                    false,
                );
            }
        }
        MouseKind::Wheel => {
            if mode != MouseMode::X10 {
                wheel(mouse, modifiers, encoding, (x, y), out);
            }
        }
    }
    Delivery::Application
}

/// The reports of a wheel event: vertical first, then horizontal.
fn wheel(
    mouse: &LogicalMouse,
    modifiers: u32,
    encoding: MouseEncoding,
    (x, y): (u32, u32),
    out: &mut Vec<u8>,
) {
    let axes = [(mouse.wheel_y, 64, 65), (mouse.wheel_x, 66, 67)];
    for (notches, negative, positive) in axes {
        let code = if notches < 0 { negative } else { positive };
        for _ in 0..notches.unsigned_abs().min(MAX_NOTCHES) {
            report(out, encoding, code + modifiers, x, y, false);
        }
    }
}

/// Mode 1007: the wheel as cursor keys on the alternate screen.
fn alternate_scroll(mouse: &LogicalMouse, modes: &InputModes, out: &mut Vec<u8>) -> Delivery {
    if mouse.kind != MouseKind::Wheel || mouse.wheel_y == 0 || !modes.alternate_scroll() {
        return Delivery::Local;
    }
    let letter = if mouse.wheel_y < 0 { b'A' } else { b'B' };
    for _ in 0..mouse.wheel_y.unsigned_abs().min(MAX_NOTCHES) {
        out.extend_from_slice(if modes.app_cursor() {
            b"\x1bO"
        } else {
            b"\x1b["
        });
        out.push(letter);
    }
    Delivery::Application
}

/// Writes one report. `button` is the whole button value: the button, the motion bit, the
/// modifiers. `sgr_release` ends an SGR report with `m`. Nothing is written when a coordinate does
/// not fit.
fn report(
    out: &mut Vec<u8>,
    encoding: MouseEncoding,
    button: u32,
    x: u32,
    y: u32,
    sgr_release: bool,
) {
    match encoding {
        MouseEncoding::Sgr => {
            out.extend_from_slice(b"\x1b[<");
            push_decimal(out, button);
            out.push(b';');
            push_decimal(out, x);
            out.push(b';');
            push_decimal(out, y);
            out.push(if sgr_release { b'm' } else { b'M' });
        }
        MouseEncoding::Urxvt => {
            out.extend_from_slice(b"\x1b[");
            push_decimal(out, 32 + button);
            out.push(b';');
            push_decimal(out, x);
            out.push(b';');
            push_decimal(out, y);
            out.push(b'M');
        }
        MouseEncoding::Legacy => {
            // 32 + 223 = 255 is the largest byte.
            if x > 223 || y > 223 {
                return;
            }
            out.extend_from_slice(b"\x1b[M");
            out.extend_from_slice(&[(32 + button) as u8, (32 + x) as u8, (32 + y) as u8]);
        }
        MouseEncoding::Utf8 => {
            // 32 + 2015 = 2047 is the largest value that UTF-8 writes in two bytes.
            if x > 2015 || y > 2015 {
                return;
            }
            out.extend_from_slice(b"\x1b[M");
            for value in [32 + button, 32 + x, 32 + y] {
                push_utf8_value(out, value);
            }
        }
    }
}

/// Writes a value of at most 2047 as the one or two bytes of UTF-8.
fn push_utf8_value(out: &mut Vec<u8>, value: u32) {
    if value < 0x80 {
        out.push(value as u8);
    } else {
        out.push(0xc0 | (value >> 6) as u8);
        out.push(0x80 | (value & 0x3f) as u8);
    }
}

#[cfg(test)]
mod tests {
    use super::super::encode_mouse;
    use super::*;
    use nh_proto::{input::Mods, modes::ModeFlags};

    fn mouse(kind: MouseKind, button: MouseButton, col: u16, row: u16) -> LogicalMouse {
        LogicalMouse {
            kind,
            button,
            col,
            row,
            mods: Mods::default(),
            wheel_x: 0,
            wheel_y: 0,
        }
    }

    fn run(event: &LogicalMouse, flags: ModeFlags) -> (Delivery, Vec<u8>) {
        let mut out = Vec::new();
        let delivery = encode_mouse(event, &InputModes::new(flags), &mut out);
        (delivery, out)
    }

    #[test]
    fn without_a_mouse_mode_the_core_keeps_the_event() {
        let click = mouse(MouseKind::Down, MouseButton::Left, 0, 0);
        assert_eq!(run(&click, ModeFlags::empty()), (Delivery::Local, vec![]));
    }

    #[test]
    fn sgr_press_and_release() {
        let flags = ModeFlags::MOUSE_1000 | ModeFlags::MOUSE_SGR;
        let down = mouse(MouseKind::Down, MouseButton::Right, 9, 4);
        assert_eq!(run(&down, flags).1, b"\x1b[<2;10;5M");
        let up = mouse(MouseKind::Up, MouseButton::Right, 9, 4);
        assert_eq!(run(&up, flags).1, b"\x1b[<2;10;5m");
    }

    #[test]
    fn legacy_release_is_button_3_and_coordinates_have_a_limit() {
        let flags = ModeFlags::MOUSE_1000;
        let up = mouse(MouseKind::Up, MouseButton::Left, 0, 0);
        assert_eq!(run(&up, flags).1, b"\x1b[M#!!");
        let far = mouse(MouseKind::Down, MouseButton::Left, 223, 0);
        assert_eq!(run(&far, flags), (Delivery::Application, vec![]));
        let last = mouse(MouseKind::Down, MouseButton::Left, 222, 0);
        assert_eq!(run(&last, flags).1, [0x1b, b'[', b'M', 32, 255, 33]);
    }

    #[test]
    fn utf8_writes_large_coordinates_in_two_bytes() {
        let flags = ModeFlags::MOUSE_1000 | ModeFlags::MOUSE_UTF8;
        let down = mouse(MouseKind::Down, MouseButton::Left, 199, 0);
        // 200 + 32 = 232 = 0xe8 -> C3 A8.
        assert_eq!(run(&down, flags).1, [0x1b, b'[', b'M', 32, 0xc3, 0xa8, 33]);
        let beyond = mouse(MouseKind::Down, MouseButton::Left, 2015, 0);
        assert!(run(&beyond, flags).1.is_empty());
    }

    #[test]
    fn urxvt_is_decimal() {
        let flags = ModeFlags::MOUSE_1000 | ModeFlags::MOUSE_URXVT;
        let up = mouse(MouseKind::Up, MouseButton::Left, 299, 0);
        assert_eq!(run(&up, flags).1, b"\x1b[35;300;1M");
    }

    #[test]
    fn motion_follows_the_mode() {
        let sgr = ModeFlags::MOUSE_SGR;
        let hover = mouse(MouseKind::Move, MouseButton::None, 1, 1);
        let drag = mouse(MouseKind::Move, MouseButton::Left, 1, 1);
        assert!(run(&hover, ModeFlags::MOUSE_1000 | sgr).1.is_empty());
        assert!(run(&drag, ModeFlags::MOUSE_1000 | sgr).1.is_empty());
        assert!(run(&hover, ModeFlags::MOUSE_1002 | sgr).1.is_empty());
        assert_eq!(run(&drag, ModeFlags::MOUSE_1002 | sgr).1, b"\x1b[<32;2;2M");
        assert_eq!(run(&hover, ModeFlags::MOUSE_1003 | sgr).1, b"\x1b[<35;2;2M");
    }

    #[test]
    fn modifiers_and_x10() {
        let flags = ModeFlags::MOUSE_1000 | ModeFlags::MOUSE_SGR;
        let mut down = mouse(MouseKind::Down, MouseButton::Left, 0, 0);
        down.mods = Mods {
            shift: true,
            alt: true,
            ctrl: true,
            ..Mods::default()
        };
        assert_eq!(run(&down, flags).1, b"\x1b[<28;1;1M");
        // X10 has no modifiers, no release and no wheel.
        let x10 = ModeFlags::MOUSE_X10;
        assert_eq!(run(&down, x10).1, b"\x1b[M !!");
        let up = mouse(MouseKind::Up, MouseButton::Left, 0, 0);
        assert!(run(&up, x10).1.is_empty());
    }

    #[test]
    fn the_wheel_makes_one_report_for_each_notch() {
        let flags = ModeFlags::MOUSE_1000 | ModeFlags::MOUSE_SGR;
        let mut wheel = mouse(MouseKind::Wheel, MouseButton::None, 0, 0);
        wheel.wheel_y = -2;
        wheel.wheel_x = 1;
        assert_eq!(
            run(&wheel, flags).1,
            b"\x1b[<64;1;1M\x1b[<64;1;1M\x1b[<67;1;1M"
        );
        wheel.wheel_y = 1000;
        wheel.wheel_x = 0;
        assert_eq!(run(&wheel, flags).1.len(), 10 * usize::from(MAX_NOTCHES));
    }

    #[test]
    fn alternate_scroll_sends_cursor_keys_on_the_alternate_screen() {
        let mut wheel = mouse(MouseKind::Wheel, MouseButton::None, 0, 0);
        wheel.wheel_y = 2;
        let flags = ModeFlags::ALT_SCREEN | ModeFlags::ALTERNATE_SCROLL;
        assert_eq!(run(&wheel, flags).1, b"\x1b[B\x1b[B");
        assert_eq!(
            run(&wheel, flags | ModeFlags::APP_CURSOR).1,
            b"\x1bOB\x1bOB"
        );
        // Not on the primary screen: the core scrolls the history.
        assert_eq!(
            run(&wheel, ModeFlags::ALTERNATE_SCROLL),
            (Delivery::Local, vec![])
        );
        // A mouse mode has priority.
        assert_eq!(
            run(&wheel, flags | ModeFlags::MOUSE_1000 | ModeFlags::MOUSE_SGR).1,
            b"\x1b[<65;1;1M\x1b[<65;1;1M"
        );
    }
}
