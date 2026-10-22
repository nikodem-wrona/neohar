//! `fs.dirs`: the folders in a folder of this machine.
//!
//! The folder browser of the task dialog calls it when the host has no folder panel, for example
//! in a window of another machine. The webview sorts and filters nothing, so the core
//! sends the entries in the order that the browser shows: the favorite folders first, in
//! the order of the projection `favorites`, then the other folders by name without case. A name
//! that starts with a dot is left out. A symlink to a folder counts as a folder, and a broken
//! link or a file does not. A folder with more than [`MAX_ENTRIES`] entries is cut, and the
//! result says `truncated`.
//!
//! Without a path, the listing is the home folder, and every favorite that still exists is in it,
//! also when it is not in the home folder: the person starts a task in a repository that is far
//! from home with one click.

use std::path::{Component, Path, PathBuf};

use nh_proto::api::{
    calls::workspace::{FsDirEntry, FsDirsResult},
    model::FavoriteValue,
};

use super::service::{TaskService, expand, gone};
use crate::{ErrorCode, error::RpcError};

/// The most entries that one answer has.
pub(crate) const MAX_ENTRIES: usize = 2_000;

/// A path without `.` parts and with its `..` parts applied, so that `/a/b/..` is `/a`.
pub(crate) fn normal(path: &Path) -> PathBuf {
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

/// Whether `path` is a folder, also through a symlink.
fn is_folder(path: &Path) -> bool {
    std::fs::metadata(path).is_ok_and(|meta| meta.is_dir())
}

/// The root of a git repository has `.git`: a folder, or a file in a linked worktree.
fn is_repo(path: &Path) -> bool {
    path.join(".git").exists()
}

fn text(path: &Path) -> Option<String> {
    path.to_str().map(str::to_owned)
}

/// The listing of `dir`. `all_favorites` puts every favorite that still exists in front, not only
/// the ones in `dir`. This function does blocking file work.
pub(crate) fn list(
    dir: &Path,
    favorites: &[FavoriteValue],
    all_favorites: bool,
) -> Result<FsDirsResult, RpcError> {
    let shown = text(dir).ok_or_else(|| RpcError::invalid_params("the path is not UTF-8"))?;
    if !is_folder(dir) {
        return Err(RpcError::new(
            ErrorCode::NotFound,
            format!("{shown} is not a folder"),
        ));
    }
    let read = std::fs::read_dir(dir).map_err(|error| {
        let code = if error.kind() == std::io::ErrorKind::PermissionDenied {
            ErrorCode::PermissionDenied
        } else {
            ErrorCode::NotFound
        };
        RpcError::new(code, format!("cannot read {shown}: {error}"))
    })?;
    let mut folders: Vec<(String, PathBuf)> = Vec::new();
    for item in read.flatten() {
        let Ok(name) = item.file_name().into_string() else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        let path = item.path();
        let is_dir = match item.file_type() {
            Ok(kind) if kind.is_dir() => true,
            Ok(kind) if kind.is_symlink() => is_folder(&path),
            _ => false,
        };
        if is_dir {
            folders.push((name, path));
        }
    }
    folders.sort_by(|(a, _), (b, _)| a.to_lowercase().cmp(&b.to_lowercase()).then(a.cmp(b)));

    // The favorites, in the order of the projection. A favorite in this folder keeps the name of
    // its folder. A favorite elsewhere (only in the home listing) has the name of the favorite.
    let mut entries: Vec<(String, PathBuf)> = Vec::new();
    for favorite in favorites {
        let path = PathBuf::from(&favorite.path);
        let inside = folders.iter().find(|(_, folder)| *folder == path);
        if let Some((name, _)) = inside {
            entries.push((name.clone(), path));
        } else if all_favorites && is_folder(&path) {
            entries.push((favorite.name.clone(), path));
        }
    }
    let is_favorite: Vec<PathBuf> = entries.iter().map(|(_, path)| path.clone()).collect();
    entries.extend(
        folders
            .into_iter()
            .filter(|(_, path)| !is_favorite.contains(path)),
    );
    let truncated = entries.len() > MAX_ENTRIES;
    entries.truncate(MAX_ENTRIES);

    let entries = entries
        .into_iter()
        .filter_map(|(name, path)| {
            let repo = is_repo(&path);
            text(&path).map(|path| FsDirEntry { name, path, repo })
        })
        .collect();
    Ok(FsDirsResult {
        parent: dir.parent().and_then(text),
        path: shown,
        entries,
        truncated,
    })
}

impl TaskService {
    /// The folders in `path`, or in the home folder of the machine without a path.
    pub(crate) async fn dirs(&self, path: Option<String>) -> Result<FsDirsResult, RpcError> {
        let home = expand("~", &self.env);
        let (dir, all_favorites) = match path {
            None => (home, true),
            Some(text) => {
                let dir = expand(&text, &self.env);
                if !dir.is_absolute() {
                    return Err(RpcError::invalid_params(format!(
                        "{text} is not an absolute path"
                    )));
                }
                (dir, false)
            }
        };
        let dir = normal(&dir);
        let favorites = self.favorites().await?;
        tokio::task::spawn_blocking(move || list(&dir, &favorites, all_favorites))
            .await
            .map_err(gone)?
    }
}

#[cfg(test)]
mod tests {
    use nh_proto::api::time::Timestamp;

    use super::*;

    /// A tree in the temporary directory: a repository, a linked worktree (`.git` is a file), a
    /// hidden folder, a symlink to a folder, a broken symlink and a file.
    fn tree(name: &str) -> PathBuf {
        let base = std::fs::canonicalize(std::env::temp_dir())
            .expect("a path")
            .join(format!("nh-dirs-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("Zeta/.git")).expect("a repository");
        std::fs::create_dir_all(base.join("alpha")).expect("a folder");
        std::fs::create_dir_all(base.join("Beta")).expect("a folder");
        std::fs::create_dir_all(base.join(".hidden")).expect("a hidden folder");
        std::fs::create_dir_all(base.join("tree")).expect("a folder");
        std::fs::write(base.join("tree/.git"), "gitdir: /elsewhere").expect("a git file");
        std::fs::write(base.join("file.txt"), "text").expect("a file");
        std::os::unix::fs::symlink(base.join("alpha"), base.join("link")).expect("a symlink");
        std::os::unix::fs::symlink(base.join("file.txt"), base.join("to-file")).expect("a link");
        std::os::unix::fs::symlink(base.join("gone"), base.join("broken")).expect("a link");
        base
    }

    fn favorite(path: &Path, name: &str) -> FavoriteValue {
        FavoriteValue {
            path: path.display().to_string(),
            name: name.to_owned(),
            used_at: Timestamp(1),
        }
    }

    fn names(result: &FsDirsResult) -> Vec<&str> {
        result.entries.iter().map(|e| e.name.as_str()).collect()
    }

    #[test]
    fn folders_are_listed_by_name_without_case_and_hidden_ones_and_files_are_left_out() {
        let base = tree("order");
        let result = list(&base, &[], false).expect("a listing");
        assert_eq!(names(&result), ["alpha", "Beta", "link", "tree", "Zeta"]);
        assert!(!result.truncated);
        assert_eq!(result.path, base.display().to_string());
        assert_eq!(
            result.parent,
            base.parent().map(|p| p.display().to_string())
        );
        let repo: Vec<(&str, bool)> = result
            .entries
            .iter()
            .map(|e| (e.name.as_str(), e.repo))
            .collect();
        // A folder with `.git` is a repository, also when `.git` is a file. A link to a plain
        // folder is not.
        assert_eq!(
            repo,
            [
                ("alpha", false),
                ("Beta", false),
                ("link", false),
                ("tree", true),
                ("Zeta", true)
            ]
        );
        assert_eq!(
            result.entries[2].path,
            base.join("link").display().to_string()
        );
    }

    #[test]
    fn favorites_come_first_in_the_order_given_and_a_missing_one_is_left_out() {
        let base = tree("favorites");
        let elsewhere = tree("elsewhere");
        let favorites = [
            favorite(&base.join("Zeta"), "Zeta"),
            favorite(&base.join("gone"), "gone"),
            favorite(&base.join("Beta"), "Beta"),
            favorite(&elsewhere.join("alpha"), "elsewhere/alpha"),
        ];
        let inside = list(&base, &favorites, false).expect("a listing");
        assert_eq!(names(&inside), ["Zeta", "Beta", "alpha", "link", "tree"]);
        // The home listing has all favorites that exist, once each, with the name of the favorite
        // when the folder is not in the listed folder.
        let home = list(&base, &favorites, true).expect("a listing");
        assert_eq!(
            names(&home),
            ["Zeta", "Beta", "elsewhere/alpha", "alpha", "link", "tree"]
        );
        assert_eq!(
            home.entries[2].path,
            elsewhere.join("alpha").display().to_string()
        );
    }

    #[test]
    fn a_folder_that_is_a_file_or_is_not_there_gives_not_found() {
        let base = tree("notfound");
        for path in [
            base.join("file.txt"),
            base.join("gone"),
            base.join("broken"),
        ] {
            let error = list(&path, &[], false).expect_err("not a folder");
            assert_eq!(error.code, ErrorCode::NotFound, "{path:?}");
        }
    }

    #[test]
    fn more_than_the_limit_is_cut_and_says_so() {
        let base = tree("limit");
        let many = base.join("many");
        std::fs::create_dir_all(&many).expect("a folder");
        for index in 0..MAX_ENTRIES + 5 {
            std::fs::create_dir(many.join(format!("d{index:05}"))).expect("a folder");
        }
        let result = list(&many, &[], false).expect("a listing");
        assert_eq!(result.entries.len(), MAX_ENTRIES);
        assert!(result.truncated);
        assert_eq!(result.entries[0].name, "d00000");
        let exact = base.join("exact");
        std::fs::create_dir_all(&exact).expect("a folder");
        for index in 0..MAX_ENTRIES {
            std::fs::create_dir(exact.join(format!("d{index:05}"))).expect("a folder");
        }
        let result = list(&exact, &[], false).expect("a listing");
        assert_eq!(result.entries.len(), MAX_ENTRIES);
        assert!(!result.truncated);
    }

    #[test]
    fn the_root_has_no_parent_and_dots_in_a_path_are_applied() {
        let result = list(Path::new("/"), &[], false).expect("a listing");
        assert_eq!(result.parent, None);
        assert_eq!(result.path, "/");
        assert_eq!(normal(Path::new("/a/b/../c/./d")), PathBuf::from("/a/c/d"));
        assert_eq!(normal(Path::new("/..")), PathBuf::from("/"));
    }
}
