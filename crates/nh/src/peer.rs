//! The checks of a client before and after it connects to a socket of a core: the directory of the
//! socket, and the user of the process that serves it.
//!
//! The runtime root can be in `/tmp`, where every user can make a file. Another user could serve a
//! fake core there and get the payloads of the hooks, the commands of the CLI or the bytes of a
//! link. The hook shim uses this module too, so each check is one system call: an `lstat` before
//! the connect and a `getpeereid` after it.

#![allow(unsafe_code)]

use std::{io, os::unix::net::UnixStream, path::Path};

/// The user id of the process at the other end of a connected socket.
#[cfg(target_os = "macos")]
pub fn peer_uid(stream: &UnixStream) -> io::Result<u32> {
    use std::os::fd::AsRawFd;

    let (mut uid, mut gid) = (0, 0);
    // SAFETY: the descriptor is an open socket for the life of `stream`, and both pointers are
    // valid for writes of their types.
    let rc = unsafe { libc::getpeereid(stream.as_raw_fd(), &mut uid, &mut gid) };
    if rc == 0 {
        Ok(uid)
    } else {
        Err(io::Error::last_os_error())
    }
}

/// The user id of the process at the other end of a connected socket.
#[cfg(not(target_os = "macos"))]
pub fn peer_uid(stream: &UnixStream) -> io::Result<u32> {
    Ok(rustix::net::sockopt::socket_peercred(stream)?.uid.as_raw())
}

/// Checks the directory of `socket` before a connect. A missing directory gives an error of kind
/// `NotFound`, as a missing socket does; a directory that another user can change gives
/// `PermissionDenied`.
pub fn before_connect(socket: &Path) -> io::Result<()> {
    nh_proto::paths::check_socket_parent(socket).map_err(io::Error::from)
}

/// Checks that a process of this user serves `socket`. An error is `PermissionDenied`: the client
/// must close the stream and send nothing.
pub fn after_connect(stream: &UnixStream, socket: &Path) -> io::Result<()> {
    nh_proto::paths::check_peer(socket, peer_uid(stream)?).map_err(io::Error::from)
}

/// Connects to `socket` with both checks.
pub fn connect(socket: &Path) -> io::Result<UnixStream> {
    before_connect(socket)?;
    let stream = UnixStream::connect(socket)?;
    after_connect(&stream, socket)?;
    Ok(stream)
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        os::unix::{fs::PermissionsExt, net::UnixListener},
        path::PathBuf,
    };

    use super::*;

    /// A short directory under the temp dir, removed on drop.
    struct Dir(PathBuf);

    impl Dir {
        fn new(mode: u32) -> Self {
            static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
            let path = std::env::temp_dir().join(format!(
                "nhp-{}-{}",
                std::process::id(),
                N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir(&path).expect("a directory");
            fs::set_permissions(&path, fs::Permissions::from_mode(mode)).expect("the mode");
            Self(path)
        }
    }

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn the_peer_of_a_socket_pair_is_this_user() {
        let (a, _b) = UnixStream::pair().expect("a socket pair");
        assert_eq!(
            peer_uid(&a).expect("the peer id"),
            rustix::process::getuid().as_raw()
        );
    }

    #[test]
    fn a_socket_in_a_private_directory_of_this_user_connects() {
        let dir = Dir::new(0o700);
        let socket = dir.0.join("api.sock");
        let _listener = UnixListener::bind(&socket).expect("a listener");
        connect(&socket).expect("the checks pass");
    }

    #[test]
    fn a_socket_in_a_directory_that_others_can_write_is_refused_before_the_connect() {
        let dir = Dir::new(0o777);
        let socket = dir.0.join("api.sock");
        let listener = UnixListener::bind(&socket).expect("a listener");
        listener.set_nonblocking(true).expect("non-blocking");
        let error = connect(&socket).expect_err("another user could have put it there");
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied, "{error}");
        assert!(
            error.to_string().contains("other users can write"),
            "{error}"
        );
        // The client did not connect at all.
        assert!(listener.accept().is_err());
    }

    #[test]
    fn a_missing_directory_is_no_core() {
        let dir = Dir::new(0o700);
        let error = connect(&dir.0.join("gone").join("api.sock")).expect_err("no directory");
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
    }
}
