//! The rows and the totals of a count. The count itself is tested in `nh-tasks`, and the whole
//! path through a core in `crates/nh/tests/tasks.rs`.

use super::*;

fn file(path: &[u8], kind: ChangeKind) -> ChangedFile {
    ChangedFile {
        path: path.to_vec(),
        old_path: None,
        kind,
        add: 2,
        del: 1,
        binary: false,
        uncommitted: false,
        stamp: 7,
    }
}

fn changes(files: Vec<ChangedFile>) -> Changes {
    Changes {
        merge_base: "0123456789abcdef0123456789abcdef01234567".to_owned(),
        short: "0123456".to_owned(),
        commits: 3,
        files,
    }
}

#[test]
fn a_row_has_its_folder_with_a_slash_and_its_name() {
    let mut renamed = file(b"src/app/main.rs", ChangeKind::Renamed);
    renamed.old_path = Some(b"src/main.rs".to_vec());
    let row = row_value("src/app/main.rs", &renamed);
    assert_eq!(
        (row.dir.as_str(), row.name.as_str()),
        ("src/app/", "main.rs")
    );
    assert_eq!(row.old_path.as_deref(), Some("src/main.rs"));
    assert_eq!(row.status, ChangeStatus::Renamed);
    let row = row_value("README.md", &file(b"README.md", ChangeKind::Untracked));
    assert_eq!((row.dir.as_str(), row.name.as_str()), ("", "README.md"));
    assert_eq!(row.status, ChangeStatus::Untracked);
}

#[test]
fn a_count_keeps_the_first_thousand_rows_and_counts_every_file() {
    let files: Vec<ChangedFile> = (0..=MAX_ROWS)
        .map(|n| file(format!("f{n:05}").as_bytes(), ChangeKind::Added))
        .collect();
    let (totals, merge_base, rows) = counted(changes(files));
    assert_eq!(rows.len(), MAX_ROWS);
    assert_eq!(rows[0].0, "f00000");
    assert_eq!(
        (totals.files, totals.commits, totals.truncated),
        (1_001, 3, true)
    );
    assert_eq!(totals.merge_base.as_deref(), Some("0123456"));
    assert!(merge_base.starts_with("0123456") && merge_base.len() == 40);
    let (totals, _, rows) = counted(changes(vec![file(b"a", ChangeKind::Modified)]));
    assert_eq!((rows.len(), totals.files, totals.truncated), (1, 1, false));
}

#[test]
fn two_paths_that_are_the_same_text_are_one_row() {
    // Two bytes that are not UTF-8 both become U+FFFD.
    let (totals, _, rows) = counted(changes(vec![
        file(b"a\xff", ChangeKind::Added),
        file(b"a\xfe", ChangeKind::Added),
        file(b"b", ChangeKind::Added),
    ]));
    let keys: Vec<&str> = rows.iter().map(|(key, _)| key.as_str()).collect();
    assert_eq!(keys, ["a\u{fffd}", "b"]);
    assert_eq!(totals.files, 3, "the totals count every file");
}
