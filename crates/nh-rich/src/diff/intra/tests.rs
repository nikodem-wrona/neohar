use super::*;

fn cut<'a>(text: &'a str, ranges: &[Range<usize>]) -> Vec<&'a str> {
    ranges.iter().map(|r| &text[r.clone()]).collect()
}

#[test]
fn one_changed_word_is_the_range() {
    let old = "let total = price * count;";
    let new = "let total = price * quantity;";
    let found = ranges(old, new).expect("ranges");
    assert_eq!(cut(old, &found.old), ["count"]);
    assert_eq!(cut(new, &found.new), ["quantity"]);
}

#[test]
fn an_added_argument_is_a_range_on_the_new_side_only() {
    let old = "call(a, b)";
    let new = "call(a, b, c)";
    let found = ranges(old, new).expect("ranges");
    assert!(found.old.is_empty());
    assert_eq!(cut(new, &found.new), [", c"]);
}

#[test]
fn one_word_that_differs_in_one_character_falls_back_to_characters() {
    let old = "version1";
    let new = "version2";
    let found = ranges(old, new).expect("ranges");
    assert_eq!(cut(old, &found.old), ["1"]);
    assert_eq!(cut(new, &found.new), ["2"]);
}

#[test]
fn a_line_that_is_all_different_has_no_ranges() {
    assert_eq!(ranges("alpha beta", "gamma delta"), None);
}

#[test]
fn equal_lines_and_long_lines_have_no_ranges() {
    assert_eq!(ranges("same", "same"), None);
    let long = "a ".repeat(MAX_LINE);
    assert_eq!(ranges(&long, "b"), None);
}

#[test]
fn indentation_is_a_range() {
    let found = ranges("    return x;", "        return x;").expect("ranges");
    // The run of spaces is one token, so each side marks all of its run.
    assert_eq!(found.old, vec![0..4]);
    assert_eq!(found.new, vec![0..8]);
}

#[test]
fn ranges_are_on_character_boundaries_for_wide_characters() {
    let old = "let s = \"héllo wörld\";";
    let new = "let s = \"héllo wörld!\";";
    let found = ranges(old, new).expect("ranges");
    for range in &found.new {
        assert!(new.is_char_boundary(range.start) && new.is_char_boundary(range.end));
    }
    for range in &found.old {
        assert!(old.is_char_boundary(range.start) && old.is_char_boundary(range.end));
    }
}
