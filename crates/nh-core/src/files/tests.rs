use super::*;
use crate::ErrorCode;

/// A tree in the temporary directory: folders and files of mixed case, hidden entries, `.git`,
/// `.DS_Store`, a symlink to a folder, a symlink to a file, a broken link and a socket.
fn tree(name: &str) -> PathBuf {
    let base = std::fs::canonicalize(std::env::temp_dir())
        .expect("a path")
        .join(format!("nh-files-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    for folder in ["src", "Docs", ".github", ".git/objects", "target"] {
        std::fs::create_dir_all(base.join(folder)).expect("a folder");
    }
    for file in ["b.txt", "A.md", ".gitignore", ".DS_Store", "zeta.rs"] {
        std::fs::write(base.join(file), "text\n").expect("a file");
    }
    std::os::unix::fs::symlink(base.join("src"), base.join("link-dir")).expect("a link");
    std::os::unix::fs::symlink(base.join("b.txt"), base.join("link-file")).expect("a link");
    std::os::unix::fs::symlink(base.join("gone"), base.join("broken")).expect("a link");
    base
}

fn names(result: &FsListResult) -> Vec<&str> {
    result.entries.iter().map(|e| e.name.as_str()).collect()
}

fn entry<'a>(result: &'a FsListResult, name: &str) -> &'a FsEntry {
    result
        .entries
        .iter()
        .find(|e| e.name == name)
        .expect("the entry")
}

#[test]
fn folders_come_first_then_the_rest_each_by_name_without_case() {
    let base = tree("order");
    let result = list(&base).expect("a listing");
    assert_eq!(
        names(&result),
        [
            ".github",
            "Docs",
            "link-dir",
            "src",
            "target",
            ".gitignore",
            "A.md",
            "b.txt",
            "broken",
            "link-file",
            "zeta.rs"
        ]
    );
    assert_eq!(result.path, base.display().to_string());
    assert!(!result.truncated);
    assert_eq!(
        entry(&result, "src").path,
        base.join("src").display().to_string()
    );
}

#[test]
fn git_and_ds_store_are_left_out_and_hidden_entries_are_marked() {
    let base = tree("hidden");
    let result = list(&base).expect("a listing");
    assert!(!names(&result).contains(&".git"));
    assert!(!names(&result).contains(&".DS_Store"));
    assert!(entry(&result, ".github").hidden);
    assert!(entry(&result, ".gitignore").hidden);
    assert!(!entry(&result, "src").hidden);
}

#[test]
fn a_link_has_the_kind_of_its_target_and_a_broken_link_is_other() {
    let base = tree("links");
    let result = list(&base).expect("a listing");
    let kind = |name| {
        let e = entry(&result, name);
        (e.kind, e.link)
    };
    assert_eq!(kind("link-dir"), (FsEntryKind::Dir, true));
    assert_eq!(kind("link-file"), (FsEntryKind::File, true));
    assert_eq!(kind("broken"), (FsEntryKind::Other, true));
    assert_eq!(kind("src"), (FsEntryKind::Dir, false));
    assert_eq!(kind("b.txt"), (FsEntryKind::File, false));
}

#[test]
fn a_socket_is_other() {
    let base = tree("socket");
    let _socket = std::os::unix::net::UnixListener::bind(base.join("s.sock")).expect("a socket");
    let result = list(&base).expect("a listing");
    assert_eq!(entry(&result, "s.sock").kind, FsEntryKind::Other);
    let error = read(&base.join("s.sock")).expect_err("not a file");
    assert_eq!(error.code, ErrorCode::NotFound);
}

#[test]
fn an_entry_has_the_icon_of_its_name() {
    let base = tree("icons");
    let result = list(&base).expect("a listing");
    let icon = |name| entry(&result, name).icon.as_str();
    assert_eq!(icon("src"), "folder-src");
    assert_eq!(icon("Docs"), "folder-docs");
    assert_eq!(icon(".github"), "folder-github");
    assert_eq!(icon("link-dir"), "folder");
    assert_eq!(icon("zeta.rs"), "rust");
    assert_eq!(icon("A.md"), "markdown");
    assert_eq!(icon(".gitignore"), "git");
    assert_eq!(icon("link-file"), "file");
    assert_eq!(icon("broken"), "file");
}

#[test]
fn a_whole_name_wins_over_an_extension_and_a_longer_extension_over_a_shorter_one() {
    use FsEntryKind::{Dir, File, Other};
    for (name, kind, icon) in [
        ("package.json", File, "nodejs"),
        ("data.json", File, "json"),
        ("Dockerfile", File, "docker"),
        ("README.md", File, "readme"),
        ("model.test.ts", File, "test-ts"),
        ("index.d.ts", File, "typescript-def"),
        ("model.ts", File, "typescript"),
        ("App.TSX", File, "react_ts"),
        ("up.sql", File, "database"),
        (".env", File, "tune"),
        (".env.local", File, "tune"),
        ("notes", File, "file"),
        ("archive.unknownext", File, "file"),
        ("trailing.", File, "file"),
        ("s.sock", Other, "file"),
        ("node_modules", Dir, "folder-node"),
        ("SRC", Dir, "folder-src"),
        ("lib.rs", Dir, "folder"),
    ] {
        assert_eq!(icon_of(name, kind), icon, "{name}");
    }
}

#[test]
fn the_tables_of_icons_are_in_lower_case_and_sorted_by_bytes() {
    for table in [
        icons::FILE_NAMES,
        icons::FILE_EXTENSIONS,
        icons::FOLDER_NAMES,
    ] {
        assert!(table.iter().all(|(name, _)| *name == name.to_lowercase()));
        assert!(table.windows(2).all(|pair| pair[0].0 < pair[1].0));
    }
}

#[test]
fn more_than_the_limit_is_cut_after_the_order() {
    let base = tree("limit");
    let many = base.join("many");
    std::fs::create_dir_all(many.join("folder")).expect("a folder");
    for index in 0..MAX_ENTRIES + 5 {
        std::fs::write(many.join(format!("f{index:05}")), "").expect("a file");
    }
    let result = list(&many).expect("a listing");
    assert_eq!(result.entries.len(), MAX_ENTRIES);
    assert!(result.truncated);
    // The folder is first, also when the files are many.
    assert_eq!(result.entries[0].name, "folder");
    assert_eq!(result.entries[1].name, "f00000");
}

#[test]
fn a_file_or_a_missing_path_is_not_a_folder() {
    let base = tree("notfound");
    for path in [base.join("b.txt"), base.join("gone"), base.join("broken")] {
        let error = list(&path).expect_err("not a folder");
        assert_eq!(error.code, ErrorCode::NotFound, "{path:?}");
    }
}

#[test]
fn a_path_must_be_absolute_and_dots_are_applied() {
    let error = absolute("src/lib.rs").expect_err("relative");
    assert_eq!(error.code, ErrorCode::InvalidParams);
    assert_eq!(
        absolute("/a/b/../c/./d").expect("absolute"),
        PathBuf::from("/a/c/d")
    );
}

fn write(base: &Path, name: &str, bytes: &[u8]) -> PathBuf {
    let path = base.join(name);
    std::fs::write(&path, bytes).expect("a file");
    path
}

#[test]
fn a_text_file_has_one_highlighted_line_for_each_line() {
    let base = tree("text");
    let path = write(&base, "main.rs", b"fn main() {\r\n    let x = 1;\n}\n");
    let result = read(&path).expect("a preview");
    assert_eq!(result.kind, FsPreviewKind::Text);
    assert_eq!(result.name, "main.rs");
    assert_eq!(result.icon, "rust");
    assert_eq!(result.path, path.display().to_string());
    let texts: Vec<&str> = result.lines.iter().map(|l| l.text.as_str()).collect();
    assert_eq!(texts, ["fn main() {", "    let x = 1;", "}"]);
    // `fn` is a keyword, from the start of its line.
    assert!(
        result.lines[0]
            .spans
            .iter()
            .any(|(start, end, class)| (*start, *end, class.as_str()) == (0, 2, "kw")),
        "{:?}",
        result.lines[0].spans
    );
    assert_eq!(result.info, "3 lines · 30 bytes");
    assert!(!result.truncated);
    assert!(result.markdown.is_none());
    assert!(result.image.is_none());
}

#[test]
fn a_file_without_a_known_language_is_plain_and_a_last_line_without_an_end_counts() {
    let base = tree("plain");
    let path = write(&base, "notes.unknownext", b"one\ntwo");
    let result = read(&path).expect("a preview");
    let texts: Vec<&str> = result.lines.iter().map(|l| l.text.as_str()).collect();
    assert_eq!(texts, ["one", "two"]);
    assert!(result.lines.iter().all(|l| l.spans.is_empty()));
    let one = write(&base, "one.txt", b"x");
    assert_eq!(read(&one).expect("a preview").info, "1 line · 1 byte");
}

#[test]
fn markdown_comes_as_html_too() {
    let base = tree("markdown");
    let path = write(&base, "README.md", b"# Title\n\nSome *text*.\n");
    let result = read(&path).expect("a preview");
    assert_eq!(result.kind, FsPreviewKind::Text);
    let html = result.markdown.expect("html").html;
    assert!(
        html.contains("<h1>") && html.contains("<em>text</em>"),
        "{html}"
    );
    assert_eq!(result.lines.len(), 3);
}

#[test]
fn too_many_lines_are_cut_and_say_so() {
    let base = tree("lines");
    let text = "line\n".repeat(MAX_LINES + 10);
    let path = write(&base, "long.txt", text.as_bytes());
    let result = read(&path).expect("a preview");
    assert_eq!(result.lines.len(), MAX_LINES);
    assert!(result.truncated);
    assert_eq!(result.info, "48.9 KB · the first 10,000 lines");
    let exact = write(&base, "exact.txt", "line\n".repeat(MAX_LINES).as_bytes());
    assert!(!read(&exact).expect("a preview").truncated);
}

#[test]
fn too_many_bytes_are_cut_at_a_line_end() {
    let base = tree("bytes");
    // Lines of 1,000 bytes: 1 MiB holds 1,048 whole lines and a part of one more.
    let line = format!("{}\n", "x".repeat(999));
    let path = write(&base, "big.md", line.repeat(1_100).as_bytes());
    let result = read(&path).expect("a preview");
    assert!(result.truncated);
    assert_eq!(result.lines.len(), MAX_TEXT / 1000);
    assert!(result.lines.iter().all(|l| l.text.len() == 999));
    // A cut file is not rendered as Markdown.
    assert!(result.markdown.is_none());
}

#[test]
fn an_empty_file_and_a_binary_file_have_no_lines() {
    let base = tree("binary");
    let empty = read(&write(&base, "empty.txt", b"")).expect("a preview");
    assert_eq!(
        (empty.kind, empty.info.as_str()),
        (FsPreviewKind::Text, "Empty")
    );
    assert!(empty.lines.is_empty());
    let binary = read(&write(&base, "data.bin", b"\x7fELF\x00\x01\x02")).expect("a preview");
    assert_eq!(binary.kind, FsPreviewKind::Binary);
    assert_eq!(binary.info, "Binary · 7 bytes");
    assert!(binary.lines.is_empty());
}

#[test]
fn bytes_that_are_not_utf8_show_as_replacement_characters() {
    let base = tree("latin");
    let result = read(&write(&base, "latin.txt", b"caf\xe9\n")).expect("a preview");
    assert_eq!(result.lines[0].text, "caf\u{fffd}");
}

#[test]
fn an_image_is_a_data_url_and_a_large_one_is_binary() {
    let base = tree("image");
    let png = write(&base, "logo.PNG", b"\x89PNG\r\n\x1a\nrest");
    let result = read(&png).expect("a preview");
    assert_eq!(result.kind, FsPreviewKind::Image);
    assert_eq!(result.info, "Image · 12 bytes");
    let url = result.image.expect("a url");
    assert_eq!(
        url,
        format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(b"\x89PNG\r\n\x1a\nrest")
        )
    );
    let large = base.join("large.jpg");
    let file = std::fs::File::create(&large).expect("a file");
    file.set_len(MAX_IMAGE + 1).expect("a size");
    let result = read(&large).expect("a preview");
    assert_eq!(result.kind, FsPreviewKind::Binary);
    assert!(result.image.is_none());
    assert_eq!(
        result.info,
        "Image · 8.0 MB · too large to show (over 8.0 MB)"
    );
}

#[test]
fn a_folder_or_a_missing_file_is_not_a_file() {
    let base = tree("notafile");
    for path in [base.join("src"), base.join("gone.txt"), base.join("broken")] {
        let error = read(&path).expect_err("not a file");
        assert_eq!(error.code, ErrorCode::NotFound, "{path:?}");
    }
    // A link to a file reads its target.
    let linked = read(&base.join("link-file")).expect("a preview");
    assert_eq!(linked.lines[0].text, "text");
}

#[test]
fn a_relative_path_comes_from_a_folder_that_holds_it() {
    let base = Path::new("/work/repo");
    assert_eq!(
        relative(Path::new("/work/repo/src/lib.rs"), base).expect("inside"),
        "src/lib.rs"
    );
    assert_eq!(relative(base, base).expect("the folder"), ".");
    let error = relative(Path::new("/work/other/a"), base).expect_err("outside");
    assert_eq!(error.code, ErrorCode::InvalidParams);
    // A name that only starts like the folder is not in it.
    assert!(relative(Path::new("/work/repository/a"), base).is_err());
}

#[test]
fn sizes_and_counts_read_well() {
    assert_eq!(size_text(0), "0 bytes");
    assert_eq!(size_text(1), "1 byte");
    assert_eq!(size_text(1023), "1023 bytes");
    assert_eq!(size_text(1024), "1.0 KB");
    assert_eq!(size_text(4300), "4.2 KB");
    assert_eq!(size_text(5 * 1024 * 1024), "5.0 MB");
    assert_eq!(size_text(3 * 1024 * 1024 * 1024), "3.0 GB");
    assert_eq!(count_text(7), "7");
    assert_eq!(count_text(1_000), "1,000");
    assert_eq!(count_text(1_234_567), "1,234,567");
}

#[test]
fn dots_in_a_path_are_applied() {
    assert_eq!(normal(Path::new("/a/b/../c/./d")), PathBuf::from("/a/c/d"));
    assert_eq!(normal(Path::new("/..")), PathBuf::from("/"));
}

#[test]
fn an_invisible_character_in_a_name_shows_as_an_escape_and_the_path_stays() {
    let base = std::fs::canonicalize(std::env::temp_dir())
        .expect("a path")
        .join(format!("nh-files-{}-invisible", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).expect("a folder");
    // `photodnammoc.png` when the override works: the file is a script.
    let name = "photo\u{202E}gnp.command";
    std::fs::write(base.join(name), "echo hi\n").expect("a file");
    let listed = list(&base).expect("a listing");
    let read_back = read(&base.join(name)).expect("a preview");
    std::fs::remove_dir_all(&base).expect("clean up");
    let shown = &listed.entries[0];
    assert_eq!(shown.name, "photo<U+202E>gnp.command");
    assert_eq!(shown.path, base.join(name).display().to_string());
    assert_eq!(read_back.name, "photo<U+202E>gnp.command");
}
