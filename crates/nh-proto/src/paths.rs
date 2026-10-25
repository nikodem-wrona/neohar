//! The one place that computes directories and socket paths.
//!
//! No other code builds these paths. The functions without an `Env` read the process environment.
//! Tests use [`Env`] directly, with fixed values.
//!
//! | Root | macOS | Linux |
//! |---|---|---|
//! | config | `~/.config/neohar` | `$XDG_CONFIG_HOME/neohar` or `~/.config/neohar` |
//! | data | `~/Library/Application Support/dev.neohar.app` | `$XDG_DATA_HOME/neohar` or `~/.local/share/neohar` |
//! | runtime | `/tmp/neohar-$UID` | `$XDG_RUNTIME_DIR/neohar`, or `/tmp/neohar-$UID` when it is unset |
//!
//! The variables `NEOHAR_CONFIG_DIR`, `NEOHAR_DATA_DIR` and `NEOHAR_RUNTIME_DIR` replace a root.
//! Every test and every development run sets all three.

use std::{
    env,
    path::{Path, PathBuf},
};

use ulid::Ulid;

/// The directory name that macOS gives the app's data.
const MACOS_APP_ID: &str = "dev.neohar.app";

/// The longest session name.
pub const MAX_SESSION_LEN: usize = 32;

/// The largest path of a Unix socket on macOS, without the terminating NUL.
pub const MAX_SOCKET_PATH: usize = 103;

/// An error from computing or preparing a path.
#[derive(Debug, thiserror::Error)]
pub enum PathsError {
    #[error(
        "invalid session name {name:?}: use 1 to {MAX_SESSION_LEN} characters from a-z, 0-9 and -"
    )]
    InvalidSession { name: String },
    #[error("the HOME environment variable is not set")]
    NoHome,
    #[error("the runtime directory {path} is not safe: {problem}")]
    UnsafeRuntimeRoot { path: PathBuf, problem: RootProblem },
    #[error("the directory {path} of a socket is not safe: {problem}")]
    UnsafeSocketDir { path: PathBuf, problem: RootProblem },
    #[error("the process that serves {path} belongs to user {found}, not to user {expected}")]
    ForeignPeer {
        path: PathBuf,
        found: u32,
        expected: u32,
    },
    #[error("cannot prepare {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
}

/// Why a runtime root is not safe to use.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum RootProblem {
    #[error("it is a symbolic link")]
    Symlink,
    #[error("it is not a directory")]
    NotADirectory,
    #[error("it belongs to user {found}, not to user {expected}")]
    WrongOwner { found: u32, expected: u32 },
    #[error("its mode is {found:04o}, not 0700")]
    WrongMode { found: u32 },
    #[error("other users can write in it (mode {found:04o})")]
    WritableByOthers { found: u32 },
}

impl From<PathsError> for std::io::Error {
    /// An error of the file system keeps its kind, so that a client can still tell a missing
    /// socket from a refusal. A directory or a peer that is not safe is `PermissionDenied`.
    fn from(error: PathsError) -> Self {
        let kind = match &error {
            PathsError::Io { source, .. } => source.kind(),
            PathsError::InvalidSession { .. } | PathsError::NoHome => {
                std::io::ErrorKind::InvalidInput
            }
            PathsError::UnsafeRuntimeRoot { .. }
            | PathsError::UnsafeSocketDir { .. }
            | PathsError::ForeignPeer { .. } => std::io::ErrorKind::PermissionDenied,
        };
        Self::new(kind, error)
    }
}

/// The kind of file at the runtime root, as `lstat` reports it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileKind {
    Directory,
    Symlink,
    Other,
}

/// The `lstat` facts that decide whether a runtime root is safe. Tests build this by hand.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RootMeta {
    pub kind: FileKind,
    pub uid: u32,
    /// The permission bits, for example `0o700`.
    pub mode: u32,
}

/// Checks that a runtime root is a real directory that `uid` owns, with mode 0700.
///
/// The runtime root lives in a directory that every user can write (`/tmp`), so another user
/// can create it first, or replace it with a symbolic link.
pub fn check_runtime_root(meta: RootMeta, uid: u32) -> Result<(), RootProblem> {
    match meta.kind {
        FileKind::Symlink => Err(RootProblem::Symlink),
        FileKind::Other => Err(RootProblem::NotADirectory),
        FileKind::Directory if meta.uid != uid => Err(RootProblem::WrongOwner {
            found: meta.uid,
            expected: uid,
        }),
        FileKind::Directory if meta.mode & 0o7777 != 0o700 => Err(RootProblem::WrongMode {
            found: meta.mode & 0o7777,
        }),
        FileKind::Directory => Ok(()),
    }
}

/// Checks the directory of a socket before a client connects to it: a real directory that `uid`
/// owns, in which no other user can write. Then no other user can have put the socket there or
/// replaced it. The runtime root and the directories of the sessions have mode 0700, so they pass.
pub fn check_socket_dir(meta: RootMeta, uid: u32) -> Result<(), RootProblem> {
    match meta.kind {
        FileKind::Symlink => Err(RootProblem::Symlink),
        FileKind::Other => Err(RootProblem::NotADirectory),
        FileKind::Directory if meta.uid != uid => Err(RootProblem::WrongOwner {
            found: meta.uid,
            expected: uid,
        }),
        FileKind::Directory if meta.mode & 0o022 != 0 => Err(RootProblem::WritableByOthers {
            found: meta.mode & 0o7777,
        }),
        FileKind::Directory => Ok(()),
    }
}

/// Checks the user of the process that serves a socket, after a client connected to it. Only a
/// process of `uid` may get what the client sends.
pub fn check_peer_uid(socket: &Path, peer: u32, uid: u32) -> Result<(), PathsError> {
    if peer == uid {
        Ok(())
    } else {
        Err(PathsError::ForeignPeer {
            path: socket.to_owned(),
            found: peer,
            expected: uid,
        })
    }
}

/// The `lstat` facts of `path`.
#[cfg(unix)]
fn lstat(path: &Path) -> Result<RootMeta, PathsError> {
    use std::os::unix::fs::MetadataExt;

    let meta = std::fs::symlink_metadata(path).map_err(|source| PathsError::Io {
        path: path.to_owned(),
        source,
    })?;
    let kind = match meta.file_type() {
        t if t.is_symlink() => FileKind::Symlink,
        t if t.is_dir() => FileKind::Directory,
        _ => FileKind::Other,
    };
    Ok(RootMeta {
        kind,
        uid: meta.uid(),
        mode: meta.mode(),
    })
}

/// [`check_socket_dir`] on the directory of `socket`, for the user of this process. It is one
/// `lstat`, so the hook shim can afford it. A missing directory is an [`PathsError::Io`] of kind
/// `NotFound`: no core serves the socket.
#[cfg(unix)]
pub fn check_socket_parent(socket: &Path) -> Result<(), PathsError> {
    let dir = match socket.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir,
        _ => Path::new("."),
    };
    check_socket_dir(lstat(dir)?, current_uid()).map_err(|problem| PathsError::UnsafeSocketDir {
        path: dir.to_owned(),
        problem,
    })
}

/// [`check_peer_uid`] for the user of this process.
pub fn check_peer(socket: &Path, peer: u32) -> Result<(), PathsError> {
    check_peer_uid(socket, peer, current_uid())
}

/// Checks a session name: 1 to 32 characters from `a-z`, `0-9` and `-`.
pub fn validate_session_name(name: &str) -> Result<(), PathsError> {
    let ok = (1..=MAX_SESSION_LEN).contains(&name.len())
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
    if ok {
        Ok(())
    } else {
        Err(PathsError::InvalidSession {
            name: name.to_owned(),
        })
    }
}

/// The operating system family that decides the layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    MacOs,
    Linux,
}

impl Platform {
    /// The platform of this build. Every Unix that is not macOS uses the Linux layout.
    pub fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::MacOs
        } else {
            Self::Linux
        }
    }
}

/// The inputs of the path functions. [`Env::from_process`] reads them from the process.
#[derive(Clone, Debug)]
pub struct Env {
    pub platform: Platform,
    pub uid: u32,
    pub home: Option<PathBuf>,
    /// `NEOHAR_CONFIG_DIR`.
    pub config_override: Option<PathBuf>,
    /// `NEOHAR_DATA_DIR`.
    pub data_override: Option<PathBuf>,
    /// `NEOHAR_RUNTIME_DIR`.
    pub runtime_override: Option<PathBuf>,
    pub xdg_config_home: Option<PathBuf>,
    pub xdg_data_home: Option<PathBuf>,
    pub xdg_runtime_dir: Option<PathBuf>,
}

/// A variable's value, or `None` when it is unset or empty.
fn var(name: &str) -> Option<PathBuf> {
    env::var_os(name)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

#[cfg(unix)]
fn current_uid() -> u32 {
    rustix::process::getuid().as_raw()
}

#[cfg(not(unix))]
fn current_uid() -> u32 {
    0
}

impl Env {
    /// Reads the environment of this process.
    pub fn from_process() -> Self {
        Self {
            platform: Platform::current(),
            uid: current_uid(),
            home: var("HOME"),
            config_override: var("NEOHAR_CONFIG_DIR"),
            data_override: var("NEOHAR_DATA_DIR"),
            runtime_override: var("NEOHAR_RUNTIME_DIR"),
            xdg_config_home: var("XDG_CONFIG_HOME"),
            xdg_data_home: var("XDG_DATA_HOME"),
            xdg_runtime_dir: var("XDG_RUNTIME_DIR"),
        }
    }

    fn home(&self) -> Result<&Path, PathsError> {
        self.home.as_deref().ok_or(PathsError::NoHome)
    }

    /// The configuration directory. It does not depend on the session.
    pub fn config_dir(&self) -> Result<PathBuf, PathsError> {
        if let Some(dir) = &self.config_override {
            return Ok(dir.clone());
        }
        match &self.xdg_config_home {
            Some(xdg) => Ok(xdg.join("neohar")),
            None => Ok(self.home()?.join(".config").join("neohar")),
        }
    }

    fn data_root(&self) -> Result<PathBuf, PathsError> {
        if let Some(dir) = &self.data_override {
            return Ok(dir.clone());
        }
        match self.platform {
            Platform::MacOs => Ok(self
                .home()?
                .join("Library")
                .join("Application Support")
                .join(MACOS_APP_ID)),
            Platform::Linux => match &self.xdg_data_home {
                Some(xdg) => Ok(xdg.join("neohar")),
                None => Ok(self.home()?.join(".local").join("share").join("neohar")),
            },
        }
    }

    /// The data directory of a session: `state.db`, scrollback, blobs and logs.
    pub fn data_dir(&self, session: &str) -> Result<PathBuf, PathsError> {
        validate_session_name(session)?;
        Ok(self.data_root()?.join(session))
    }

    /// The runtime root. This function does not touch the file system.
    /// Use [`Env::ensure_runtime_root`] before you create a socket.
    pub fn runtime_root(&self) -> Result<PathBuf, PathsError> {
        if let Some(dir) = &self.runtime_override {
            return Ok(dir.clone());
        }
        let tmp = || PathBuf::from(format!("/tmp/neohar-{}", self.uid));
        match (self.platform, &self.xdg_runtime_dir) {
            (Platform::Linux, Some(xdg)) => Ok(xdg.join("neohar")),
            _ => Ok(tmp()),
        }
    }

    /// The runtime directory of a session: the API socket and the `holders` directory.
    pub fn runtime_dir(&self, session: &str) -> Result<PathBuf, PathsError> {
        validate_session_name(session)?;
        Ok(self.runtime_root()?.join(session))
    }

    /// The directory of the holder sockets of a session.
    pub fn holders_dir(&self, session: &str) -> Result<PathBuf, PathsError> {
        Ok(self.runtime_dir(session)?.join("holders"))
    }

    /// The socket of the holder of a pane.
    pub fn holder_socket(&self, session: &str, pane: Ulid) -> Result<PathBuf, PathsError> {
        Ok(self.holders_dir(session)?.join(format!("{pane}.sock")))
    }

    /// The socket of the JSON-RPC API of a session.
    pub fn api_socket(&self, session: &str) -> Result<PathBuf, PathsError> {
        Ok(self.runtime_dir(session)?.join("api.sock"))
    }

    /// The socket of the links of remote clients. Both hosts of a core serve it: the
    /// app and `nh daemon`. `nh bridge` copies the bytes of ssh to it.
    pub fn link_socket(&self, session: &str) -> Result<PathBuf, PathsError> {
        Ok(self.runtime_dir(session)?.join("link.sock"))
    }

    /// The symlink that hook configs name: `~/.local/bin/nh`. It depends on neither the
    /// session nor the overrides, because hooks name this one path.
    pub fn nh_symlink(&self) -> Result<PathBuf, PathsError> {
        Ok(self.home()?.join(".local").join("bin").join("nh"))
    }

    /// Any of the three directory overrides is set. A run with overrides is a test or a
    /// development run, and it must not touch the real `~/.local/bin/nh`.
    pub fn has_overrides(&self) -> bool {
        self.config_override.is_some()
            || self.data_override.is_some()
            || self.runtime_override.is_some()
    }

    /// Creates the runtime root with mode 0700 when it is missing, and checks it.
    ///
    /// It refuses a root that is a symbolic link, belongs to another user, or has another mode.
    #[cfg(unix)]
    pub fn ensure_runtime_root(&self) -> Result<PathBuf, PathsError> {
        use std::os::unix::fs::DirBuilderExt;

        let root = self.runtime_root()?;
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&root)
            .map_err(|source| PathsError::Io {
                path: root.clone(),
                source,
            })?;
        self.verify_runtime_root()
    }

    /// Checks the runtime root as [`Env::ensure_runtime_root`] does, but creates nothing. A client
    /// calls it before it connects to a socket of a core. A missing root is an
    /// [`PathsError::Io`] of kind `NotFound`: no core runs.
    #[cfg(unix)]
    pub fn verify_runtime_root(&self) -> Result<PathBuf, PathsError> {
        let root = self.runtime_root()?;
        check_runtime_root(lstat(&root)?, self.uid).map_err(|problem| {
            PathsError::UnsafeRuntimeRoot {
                path: root.clone(),
                problem,
            }
        })?;
        Ok(root)
    }
}

/// The configuration directory of this process.
pub fn config_dir() -> Result<PathBuf, PathsError> {
    Env::from_process().config_dir()
}

/// The data directory of a session.
pub fn data_dir(session: &str) -> Result<PathBuf, PathsError> {
    Env::from_process().data_dir(session)
}

/// The runtime directory of a session. It does not touch the file system.
pub fn runtime_dir(session: &str) -> Result<PathBuf, PathsError> {
    Env::from_process().runtime_dir(session)
}

/// The socket of the holder of a pane.
pub fn holder_socket(session: &str, pane: Ulid) -> Result<PathBuf, PathsError> {
    Env::from_process().holder_socket(session, pane)
}

/// The socket of the JSON-RPC API of a session.
pub fn api_socket(session: &str) -> Result<PathBuf, PathsError> {
    Env::from_process().api_socket(session)
}

/// Creates and checks the runtime root of this process.
#[cfg(unix)]
pub fn ensure_runtime_root() -> Result<PathBuf, PathsError> {
    Env::from_process().ensure_runtime_root()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(platform: Platform) -> Env {
        Env {
            platform,
            uid: 501,
            home: Some(PathBuf::from("/Users/ada")),
            config_override: None,
            data_override: None,
            runtime_override: None,
            xdg_config_home: None,
            xdg_data_home: None,
            xdg_runtime_dir: None,
        }
    }

    fn pane() -> Ulid {
        Ulid::from_string("01J9ZZZZZZZZZZZZZZZZZZZZZZ").expect("a valid ULID")
    }

    #[test]
    fn session_names_are_1_to_32_of_a_z_0_9_and_dash() {
        for ok in ["a", "default", "dev", "0", "a-b-c", &"x".repeat(32), "-"] {
            assert!(validate_session_name(ok).is_ok(), "{ok:?}");
        }
        for bad in [
            "",
            &"x".repeat(33),
            "A",
            "a_b",
            "a/b",
            "..",
            "a b",
            "é",
            "a\0",
        ] {
            assert!(validate_session_name(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn the_macos_layout() {
        let e = env(Platform::MacOs);
        assert_eq!(
            e.config_dir().expect("config"),
            Path::new("/Users/ada/.config/neohar")
        );
        assert_eq!(
            e.data_dir("default").expect("data"),
            Path::new("/Users/ada/Library/Application Support/dev.neohar.app/default")
        );
        assert_eq!(
            e.runtime_dir("default").expect("runtime"),
            Path::new("/tmp/neohar-501/default")
        );
        assert_eq!(
            e.holder_socket("default", pane()).expect("socket"),
            Path::new("/tmp/neohar-501/default/holders/01J9ZZZZZZZZZZZZZZZZZZZZZZ.sock")
        );
        assert_eq!(
            e.api_socket("default").expect("api"),
            Path::new("/tmp/neohar-501/default/api.sock")
        );
    }

    #[test]
    fn the_nh_symlink_is_in_the_home_and_a_run_with_overrides_is_marked() {
        let e = env(Platform::MacOs);
        assert_eq!(
            e.nh_symlink().expect("symlink"),
            Path::new("/Users/ada/.local/bin/nh")
        );
        assert!(!e.has_overrides());
        for with in [
            Env {
                config_override: Some("/c".into()),
                ..e.clone()
            },
            Env {
                data_override: Some("/d".into()),
                ..e.clone()
            },
            Env {
                runtime_override: Some("/r".into()),
                ..e.clone()
            },
        ] {
            assert!(with.has_overrides());
            // The symlink path does not move with the overrides.
            assert_eq!(
                with.nh_symlink().expect("symlink"),
                e.nh_symlink().expect("symlink")
            );
        }
        let no_home = Env { home: None, ..e };
        assert!(no_home.nh_symlink().is_err());
    }

    #[test]
    fn macos_ignores_xdg_runtime_dir() {
        let e = Env {
            xdg_runtime_dir: Some("/run/user/501".into()),
            ..env(Platform::MacOs)
        };
        assert_eq!(
            e.runtime_root().expect("root"),
            Path::new("/tmp/neohar-501")
        );
    }

    #[test]
    fn the_linux_layout() {
        let e = Env {
            xdg_config_home: Some("/home/ada/cfg".into()),
            xdg_data_home: Some("/home/ada/data".into()),
            xdg_runtime_dir: Some("/run/user/1000".into()),
            home: Some("/home/ada".into()),
            ..env(Platform::Linux)
        };
        assert_eq!(
            e.config_dir().expect("config"),
            Path::new("/home/ada/cfg/neohar")
        );
        assert_eq!(
            e.data_dir("dev").expect("data"),
            Path::new("/home/ada/data/neohar/dev")
        );
        assert_eq!(
            e.runtime_dir("dev").expect("runtime"),
            Path::new("/run/user/1000/neohar/dev")
        );
    }

    #[test]
    fn linux_without_xdg_variables_uses_the_home_defaults_and_tmp() {
        let e = Env {
            home: Some("/home/ada".into()),
            ..env(Platform::Linux)
        };
        assert_eq!(
            e.config_dir().expect("config"),
            Path::new("/home/ada/.config/neohar")
        );
        assert_eq!(
            e.data_dir("dev").expect("data"),
            Path::new("/home/ada/.local/share/neohar/dev")
        );
        assert_eq!(
            e.runtime_root().expect("root"),
            Path::new("/tmp/neohar-501")
        );
    }

    #[test]
    fn the_overrides_replace_the_roots() {
        let e = Env {
            config_override: Some("/t/config".into()),
            data_override: Some("/t/data".into()),
            runtime_override: Some("/t/run".into()),
            xdg_runtime_dir: Some("/run/user/1000".into()),
            home: None,
            ..env(Platform::Linux)
        };
        assert_eq!(e.config_dir().expect("config"), Path::new("/t/config"));
        assert_eq!(e.data_dir("dev").expect("data"), Path::new("/t/data/dev"));
        assert_eq!(
            e.api_socket("dev").expect("api"),
            Path::new("/t/run/dev/api.sock")
        );
    }

    #[test]
    fn a_missing_home_is_an_error_unless_an_override_replaces_it() {
        let e = Env {
            home: None,
            ..env(Platform::MacOs)
        };
        assert!(matches!(e.config_dir(), Err(PathsError::NoHome)));
        assert!(matches!(e.data_dir("dev"), Err(PathsError::NoHome)));
        assert!(e.runtime_dir("dev").is_ok());
    }

    #[test]
    fn an_invalid_session_name_is_rejected_by_every_path() {
        let e = env(Platform::MacOs);
        assert!(e.data_dir("A").is_err());
        assert!(e.runtime_dir("../x").is_err());
        assert!(e.holder_socket("", pane()).is_err());
        assert!(e.api_socket(&"x".repeat(33)).is_err());
    }

    /// With the longest UID and the longest session name, a socket path still
    /// fits in `sun_path` on macOS.
    #[test]
    fn the_longest_socket_paths_fit_in_sun_path() {
        let session = "x".repeat(MAX_SESSION_LEN);
        let e = Env {
            uid: u32::MAX,
            ..env(Platform::MacOs)
        };
        let holder = e.holder_socket(&session, Ulid::max()).expect("socket");
        let api = e.api_socket(&session).expect("api");
        println!("longest holder socket: {} bytes", holder.as_os_str().len());
        assert!(
            holder.as_os_str().len() <= MAX_SOCKET_PATH,
            "{}",
            holder.display()
        );
        assert!(
            api.as_os_str().len() <= MAX_SOCKET_PATH,
            "{}",
            api.display()
        );
    }

    #[test]
    fn a_runtime_root_must_be_a_real_directory_owned_by_the_user_with_mode_0700() {
        let good = RootMeta {
            kind: FileKind::Directory,
            uid: 501,
            mode: 0o700,
        };
        assert_eq!(check_runtime_root(good, 501), Ok(()));
        // The mode of a real directory includes the file type bits.
        assert_eq!(
            check_runtime_root(
                RootMeta {
                    mode: 0o40700,
                    ..good
                },
                501
            ),
            Ok(())
        );
        assert_eq!(
            check_runtime_root(
                RootMeta {
                    kind: FileKind::Symlink,
                    ..good
                },
                501
            ),
            Err(RootProblem::Symlink)
        );
        assert_eq!(
            check_runtime_root(
                RootMeta {
                    kind: FileKind::Other,
                    ..good
                },
                501
            ),
            Err(RootProblem::NotADirectory)
        );
        assert_eq!(
            check_runtime_root(RootMeta { uid: 0, ..good }, 501),
            Err(RootProblem::WrongOwner {
                found: 0,
                expected: 501
            })
        );
        assert_eq!(
            check_runtime_root(
                RootMeta {
                    mode: 0o755,
                    ..good
                },
                501
            ),
            Err(RootProblem::WrongMode { found: 0o755 })
        );
        assert_eq!(
            check_runtime_root(
                RootMeta {
                    mode: 0o770,
                    ..good
                },
                501
            ),
            Err(RootProblem::WrongMode { found: 0o770 })
        );
    }

    #[test]
    fn the_directory_of_a_socket_must_be_owned_by_the_user_and_closed_to_other_writers() {
        let good = RootMeta {
            kind: FileKind::Directory,
            uid: 501,
            mode: 0o40700,
        };
        assert_eq!(check_socket_dir(good, 501), Ok(()));
        // Others may read and enter, but not write.
        assert_eq!(
            check_socket_dir(
                RootMeta {
                    mode: 0o755,
                    ..good
                },
                501
            ),
            Ok(())
        );
        assert_eq!(
            check_socket_dir(
                RootMeta {
                    kind: FileKind::Symlink,
                    ..good
                },
                501
            ),
            Err(RootProblem::Symlink)
        );
        assert_eq!(
            check_socket_dir(
                RootMeta {
                    kind: FileKind::Other,
                    ..good
                },
                501
            ),
            Err(RootProblem::NotADirectory)
        );
        // `/tmp` belongs to root, and another user can have made a directory there first.
        assert_eq!(
            check_socket_dir(
                RootMeta {
                    uid: 502,
                    mode: 0o700,
                    ..good
                },
                501
            ),
            Err(RootProblem::WrongOwner {
                found: 502,
                expected: 501
            })
        );
        for mode in [0o1777, 0o770, 0o702, 0o720] {
            assert_eq!(
                check_socket_dir(RootMeta { mode, ..good }, 501),
                Err(RootProblem::WritableByOthers { found: mode }),
                "{mode:o}"
            );
        }
    }

    #[test]
    fn only_a_process_of_the_user_may_serve_a_socket() {
        let socket = Path::new("/tmp/neohar-501/default/link.sock");
        assert!(check_peer_uid(socket, 501, 501).is_ok());
        let error = check_peer_uid(socket, 502, 501).expect_err("another user");
        assert!(matches!(
            error,
            PathsError::ForeignPeer {
                found: 502,
                expected: 501,
                ..
            }
        ));
        assert_eq!(
            std::io::Error::from(error).kind(),
            std::io::ErrorKind::PermissionDenied
        );
    }

    #[test]
    fn an_error_of_the_file_system_keeps_its_kind_as_an_io_error() {
        let missing = PathsError::Io {
            path: "/tmp/x".into(),
            source: std::io::Error::from(std::io::ErrorKind::NotFound),
        };
        assert_eq!(
            std::io::Error::from(missing).kind(),
            std::io::ErrorKind::NotFound
        );
        let unsafe_root = PathsError::UnsafeRuntimeRoot {
            path: "/tmp/neohar-501".into(),
            problem: RootProblem::Symlink,
        };
        let error = std::io::Error::from(unsafe_root);
        assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
        assert!(error.to_string().contains("/tmp/neohar-501"), "{error}");
    }

    #[cfg(unix)]
    mod on_disk {
        use std::{
            fs,
            os::unix::fs::{PermissionsExt, symlink},
            sync::atomic::{AtomicU32, Ordering},
        };

        use super::*;

        /// A directory under the system temp dir that is removed on drop.
        struct Scratch(PathBuf);

        impl Scratch {
            fn new() -> Self {
                static N: AtomicU32 = AtomicU32::new(0);
                let dir = std::env::temp_dir().join(format!(
                    "nh-paths-{}-{}",
                    std::process::id(),
                    N.fetch_add(1, Ordering::Relaxed)
                ));
                fs::create_dir_all(&dir).expect("the scratch directory is created");
                Self(dir)
            }
        }

        impl Drop for Scratch {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }

        fn env_with_root(root: &Path) -> Env {
            Env {
                uid: current_uid(),
                runtime_override: Some(root.to_owned()),
                ..env(Platform::current())
            }
        }

        fn mode(path: &Path) -> u32 {
            fs::symlink_metadata(path)
                .expect("the path exists")
                .permissions()
                .mode()
                & 0o7777
        }

        #[test]
        fn ensure_creates_the_root_with_mode_0700_and_accepts_it_again() {
            let scratch = Scratch::new();
            let root = scratch.0.join("a").join("run");
            let e = env_with_root(&root);
            assert_eq!(e.ensure_runtime_root().expect("created"), root);
            assert_eq!(mode(&root), 0o700);
            assert_eq!(e.ensure_runtime_root().expect("accepted again"), root);
        }

        #[test]
        fn ensure_refuses_a_root_with_mode_0755() {
            let scratch = Scratch::new();
            let root = scratch.0.join("run");
            fs::create_dir(&root).expect("the root is created");
            fs::set_permissions(&root, fs::Permissions::from_mode(0o755)).expect("the mode is set");
            assert!(matches!(
                env_with_root(&root).ensure_runtime_root(),
                Err(PathsError::UnsafeRuntimeRoot {
                    problem: RootProblem::WrongMode { found: 0o755 },
                    ..
                })
            ));
        }

        #[test]
        fn ensure_refuses_a_symbolic_link_to_a_good_directory() {
            let scratch = Scratch::new();
            let target = scratch.0.join("target");
            fs::create_dir(&target).expect("the target is created");
            fs::set_permissions(&target, fs::Permissions::from_mode(0o700))
                .expect("the mode is set");
            let link = scratch.0.join("link");
            symlink(&target, &link).expect("the link is created");
            assert!(matches!(
                env_with_root(&link).ensure_runtime_root(),
                Err(PathsError::UnsafeRuntimeRoot {
                    problem: RootProblem::Symlink,
                    ..
                })
            ));
        }

        #[test]
        fn verify_creates_nothing_and_refuses_what_ensure_refuses() {
            let scratch = Scratch::new();
            let root = scratch.0.join("run");
            let e = env_with_root(&root);
            let missing = e.verify_runtime_root().expect_err("no root yet");
            assert_eq!(
                std::io::Error::from(missing).kind(),
                std::io::ErrorKind::NotFound
            );
            assert!(!root.exists(), "verify creates nothing");
            fs::create_dir(&root).expect("the root is created");
            fs::set_permissions(&root, fs::Permissions::from_mode(0o777)).expect("the mode is set");
            assert!(matches!(
                e.verify_runtime_root(),
                Err(PathsError::UnsafeRuntimeRoot {
                    problem: RootProblem::WrongMode { found: 0o777 },
                    ..
                })
            ));
            fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).expect("the mode is set");
            assert_eq!(e.verify_runtime_root().expect("a good root"), root);
        }

        #[test]
        fn the_parent_of_a_socket_is_checked_without_following_a_link() {
            let scratch = Scratch::new();
            let dir = scratch.0.join("session");
            fs::create_dir(&dir).expect("the directory is created");
            fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).expect("the mode is set");
            let socket = dir.join("link.sock");
            check_socket_parent(&socket).expect("a private directory");

            fs::set_permissions(&dir, fs::Permissions::from_mode(0o777)).expect("the mode is set");
            assert!(matches!(
                check_socket_parent(&socket),
                Err(PathsError::UnsafeSocketDir {
                    problem: RootProblem::WritableByOthers { found: 0o777 },
                    ..
                })
            ));
            fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).expect("the mode is set");

            let link = scratch.0.join("link");
            symlink(&dir, &link).expect("the link is created");
            assert!(matches!(
                check_socket_parent(&link.join("link.sock")),
                Err(PathsError::UnsafeSocketDir {
                    problem: RootProblem::Symlink,
                    ..
                })
            ));

            let gone = check_socket_parent(&scratch.0.join("nothing").join("link.sock"))
                .expect_err("no directory");
            assert_eq!(
                std::io::Error::from(gone).kind(),
                std::io::ErrorKind::NotFound
            );
        }

        #[test]
        fn ensure_refuses_a_root_that_belongs_to_another_user() {
            let scratch = Scratch::new();
            let root = scratch.0.join("run");
            let mut e = env_with_root(&root);
            e.ensure_runtime_root().expect("created");
            // The directory is ours. Ask for a check as another user.
            e.uid = e.uid.wrapping_add(1);
            assert!(matches!(
                e.ensure_runtime_root(),
                Err(PathsError::UnsafeRuntimeRoot {
                    problem: RootProblem::WrongOwner { .. },
                    ..
                })
            ));
        }
    }
}
