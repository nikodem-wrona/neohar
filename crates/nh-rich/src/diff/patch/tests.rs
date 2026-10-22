use super::*;

fn kinds(hunk: &PatchHunk) -> Vec<LineKind> {
    hunk.lines.iter().map(|(kind, _)| *kind).collect()
}

#[test]
fn a_git_patch_with_many_files() {
    let patch = "\
diff --git a/src/a.rs b/src/a.rs
index 111..222 100644
--- a/src/a.rs
+++ b/src/a.rs
@@ -1,3 +1,3 @@ fn main
 keep
-old
+new
 end
diff --git a/new.txt b/new.txt
new file mode 100644
index 0000000..333
--- /dev/null
+++ b/new.txt
@@ -0,0 +1,2 @@
+one
+two
diff --git a/gone.txt b/gone.txt
deleted file mode 100644
--- a/gone.txt
+++ /dev/null
@@ -1 +0,0 @@
-bye
";
    let files = parse(patch);
    assert_eq!(files.len(), 3);
    assert_eq!(files[0].new_path.as_deref(), Some("src/a.rs"));
    assert_eq!(files[0].status, None);
    assert_eq!(files[0].hunks[0].header, "@@ -1,3 +1,3 @@ fn main");
    use LineKind::{Add, Ctx, Del};
    assert_eq!(kinds(&files[0].hunks[0]), [Ctx, Del, Add, Ctx]);
    assert_eq!(files[1].status, Some(FileStatus::Added));
    assert_eq!(files[1].new_path.as_deref(), Some("new.txt"));
    assert_eq!(files[1].hunks[0].lines.len(), 2);
    assert_eq!(files[2].status, Some(FileStatus::Deleted));
    assert_eq!(files[2].old_path.as_deref(), Some("gone.txt"));
}

#[test]
fn a_rename_has_both_paths() {
    let patch = "diff --git a/old.rs b/new.rs\nsimilarity index 90%\nrename from old.rs\nrename to new.rs\n--- a/old.rs\n+++ b/new.rs\n@@ -1 +1 @@\n-a\n+b\n";
    let files = parse(patch);
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].status, Some(FileStatus::Renamed));
    assert_eq!(files[0].old_path.as_deref(), Some("old.rs"));
    assert_eq!(files[0].new_path.as_deref(), Some("new.rs"));
}

#[test]
fn a_plain_unified_diff_without_git_headers() {
    let patch = "--- a.txt\t2024-01-01\n+++ a.txt\t2024-01-02\n@@ -1 +1 @@\n-x\n+y\n--- b.txt\n+++ b.txt\n@@ -1 +1 @@\n-p\n+q\n";
    let files = parse(patch);
    assert_eq!(files.len(), 2);
    assert_eq!(files[0].new_path.as_deref(), Some("a.txt"));
    assert_eq!(files[1].new_path.as_deref(), Some("b.txt"));
}

#[test]
fn hunks_without_a_file_header_make_one_file() {
    let files = parse("@@ -1,2 +1,2 @@\n a\n-b\n+c\n");
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].new_path, None);
    assert_eq!(files[0].hunks[0].lines.len(), 3);
}

#[test]
fn a_deleted_line_that_starts_with_dashes_is_not_a_file_header() {
    // The deleted line is "-- comment", which the patch writes as "--- comment".
    let patch = "--- a.sql\n+++ a.sql\n@@ -1,2 +1,1 @@\n--- comment\n select 1;\n";
    let files = parse(patch);
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0].hunks[0].lines[0],
        (LineKind::Del, "-- comment".to_owned())
    );
    assert_eq!(files[0].hunks[0].lines.len(), 2);
}

#[test]
fn the_marker_for_a_missing_line_end_is_not_a_line() {
    let patch = "--- a\n+++ a\n@@ -1 +1 @@\n-x\n\\ No newline at end of file\n+y\n\\ No newline at end of file\n";
    let files = parse(patch);
    assert_eq!(files[0].hunks[0].lines.len(), 2);
}

#[test]
fn an_empty_context_line_that_lost_its_space_is_still_context() {
    let patch = "--- a\n+++ a\n@@ -1,3 +1,3 @@\n a\n\n-b\n+c\n";
    let files = parse(patch);
    use LineKind::{Add, Ctx, Del};
    assert_eq!(kinds(&files[0].hunks[0]), [Ctx, Ctx, Del, Add]);
}

#[test]
fn a_binary_patch_is_marked() {
    let patch = "diff --git a/i.png b/i.png\nBinary files a/i.png and b/i.png differ\n";
    let files = parse(patch);
    assert!(files[0].binary);
    assert!(files[0].hunks.is_empty());
}

#[test]
fn text_that_is_not_a_patch_gives_no_files() {
    assert!(parse("").is_empty());
    assert!(parse("hello\nthere\n").is_empty());
}

#[test]
fn hunk_numbers_read_the_header() {
    assert_eq!(hunk_numbers("@@ -1,3 +4,5 @@"), Some((1, 3, 4, 5)));
    assert_eq!(hunk_numbers("@@ -7 +9 @@ fn x"), Some((7, 1, 9, 1)));
    assert_eq!(hunk_numbers("@@ nonsense @@"), None);
}

#[test]
fn a_header_without_numbers_still_reads_the_body() {
    let files = parse("@@ odd @@\n a\n+b\n\nnext\n");
    assert_eq!(files[0].hunks[0].lines.len(), 2);
}
