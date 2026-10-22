use super::*;

fn parsed(text: &str) -> Node {
    parse(text).unwrap_or_else(|e| panic!("{text:?}: {e}"))
}

#[test]
fn the_places_of_the_values_are_exact() {
    let text = "{ \"a\": [1, \"x\"],\n  \"b\": {\"c\": null} }";
    let root = parsed(text);
    assert_eq!((root.start, root.end), (0, text.len()));
    let a = root.get("a").expect("a");
    assert_eq!(a.value.text(text), "[1, \"x\"]");
    assert_eq!(a.value.elements().map(<[Node]>::len), Some(2));
    assert_eq!(&text[a.start..a.value.end], "\"a\": [1, \"x\"]");
    let c = root.get("b").and_then(|b| b.value.get("c")).expect("c");
    assert_eq!(c.value.text(text), "null");
    assert_eq!(root.get("b").expect("b").value.text(text), "{\"c\": null}");
}

#[test]
fn strings_with_escapes_and_unicode_are_read() {
    let text = r#"{"k\"ey": "a\\b é é", "n": -1.5e3, "t": true}"#;
    let root = parsed(text);
    assert_eq!(
        root.get("k\"ey").and_then(|m| m.value.as_str()),
        Some("a\\b é é")
    );
    assert!(root.get("n").is_some() && root.get("t").is_some());
}

#[test]
fn a_text_that_is_not_json_is_refused() {
    for bad in [
        "",
        "{",
        "{\"a\" 1}",
        "{\"a\": 1,}",
        "[1 2]",
        "{\"a\": 1} x",
        "{\"a\": 1} // c",
        "// c\n{}",
        "{\"a\": tru}",
        "{'a': 1}",
        "{\"a\": \"x}",
    ] {
        assert!(parse(bad).is_err(), "{bad:?}");
    }
    assert!(parse("\u{feff}{}").is_ok(), "a byte order mark is allowed");
}

#[test]
fn the_indentation_and_the_unit_come_from_the_text() {
    let two = "{\n  \"a\": 1\n}";
    assert_eq!(indent_unit(two, &parsed(two)), "  ");
    let four = "{\n    \"a\": 1\n}";
    assert_eq!(indent_unit(four, &parsed(four)), "    ");
    let tab = "{\n\t\"a\": 1\n}";
    assert_eq!(indent_unit(tab, &parsed(tab)), "\t");
    let inline = "{\"a\": 1}";
    assert_eq!(indent_unit(inline, &parsed(inline)), "  ");
    assert_eq!(line_indent("{\n    \"a\": 1\n}", 8), "    ");
}

fn push_member(text: &str, member: &str) -> String {
    let root = parsed(text);
    let unit = indent_unit(text, &root);
    let edit = push(text, &root, member, &unit);
    let out = edit.apply(text);
    parse(&out).unwrap_or_else(|e| panic!("{out:?}: {e}"));
    out
}

#[test]
fn a_member_goes_after_the_last_one_in_the_style_of_the_text() {
    assert_eq!(
        push_member("{\n  \"a\": 1\n}\n", "\"b\": 2"),
        "{\n  \"a\": 1,\n  \"b\": 2\n}\n"
    );
    assert_eq!(
        push_member("{\n\t\"a\": 1,\n\t\"c\": [1]\n}", "\"b\": {\n\t\"x\": 1\n}"),
        "{\n\t\"a\": 1,\n\t\"c\": [1],\n\t\"b\": {\n\t\t\"x\": 1\n\t}\n}"
    );
    assert_eq!(
        push_member("{\"a\": 1}", "\"b\": 2"),
        "{\"a\": 1, \"b\": 2}"
    );
    assert_eq!(push_member("{}\n", "\"a\": 1"), "{\n  \"a\": 1\n}\n");
    assert_eq!(push_member("{ }", "\"a\": 1"), "{\n  \"a\": 1\n}");
}

#[test]
fn a_member_in_a_nested_object_gets_the_indentation_of_its_neighbours() {
    let text = "{\n  \"x\": {\n    \"a\": 1\n  }\n}";
    let root = parsed(text);
    let x = &root.get("x").expect("x").value;
    let edit = push(text, x, "\"b\": 2", "  ");
    assert_eq!(
        edit.apply(text),
        "{\n  \"x\": {\n    \"a\": 1,\n    \"b\": 2\n  }\n}"
    );
    let empty = "{\n  \"x\": {}\n}";
    let root = parsed(empty);
    let edit = push(empty, &root.get("x").expect("x").value, "\"b\": 2", "  ");
    assert_eq!(edit.apply(empty), "{\n  \"x\": {\n    \"b\": 2\n  }\n}");
}

#[test]
fn an_element_goes_after_the_last_one() {
    let text = "{\n  \"l\": [\n    1,\n    2\n  ]\n}";
    let root = parsed(text);
    let list = &root.get("l").expect("l").value;
    assert_eq!(
        push(text, list, "{\n  \"a\": 1\n}", "  ").apply(text),
        "{\n  \"l\": [\n    1,\n    2,\n    {\n      \"a\": 1\n    }\n  ]\n}"
    );
    let inline = "{\"l\": [1, 2]}";
    let root = parsed(inline);
    assert_eq!(
        push(inline, &root.get("l").expect("l").value, "3", "  ").apply(inline),
        "{\"l\": [1, 2, 3]}"
    );
    let empty = "{\"l\": []}";
    let root = parsed(empty);
    let out = push(empty, &root.get("l").expect("l").value, "3", "  ").apply(empty);
    parse(&out).expect("JSON");
}

#[test]
fn a_child_goes_out_with_its_comma() {
    let text = "{\n  \"a\": 1,\n  \"b\": 2,\n  \"c\": 3\n}";
    let root = parsed(text);
    assert_eq!(
        remove(&root, 0).apply(text),
        "{\n  \"b\": 2,\n  \"c\": 3\n}"
    );
    assert_eq!(
        remove(&root, 1).apply(text),
        "{\n  \"a\": 1,\n  \"c\": 3\n}"
    );
    assert_eq!(
        remove(&root, 2).apply(text),
        "{\n  \"a\": 1,\n  \"b\": 2\n}"
    );
    let one = "{\n  \"a\": 1\n}";
    assert_eq!(remove(&parsed(one), 0).apply(one), "{}");
    let array = "[1, 2, 3]";
    assert_eq!(remove(&parsed(array), 1).apply(array), "[1, 3]");
}

#[test]
fn pushing_and_removing_the_last_child_gives_the_text_back() {
    for text in [
        "{\n  \"a\": 1\n}\n",
        "{\n    \"a\": 1,\n    \"b\": [1, 2]\n}",
        "{\"a\": 1}",
        "{}",
    ] {
        let root = parsed(text);
        let unit = indent_unit(text, &root);
        let grown = push(text, &root, "\"z\": {\n  \"k\": 1\n}", &unit).apply(text);
        let grown_root = parsed(&grown);
        let last = grown_root.members().map_or(0, <[Member]>::len) - 1;
        assert_eq!(remove(&grown_root, last).apply(&grown), text, "{text:?}");
    }
}

#[test]
fn a_replacement_changes_one_value() {
    let text = "{\"a\": [1, 2], \"b\": 3}";
    let root = parsed(text);
    let a = &root.get("a").expect("a").value;
    assert_eq!(replace(a, "[]").apply(text), "{\"a\": [], \"b\": 3}");
}
