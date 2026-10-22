//! Who may talk to the holder: the peer's user id and the per-holder secret.

#![allow(unsafe_code)]

use std::{io, os::unix::net::UnixStream};

/// The user id of the process on the other end of the socket.
#[cfg(target_os = "macos")]
pub fn peer_uid(stream: &UnixStream) -> io::Result<u32> {
    use std::os::fd::AsRawFd;

    let (mut uid, mut gid) = (0, 0);
    // SAFETY: the descriptor is a valid socket and both pointers are valid for writes.
    let rc = unsafe { libc::getpeereid(stream.as_raw_fd(), &mut uid, &mut gid) };
    if rc == 0 {
        Ok(uid)
    } else {
        Err(io::Error::last_os_error())
    }
}

/// The user id of the process on the other end of the socket.
#[cfg(not(target_os = "macos"))]
pub fn peer_uid(stream: &UnixStream) -> io::Result<u32> {
    Ok(rustix::net::sockopt::socket_peercred(stream)?.uid.as_raw())
}

/// Compares two secrets in time that does not depend on where they differ.
pub fn secrets_equal(a: &[u8; 32], b: &[u8; 32]) -> bool {
    std::hint::black_box(a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y))) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equal_secrets_match_and_any_difference_does_not() {
        let a = [7u8; 32];
        assert!(secrets_equal(&a, &[7; 32]));
        for i in 0..32 {
            let mut b = a;
            b[i] ^= 1;
            assert!(!secrets_equal(&a, &b), "byte {i}");
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
}
