use super::*;

fn numbered(lines: usize, changed: usize) -> (String, String) {
    let old: Vec<String> = (1..=lines).map(|n| format!("line {n}")).collect();
    let mut new = old.clone();
    new[changed - 1] = "changed".to_owned();
    (old.join("\n") + "\n", new.join("\n") + "\n")
}

fn kinds(hunk: &Hunk) -> Vec<LineKind> {
    hunk.lines.iter().map(|line| line.kind).collect()
}

fn edit_diff(path: &str, old: &str, new: &str, file: Option<&str>) -> FileDiff {
    let edits = [Edit {
        old,
        new,
        replace_all: false,
    }];
    let result = diff(&[Change::Edits {
        path,
        edits: &edits,
        file,
    }]);
    result.files.into_iter().next().expect("a file")
}

#[test]
fn a_change_has_three_lines_of_context_on_each_side() {
    let (old, new) = numbered(20, 10);
    let file = edit_diff("notes.txt", &old, &new, None);
    assert_eq!(file.hunks.len(), 1);
    let hunk = &file.hunks[0];
    use LineKind::{Add, Ctx, Del};
    assert_eq!(kinds(hunk), [Ctx, Ctx, Ctx, Del, Add, Ctx, Ctx, Ctx]);
    assert_eq!(file.stats, Stats { add: 1, del: 1 });
    assert!(!file.truncated);
    assert_eq!(file.status, FileStatus::Modified);
}

#[test]
fn two_changes_far_apart_make_two_hunks() {
    let old: Vec<String> = (1..=30).map(|n| format!("line {n}")).collect();
    let mut new = old.clone();
    new[2] = "first".to_owned();
    new[26] = "second".to_owned();
    let file = edit_diff(
        "a.txt",
        &(old.join("\n") + "\n"),
        &(new.join("\n") + "\n"),
        None,
    );
    assert_eq!(file.hunks.len(), 2);
}

#[test]
fn an_edit_without_the_file_has_no_line_numbers() {
    let (old, new) = numbered(5, 3);
    let file = edit_diff("a.txt", &old, &new, None);
    let hunk = &file.hunks[0];
    assert_eq!(hunk.header, "@@");
    assert!(
        hunk.lines
            .iter()
            .all(|line| line.old.is_none() && line.new.is_none())
    );
}

#[test]
fn an_edit_in_the_file_is_a_diff_of_the_file_with_real_line_numbers() {
    let file_text = "one\ntwo\nthree\nfour\nfive\nsix\nseven\neight\nnine\nten\n";
    let file = edit_diff("a.txt", "six\n", "SIX\n", Some(file_text));
    assert_eq!(file.hunks.len(), 1);
    let hunk = &file.hunks[0];
    // Three lines of context from the file around the change.
    assert_eq!(hunk.header, "@@ -3,7 +3,7 @@");
    use LineKind::{Add, Ctx, Del};
    assert_eq!(kinds(hunk), [Ctx, Ctx, Ctx, Del, Add, Ctx, Ctx, Ctx]);
    let del = hunk
        .lines
        .iter()
        .find(|line| line.kind == Del)
        .expect("a del");
    assert_eq!((del.old, del.new), (Some(6), None));
    let add = hunk
        .lines
        .iter()
        .find(|line| line.kind == Add)
        .expect("an add");
    assert_eq!((add.old, add.new), (None, Some(6)));
    assert_eq!((hunk.lines[0].old, hunk.lines[0].new), (Some(3), Some(3)));
    assert_eq!(file.stats, Stats { add: 1, del: 1 });
}

#[test]
fn an_old_text_that_is_not_in_the_place_that_the_edit_says_falls_back_to_the_texts() {
    let file_text = "x = 1\ny = 2\nx = 1\n";
    // Twice in the file, and the edit does not say all.
    let file = edit_diff("a.txt", "x = 1\n", "x = 3\n", Some(file_text));
    assert_eq!(file.hunks[0].header, "@@");
    assert_eq!(file.hunks[0].lines.len(), 2);
    // Not in the file at all.
    let file = edit_diff("a.txt", "z = 1\n", "z = 3\n", Some(file_text));
    assert_eq!(file.hunks[0].header, "@@");
}

#[test]
fn replace_all_changes_every_place() {
    let file_text = "x = 1\ny = 2\nx = 1\n";
    let edits = [Edit {
        old: "x = 1",
        new: "x = 3",
        replace_all: true,
    }];
    let result = diff(&[Change::Edits {
        path: "a.txt",
        edits: &edits,
        file: Some(file_text),
    }]);
    assert_eq!(result.files[0].stats, Stats { add: 2, del: 2 });
    assert_eq!(result.files[0].hunks[0].header, "@@ -1,3 +1,3 @@");
}

#[test]
fn a_multi_edit_in_the_file_applies_the_edits_in_order() {
    let file_text = "a\nb\nc\n";
    let edits = [
        Edit {
            old: "a",
            new: "b",
            replace_all: false,
        },
        // The second edit finds the text that the first one made.
        Edit {
            old: "b\nb",
            new: "c\nb",
            replace_all: false,
        },
    ];
    let result = diff(&[Change::Edits {
        path: "a.txt",
        edits: &edits,
        file: Some(file_text),
    }]);
    let file = &result.files[0];
    assert_eq!(file.hunks.len(), 1);
    assert_eq!(file.stats, Stats { add: 1, del: 1 });
}

#[test]
fn a_multi_edit_has_a_hunk_for_each_edit_and_the_sum_of_the_stats() {
    let edits = [
        Edit {
            old: "a\n",
            new: "b\n",
            replace_all: false,
        },
        Edit {
            old: "c\nd\n",
            new: "c\n",
            replace_all: false,
        },
    ];
    let result = diff(&[Change::Edits {
        path: "x.txt",
        edits: &edits,
        file: None,
    }]);
    let file = &result.files[0];
    assert_eq!(file.hunks.len(), 2);
    assert_eq!(file.stats, Stats { add: 1, del: 2 });
}

#[test]
fn a_new_file_is_all_additions() {
    let result = diff(&[Change::Write {
        path: "src/new.rs",
        before: None,
        after: "fn a() {}\nfn b() {}\n",
    }]);
    let file = &result.files[0];
    assert_eq!(file.status, FileStatus::Added);
    assert_eq!(file.stats, Stats { add: 2, del: 0 });
    assert_eq!(file.hunks[0].header, "@@ -0,0 +1,2 @@");
    assert!(
        file.hunks[0]
            .lines
            .iter()
            .all(|line| line.kind == LineKind::Add)
    );
    assert_eq!(file.hunks[0].lines[1].new, Some(2));
}

#[test]
fn a_write_over_a_file_is_a_modification() {
    let result = diff(&[Change::Write {
        path: "a.txt",
        before: Some("one\ntwo\n"),
        after: "one\nthree\n",
    }]);
    let file = &result.files[0];
    assert_eq!(file.status, FileStatus::Modified);
    assert_eq!(file.hunks[0].header, "@@ -1,2 +1,2 @@");
    assert_eq!(file.stats, Stats { add: 1, del: 1 });
}

#[test]
fn a_removed_file_is_all_deletions() {
    let result = diff(&[Change::Remove {
        path: "old.txt",
        before: Some("a\nb\nc\n"),
    }]);
    let file = &result.files[0];
    assert_eq!(file.status, FileStatus::Deleted);
    assert_eq!(file.stats, Stats { add: 0, del: 3 });
    assert_eq!(file.hunks[0].header, "@@ -1,3 +0,0 @@");
}

#[test]
fn a_changed_line_has_the_changed_words_marked() {
    let file = edit_diff(
        "a.rs",
        "let total = price * count;\n",
        "let total = price * quantity;\n",
        None,
    );
    let lines = &file.hunks[0].lines;
    let del = &lines[0];
    let add = &lines[1];
    let range = |line: &DiffLine| -> String {
        let (start, end) = line.intra.as_ref().expect("ranges")[0];
        line.text.text[start as usize..end as usize].to_owned()
    };
    assert_eq!(range(del), "count");
    assert_eq!(range(add), "quantity");
    // Lines that did not change have no ranges.
    let both = edit_diff("a.rs", "a\nb\nc\n", "a\nB\nc\n", None);
    assert!(both.hunks[0].lines[0].intra.is_none());
}

#[test]
fn ranges_inside_a_line_count_utf16_code_units() {
    let file = edit_diff("a.txt", "😀 old end\n", "😀 new end\n", None);
    let del = &file.hunks[0].lines[0];
    let (start, end) = del.intra.as_ref().expect("ranges")[0];
    // The emoji is two code units, so "old" starts at 3 and not at 5.
    assert_eq!((start, end), (3, 6));
    let units: Vec<u16> = del.text.text.encode_utf16().collect();
    assert_eq!(String::from_utf16(&units[3..6]).expect("text"), "old");
}

#[test]
fn lines_are_highlighted_by_the_language_of_the_path() {
    let file = edit_diff("src/lib.rs", "fn a() {}\n", "fn b() {}\n", None);
    let add = &file.hunks[0].lines[1];
    assert!(
        add.text
            .spans
            .iter()
            .any(|span| span.class == crate::class::KEYWORD),
        "{:?}",
        add.text
    );
    // No language, no spans.
    let plain = edit_diff("notes.unknownext", "fn a() {}\n", "fn b() {}\n", None);
    assert!(plain.hunks[0].lines[1].text.spans.is_empty());
}

#[test]
fn the_highlight_goes_on_across_the_lines_of_the_file() {
    // The comment is open on the lines that the diff shows from the middle of the file.
    let old = "/* start\nmiddle\nend */\nfn a() {}\n";
    let new = "/* start\nmiddle\nend */\nfn b() {}\n";
    let result = diff(&[Change::Write {
        path: "x.rs",
        before: Some(old),
        after: new,
    }]);
    let middle = result.files[0].hunks[0]
        .lines
        .iter()
        .find(|line| line.text.text == "middle")
        .expect("a context line");
    assert!(
        middle
            .text
            .spans
            .iter()
            .any(|span| span.class == crate::class::COMMENT),
        "{:?}",
        middle.text
    );
}

#[test]
fn a_file_over_the_cap_is_cut_and_the_totals_stay_right() {
    let after: String = (0..5000).map(|n| format!("line {n}\n")).collect();
    let result = diff(&[Change::Write {
        path: "big.txt",
        before: None,
        after: &after,
    }]);
    let file = &result.files[0];
    assert!(file.truncated);
    let shown: usize = file.hunks.iter().map(|hunk| hunk.lines.len()).sum();
    assert_eq!(shown, MAX_LINES);
    assert_eq!(file.stats.add, 5000);
}

#[test]
fn a_file_just_under_the_cap_is_not_cut() {
    let after: String = (0..MAX_LINES).map(|n| format!("line {n}\n")).collect();
    let result = diff(&[Change::Write {
        path: "big.txt",
        before: None,
        after: &after,
    }]);
    assert!(!result.files[0].truncated);
}

#[test]
fn text_with_a_nul_byte_is_a_binary_file() {
    let result = diff(&[Change::Write {
        path: "a.bin",
        before: Some("a\0b"),
        after: "a\0c",
    }]);
    let file = &result.files[0];
    assert_eq!(file.hunks[0].header, "Binary file");
    assert!(file.hunks[0].lines.is_empty());
}

#[test]
fn no_change_gives_no_hunks() {
    let file = edit_diff("a.txt", "same\n", "same\n", None);
    assert!(file.hunks.is_empty());
    assert_eq!(file.stats, Stats::default());
}

#[test]
fn crlf_text_has_no_carriage_return_in_the_lines() {
    let file = edit_diff("a.txt", "a\r\nb\r\n", "a\r\nc\r\n", None);
    for line in &file.hunks[0].lines {
        assert!(!line.text.text.contains('\r'), "{:?}", line.text);
    }
}

#[test]
fn between_takes_the_language_from_the_name_or_the_path() {
    let by_name = between("fn a() {}\n", "fn b() {}\n", None, Some("rust"));
    assert!(!by_name.files[0].hunks[0].lines[1].text.spans.is_empty());
    let by_path = between("fn a() {}\n", "fn b() {}\n", Some("x.rs"), None);
    assert_eq!(by_path.files[0].path, "x.rs");
    assert!(!by_path.files[0].hunks[0].lines[1].text.spans.is_empty());
    let none = between("fn a() {}\n", "fn b() {}\n", None, None);
    assert!(none.files[0].hunks[0].lines[1].text.spans.is_empty());
}

#[test]
fn the_unified_text_holds_the_whole_diff() {
    let after: String = (0..5000).map(|n| format!("line {n}\n")).collect();
    let text = unified(&[Change::Write {
        path: "big.txt",
        before: None,
        after: &after,
    }]);
    assert!(
        text.starts_with("--- a/big.txt\n+++ b/big.txt\n@@ -0,0 +1,5000 @@"),
        "{}",
        &text[..80]
    );
    assert_eq!(text.matches("\n+line").count(), 5000);
}

#[test]
fn a_patch_gives_a_file_for_each_header() {
    let patch = "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1,2 +1,2 @@\n one\n-two\n+three\n";
    let result = diff(&[Change::Patch {
        path: None,
        status: None,
        text: patch,
    }]);
    assert_eq!(result.files.len(), 1);
    let file = &result.files[0];
    assert_eq!(file.path, "a.txt");
    assert_eq!(file.stats, Stats { add: 1, del: 1 });
    assert_eq!(file.hunks[0].header, "@@ -1,2 +1,2 @@");
    let lines = &file.hunks[0].lines;
    assert_eq!((lines[0].old, lines[0].new), (Some(1), Some(1)));
    assert_eq!((lines[1].old, lines[1].new), (Some(2), None));
    assert_eq!((lines[2].old, lines[2].new), (None, Some(2)));
}
