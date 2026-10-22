//! Effects and replies: what the engine does to the world.

use nh_proto::answers;
use nh_term::{AlacrittyEngine, Effect, MIN_HISTORY, Recorder, Replaying, VtEngine, xterm_palette};

fn engine() -> AlacrittyEngine {
    AlacrittyEngine::new(80, 24, MIN_HISTORY, xterm_palette())
}

fn effects(bytes: &[u8]) -> Vec<Effect> {
    let mut recorder = Recorder::default();
    engine().advance(bytes, &mut recorder);
    recorder.0
}

/// A query, and the one reply that the engine writes to the child for it.
fn reply(query: &[u8]) -> Vec<u8> {
    let found = effects(query);
    let writes: Vec<_> = found
        .iter()
        .filter_map(|e| {
            if let Effect::PtyWrite(b) = e {
                Some(b.clone())
            } else {
                None
            }
        })
        .collect();
    assert_eq!(writes.len(), 1, "query {query:?} gave {found:?}");
    writes.into_iter().next().expect("one write")
}

#[test]
fn each_effect_happens_for_its_sequence() {
    let cases: [(&str, &[u8], Effect); 8] = [
        ("bell", b"\x07", Effect::Bell),
        ("title", b"\x1b]2;build\x07", Effect::TitleChanged),
        ("title reset", b"\x1b]2;\x07", Effect::TitleChanged),
        (
            "clipboard write",
            b"\x1b]52;c;aGVsbG8=\x07",
            Effect::ClipboardStore {
                clipboard: b'c',
                text: "hello".into(),
            },
        ),
        (
            "selection write",
            b"\x1b]52;p;aGk=\x07",
            Effect::ClipboardStore {
                clipboard: b'p',
                text: "hi".into(),
            },
        ),
        (
            "clipboard read",
            b"\x1b]52;c;?\x07",
            Effect::ClipboardLoadRequested,
        ),
        (
            "OSC 9 notification",
            b"\x1b]9;tests passed\x07",
            Effect::Notify {
                title: String::new(),
                body: "tests passed".into(),
            },
        ),
        (
            "OSC 777 notification",
            b"\x1b]777;notify;Claude Code;needs approval\x1b\\",
            Effect::Notify {
                title: "Claude Code".into(),
                body: "needs approval".into(),
            },
        ),
    ];
    for (name, bytes, expected) in cases {
        let found = effects(bytes);
        assert!(found.contains(&expected), "{name}: {found:?}");
    }
    assert!(matches!(
        effects(b"\x1b[c").as_slice(),
        [Effect::PtyWrite(_)]
    ));
}

#[test]
fn a_replaying_sink_drops_every_effect_and_the_screen_still_changes() {
    // Every kind of effect is in this stream.
    let bytes: &[u8] =
        b"\x07\x1b]2;build\x07\x1b]52;c;aGk=\x07\x1b]52;c;?\x07\x1b]9;x\x07\x1b]777;notify;t;b\x07\
        \x1b[c\x1b[>c\x1b[=c\x1b[>q\x1b[6n\x1b[?2004$p\x1b]11;?\x07\x1b[14t\x1b[18t shown";
    let seen = effects(bytes);
    for kind in [
        |e: &Effect| matches!(e, Effect::Bell),
        |e: &Effect| matches!(e, Effect::TitleChanged),
        |e: &Effect| matches!(e, Effect::ClipboardStore { .. }),
        |e: &Effect| matches!(e, Effect::ClipboardLoadRequested),
        |e: &Effect| matches!(e, Effect::Notify { .. }),
        |e: &Effect| matches!(e, Effect::PtyWrite(_)),
    ] {
        assert!(
            seen.iter().any(kind),
            "the stream lacks a kind of effect: {seen:?}"
        );
    }

    let mut engine = engine();
    let mut replaying = Replaying::default();
    engine.advance(bytes, &mut replaying);
    assert_eq!(
        replaying.dropped,
        seen.len(),
        "every effect that the recorder sees is dropped"
    );
    assert_eq!(engine.title(), Some("build"));
    assert_eq!(engine.bell_count(), 1);
    let mut row = String::new();
    engine.visit_row(0, &mut |c| row.push(c.ch));
    assert!(row.contains("shown"));
}

#[test]
fn the_replies_that_need_no_screen_are_the_shared_bytes() {
    assert_eq!(reply(b"\x1b[c"), answers::DA1);
    assert_eq!(reply(b"\x1b[0c"), answers::DA1);
    assert_eq!(reply(b"\x1b[>c"), answers::DA2);
    assert_eq!(reply(b"\x1b[=c"), answers::DA3);
    assert_eq!(reply(b"\x1b[>q"), answers::XTVERSION);
    assert_eq!(reply(b"\x1b[>0q"), answers::XTVERSION);
}

#[test]
fn decrqm_for_the_modes_that_the_holder_answers_is_what_the_holder_would_say() {
    // The holder answers for these two modes. The engine must say the same.
    assert_eq!(
        reply(b"\x1b[?2004$p"),
        answers::decrqm(true, 2004, Some(false))
    );
    assert_eq!(
        reply(b"\x1b[?2004h\x1b[?2004$p").len(),
        reply(b"\x1b[?2004$p").len()
    );
    let set = effects(b"\x1b[?2004h\x1b[?2004$p");
    assert_eq!(
        set,
        [Effect::PtyWrite(answers::decrqm(true, 2004, Some(true)))]
    );
    let reset = effects(b"\x1b[?1004h\x1b[?1004l\x1b[?1004$p");
    assert_eq!(
        reset,
        [Effect::PtyWrite(answers::decrqm(true, 1004, Some(false)))]
    );
    let on = effects(b"\x1b[?1004h\x1b[?1004$p");
    assert_eq!(
        on,
        [Effect::PtyWrite(answers::decrqm(true, 1004, Some(true)))]
    );
}

#[test]
fn the_queries_that_need_the_screen_get_an_answer_from_the_engine() {
    assert_eq!(reply(b"abc\x1b[6n"), b"\x1b[1;4R");
    assert_eq!(reply(b"\x1b[3;7H\x1b[6n"), b"\x1b[3;7R");
    assert_eq!(reply(b"\x1b[18t"), b"\x1b[8;24;80t");
    // The colour queries use the theme: foreground #e5e5e5, background black.
    assert_eq!(reply(b"\x1b]10;?\x07"), b"\x1b]10;rgb:e5e5/e5e5/e5e5\x07");
    assert_eq!(
        reply(b"\x1b]11;?\x1b\\"),
        b"\x1b]11;rgb:0000/0000/0000\x1b\\"
    );
    let mut engine = engine();
    engine.set_cell_size(9, 18);
    let mut recorder = Recorder::default();
    engine.advance(b"\x1b[14t", &mut recorder);
    assert_eq!(recorder.0, [Effect::PtyWrite(b"\x1b[4;432;720t".to_vec())]);
}

#[test]
fn replies_and_effects_keep_the_order_of_the_input() {
    let found = effects(b"\x1b[=c\x1b[c\x1b[>q\x07\x1b[6n\x1b]9;a\x07");
    assert_eq!(
        found,
        [
            Effect::PtyWrite(answers::DA3.to_vec()),
            Effect::PtyWrite(answers::DA1.to_vec()),
            Effect::PtyWrite(answers::XTVERSION.to_vec()),
            Effect::Bell,
            Effect::PtyWrite(b"\x1b[1;1R".to_vec()),
            Effect::Notify {
                title: String::new(),
                body: "a".into()
            },
        ]
    );
}

#[test]
fn a_query_cut_between_two_chunks_is_still_answered_once() {
    let mut engine = engine();
    let mut recorder = Recorder::default();
    engine.advance(b"x\x1b[", &mut recorder);
    engine.advance(b">", &mut recorder);
    engine.advance(b"q", &mut recorder);
    assert_eq!(recorder.0, [Effect::PtyWrite(answers::XTVERSION.to_vec())]);
}

#[test]
fn the_palette_follows_the_program() {
    let mut engine = engine();
    let theme = engine.palette();
    engine.advance(
        b"\x1b]4;1;rgb:12/34/56\x07\x1b]11;rgb:aa/bb/cc\x07",
        &mut Replaying::default(),
    );
    let p = engine.palette();
    assert_eq!(
        (p.colors[1], p.bg, p.fg),
        (0x0012_3456, 0x00aa_bbcc, theme.fg)
    );
    engine.advance(b"\x1b]104\x07\x1b]111\x07", &mut Replaying::default());
    assert_eq!(engine.palette(), theme);
}

#[test]
fn modes_are_reported_for_the_ui() {
    use nh_proto::modes::ModeFlags;
    let mut engine = engine();
    let default = engine.modes();
    assert!(default.contains(ModeFlags::AUTOWRAP) && !default.contains(ModeFlags::ALT_SCREEN));
    engine.advance(
        b"\x1b[?1049h\x1b[?1h\x1b[?1000h\x1b[?1006h\x1b[?2004h\x1b[?1004h\x1b[>3u",
        &mut Replaying::default(),
    );
    let modes = engine.modes();
    for flag in [
        ModeFlags::ALT_SCREEN,
        ModeFlags::APP_CURSOR,
        ModeFlags::MOUSE_1000,
        ModeFlags::MOUSE_SGR,
        ModeFlags::BRACKETED_PASTE,
        ModeFlags::FOCUS_EVENTS,
        ModeFlags::KITTY_DISAMBIGUATE,
        ModeFlags::KITTY_REPORT_EVENT_TYPES,
    ] {
        assert!(modes.contains(flag), "{flag:?} in {modes:?}");
    }
    assert_eq!(modes.kitty_flags(), 3);
}
