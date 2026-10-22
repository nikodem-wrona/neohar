//! Opening a PTY and working with its master.

#![allow(unsafe_code)]

use std::{
    io,
    os::fd::{AsRawFd, OwnedFd},
};

use rustix::{
    fs::{self, Mode, OFlags},
    io::{FdFlags, fcntl_setfd},
    pty::{self, OpenptFlags},
    termios::{self, Winsize},
};

/// A PTY master and slave. The holder keeps the master. The child gets the slave.
pub struct Pty {
    pub master: OwnedFd,
    pub slave: OwnedFd,
}

/// The window size of a PTY, in the units of `TIOCSWINSZ`.
pub fn winsize(cols: u16, rows: u16, px_width: u16, px_height: u16) -> Winsize {
    Winsize {
        ws_row: rows,
        ws_col: cols,
        ws_xpixel: px_width,
        ws_ypixel: px_height,
    }
}

/// Opens a PTY pair of the given size. Both descriptors close on `exec`. The master does not
/// block.
pub fn open(size: Winsize) -> io::Result<Pty> {
    // `O_CLOEXEC` is not accepted by `posix_openpt` on every system, so set the flag afterwards.
    let master = pty::openpt(OpenptFlags::RDWR | OpenptFlags::NOCTTY)?;
    pty::grantpt(&master)?;
    pty::unlockpt(&master)?;
    fcntl_setfd(&master, FdFlags::CLOEXEC)?;
    fs::fcntl_setfl(&master, OFlags::NONBLOCK)?;
    let name = pty::ptsname(&master, Vec::new())?;
    let slave = fs::open(
        name.as_c_str(),
        OFlags::RDWR | OFlags::NOCTTY | OFlags::CLOEXEC,
        Mode::empty(),
    )?;
    termios::tcsetwinsize(&master, size)?;
    Ok(Pty { master, slave })
}

/// The foreground process group of the PTY, if it has one.
///
/// This calls `libc` and not `rustix`, because `rustix` assumes that the group is a valid process
/// id. A PTY without a foreground group gives zero or a negative number.
pub fn foreground_pgid(master: &OwnedFd) -> Option<i32> {
    // SAFETY: the descriptor is open for the whole call, and `tcgetpgrp` only reads.
    let pgid = unsafe { libc::tcgetpgrp(master.as_raw_fd()) };
    (pgid > 0).then_some(pgid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_pty_has_the_requested_size_and_no_foreground_group() {
        let pty = open(winsize(100, 30, 0, 0)).expect("a PTY opens");
        let size = termios::tcgetwinsize(&pty.slave).expect("the size is readable");
        assert_eq!((size.ws_col, size.ws_row), (100, 30));
        // Nothing has a controlling terminal on this PTY yet.
        assert_eq!(foreground_pgid(&pty.master), None);
    }
}
