//! Focus reports (`ctlseqs`, mode 1004).

use super::InputModes;

/// Appends `CSI I` when the pane got focus, or `CSI O` when it lost focus. Nothing is written
/// unless the application turned focus events on.
pub fn encode_focus(focused: bool, modes: &InputModes, out: &mut Vec<u8>) {
    if modes.focus_events() {
        out.extend_from_slice(if focused { b"\x1b[I" } else { b"\x1b[O" });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nh_proto::modes::ModeFlags;

    #[test]
    fn focus_is_reported_only_when_asked_for() {
        let mut out = Vec::new();
        encode_focus(true, &InputModes::default(), &mut out);
        assert!(out.is_empty());
        let on = InputModes::new(ModeFlags::FOCUS_EVENTS);
        encode_focus(true, &on, &mut out);
        encode_focus(false, &on, &mut out);
        assert_eq!(out, b"\x1b[I\x1b[O");
    }
}
