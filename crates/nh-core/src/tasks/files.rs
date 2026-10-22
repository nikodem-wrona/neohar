//! The files of a task: screenshots, markdown files and other files that the person gives
//! with the task.
//!
//! The core keeps the bytes as blobs, so a draft keeps its files when the originals move. The
//! start copies them into `<worktree>/.neohar/attachments/`. The directory `.neohar` has a
//! `.gitignore` with `*`, so git ignores the files and the diff of the task does not count them.
//! The prompt of the agent ends with the list of their paths.

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use nh_proto::api::calls::agent::Upload;

use crate::error::RpcError;

/// The largest file that a task takes.
pub(crate) const MAX_BYTES: u64 = 20 * 1024 * 1024;

/// The most files that a task has.
pub(crate) const MAX_FILES: usize = 20;

/// The longest name, in characters.
const MAX_NAME: usize = 100;

/// The directory in the worktree that holds what neohar puts there.
pub(crate) const DIR: &str = ".neohar";

/// A file that came with a call, before the store keeps it.
#[derive(Debug)]
pub(crate) struct Incoming {
    pub name: String,
    pub bytes: Vec<u8>,
}

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

fn too_large(name: &str) -> RpcError {
    RpcError::invalid_params(format!(
        "the file {name} is larger than {} MiB",
        MAX_BYTES / 1024 / 1024
    ))
}

/// Reads the files of a call: the paths, then the uploads. A path must be absolute and name a
/// file. A file over [`MAX_BYTES`] is an error. The reads run on a thread for blocking work.
pub(crate) async fn read(
    paths: &[String],
    uploads: Vec<Upload>,
    home: Option<&Path>,
) -> Result<Vec<Incoming>, RpcError> {
    let (paths, home) = (paths.to_vec(), home.map(Path::to_path_buf));
    tokio::task::spawn_blocking(move || read_now(&paths, uploads, home.as_deref()))
        .await
        .map_err(|e| RpcError::internal(e.to_string()))?
}

fn read_now(
    paths: &[String],
    uploads: Vec<Upload>,
    home: Option<&Path>,
) -> Result<Vec<Incoming>, RpcError> {
    let mut out = Vec::with_capacity(paths.len() + uploads.len());
    for text in paths {
        let path = match (text.strip_prefix("~/"), home) {
            (Some(rest), Some(home)) => home.join(rest),
            _ => PathBuf::from(text),
        };
        if !path.is_absolute() {
            return Err(RpcError::invalid_params(format!(
                "the path {text} is not absolute"
            )));
        }
        let meta = std::fs::metadata(&path)
            .map_err(|e| RpcError::invalid_params(format!("{}: {e}", path.display())))?;
        if !meta.is_file() {
            return Err(RpcError::invalid_params(format!(
                "{} is not a file",
                path.display()
            )));
        }
        let name = clean_name(&path.display().to_string());
        if meta.len() > MAX_BYTES {
            return Err(too_large(&name));
        }
        let bytes = std::fs::read(&path)
            .map_err(|e| RpcError::invalid_params(format!("{}: {e}", path.display())))?;
        out.push(Incoming { name, bytes });
    }
    for upload in uploads {
        let name = clean_name(&upload.name);
        if upload.data.0.len() as u64 > MAX_BYTES {
            return Err(too_large(&name));
        }
        out.push(Incoming {
            name,
            bytes: upload.data.0,
        });
    }
    Ok(out)
}

/// The names that the files get in the directory: the same name twice gets `-2`, `-3` before the
/// extension.
pub(crate) fn unique_names<'a>(names: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut taken: HashSet<String> = HashSet::new();
    let mut out = Vec::new();
    for name in names {
        let path = Path::new(name);
        let (stem, extension) = match (path.file_stem(), path.extension()) {
            (Some(stem), Some(extension)) => (
                stem.to_string_lossy().into_owned(),
                format!(".{}", extension.to_string_lossy()),
            ),
            _ => (name.to_owned(), String::new()),
        };
        let mut candidate = name.to_owned();
        let mut n = 2;
        while !taken.insert(candidate.to_ascii_lowercase()) {
            candidate = format!("{stem}-{n}{extension}");
            n += 1;
        }
        out.push(candidate);
    }
    out
}

/// Writes the files into the worktree and returns their paths, in order. The directory
/// `.neohar` gets a `.gitignore` that ignores everything in it.
pub(crate) async fn write_all(
    worktree: &Path,
    files: Vec<(String, Vec<u8>)>,
) -> std::io::Result<Vec<PathBuf>> {
    let worktree = worktree.to_path_buf();
    tokio::task::spawn_blocking(move || write_now(&worktree, &files))
        .await
        .map_err(std::io::Error::other)?
}

fn write_now(worktree: &Path, files: &[(String, Vec<u8>)]) -> std::io::Result<Vec<PathBuf>> {
    let root = worktree.join(DIR);
    let dir = root.join("attachments");
    std::fs::create_dir_all(&dir)?;
    std::fs::write(root.join(".gitignore"), "*\n")?;
    let names = unique_names(files.iter().map(|(name, _)| name.as_str()));
    let mut paths = Vec::with_capacity(files.len());
    for (name, (_, bytes)) in names.iter().zip(files) {
        let path = dir.join(name);
        std::fs::write(&path, bytes)?;
        paths.push(path);
    }
    Ok(paths)
}

/// The first prompt of the agent: the prompt of the task, then the list of its files. `None`
/// when the task has neither.
pub(crate) fn prompt_with(prompt: Option<&str>, paths: &[PathBuf]) -> Option<String> {
    let prompt = prompt.map(str::trim).filter(|text| !text.is_empty());
    if paths.is_empty() {
        return prompt.map(str::to_owned);
    }
    let list: Vec<String> = paths
        .iter()
        .map(|path| format!("- {}", path.display()))
        .collect();
    let files = format!("Files of this task:\n{}", list.join("\n"));
    Some(match prompt {
        Some(prompt) => format!("{prompt}\n\n{files}"),
        None => files,
    })
}

#[cfg(test)]
mod tests {
    use nh_proto::api::common::Bytes;

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

    #[test]
    fn the_same_name_twice_gets_a_number() {
        assert_eq!(
            unique_names(["a.png", "b.md", "A.png", "a.png", "notes"]),
            ["a.png", "b.md", "A-2.png", "a-3.png", "notes"]
        );
        assert_eq!(unique_names(["notes", "notes"]), ["notes", "notes-2"]);
    }

    #[test]
    fn the_prompt_ends_with_the_paths_of_the_files() {
        let paths = [
            PathBuf::from("/wt/.neohar/attachments/login.png"),
            PathBuf::from("/wt/.neohar/attachments/plan.md"),
        ];
        assert_eq!(
            prompt_with(Some("Fix the login page.\n"), &paths).as_deref(),
            Some(
                "Fix the login page.\n\nFiles of this task:\n- /wt/.neohar/attachments/login.png\n- /wt/.neohar/attachments/plan.md"
            )
        );
        assert_eq!(
            prompt_with(None, &paths[..1]).as_deref(),
            Some("Files of this task:\n- /wt/.neohar/attachments/login.png")
        );
        assert_eq!(prompt_with(Some("  "), &[]), None);
        assert_eq!(prompt_with(Some("go"), &[]).as_deref(), Some("go"));
    }

    #[tokio::test]
    async fn files_are_read_from_paths_and_uploads_and_written_where_git_ignores_them() {
        let dir = std::env::temp_dir().join(format!("nh-files-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a directory");
        let source = dir.join("plan.md");
        std::fs::write(&source, "# Plan\n").expect("a file");
        let read = read(
            &[source.display().to_string()],
            vec![Upload {
                name: "Pasted.png".to_owned(),
                data: Bytes(vec![1, 2, 3]),
            }],
            None,
        )
        .await
        .expect("the files");
        assert_eq!(
            read.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(),
            ["plan.md", "Pasted.png"]
        );
        let refused = super::read(&["plan.md".to_owned()], Vec::new(), None).await;
        assert!(refused.is_err(), "a relative path");
        let missing =
            super::read(&[dir.join("none").display().to_string()], Vec::new(), None).await;
        assert!(missing.is_err(), "a missing file");

        let worktree = dir.join("wt");
        std::fs::create_dir_all(&worktree).expect("a worktree");
        let files: Vec<(String, Vec<u8>)> = read.into_iter().map(|f| (f.name, f.bytes)).collect();
        let paths = write_all(&worktree, files).await.expect("written");
        assert_eq!(
            paths,
            [
                worktree.join(".neohar/attachments/plan.md"),
                worktree.join(".neohar/attachments/Pasted.png")
            ]
        );
        assert_eq!(std::fs::read(&paths[1]).expect("bytes"), [1, 2, 3]);
        assert_eq!(
            std::fs::read_to_string(worktree.join(".neohar/.gitignore")).expect("ignore"),
            "*\n"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
