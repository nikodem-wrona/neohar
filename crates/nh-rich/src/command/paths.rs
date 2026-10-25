//! Paths in a command: where they lead, and whether a program may write to them.
//!
//! A word becomes a path without the file system: `.`, `..` and `~` are resolved as text. The
//! check of the place then follows the symbolic links of the part of the path that exists
//! ([`real`]), because a link in the workspace can lead out of it: `src/link/.zshrc` with
//! `src/link -> ~` writes to the home of the user.

use std::path::{Component, Path, PathBuf};

/// The most links that [`real`] follows in one path, as the limit of the system.
const MAX_LINKS: usize = 40;

/// Where an absolute path leads: the symbolic links of the part that exists are followed, and the
/// rest is added as it is. A link whose target does not exist yet leads to that target, because a
/// write through it makes the target. A path that the file system cannot read stays as it is.
pub(super) fn real(path: &Path) -> PathBuf {
    let mut current = normalize(path);
    for _ in 0..MAX_LINKS {
        match follow(&current) {
            Followed::Done(out) => return out,
            Followed::Link(next) => current = next,
        }
    }
    current
}

enum Followed {
    /// The place where the path leads.
    Done(PathBuf),
    /// The path goes through a link whose target does not exist: the path from that target.
    Link(PathBuf),
}

/// One step of [`real`]: the longest part of `path` that exists, with its links followed, and
/// the rest of the path.
fn follow(path: &Path) -> Followed {
    let mut existing = path.to_path_buf();
    let mut rest: Vec<std::ffi::OsString> = Vec::new();
    loop {
        if let Ok(mut found) = std::fs::canonicalize(&existing) {
            found.extend(rest.iter().rev());
            return Followed::Done(found);
        }
        if let Ok(target) = std::fs::read_link(&existing) {
            let parent = existing.parent().unwrap_or_else(|| Path::new("/"));
            let mut next = parent.join(target);
            next.extend(rest.iter().rev());
            return Followed::Link(normalize(&next));
        }
        match (existing.file_name(), existing.parent()) {
            (Some(name), Some(parent)) => {
                rest.push(name.to_owned());
                existing = parent.to_path_buf();
            }
            _ => return Followed::Done(path.to_path_buf()),
        }
    }
}

/// A path with `.` and `..` resolved, without the file system.
pub(super) fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                // Above the root there is nothing: `/..` is `/`.
                if !out.pop() && !out.has_root() {
                    out.push("..");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// The absolute path for a word of a command, or `None` when its value is not known before the
/// shell runs (a variable, a pattern) or it names the home of another user.
pub(super) fn resolve(
    word: &str,
    dynamic: bool,
    cwd: &Path,
    home: Option<&Path>,
) -> Option<PathBuf> {
    if word.is_empty() {
        return None;
    }
    let expanded = if word == "~" {
        home?.to_path_buf()
    } else if let Some(rest) = word.strip_prefix("~/") {
        home?.join(rest)
    } else if word.starts_with('~') || dynamic {
        return None;
    } else {
        PathBuf::from(word)
    };
    let absolute = if expanded.is_absolute() {
        expanded
    } else {
        cwd.join(expanded)
    };
    Some(normalize(&absolute))
}

/// The files that every program may write: the null device and the standard streams.
pub(super) fn is_harmless_target(path: &Path) -> bool {
    matches!(
        path.to_str(),
        Some("/dev/null" | "/dev/stdout" | "/dev/stderr" | "/dev/stdin" | "/dev/tty")
    )
}

/// Whether `path` leads to a place below the workspace, after the links of both ([`real`]).
pub(super) fn is_inside(path: &Path, workspace: &Path) -> bool {
    real(path).starts_with(real(workspace))
}

/// Whether a path holds secrets by its name or by the place where its links lead.
pub(super) fn is_credential_or_leads_to_one(path: &Path) -> bool {
    is_credential(path) || is_credential(&real(path))
}

/// The path for a reason: the path as the command gives it, and the place where its links lead
/// when that place is another.
pub(super) fn shown(path: &Path) -> String {
    let target = real(path);
    // The links of the system itself (`/tmp` to `/private/tmp` on macOS) say nothing new.
    if target == path || target.ends_with(path.strip_prefix("/").unwrap_or(path)) {
        path.display().to_string()
    } else {
        format!("{} (a link to {})", path.display(), target.display())
    }
}

/// A file that holds secrets: environment files, keys and credentials. An edit of one of them is
/// high risk. The examples (`.env.example`) hold no secrets.
pub(super) fn is_credential(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    let name = name.to_ascii_lowercase();
    if name == ".env" || name.starts_with(".env.") {
        return !matches!(
            name.as_str(),
            ".env.example" | ".env.sample" | ".env.template" | ".env.dist" | ".env.defaults"
        );
    }
    if matches!(
        name.as_str(),
        "credentials"
            | "credentials.json"
            | "credentials.yml"
            | "credentials.yaml"
            | ".netrc"
            | "_netrc"
            | ".npmrc"
            | ".pypirc"
            | ".git-credentials"
            | ".dockercfg"
            | "id_rsa"
            | "id_dsa"
            | "id_ecdsa"
            | "id_ed25519"
            | "authorized_keys"
            | "known_hosts"
            | "secrets.json"
            | "secrets.yml"
            | "secrets.yaml"
            | "secrets.toml"
            | "service-account.json"
            | "kubeconfig"
            | "terraform.tfvars"
            | ".htpasswd"
    ) {
        return true;
    }
    let extension = name.rsplit_once('.').map(|(_, ext)| ext);
    if matches!(
        extension,
        Some("pem" | "key" | "p12" | "pfx" | "keystore" | "jks" | "kdbx" | "asc")
    ) {
        return true;
    }
    if name.starts_with("service-account") && name.ends_with(".json") {
        return true;
    }
    // Directories that hold secrets.
    path.components().any(|c| {
        matches!(
            c.as_os_str().to_str(),
            Some(".ssh" | ".gnupg" | ".aws" | ".kube")
        )
    })
}

#[cfg(test)]
mod tests;
