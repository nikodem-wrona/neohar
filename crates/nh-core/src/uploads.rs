//! The names and the types of the files that a client puts in the blob store: a file that the
//! person drops on a pane.

use std::path::Path;

/// The largest file that a client may put in the blob store.
pub(crate) const MAX_BYTES: u64 = 20 * 1024 * 1024;

/// The longest name, in characters.
const MAX_NAME: usize = 100;

/// The name of a file, safe to write in a directory: the last part of a path, without control
/// characters and without dots in front. An empty name is `file`.
pub(crate) fn clean_name(name: &str) -> String {
    let last = name.rsplit(['/', '\\']).next().unwrap_or_default();
    let kept: String = last.chars().filter(|c| !c.is_control()).collect();
    let trimmed = kept.trim().trim_start_matches('.').trim();
    let short: String = trimmed.chars().take(MAX_NAME).collect();
    if short.is_empty() {
        "file".to_owned()
    } else {
        short
    }
}

/// The type of a file, from the extension of its name.
pub(crate) fn mime_of(name: &str) -> &'static str {
    let extension = Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    match extension.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "heic" => "image/heic",
        "svg" => "image/svg+xml",
        "pdf" => "application/pdf",
        "md" | "markdown" => "text/markdown",
        "txt" | "log" => "text/plain",
        "json" => "application/json",
        "csv" => "text/csv",
        "html" | "htm" => "text/html",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_keeps_its_last_part_and_loses_what_is_not_safe() {
        assert_eq!(clean_name("/Users/ada/Desktop/Shot 1.png"), "Shot 1.png");
        assert_eq!(clean_name("C:\\work\\notes.md"), "notes.md");
        assert_eq!(clean_name("../../etc/passwd"), "passwd");
        assert_eq!(clean_name(".env"), "env");
        assert_eq!(clean_name("a\u{0}b\nc.txt"), "abc.txt");
        assert_eq!(clean_name(".."), "file");
        assert_eq!(clean_name(""), "file");
        assert_eq!(clean_name(&"x".repeat(300)).chars().count(), MAX_NAME);
    }

    #[test]
    fn the_type_comes_from_the_extension() {
        assert_eq!(mime_of("Shot.PNG"), "image/png");
        assert_eq!(mime_of("plan.md"), "text/markdown");
        assert_eq!(mime_of("photo.jpeg"), "image/jpeg");
        assert_eq!(mime_of("Makefile"), "application/octet-stream");
    }
}
