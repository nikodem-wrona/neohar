use super::*;

/// The text of each span with its class, for a readable assertion.
fn pieces(spans: &Spans) -> Vec<(String, &'static str)> {
    let units: Vec<u16> = spans.text.encode_utf16().collect();
    spans
        .spans
        .iter()
        .map(|span| {
            let piece = String::from_utf16(&units[span.start as usize..span.end as usize])
                .expect("text in range");
            (piece, span.class)
        })
        .collect()
}

#[test]
fn rust_has_keywords_strings_and_comments() {
    let spans = highlight("fn main() { let x = \"hi\"; } // done\n", "rust");
    let found = pieces(&spans);
    assert!(
        found.contains(&("fn".to_owned(), class::KEYWORD)),
        "{found:?}"
    );
    assert!(
        found.contains(&("let".to_owned(), class::KEYWORD)),
        "{found:?}"
    );
    assert!(
        found
            .iter()
            .any(|(text, class)| *class == class::STRING && text.contains("hi")),
        "{found:?}"
    );
    assert!(
        found
            .iter()
            .any(|(text, class)| *class == class::COMMENT && text.contains("done")),
        "{found:?}"
    );
}

#[test]
fn a_string_with_many_lines_goes_on_across_a_line_end() {
    let spans = highlight("let a = \"one\ntwo\";\n", "rust");
    let strings: Vec<String> = pieces(&spans)
        .into_iter()
        .filter(|(_, class)| *class == class::STRING)
        .map(|(text, _)| text)
        .collect();
    assert!(
        strings.iter().any(|text| text.contains("two")),
        "{strings:?}"
    );
}

#[test]
fn offsets_count_utf16_code_units() {
    // The emoji is two code units and six bytes after it would move a byte offset.
    let spans = highlight("let s = \"😀\"; // é\n", "rust");
    let units: Vec<u16> = spans.text.encode_utf16().collect();
    for span in &spans.spans {
        assert!(span.start < span.end);
        assert!((span.end as usize) <= units.len());
    }
    let comment = spans
        .spans
        .iter()
        .find(|span| span.class == class::COMMENT)
        .expect("a comment");
    let text = String::from_utf16(&units[comment.start as usize..comment.end as usize])
        .expect("text in range");
    assert_eq!(text, "// é");
}

#[test]
fn crlf_line_ends_do_not_move_the_spans() {
    let spans = highlight("let a = 1;\r\nlet b = 2;\r\n", "rust");
    let lets: Vec<&Span> = spans
        .spans
        .iter()
        .filter(|span| span.class == class::KEYWORD)
        .collect();
    assert_eq!(lets.len(), 2);
    // The second line starts after "let a = 1;\r\n", which is 12 code units.
    assert_eq!(lets[1].start, 12);
}

#[test]
fn an_unknown_language_gives_plain_text() {
    let spans = highlight("hello there", "klingon");
    assert_eq!(spans.text, "hello there");
    assert!(spans.spans.is_empty());
    assert!(highlight("x", "").spans.is_empty());
}

#[test]
fn language_names_and_extensions_resolve() {
    for token in [
        "rust", "rs", "Rust", "python", "py", "sh", "bash", "json", "yaml", "yml",
    ] {
        assert!(language(token).is_some(), "{token}");
    }
    // These have no grammar of their own and use a near one.
    for token in ["ts", "tsx", "typescript", "jsx"] {
        assert_eq!(
            language(token).map(|l| l.name()),
            Some("JavaScript"),
            "{token}"
        );
    }
    assert!(
        language("plain text").is_none(),
        "plain text is no language"
    );
    assert_eq!(
        language_of_path("src/main.rs").map(|l| l.name()),
        Some("Rust")
    );
    assert_eq!(
        language_of_path("web/App.tsx").map(|l| l.name()),
        Some("JavaScript")
    );
    assert_eq!(
        language_of_path("Makefile").map(|l| l.name()),
        Some("Makefile")
    );
    assert!(language_of_path("notes.unknownext").is_none());
    assert!(language_of_path("README").is_none());
}

#[test]
fn a_long_line_is_left_plain_and_the_next_line_is_still_read() {
    let long = format!("let a = \"{}\";\nlet b = 1;\n", "x".repeat(MAX_LINE + 10));
    let spans = highlight(&long, "rust");
    let keywords: Vec<&Span> = spans
        .spans
        .iter()
        .filter(|span| span.class == class::KEYWORD)
        .collect();
    assert_eq!(keywords.len(), 1, "{keywords:?}");
}

#[test]
fn spans_do_not_overlap_and_stay_in_order() {
    let source = "use std::fmt;\n\n/// docs\nstruct A<'a> { b: &'a str, c: u32 }\nimpl A<'_> { fn f(&self) -> u32 { 0x1f + 2 } }\n";
    let spans = highlight(source, "rust");
    let mut end = 0;
    for span in &spans.spans {
        assert!(span.start >= end, "{span:?} after {end}");
        assert!(span.end > span.start);
        end = span.end;
    }
    assert!(!spans.spans.is_empty());
}
