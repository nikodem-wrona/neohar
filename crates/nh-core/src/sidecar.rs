//! The `nh` symlink.
//!
//! Hook configs and shims name `~/.local/bin/nh` and nothing else. The app keeps that path a
//! symlink to the `nh` binary in its bundle, so an update never leaves a hook behind. This module
//! only makes the link. The host decides when to call it: a product build, started without the
//! directory overrides.

use std::{
    io,
    os::unix::fs::symlink,
    path::{Path, PathBuf},
};

/// What [`ensure_symlink`] did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LinkOutcome {
    /// There was no link. It exists now.
    Created,
    /// The link pointed somewhere else. It points at the sidecar now.
    Replaced { previous: PathBuf },
    /// The link was right already. Nothing changed.
    Unchanged,
    /// Nothing changed, and the user should hear about it (a health warning).
    Skipped(SkipReason),
}

/// Why [`ensure_symlink`] left the path alone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SkipReason {
    /// The sidecar does not exist, so a link would dangle.
    TargetMissing,
    /// The path is a file or a directory that the user owns. The app never overwrites it.
    NotASymlink,
}

/// Makes `link` a symlink to `target`, the sidecar in the app bundle.
///
/// - Missing: creates the link, and the directory of the link when needed.
/// - A symlink to something else: replaces it atomically (a temporary link, then `rename`), so a
///   hook that runs at the same moment finds the old or the new target, never none.
/// - A regular file or a directory: does not touch it.
///
/// # Errors
///
/// When the directory or the link cannot be written.
pub fn ensure_symlink(link: &Path, target: &Path) -> io::Result<LinkOutcome> {
    if !target.is_file() {
        return Ok(LinkOutcome::Skipped(SkipReason::TargetMissing));
    }
    let previous = match std::fs::symlink_metadata(link) {
        Ok(meta) if meta.file_type().is_symlink() => match std::fs::read_link(link) {
            Ok(current) if current == target => return Ok(LinkOutcome::Unchanged),
            Ok(current) => Some(current),
            // A link that cannot be read is not worth keeping.
            Err(_) => Some(PathBuf::new()),
        },
        Ok(_) => return Ok(LinkOutcome::Skipped(SkipReason::NotASymlink)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => None,
        Err(e) => return Err(e),
    };
    let dir = link
        .parent()
        .ok_or_else(|| io::Error::other("the link has no directory"))?;
    std::fs::create_dir_all(dir)?;
    let temporary = dir.join(format!(".nh-link-{}", std::process::id()));
    // A temporary link of an earlier crash with the same pid would be in the way.
    let _ = std::fs::remove_file(&temporary);
    symlink(target, &temporary)?;
    if let Err(error) = std::fs::rename(&temporary, link) {
        let _ = std::fs::remove_file(&temporary);
        return Err(error);
    }
    Ok(match previous {
        Some(previous) => LinkOutcome::Replaced { previous },
        None => LinkOutcome::Created,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A temp `HOME` with a fake sidecar, removed on drop.
    struct Home {
        root: PathBuf,
    }

    impl Home {
        fn new() -> Self {
            static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
            let root = std::env::temp_dir().join(format!(
                "nhs{}-{}",
                std::process::id(),
                N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&root).expect("a home");
            Self { root }
        }

        fn link(&self) -> PathBuf {
            self.root.join(".local").join("bin").join("nh")
        }

        fn sidecar(&self, name: &str) -> PathBuf {
            let path = self.root.join(name);
            std::fs::write(&path, b"#!/bin/sh\n").expect("a sidecar");
            path
        }
    }

    impl Drop for Home {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn a_missing_link_is_created_with_its_directory() {
        let home = Home::new();
        let sidecar = home.sidecar("nh-new");
        assert!(!home.link().parent().expect("a directory").exists());
        assert_eq!(
            ensure_symlink(&home.link(), &sidecar).expect("it works"),
            LinkOutcome::Created
        );
        assert_eq!(std::fs::read_link(home.link()).expect("a link"), sidecar);
    }

    #[test]
    fn a_right_link_is_left_alone() {
        let home = Home::new();
        let sidecar = home.sidecar("nh-new");
        ensure_symlink(&home.link(), &sidecar).expect("it works");
        assert_eq!(
            ensure_symlink(&home.link(), &sidecar).expect("it works"),
            LinkOutcome::Unchanged
        );
    }

    #[test]
    fn a_link_to_an_old_app_is_replaced() {
        let home = Home::new();
        let old = home.sidecar("nh-old");
        let new = home.sidecar("nh-new");
        ensure_symlink(&home.link(), &old).expect("it works");
        assert_eq!(
            ensure_symlink(&home.link(), &new).expect("it works"),
            LinkOutcome::Replaced { previous: old }
        );
        assert_eq!(std::fs::read_link(home.link()).expect("a link"), new);
        let names: Vec<_> = std::fs::read_dir(home.link().parent().expect("a directory"))
            .expect("a listing")
            .map(|e| e.expect("an entry").file_name())
            .collect();
        assert_eq!(names, ["nh"], "no temporary link stays");
    }

    #[test]
    fn a_dangling_link_is_replaced() {
        let home = Home::new();
        let new = home.sidecar("nh-new");
        std::fs::create_dir_all(home.link().parent().expect("a directory")).expect("a directory");
        symlink(home.root.join("gone"), home.link()).expect("a dangling link");
        assert!(matches!(
            ensure_symlink(&home.link(), &new).expect("it works"),
            LinkOutcome::Replaced { .. }
        ));
        assert_eq!(std::fs::read_link(home.link()).expect("a link"), new);
    }

    #[test]
    fn a_file_or_a_directory_of_the_user_stays() {
        let home = Home::new();
        let sidecar = home.sidecar("nh-new");
        let dir = home.link().parent().expect("a directory").to_owned();
        std::fs::create_dir_all(&dir).expect("a directory");

        std::fs::write(home.link(), b"my own nh").expect("a file");
        assert_eq!(
            ensure_symlink(&home.link(), &sidecar).expect("it works"),
            LinkOutcome::Skipped(SkipReason::NotASymlink)
        );
        assert_eq!(
            std::fs::read(home.link()).expect("still there"),
            b"my own nh"
        );

        std::fs::remove_file(home.link()).expect("a clean path");
        std::fs::create_dir(home.link()).expect("a directory");
        assert_eq!(
            ensure_symlink(&home.link(), &sidecar).expect("it works"),
            LinkOutcome::Skipped(SkipReason::NotASymlink)
        );
        assert!(home.link().is_dir());
    }

    #[test]
    fn a_missing_sidecar_makes_no_link() {
        let home = Home::new();
        assert_eq!(
            ensure_symlink(&home.link(), &home.root.join("nope")).expect("it works"),
            LinkOutcome::Skipped(SkipReason::TargetMissing)
        );
        assert!(std::fs::symlink_metadata(home.link()).is_err());
    }
}
