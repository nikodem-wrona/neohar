use super::*;

#[test]
fn a_snippet_loses_its_marks_and_gives_the_ranges_of_the_matches() {
    let spans = spans_of("error: \u{2}migration\u{3} failed: \u{2}no\u{3} such column");
    assert_eq!(spans.text, "error: migration failed: no such column");
    assert_eq!(
        spans.spans,
        vec![(7, 16, "match".to_owned()), (25, 27, "match".to_owned())]
    );
}

#[test]
fn the_ranges_count_utf_16_code_units_and_a_bracket_is_only_text() {
    // "é" is one unit and the emoji is two.
    let spans = spans_of("[é😀] \u{2}x\u{3}");
    assert_eq!(spans.text, "[é😀] x");
    assert_eq!(spans.spans, vec![(6, 7, "match".to_owned())]);
}

#[test]
fn a_mark_without_a_pair_or_with_no_text_gives_no_range() {
    assert!(spans_of("a \u{2}\u{3} b").spans.is_empty());
    assert!(spans_of("a \u{3} b \u{2} c").spans.is_empty());
    assert_eq!(spans_of("plain").spans, vec![]);
}
