//! `nh daemon --detach`: leave the caller as a holder does.
//!
//! The process forks twice with `setsid` between the forks, so the daemon is in a session of its
//! own, has no terminal and is not a child of the caller. The first process waits for one line on
//! an internal pipe and copies it to its own stdout, which is the ready pipe of the caller:
//! `READY <pid>` when the core serves `api.sock` and `link.sock`, or `ERROR <reason>`. It exits 0
//! after `READY` and 1 otherwise, so a caller can read the line or only the exit status. A daemon
//! that dies before its line closes the pipe, and the first process exits 1.
//!
//! The fork happens before the process starts a thread: the runtime of the core starts later.

#![allow(unsafe_code)]

use std::{
    fs::File,
    io::{self, BufRead, BufReader, Write},
    os::fd::OwnedFd,
};

use rustix::{
    io::{FdFlags, fcntl_setfd},
    pipe::pipe,
    process::{Pid, WaitOptions, setsid, waitpid},
};

/// The write end of the ready pipe, in the daemon.
pub struct Ready(Option<File>);

impl Ready {
    /// Tells the caller that the daemon serves its sockets, and closes the pipe.
    pub fn ready(&mut self, pid: u32) {
        self.line(&format!("READY {pid}"));
    }

    /// Tells the caller why the daemon does not start, and closes the pipe.
    pub fn error(&mut self, reason: &str) {
        let reason = reason.replace('\n', " ");
        self.line(&format!("ERROR {reason}"));
    }

    fn line(&mut self, line: &str) {
        if let Some(mut pipe) = self.0.take() {
            let _ = writeln!(pipe, "{line}");
        }
    }
}

/// Forks twice. Only the daemon returns. The first process copies the line of the daemon to its
/// stdout and exits; the middle process exits at once.
pub fn detach() -> io::Result<Ready> {
    let (read, write): (OwnedFd, OwnedFd) = pipe()?;
    // macOS has no `pipe2`. The process has one thread, so no `exec` can come in between. The
    // programs that the core starts later must not inherit the pipe.
    fcntl_setfd(&read, FdFlags::CLOEXEC)?;
    fcntl_setfd(&write, FdFlags::CLOEXEC)?;
    // SAFETY: the process has one thread here, so `fork` copies a consistent process. The first
    // process and the middle process leave with `exit` or `_exit` and touch nothing that a fork can
    // leave in a bad state.
    match unsafe { libc::fork() } {
        -1 => Err(io::Error::last_os_error()),
        0 => {
            drop(read);
            if setsid().is_err() {
                // SAFETY: `_exit` runs no destructors and is safe after `fork`.
                unsafe { libc::_exit(1) };
            }
            // SAFETY: as above, the process still has one thread.
            match unsafe { libc::fork() } {
                0 => Ok(Ready(Some(File::from(write)))),
                // SAFETY: `_exit` runs no destructors and is safe after `fork`.
                -1 => unsafe { libc::_exit(1) },
                // SAFETY: as above.
                _ => unsafe { libc::_exit(0) },
            }
        }
        middle => {
            drop(write);
            let status = Pid::from_raw(middle).map(|pid| waitpid(Some(pid), WaitOptions::empty()));
            let detached = matches!(status, Some(Ok(Some((_, s)))) if s.exit_status() == Some(0));
            let mut line = String::new();
            if detached {
                let _ = BufReader::new(File::from(read)).read_line(&mut line);
            } else {
                line = "ERROR cannot detach from the caller\n".to_owned();
            }
            let mut out = io::stdout().lock();
            let _ = out.write_all(line.as_bytes());
            let _ = out.flush();
            std::process::exit(if line.starts_with("READY ") { 0 } else { 1 });
        }
    }
}
