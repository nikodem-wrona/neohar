//! The guard of the plugin for a single instance.
//!
//! On macOS, `tauri-plugin-single-instance` uses the fixed socket `/tmp/<identifier>_si.sock`,
//! with `.` and `-` of the identifier changed to `_`. It has no option for another path. At start,
//! the plugin connects to whatever listens there, sends the working directory and the arguments,
//! and ends the process. Every user can make a file in `/tmp`, so another user can make that socket
//! first. Then each launch of neohar sends its arguments to that user and ends.
//!
//! The app registers the plugin only when the path is free or is a socket of this user. Else it
//! logs a warning and runs without the plugin. A second launch then does not hand over to the
//! first one: it is one more client of the same core through `link.sock`. A path that another
//! user makes after this check and before the plugin connects is not caught: that user must win
//! a race at each launch.

use std::path::{Path, PathBuf};

/// The socket of the plugin for an app identifier, as the plugin computes it on macOS.
pub fn socket_path(identifier: &str) -> PathBuf {
    PathBuf::from(format!(
        "/tmp/{}_si.sock",
        identifier.replace(['.', '-'], "_")
    ))
}

/// What `lstat` found at the path of the socket.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Found {
    Nothing,
    /// A socket and the user that owns it.
    Socket {
        uid: u32,
    },
    /// Any other file (a directory, a symbolic link, a plain file) and the user that owns it.
    Other {
        uid: u32,
    },
}

/// Whether the app can use the plugin, from what is at its path. `Err` has the reason for the log.
pub fn judge(found: Found, uid: u32) -> Result<(), String> {
    match found {
        // The plugin binds its socket: the first launch.
        Found::Nothing => Ok(()),
        // The socket of a running neohar of this user, or one that a crash left.
        Found::Socket { uid: owner } if owner == uid => Ok(()),
        Found::Socket { uid: owner } => {
            Err(format!("it is a socket of user {owner}, not of user {uid}"))
        }
        Found::Other { uid: owner } => {
            Err(format!("it is not a socket (it belongs to user {owner})"))
        }
    }
}

/// What `lstat` finds at `path`. An error other than a missing file counts as a file of nobody.
fn look(path: &Path) -> Found {
    use std::os::unix::fs::{FileTypeExt, MetadataExt};

    match std::fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Found::Nothing,
        Err(_) => Found::Other { uid: u32::MAX },
        Ok(meta) if meta.file_type().is_socket() => Found::Socket { uid: meta.uid() },
        Ok(meta) => Found::Other { uid: meta.uid() },
    }
}

/// Whether the app registers the plugin for the socket at `path`. It logs why not.
pub fn allowed_at(path: &Path, uid: u32) -> bool {
    match judge(look(path), uid) {
        Ok(()) => true,
        Err(reason) => {
            tracing::warn!(
                path = %path.display(),
                reason,
                "the socket of the single instance is not safe: another user can have made it. \
                 The app runs without it, so a second launch does not hand over to this one"
            );
            false
        }
    }
}

/// Whether the app registers the plugin for its identifier. Only macOS uses a socket in `/tmp`.
pub fn allowed(identifier: &str) -> bool {
    if cfg!(target_os = "macos") {
        allowed_at(&socket_path(identifier), current_uid())
    } else {
        true
    }
}

fn current_uid() -> u32 {
    // `getuid` of the process, through the same code as the runtime root of `nh-proto`.
    nh_proto::paths::Env::from_process().uid
}

#[cfg(test)]
mod tests {
    use std::os::unix::net::UnixListener;

    use super::*;

    #[test]
    fn the_path_is_the_one_of_the_plugin() {
        assert_eq!(
            socket_path("dev.neohar.app"),
            Path::new("/tmp/dev_neohar_app_si.sock")
        );
        assert_eq!(socket_path("a-b.c"), Path::new("/tmp/a_b_c_si.sock"));
    }

    #[test]
    fn only_a_free_path_or_a_socket_of_this_user_is_used() {
        assert_eq!(judge(Found::Nothing, 501), Ok(()));
        assert_eq!(judge(Found::Socket { uid: 501 }, 501), Ok(()));
        assert!(judge(Found::Socket { uid: 502 }, 501).is_err());
        assert!(judge(Found::Other { uid: 501 }, 501).is_err());
        assert!(judge(Found::Other { uid: 502 }, 501).is_err());
    }

    /// A short directory under the temp dir, removed on drop.
    struct Scratch(PathBuf);

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn the_files_on_disk_are_judged() {
        let scratch = Scratch(std::env::temp_dir().join(format!("nhsi-{}", std::process::id())));
        let _ = std::fs::remove_dir_all(&scratch.0);
        std::fs::create_dir_all(&scratch.0).expect("a directory");
        let uid = current_uid();
        let socket = scratch.0.join("si.sock");
        assert!(allowed_at(&socket, uid), "a free path");
        let _listener = UnixListener::bind(&socket).expect("a socket");
        assert!(allowed_at(&socket, uid), "a socket of this user");
        assert!(
            !allowed_at(&socket, uid.wrapping_add(1)),
            "a socket of another user"
        );
        let file = scratch.0.join("file");
        std::fs::write(&file, b"x").expect("a file");
        assert!(!allowed_at(&file, uid), "a plain file");
        let link = scratch.0.join("link");
        std::os::unix::fs::symlink(&socket, &link).expect("a link");
        assert!(!allowed_at(&link, uid), "a link to a socket of this user");
    }
}
