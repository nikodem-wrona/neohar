use super::*;

#[test]
fn utf16_length_counts_code_units() {
    assert_eq!(utf16_len(""), 0);
    assert_eq!(utf16_len("abc"), 3);
    assert_eq!(utf16_len("é"), 1);
    assert_eq!(utf16_len("日本"), 2);
    // An emoji outside the basic plane is two code units.
    assert_eq!(utf16_len("a😀b"), 4);
}

#[test]
fn an_ascii_string_maps_to_itself() {
    let map = Utf16Map::new("hello");
    assert_eq!(map.at(0), 0);
    assert_eq!(map.at(3), 3);
    assert_eq!(map.at(5), 5);
    assert_eq!(map.at(99), 5);
}

#[test]
fn byte_offsets_become_utf16_offsets() {
    // 'a' (1 byte, 1 unit), 'é' (2 bytes, 1 unit), '😀' (4 bytes, 2 units), 'b'.
    let text = "aé😀b";
    let map = Utf16Map::new(text);
    assert_eq!(map.at(0), 0);
    assert_eq!(map.at(1), 1);
    assert_eq!(map.at(3), 2);
    assert_eq!(map.at(7), 4);
    assert_eq!(map.at(8), 5);
    assert_eq!(map.at(100), 5);
    // The slice a JavaScript client takes gives the same piece of text.
    let units: Vec<u16> = text.encode_utf16().collect();
    let piece = String::from_utf16(&units[map.at(3) as usize..map.at(7) as usize]).expect("text");
    assert_eq!(piece, "😀");
}

#[test]
fn a_cap_cuts_at_a_character_boundary() {
    let text = "ab😀cd";
    let capped = cap(text, 4);
    assert!(capped.truncated);
    assert_eq!(capped.text, "ab");
    assert_eq!(cap(text, 100).text, text);
    assert!(!cap(text, 100).truncated);
}

#[test]
fn a_cap_moves_back_to_the_end_of_a_line_when_one_is_near() {
    let text = format!("{}\n{}", "x".repeat(100), "y".repeat(100));
    let capped = cap(&text, 150);
    assert!(capped.truncated);
    assert_eq!(capped.text.len(), 101);
    assert!(capped.text.ends_with('\n'));
    // A line without a line end near the cut keeps the full length.
    let long = "z".repeat(1000);
    assert_eq!(cap(&long, 500).text.len(), 500);
}

#[test]
fn the_output_cap_is_64_kilobytes() {
    let text = "a".repeat(OUTPUT_CAP + 10);
    let capped = cap_output(&text);
    assert!(capped.truncated);
    assert_eq!(capped.text.len(), OUTPUT_CAP);
    assert!(!cap_output("short").truncated);
}

#[test]
fn text_without_controls_is_returned_as_it_is() {
    let text = "plain\ttext\nwith\r\nline ends and é and 😀";
    assert!(matches!(
        strip_terminal_controls(text),
        std::borrow::Cow::Borrowed(_)
    ));
}

#[test]
fn an_osc_8_hyperlink_goes_away_with_its_url() {
    let link = "see \u{1b}]8;;file:///etc/passwd\u{7}this\u{1b}]8;;\u{7} now";
    assert_eq!(strip_terminal_controls(link), "see this now");
    let st = "a\u{1b}]8;;https://evil.example\u{1b}\\b\u{1b}]8;;\u{1b}\\c";
    assert_eq!(strip_terminal_controls(st), "abc");
}

#[test]
fn colours_and_cursor_moves_go_away() {
    assert_eq!(
        strip_terminal_controls("\u{1b}[31mred\u{1b}[0m \u{1b}[2K\u{1b}[1;1Hx"),
        "red x"
    );
}

#[test]
fn other_escape_sequences_and_control_characters_go_away() {
    assert_eq!(strip_terminal_controls("a\u{1b}cb"), "ab");
    assert_eq!(
        strip_terminal_controls("a\u{0}b\u{7}c\u{8}d\u{7f}e\u{9b}f"),
        "abcdef"
    );
    assert_eq!(strip_terminal_controls("a\u{1b}Pdata\u{1b}\\b"), "ab");
    // An escape at the end, or an open OSC, does not panic.
    assert_eq!(strip_terminal_controls("a\u{1b}"), "a");
    assert_eq!(strip_terminal_controls("a\u{1b}]8;;file:///x"), "a");
    assert_eq!(strip_terminal_controls("a\u{1b}[31"), "a");
}

#[test]
fn tabs_and_line_ends_stay() {
    assert_eq!(
        strip_terminal_controls("a\tb\nc\r\nd\u{1b}[0m"),
        "a\tb\nc\r\nd"
    );
}

#[test]
fn invisible_characters_become_visible_escapes() {
    // The name shows as `photodnammoc.png` when the override works.
    assert_eq!(
        visible("photo\u{202E}gnp.command"),
        "photo<U+202E>gnp.command"
    );
    for ch in [
        '\u{200B}',
        '\u{200F}',
        '\u{2066}',
        '\u{2069}',
        '\u{2060}',
        '\u{FEFF}',
        '\u{2028}',
        '\u{2029}',
        '\u{E0041}',
        '\u{00AD}',
    ] {
        assert!(is_invisible(ch), "{:04X}", u32::from(ch));
        assert!(
            visible(&format!("a{ch}b")).contains("<U+"),
            "{:04X}",
            u32::from(ch)
        );
    }
    // Control characters too. A tab stays, and a line feed stays only in text with lines.
    assert_eq!(visible("a\u{1b}[2Kb\tc\nd"), "a<U+001B>[2Kb\tc<U+000A>d");
    assert_eq!(
        visible_lines("a\u{1b}b\tc\nd\re"),
        "a<U+001B>b\tc\nd<U+000D>e"
    );
    // Text without them is not copied, and the letters of other scripts stay.
    assert!(matches!(visible("ls -la"), std::borrow::Cow::Borrowed(_)));
    assert_eq!(visible("日本 é 😀"), "日本 é 😀");
}

#[test]
fn the_ranges_of_highlighted_text_move_with_the_escapes() {
    let span = |start, end, class| crate::Span { start, end, class };
    let spans = crate::Spans {
        text: "echo \u{202E}x😀 y".to_owned(),
        // `x😀` starts after the override, which is one unit.
        spans: vec![span(0, 4, "c"), span(6, 9, "s"), span(10, 11, "y")],
    };
    let shown = visible_spans(spans);
    assert_eq!(shown.text, "echo <U+202E>x😀 y");
    let units: Vec<u16> = shown.text.encode_utf16().collect();
    let pieces: Vec<String> = shown
        .spans
        .iter()
        .map(|s| String::from_utf16(&units[s.start as usize..s.end as usize]).expect("text"))
        .collect();
    assert_eq!(pieces, ["echo", "x😀", "y"]);
}
