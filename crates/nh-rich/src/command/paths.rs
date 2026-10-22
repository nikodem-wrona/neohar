//! Paths in a command: where they lead, and whether a program may write to them.
//!
//! The check is lexical. It resolves `.` and `..` and `~`, and it does not follow symbolic links,
//! because a pure function cannot ask the file system.

use std::path::{Component, Path, PathBuf};

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

pub(super) fn is_inside(path: &Path, workspace: &Path) -> bool {
    path.starts_with(workspace)
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
