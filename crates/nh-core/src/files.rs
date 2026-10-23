//! `fs.list`, `fs.read`, `fs.open` and `fs.copy_path`: the Files tab of the drawer.
//!
//! The webview reads no file system and formats nothing, so the core sends what the tab shows. A
//! listing has the folders first, then the other entries, each group by name without case. A
//! preview has one line of highlighted text for each line of the file, the HTML of a Markdown
//! file, or an image as a `data:` URL, and the text of its head. Each limit sets `truncated`.
//! An entry and a preview also have the name of their icon.
//!
//! The file work blocks, so it runs on the blocking pool and never on the state actor. Open and
//! reveal go to the host of the window, and only the host in the process can do them: the path is
//! a path of this machine.

use std::{
    io::Read,
    path::{Component, Path, PathBuf},
    sync::Arc,
};

use base64::Engine;
use nh_proto::api::{
    calls::{
        pane::CopyResult,
        workspace::{FsEntry, FsEntryKind, FsListResult, FsPreviewKind, FsReadResult},
    },
    common::Empty,
    methods::{Call, Method},
};
use serde::Serialize;
use serde_json::{Value, to_value};

use crate::{
    ErrorCode,
    actor::{Origin, Scope},
    error::RpcError,
    host::{Host, OpenRefused, PathOpen},
    runtime::Inner,
};

/// The most entries that one listing has.
pub(crate) const MAX_ENTRIES: usize = 2_000;
/// The most bytes of a file that a preview reads.
pub(crate) const MAX_TEXT: usize = 1024 * 1024;
/// The most lines that a preview has.
pub(crate) const MAX_LINES: usize = 10_000;
/// A Markdown file up to this size also comes as HTML.
pub(crate) const MAX_MARKDOWN: usize = 256 * 1024;
/// An image up to this size comes as a `data:` URL.
pub(crate) const MAX_IMAGE: u64 = 8 * 1024 * 1024;
/// A NUL byte in this many first bytes makes a file binary.
const SNIFF: usize = 8 * 1024;
/// The names that a listing leaves out.
const LEFT_OUT: &[&str] = &[".git", ".DS_Store"];

mod icons;

/// Whether this module runs the method.
pub(crate) fn handles(method: &str) -> bool {
    matches!(
        Method::from_name(method),
        Some(Method::FsList | Method::FsRead | Method::FsOpen | Method::FsCopyPath)
    )
}

fn json_of<T: Serialize>(value: &T) -> Result<Value, RpcError> {
    to_value(value).map_err(|e| RpcError::internal(e.to_string()))
}

/// Runs a call that [`handles`] accepted.
pub(crate) async fn run(inner: &Arc<Inner>, call: Call, origin: Origin) -> Result<Value, RpcError> {
    match call {
        Call::FsList(p) => {
            let dir = absolute(&p.path)?;
            json_of(&blocking(inner, move || list(&dir)).await?)
        }
        Call::FsRead(p) => {
            let path = absolute(&p.path)?;
            json_of(&blocking(inner, move || read(&path)).await?)
        }
        Call::FsCopyPath(p) => {
            let path = absolute(&p.path)?;
            let text = match p.relative_to {
                Some(base) => relative(&path, &absolute(&base)?)?,
                None => text_of(&path)?,
            };
            // A phone writes its own clipboard in the tap.
            if p.return_text {
                return json_of(&CopyResult { text: Some(text) });
            }
            if origin.scope == Scope::Phone || !inner.host.clipboard_write_for(origin.device, text)
            {
                return Err(RpcError::new(
                    ErrorCode::Unsupported,
                    "This client has no clipboard that the core can write.",
                ));
            }
            json_of(&CopyResult { text: None })
        }
        Call::FsOpen(p) => {
            inner.render.require_window(p.window)?;
            crate::window_calls::check_owner(inner, &origin, p.window)?;
            let path = absolute(&p.path)?;
            let shown = text_of(&path)?;
            let there = path.clone();
            if !blocking(inner, move || Ok(there.symlink_metadata().is_ok())).await? {
                return Err(RpcError::new(
                    ErrorCode::NotFound,
                    format!("{shown} is gone"),
                ));
            }
            let editor = inner.config.config().ui.editor.trim().to_owned();
            let open = PathOpen {
                window: p.window,
                path,
                reveal: p.reveal,
                with: (!p.reveal && !editor.is_empty()).then_some(editor),
            };
            match inner.host.open_path(open) {
                Ok(()) => json_of(&Empty {}),
                Err(OpenRefused::Unsupported) => Err(RpcError::new(
                    ErrorCode::Unsupported,
                    "Only a window of the machine of the file can open it.",
                )),
                Err(OpenRefused::Failed(message)) => Err(RpcError::internal(format!(
                    "the system did not open {shown}: {message}"
                ))),
            }
        }
        other => Err(RpcError::internal(format!("not a call of fs: {other:?}"))),
    }
}

/// Runs file work on the blocking pool.
async fn blocking<T: Send + 'static>(
    inner: &Inner,
    work: impl FnOnce() -> Result<T, RpcError> + Send + 'static,
) -> Result<T, RpcError> {
    inner
        .runtime
        .spawn_blocking(work)
        .await
        .map_err(|e| RpcError::internal(format!("the file task failed: {e}")))?
}

/// A path without `.` parts and with its `..` parts applied, so that `/a/b/..` is `/a`.
fn normal(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    if out.as_os_str().is_empty() {
        PathBuf::from("/")
    } else {
        out
    }
}

/// An absolute path with its `.` and `..` parts applied. Any other path is `invalid_params`.
fn absolute(text: &str) -> Result<PathBuf, RpcError> {
    let path = Path::new(text);
    if !path.is_absolute() {
        return Err(RpcError::invalid_params(format!(
            "{text} is not an absolute path"
        )));
    }
    Ok(normal(path))
}

fn text_of(path: &Path) -> Result<String, RpcError> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| RpcError::invalid_params("the path is not UTF-8"))
}

/// The error of a file operation, by its kind.
fn io_error(shown: &str, error: &std::io::Error) -> RpcError {
    let code = if error.kind() == std::io::ErrorKind::PermissionDenied {
        ErrorCode::PermissionDenied
    } else {
        ErrorCode::NotFound
    };
    RpcError::new(code, format!("cannot read {shown}: {error}"))
}

/// The entries of `dir` in the order of the tree. This function does blocking file work.
pub(crate) fn list(dir: &Path) -> Result<FsListResult, RpcError> {
    let shown = text_of(dir)?;
    if !std::fs::metadata(dir).is_ok_and(|meta| meta.is_dir()) {
        return Err(RpcError::new(
            ErrorCode::NotFound,
            format!("{shown} is not a folder"),
        ));
    }
    let read = std::fs::read_dir(dir).map_err(|error| io_error(&shown, &error))?;
    let mut entries: Vec<FsEntry> = Vec::new();
    for item in read.flatten() {
        // A name that is not UTF-8 cannot travel as a path of the API.
        let Ok(name) = item.file_name().into_string() else {
            continue;
        };
        if LEFT_OUT.contains(&name.as_str()) {
            continue;
        }
        let path = item.path();
        let Some(text) = path.to_str().map(str::to_owned) else {
            continue;
        };
        let (kind, link) = match item.file_type() {
            Ok(kind) if kind.is_symlink() => (kind_of(std::fs::metadata(&path).ok()), true),
            Ok(kind) if kind.is_dir() => (FsEntryKind::Dir, false),
            Ok(kind) if kind.is_file() => (FsEntryKind::File, false),
            _ => (FsEntryKind::Other, false),
        };
        entries.push(FsEntry {
            hidden: name.starts_with('.'),
            icon: icon_of(&name, kind).to_owned(),
            name,
            path: text,
            kind,
            link,
        });
    }
    // A folder can hold many entries, so each key is made once.
    entries.sort_by_cached_key(|entry| {
        (
            entry.kind != FsEntryKind::Dir,
            entry.name.to_lowercase(),
            entry.name.clone(),
        )
    });
    let truncated = entries.len() > MAX_ENTRIES;
    entries.truncate(MAX_ENTRIES);
    Ok(FsListResult {
        path: shown,
        entries,
        truncated,
    })
}

/// The kind of the target of a symlink. A broken link is `other`.
fn kind_of(target: Option<std::fs::Metadata>) -> FsEntryKind {
    match target {
        Some(meta) if meta.is_dir() => FsEntryKind::Dir,
        Some(meta) if meta.is_file() => FsEntryKind::File,
        _ => FsEntryKind::Other,
    }
}

/// The icon of an entry in the Material Icon Theme. The theme compares names without case.
/// A folder has the icon of its name. A file has the icon of its whole name, else of its longest
/// extension: `a.test.ts` tries `test.ts`, then `ts`, and `.env` tries `env`.
pub(crate) fn icon_of(name: &str, kind: FsEntryKind) -> &'static str {
    let lower = name.to_lowercase();
    let find = |table: &[(&str, &'static str)], key: &str| {
        table
            .binary_search_by(|(name, _)| (*name).cmp(key))
            .ok()
            .map(|at| table[at].1)
    };
    if kind == FsEntryKind::Dir {
        return find(icons::FOLDER_NAMES, &lower).unwrap_or("folder");
    }
    find(icons::FILE_NAMES, &lower)
        .or_else(|| {
            lower
                .match_indices('.')
                .find_map(|(at, _)| find(icons::FILE_EXTENSIONS, &lower[at + 1..]))
        })
        .unwrap_or("file")
}

/// The type of an image by the extension of its name, for the ones that a page can show.
fn image_type(name: &str) -> Option<&'static str> {
    let extension = name.rsplit_once('.')?.1.to_ascii_lowercase();
    Some(match extension.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "ico" => "image/x-icon",
        "svg" => "image/svg+xml",
        _ => return None,
    })
}

fn is_markdown(name: &str) -> bool {
    name.rsplit_once('.').is_some_and(|(_, extension)| {
        extension.eq_ignore_ascii_case("md") || extension.eq_ignore_ascii_case("markdown")
    })
}

/// A size for a person: bytes, then KB, MB and GB of 1,024.
pub(crate) fn size_text(bytes: u64) -> String {
    const UNITS: [&str; 3] = ["KB", "MB", "GB"];
    if bytes < 1024 {
        return if bytes == 1 {
            "1 byte".to_owned()
        } else {
            format!("{bytes} bytes")
        };
    }
    let mut value = bytes as f64 / 1024.0;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}

/// A number with a comma between each three digits.
fn count_text(count: usize) -> String {
    let digits = count.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

/// The preview of the regular file at `path`. This function does blocking file work.
pub(crate) fn read(path: &Path) -> Result<FsReadResult, RpcError> {
    let shown = text_of(path)?;
    let meta = std::fs::metadata(path).map_err(|error| io_error(&shown, &error))?;
    if !meta.is_file() {
        return Err(RpcError::new(
            ErrorCode::NotFound,
            format!("{shown} is not a file"),
        ));
    }
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(&shown)
        .to_owned();
    let size = meta.len();
    let preview = |kind, info: String| FsReadResult {
        path: shown.clone(),
        name: name.clone(),
        icon: icon_of(&name, FsEntryKind::File).to_owned(),
        kind,
        info,
        lines: Vec::new(),
        markdown: None,
        image: None,
        truncated: false,
    };

    if let Some(mime) = image_type(&name) {
        if size > MAX_IMAGE {
            return Ok(preview(
                FsPreviewKind::Binary,
                format!(
                    "Image · {} · too large to show (over {})",
                    size_text(size),
                    size_text(MAX_IMAGE)
                ),
            ));
        }
        let bytes = std::fs::read(path).map_err(|error| io_error(&shown, &error))?;
        let mut out = preview(FsPreviewKind::Image, format!("Image · {}", size_text(size)));
        out.image = Some(format!(
            "data:{mime};base64,{}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        ));
        return Ok(out);
    }

    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .and_then(|file| {
            file.take(MAX_TEXT as u64 + 1)
                .read_to_end(&mut bytes)
                .map(|_| ())
        })
        .map_err(|error| io_error(&shown, &error))?;
    if bytes.is_empty() {
        return Ok(preview(FsPreviewKind::Text, "Empty".to_owned()));
    }
    if bytes[..bytes.len().min(SNIFF)].contains(&0) {
        return Ok(preview(
            FsPreviewKind::Binary,
            format!("Binary · {}", size_text(size)),
        ));
    }
    let mut truncated = bytes.len() > MAX_TEXT;
    if truncated {
        // The last line stays whole: the cut goes back to the last line end.
        bytes.truncate(MAX_TEXT);
        if let Some(end) = bytes.iter().rposition(|&byte| byte == b'\n') {
            bytes.truncate(end + 1);
        }
    }
    let text = String::from_utf8_lossy(&bytes);
    let mut raw: Vec<&str> = text.split_inclusive('\n').collect();
    if raw.len() > MAX_LINES {
        raw.truncate(MAX_LINES);
        truncated = true;
    }
    let language = nh_rich::highlight::language_of_path(&name);
    let mut highlighter = language.map(nh_rich::highlight::LineHighlighter::new);
    let mut consumed = 0usize;
    let lines = raw
        .iter()
        .map(|raw| {
            let line = nh_rich::highlight::without_line_end(raw);
            consumed += raw.len();
            let spans = match highlighter.as_mut() {
                Some(highlighter) if consumed <= nh_rich::highlight::HIGHLIGHT_CAP => {
                    highlighter.line(line)
                }
                _ => Vec::new(),
            };
            crate::rich_calls::spans(&nh_rich::Spans {
                text: line.to_owned(),
                spans,
            })
        })
        .collect::<Vec<_>>();
    let info = if truncated {
        format!(
            "{} · the first {} lines",
            size_text(size),
            count_text(lines.len())
        )
    } else if lines.len() == 1 {
        format!("1 line · {}", size_text(size))
    } else {
        format!("{} lines · {}", count_text(lines.len()), size_text(size))
    };
    let markdown = (is_markdown(&name) && !truncated && bytes.len() <= MAX_MARKDOWN)
        .then(|| crate::rich_calls::markdown(&nh_rich::markdown(&text)));
    Ok(FsReadResult {
        lines,
        markdown,
        truncated,
        ..preview(FsPreviewKind::Text, info)
    })
}

/// The path from `base` to `path`, for "Copy relative path". `base` itself is `.`.
pub(crate) fn relative(path: &Path, base: &Path) -> Result<String, RpcError> {
    let rest = path.strip_prefix(base).map_err(|_| {
        RpcError::invalid_params(format!("{} is not in {}", path.display(), base.display()))
    })?;
    if rest.as_os_str().is_empty() {
        return Ok(".".to_owned());
    }
    text_of(rest)
}

#[cfg(test)]
mod tests;
