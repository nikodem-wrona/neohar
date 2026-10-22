//! Snapshot and restore, one test per element of the snapshot.
//!
//! Each test feeds output to an engine and takes a snapshot. A second engine restores it. Then
//! both engines read more output, and they must agree after every piece: on the screen (cells,
//! cursor, modes, title, palette), on the replies they write to the child, and on the whole
//! snapshot that each of them would take. A state that the snapshot misses shows up as a
//! difference in the output that follows.

use nh_term::{
    AlacrittyEngine, Effect, EngineConfig, MIN_HISTORY, Recorder, RestoreError, SNAPSHOT_VERSION,
    Snapshot, SnapshotError, VtEngine, client_grid::dump_engine, xterm_palette,
};

fn config() -> EngineConfig {
    EngineConfig {
        history: MIN_HISTORY,
        theme: xterm_palette(),
        cell_px: (0, 0),
    }
}

fn engine(cols: u16, rows: u16) -> AlacrittyEngine {
    AlacrittyEngine::new(cols, rows, MIN_HISTORY, xterm_palette())
}

fn feed(engine: &mut AlacrittyEngine, bytes: &[u8]) -> Vec<Effect> {
    let mut recorder = Recorder::default();
    engine.advance(bytes, &mut recorder);
    recorder.0
}

/// Takes a snapshot, sends it through the encoding, and restores it.
fn restore(engine: &mut AlacrittyEngine) -> AlacrittyEngine {
    let snapshot = engine.snapshot(4242).expect("a snapshot is possible");
    let bytes = snapshot.encode();
    let snapshot = Snapshot::decode(&bytes).expect("the snapshot decodes");
    assert_eq!(snapshot.journal_offset, 4242);
    AlacrittyEngine::restore(&snapshot, &config()).expect("the snapshot restores")
}

/// Panics when the engines differ.
fn assert_same(a: &mut AlacrittyEngine, b: &mut AlacrittyEngine, when: &str) {
    if let Some(difference) = dump_engine(a).diff(&dump_engine(b)) {
        panic!("{when}: the screens differ: {difference}");
    }
    assert_eq!(a.viewport(), b.viewport(), "{when}: the viewport differs");
    let sa = a.snapshot(0).expect("snapshot of the first engine");
    let sb = b.snapshot(0).expect("snapshot of the second engine");
    assert_eq!(sa, sb, "{when}: the snapshots differ");
}

/// Feeds `before` to an engine, restores a second one from it, then feeds each of `after` to both.
fn check(cols: u16, rows: u16, before: &[u8], after: &[&[u8]]) {
    let mut a = engine(cols, rows);
    feed(&mut a, before);
    let mut b = restore(&mut a);
    assert_same(&mut a, &mut b, "right after the restore");
    for (n, piece) in after.iter().enumerate() {
        let from_a = feed(&mut a, piece);
        let from_b = feed(&mut b, piece);
        assert_eq!(from_a, from_b, "piece {n}: the effects differ");
        assert_same(&mut a, &mut b, &format!("after piece {n} ({piece:?})"));
    }
}

/// Output that reads many parts of the state: it prints, wraps, scrolls, tabs, and asks.
const PROBE: &[&[u8]] = &[
    b"abc\r\n",
    b"\x1b[6n",
    b"\x1b[?1$p\x1b[?6$p\x1b[?7$p\x1b[?25$p\x1b[?1000$p\x1b[?1004$p\x1b[?2004$p\x1b[4$p\x1b[20$p",
    b"\x1b[?u",
    b"\tx\ty\tz\r\n",
    b"0123456789012345678901234567890123456789012345678901234567890123456789012345678901234",
    b"\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n\r\n",
    b"\x1b8x\x1b[Hy",
    b"\x1b[10b",
    b"\x1b]11;?\x07\x1b]10;?\x07\x1b]4;1;?\x07",
    b"\x1b[22;0t\x1b]2;pushed\x07\x1b[23;0t",
    b"\x1b[>1u\x1b[?u\x1b[<u\x1b[?u",
];

#[test]
fn an_empty_screen_restores() {
    check(80, 24, b"", PROBE);
}

#[test]
fn text_and_every_kind_of_style() {
    let mut before = Vec::new();
    for sgr in [
        "1",
        "2",
        "3",
        "4",
        "5",
        "7",
        "8",
        "9",
        "21",
        "53",
        "4:3",
        "4:4",
        "4:5",
        "31",
        "41",
        "91",
        "101",
        "38;5;200",
        "48;5;17",
        "38;2;10;20;30",
        "48;2;250;240;230",
        "58;5;99",
        "58;2;1;2;3",
        "38:2::7:8:9",
    ] {
        before.extend(format!("\x1b[{sgr}mstyle {sgr} \x1b[0m\r\n").bytes());
    }
    before.extend(
        "日本語 wide, e\u{301} combined, 👨\u{200d}👩\u{200d}👧 family, 🇩🇪 flag\r\n".bytes(),
    );
    before.extend(b"\x1b]8;id=1;http://example.com\x1b\\link\x1b]8;;\x1b\\ then plain\r\n");
    before.extend(b"\x1b[1;4;38;2;9;8;7;48;5;3m");
    check(80, 24, &before, PROBE);
}

#[test]
fn the_pending_wrap_at_the_right_margin() {
    // Exactly 20 characters leave the cursor at the margin with the wrap pending.
    check(20, 6, b"01234567890123456789", PROBE);
    check(20, 6, b"01234567890123456789", &[b"A", b"\x1b[6n", b"B"]);
    // A control character does not clear the wrap, a cursor move does.
    check(20, 6, b"01234567890123456789\x08", &[b"X", b"Y"]);
    check(20, 6, b"01234567890123456789\x1b[D", &[b"X", b"Y"]);
    // Pending wrap in the saved cursor.
    check(
        20,
        6,
        b"\x1b[3;1H01234567890123456789\x1b7\x1b[H",
        &[b"\x1b8Z", b"W"],
    );
    // With autowrap off the character overwrites the last cell.
    check(
        20,
        6,
        b"\x1b[?7l01234567890123456789",
        &[b"A", b"B", b"\x1b[?7hC", b"D"],
    );
}

macro_rules! mode_test {
    ($name:ident, $set:expr) => {
        #[test]
        fn $name() {
            check(80, 24, $set, PROBE);
            // And after the mode is on while the other screen is on show.
            let mut with_alt = b"\x1b[?1049h".to_vec();
            with_alt.extend($set);
            check(80, 24, &with_alt, PROBE);
        }
    };
}

mode_test!(mode_application_cursor_keys, b"\x1b[?1h");
mode_test!(mode_application_keypad, b"\x1b=");
mode_test!(mode_mouse_clicks, b"\x1b[?1000h");
mode_test!(mode_mouse_drag, b"\x1b[?1002h");
mode_test!(mode_mouse_motion, b"\x1b[?1003h");
mode_test!(mode_mouse_x10, b"\x1b[?9h");
mode_test!(mode_mouse_sgr_encoding, b"\x1b[?1000h\x1b[?1006h");
mode_test!(mode_mouse_utf8_encoding, b"\x1b[?1000h\x1b[?1005h");
mode_test!(mode_mouse_urxvt_encoding, b"\x1b[?1000h\x1b[?1015h");
mode_test!(mode_alternate_scroll_off, b"\x1b[?1007l");
mode_test!(mode_bracketed_paste, b"\x1b[?2004h");
mode_test!(mode_focus_events, b"\x1b[?1004h");
mode_test!(mode_origin, b"\x1b[5;15r\x1b[?6h");
mode_test!(mode_autowrap_off, b"\x1b[?7l");
mode_test!(mode_insert, b"\x1b[4h");
mode_test!(mode_line_feed_new_line, b"\x1b[20h");
mode_test!(mode_cursor_hidden, b"\x1b[?25l");
mode_test!(mode_urgency_hints_off, b"\x1b[?1042l");
mode_test!(mode_modify_other_keys, b"\x1b[>4;2m");
mode_test!(mode_kitty_keyboard_flags, b"\x1b[>11u");

#[test]
fn a_mouse_mode_replaces_another() {
    // The modes exclude each other, as in xterm.
    let mut a = engine(80, 24);
    feed(&mut a, b"\x1b[?9h");
    feed(&mut a, b"\x1b[?1002h");
    let m = a.modes();
    assert!(m.contains(nh_proto::modes::ModeFlags::MOUSE_1002));
    assert!(!m.contains(nh_proto::modes::ModeFlags::MOUSE_X10));
    feed(&mut a, b"\x1b[?9h");
    let m = a.modes();
    assert!(m.contains(nh_proto::modes::ModeFlags::MOUSE_X10));
    assert!(!m.contains(nh_proto::modes::ModeFlags::MOUSE_1002));
    let mut b = restore(&mut a);
    assert_same(&mut a, &mut b, "after the restore");
    assert_eq!(b.modes().modify_other_keys(), 0);
}

#[test]
fn the_extra_modes_show_in_the_mode_flags() {
    use nh_proto::modes::ModeFlags;
    let mut a = engine(80, 24);
    feed(&mut a, b"\x1b[?1015h\x1b[>4;2m");
    assert!(a.modes().contains(ModeFlags::MOUSE_URXVT));
    assert_eq!(a.modes().modify_other_keys(), 2);
    let b = restore(&mut a);
    assert!(b.modes().contains(ModeFlags::MOUSE_URXVT));
    assert_eq!(b.modes().modify_other_keys(), 2);
}

#[test]
fn tab_stops() {
    // Clear all, set three, and use them.
    let before = b"\x1b[3g\x1b[1;5H\x1bH\x1b[1;13H\x1bH\x1b[1;30H\x1bH\x1b[H";
    check(
        40,
        6,
        before,
        &[
            b"\t1\t2\t3\t4\t5\r\n",
            b"\x1b[I\x1b[I#",
            b"\x1b[Z\x1b[Z#",
            b"\x1b[2;30H\x1b[0g\r\t.",
        ],
    );
    // The default stops, and a width that grew, with its default stops.
    check(40, 6, b"", &[b"\t\t\t\tx"]);
    let mut a = engine(20, 6);
    feed(&mut a, b"\x1b[1;4H\x1bH");
    a.resize(50, 6);
    let mut b = restore(&mut a);
    for piece in [&b"\r\t.\t.\t.\t.\t."[..], b"\x1b[1;1H\x1b[3I#"] {
        feed(&mut a, piece);
        feed(&mut b, piece);
        assert_same(&mut a, &mut b, "tab stops after a resize");
    }
}

#[test]
fn charsets_and_the_shift_state() {
    let line = "lqkxjmwvtun";
    // G0 as line drawing.
    check(
        40,
        6,
        b"\x1b(0",
        &[line.as_bytes(), b"\x1b(B", line.as_bytes()],
    );
    // G1 as line drawing, shifted in and out.
    check(
        40,
        6,
        b"\x1b)0\x0e",
        &[
            line.as_bytes(),
            b"\x0f",
            line.as_bytes(),
            b"\x0e",
            line.as_bytes(),
        ],
    );
    // G2 and G3 and the locking shifts.
    check(
        40,
        6,
        b"\x1b*0\x1b+0\x1bn",
        &[
            line.as_bytes(),
            b"\x1bo",
            line.as_bytes(),
            b"\x0f",
            line.as_bytes(),
        ],
    );
    // The designations are saved with the cursor.
    check(
        40,
        6,
        b"\x1b(0\x1b7\x1b(B",
        &[line.as_bytes(), b"\x1b8", line.as_bytes()],
    );
    // The active set is G1 and G0 and G1 are both ASCII: a later designation shows which is active.
    check(
        40,
        6,
        b"\x0e",
        &[b"\x1b(0", line.as_bytes(), b"\x1b)0", line.as_bytes()],
    );
}

#[test]
fn the_scroll_region() {
    let fill: String = (1..=12).map(|n| format!("line {n}\r\n")).collect();
    let before = format!("{fill}\x1b[3;8r\x1b[5;1H");
    check(
        40,
        12,
        before.as_bytes(),
        &[
            b"a\r\nb\r\nc\r\nd\r\ne\r\nf\r\ng\r\nh\r\n",
            b"\x1bM\x1bM\x1bM\x1bM\x1bM\x1bM\x1bM\x1bM\x1bM",
            b"\x1b[2S",
            b"\x1b[3T",
            b"\x1b[2L\x1b[2M",
            b"\x1b[r",
        ],
    );
    // A region that starts on the last row and one the height clamps.
    check(40, 12, b"\x1b[30;40r", &[b"x\r\ny\r\n", b"\x1b[S"]);
}

#[test]
fn the_screens_and_what_comes_back_after_the_alternate_one() {
    // Content on both screens, with a different cursor on each.
    let before = b"primary one\r\nprimary two\x1b[2;3H\x1b7\x1b[4;5H\x1b[1;31m\x1b[?1049hALT\x1b[3;3Halt row\x1b[44m\x1b[2;2H";
    let leave: &[&[u8]] = &[
        b"more alt",
        b"\x1b[?1049l",
        b"after leave",
        b"\x1b8@",
        b"\x1b[?1049h",
        b"again alt",
        b"\x1b[?1049l",
    ];
    check(40, 8, before, leave);
    // Leaving after the restore, from a snapshot taken in the primary screen too.
    check(
        40,
        8,
        b"primary\x1b7\x1b[?1049hx\x1b7\x1b[?1049l",
        &[b"\x1b8Z", b"\x1b[?1049h\x1b8Q"],
    );
    // The saved cursor of the alternate screen survives the screen and a snapshot.
    check(
        40,
        8,
        b"\x1b[?1049h\x1b[5;5H\x1b7\x1b[?1049l\x1b[2;2H",
        &[b"\x1b[?1049h\x1b8*", b"\x1b[?1049l"],
    );
}

#[test]
fn the_saved_cursor_with_its_attributes() {
    check(
        40,
        8,
        b"\x1b[3;7H\x1b[1;4;31;42m\x1b7\x1b[0m\x1b[H",
        &[b"x\x1b8y", b"\x1b[0mz\x1b8w"],
    );
}

#[test]
fn titles_and_the_title_stack() {
    check(
        40,
        8,
        b"\x1b]2;first\x07\x1b[22;0t\x1b]2;second\x07\x1b[22;0t\x1b]2;third\x07",
        &[b"\x1b[23;0t", b"\x1b[23;0t", b"\x1b[23;0t", b"\x1b[23;0t"],
    );
    // Entries pushed before any title was set hold no title.
    check(
        40,
        8,
        b"\x1b[22;0t\x1b[22;0t\x1b]2;x\x07\x1b[22;0t",
        &[b"\x1b[23;0t", b"\x1b[23;0t", b"\x1b[23;0t", b"\x1b[23;0t"],
    );
    check(40, 8, b"\x1b]0;both\x07", &[b"\x1b]2;\x07", b"\x1b[23;0t"]);
}

#[test]
fn the_palette_and_the_dynamic_colours() {
    check(
        40,
        8,
        b"\x1b]4;1;rgb:12/34/56\x07\x1b]4;200;rgb:ff/00/aa\x07\x1b]10;rgb:01/02/03\x07\x1b]11;rgb:aa/bb/cc\x07\x1b]12;rgb:11/22/33\x07\x1b[31mred\x1b[38;5;200mx",
        &[
            b"\x1b]4;1;?\x07\x1b]4;200;?\x07\x1b]10;?\x07\x1b]11;?\x07\x1b]12;?\x07",
            b"\x1b]104;1\x07\x1b]4;1;?\x07",
            b"\x1b]110\x07\x1b]10;?\x07",
        ],
    );
}

#[test]
fn the_kitty_keyboard_stack() {
    check(
        40,
        8,
        b"\x1b[>1u\x1b[>5u\x1b[>9u",
        &[
            b"\x1b[?u",
            b"\x1b[<u\x1b[?u",
            b"\x1b[<2u\x1b[?u",
            b"\x1b[<u\x1b[?u",
        ],
    );
    // The flags set with `CSI = ... u` are not on the stack.
    check(
        40,
        8,
        b"\x1b[>1u\x1b[=6;1u",
        &[b"\x1b[?u", b"\x1b[<u\x1b[?u", b"\x1b[<u\x1b[?u"],
    );
    // One stack for each screen.
    check(
        40,
        8,
        b"\x1b[>1u\x1b[?1049h\x1b[>4u\x1b[>8u",
        &[
            b"\x1b[?u",
            b"\x1b[?1049l\x1b[?u",
            b"\x1b[<u\x1b[?u",
            b"\x1b[?1049h\x1b[?u",
            b"\x1b[<u\x1b[?u",
        ],
    );
    check(
        40,
        8,
        b"\x1b[>1u\x1b[?1049h\x1b[>4u\x1b[?1049l",
        &[b"\x1b[?u", b"\x1b[?1049h\x1b[?u", b"\x1b[<u\x1b[?u"],
    );
}

#[test]
fn the_cursor_shape_and_the_blink() {
    for style in 0..=6 {
        let before = format!("\x1b[{style} q");
        check(
            40,
            8,
            before.as_bytes(),
            &[b"x", b"\x1b[?12h", b"\x1b[?12l", b"\x1b[5 q"],
        );
    }
    check(40, 8, b"\x1b[?12h", &[b"\x1b[0 q", b"\x1b[?25l"]);
}

#[test]
fn rep_repeats_the_last_character_after_a_restore() {
    check(40, 8, b"ab", &[b"\x1b[5b", b"\x1b[3b"]);
    check(40, 8, "日".as_bytes(), &[b"\x1b[4b"]);
    check(40, 8, b"x\r\n\x1b[2J\x1b[H", &[b"\x1b[4b"]);
    check(40, 8, "e\u{301}".as_bytes(), &[b"\x1b[2b"]);
}

#[test]
fn hyperlinks_stay_on_their_cells() {
    let mut a = engine(40, 4);
    feed(
        &mut a,
        b"\x1b]8;id=a;http://example.com/a\x1b\\link\x1b]8;;\x1b\\ plain",
    );
    let mut b = restore(&mut a);
    assert_same(&mut a, &mut b, "after the restore");
    let snapshot = b.snapshot(0).expect("snapshot");
    assert!(
        snapshot.cells.iter().any(|c| c
            .hyperlink
            .as_ref()
            .is_some_and(|(id, uri)| id == "a" && uri == "http://example.com/a")),
        "the hyperlink is in the cell table"
    );
    // The current link is part of the cursor's attributes.
    check(
        40,
        4,
        b"\x1b]8;id=b;http://example.com/b\x1b\\open",
        &[b" more", b"\x1b]8;;\x1b\\ closed"],
    );
}

#[test]
fn a_snapshot_does_not_change_the_engine() {
    // In the primary screen, with a saved cursor in each screen.
    let mut a = engine(40, 8);
    feed(
        &mut a,
        b"one\x1b7\x1b[3;3H\x1b[?1049h\x1b[2;2Halt\x1b7\x1b[?1049l\x1b[5;5H",
    );
    let first = a.snapshot(1).expect("snapshot");
    let second = a.snapshot(1).expect("snapshot");
    assert_eq!(first, second);
    let mut untouched = engine(40, 8);
    feed(
        &mut untouched,
        b"one\x1b7\x1b[3;3H\x1b[?1049h\x1b[2;2Halt\x1b7\x1b[?1049l\x1b[5;5H",
    );
    for piece in [&b"\x1b8x"[..], b"\x1b[?1049h\x1b8y", b"\x1b[?1049l"] {
        feed(&mut a, piece);
        feed(&mut untouched, piece);
        assert_eq!(dump_engine(&a).diff(&dump_engine(&untouched)), None);
    }
    // In the alternate screen.
    let mut a = engine(40, 8);
    feed(&mut a, b"prim\x1b[?1049hhello\x1b[2;4Hworld\x1b[44m");
    let first = a.snapshot(1).expect("snapshot");
    assert_eq!(first, a.snapshot(1).expect("snapshot"));
    let mut untouched = engine(40, 8);
    feed(
        &mut untouched,
        b"prim\x1b[?1049hhello\x1b[2;4Hworld\x1b[44m",
    );
    for piece in [&b"more"[..], b"\x1b[?1049l", b"\x1b[?1049h!"] {
        feed(&mut a, piece);
        feed(&mut untouched, piece);
        assert_eq!(dump_engine(&a).diff(&dump_engine(&untouched)), None);
    }
}

#[test]
fn damage_that_the_pane_did_not_take_stays() {
    let mut a = engine(40, 8);
    let mut untouched = engine(40, 8);
    let _ = (a.take_damage(), untouched.take_damage());
    for e in [&mut a, &mut untouched] {
        feed(e, b"\x1b[3;1Hxy");
    }
    let _ = a.snapshot(1).expect("snapshot");
    let damage = a.take_damage();
    assert!(
        !damage.full,
        "the snapshot must not damage the whole screen"
    );
    assert_eq!(damage, untouched.take_damage());
    // The swaps that read the other screen do not damage it either.
    for e in [&mut a, &mut untouched] {
        feed(e, b"\x1b[?1049h");
        let _ = e.take_damage();
    }
    let _ = a.snapshot(2).expect("snapshot");
    assert_eq!(a.take_damage(), untouched.take_damage());
    // A snapshot after a snapshot, with damage in between, keeps all of it.
    for e in [&mut a, &mut untouched] {
        feed(e, b"\x1b[2;2Hq");
    }
    let _ = a.snapshot(3).expect("snapshot");
    feed(&mut a, b"\x1b[6;1Hr");
    feed(&mut untouched, b"\x1b[6;1Hr");
    let _ = a.snapshot(4).expect("snapshot");
    assert_eq!(a.take_damage(), untouched.take_damage());
}

#[test]
fn a_restored_engine_is_all_damage_and_keeps_counting() {
    let mut a = engine(20, 4);
    let lines: String = (0..30).map(|n| format!("row {n}\r\n")).collect();
    feed(&mut a, lines.as_bytes());
    let mut taken = Vec::new();
    a.take_new_history(&mut taken);
    let scrolled = a.viewport().total_lines;
    let mut b = restore(&mut a);
    assert!(b.take_damage().full);
    assert_eq!(b.viewport(), a.viewport());
    feed(&mut a, b"next\r\n");
    feed(&mut b, b"next\r\n");
    let (mut from_a, mut from_b) = (Vec::new(), Vec::new());
    a.take_new_history(&mut from_a);
    b.take_new_history(&mut from_b);
    assert_eq!(from_a, from_b);
    assert_eq!(from_a[0].line_no, scrolled - 4);
}

#[test]
fn lines_that_the_pane_did_not_take_are_in_the_snapshot() {
    let mut a = engine(20, 4);
    let lines: String = (0..10).map(|n| format!("row {n}\r\n")).collect();
    feed(&mut a, lines.as_bytes());
    // Nobody called `take_new_history`.
    let mut b = restore(&mut a);
    let (mut from_a, mut from_b) = (Vec::new(), Vec::new());
    a.take_new_history(&mut from_a);
    b.take_new_history(&mut from_b);
    assert!(!from_a.is_empty());
    assert_eq!(from_a, from_b);
}

#[test]
fn a_resize_after_the_restore() {
    let fill: String = (1..=30).map(|n| format!("line {n}\r\n")).collect();
    // Without scrollback to pull in, so the grown grid is the same in both engines.
    let mut a = engine(40, 10);
    feed(&mut a, b"\x1b[?1049hx\x1b[5;5Hy");
    let mut b = restore(&mut a);
    for (cols, rows) in [(30, 10), (50, 14), (50, 6), (20, 20)] {
        a.resize(cols, rows);
        b.resize(cols, rows);
        assert_same(
            &mut a,
            &mut b,
            &format!("after the resize to {cols}x{rows}"),
        );
        feed(&mut a, b"zz\r\n");
        feed(&mut b, b"zz\r\n");
        assert_same(&mut a, &mut b, "after output");
    }
    let _ = fill;
}

#[test]
fn the_scrollback_in_memory_is_not_part_of_the_snapshot() {
    // The history is in the archive. A grid that grows takes blank lines from the restored
    // engine, where the other engine takes lines of its scrollback.
    let mut a = engine(20, 4);
    let lines: String = (0..30).map(|n| format!("row {n}\r\n")).collect();
    feed(&mut a, lines.as_bytes());
    let mut b = restore(&mut a);
    a.resize(20, 8);
    b.resize(20, 8);
    let (da, db) = (dump_engine(&a), dump_engine(&b));
    assert!(da.diff(&db).is_some(), "the engines differ here, by design");
    // The old screen is at the bottom of one engine and at the top of the other.
    assert_eq!(da.rows[4..], db.rows[..4]);
    assert!(db.rows[4..].iter().flatten().all(|(text, _)| text == " "));
}

#[test]
fn a_synchronized_update_blocks_the_snapshot() {
    let mut a = engine(40, 8);
    feed(&mut a, b"before\x1b[?2026hheld back");
    assert_eq!(a.snapshot(1), Err(SnapshotError::SyncOpen));
    // The update ends and the screen shows what it held.
    feed(&mut a, b"\x1b[?2026l");
    let mut b = restore(&mut a);
    assert_same(&mut a, &mut b, "after the update");
    // An update that times out is closed by the core.
    feed(&mut a, b"\x1b[?2026hmore");
    assert!(a.sync_deadline().is_some());
    assert_eq!(a.snapshot(2), Err(SnapshotError::SyncOpen));
    a.flush_sync(&mut Recorder::default());
    assert!(a.snapshot(2).is_ok());
}

#[test]
fn a_bad_snapshot_does_not_restore() {
    let mut a = engine(40, 8);
    feed(&mut a, b"text");
    let good = a.snapshot(1).expect("snapshot");
    let mut bad = good.clone();
    bad.version = SNAPSHOT_VERSION + 1;
    assert!(matches!(
        AlacrittyEngine::restore(&bad, &config()),
        Err(RestoreError::Version { .. })
    ));
    let mut bad = good.clone();
    bad.primary.rows.pop();
    assert!(matches!(
        AlacrittyEngine::restore(&bad, &config()),
        Err(RestoreError::Invalid(_))
    ));
    // A region that no escape sequence can make is refused, not turned into another one.
    let mut bad = good.clone();
    bad.region = (3, 4);
    assert!(matches!(
        AlacrittyEngine::restore(&bad, &config()),
        Err(RestoreError::Invalid(_))
    ));
    // A colour that does not exist.
    let mut bad = good;
    bad.cells[0].fg = nh_term::SnapColor::Named(999);
    assert!(AlacrittyEngine::restore(&bad, &config()).is_err());
}

#[test]
fn the_cell_size_and_the_history_capacity_come_from_the_config() {
    let mut a = engine(40, 8);
    feed(&mut a, b"x");
    let snapshot = a.snapshot(1).expect("snapshot");
    let mut b = AlacrittyEngine::restore(
        &snapshot,
        &EngineConfig {
            history: 4096,
            theme: xterm_palette(),
            cell_px: (9, 18),
        },
    )
    .expect("restores");
    // 8 rows of 18 pixels and 40 columns of 9 pixels.
    let replies = feed(&mut b, b"\x1b[14t");
    assert_eq!(replies, [Effect::PtyWrite(b"\x1b[4;144;360t".to_vec())]);
}

#[test]
fn a_snapshot_stays_small() {
    // A 200x60 screen with text on half of it.
    let mut a = engine(200, 60);
    for row in 0..30 {
        feed(
            &mut a,
            format!(
                "\x1b[3{}m{row:3} {}\r\n",
                row % 8,
                "lorem ipsum ".repeat(12)
            )
            .as_bytes(),
        );
    }
    let snapshot = a.snapshot(0).expect("snapshot");
    let size = snapshot.encode().len();
    assert!(size < 40_000, "a 200x60 snapshot takes {size} bytes");
    let blank = engine(200, 60)
        .snapshot(0)
        .expect("snapshot")
        .encode()
        .len();
    assert!(blank < 1_000, "a blank 200x60 snapshot takes {blank} bytes");
}
