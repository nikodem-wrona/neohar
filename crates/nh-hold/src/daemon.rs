//! Starting the holder: read the config, bind the socket, detach from the caller.
//!
//! The core runs `nh hold --pane <ulid> --config-fd 3` with stdin on `/dev/null`, stdout on a
//! ready pipe and descriptor 3 on a config pipe. The holder binds its socket before it forks, so
//! "ready" means "listening". It then forks twice and calls `setsid`, so that it is no longer
//! tied to the app. The last process writes `READY <pid>` to the ready pipe.

#![allow(unsafe_code)]

use std::{
    fs::{File, OpenOptions},
    io::{self, Write},
    os::fd::AsFd,
};

use rustix::{
    fs::Mode,
    io::fcntl_dupfd_cloexec,
    process::{Pid, WaitOptions, chdir, getuid, setsid, umask, waitpid},
    stdio::{dup2_stderr, dup2_stdin, dup2_stdout},
};

use crate::{Error, config, event_loop::Holder, log::Log, server};

/// Runs the holder from `nh hold`. It returns when the holder exits.
pub fn run(args: &[String]) -> Result<(), Error> {
    let args = config::parse_args(args)?;
    let mut ready = take_ready_pipe()?;
    match start(&args, &mut ready) {
        Ok(holder) => holder.run(ready),
        Err(e) => {
            let _ = writeln!(ready, "ERROR {e}");
            Err(e)
        }
    }
}

/// Moves the ready pipe off descriptor 1, which later becomes `/dev/null`.
fn take_ready_pipe() -> Result<File, Error> {
    let fd =
        fcntl_dupfd_cloexec(io::stdout().as_fd(), 10).map_err(|e| Error::new("ready pipe", e))?;
    Ok(File::from(fd))
}

fn start(args: &config::Args, ready: &mut File) -> Result<Holder, Error> {
    let config = config::read_config(args)?;
    let uid = getuid().as_raw();
    // Files and directories that the holder creates belong to its user only.
    umask(Mode::from_bits_truncate(0o077));
    let (listener, socket_inode) = server::bind(&config.socket_path, uid)?;
    detach(ready)?;
    chdir("/").map_err(|e| Error::new("chdir", e))?;
    let null = OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/null")
        .map_err(|e| Error::new("/dev/null", e))?;
    let log_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&config.log_path)
        .map_err(|e| Error::new("log file", e))?;
    dup2_stdin(&null)
        .and_then(|()| dup2_stdout(&null))
        .and_then(|()| dup2_stderr(&log_file))
        .map_err(|e| Error::new("stdio", e))?;
    let log = Log::open(&config.log_path);
    Holder::start(config, (listener, socket_inode), log, uid)
}

/// Forks twice with `setsid` between the forks. Only the last process returns.
fn detach(ready: &mut File) -> Result<(), Error> {
    // SAFETY: the holder has one thread, so `fork` copies a consistent process. The first process
    // and the middle process leave with `_exit`, which runs no destructors.
    unsafe {
        match libc::fork() {
            -1 => Err(Error::new("fork", io::Error::last_os_error())),
            0 => {
                if setsid().is_err() {
                    libc::_exit(1);
                }
                match libc::fork() {
                    0 => Ok(()),
                    -1 => libc::_exit(1),
                    _ => libc::_exit(0),
                }
            }
            middle => {
                let status =
                    Pid::from_raw(middle).map(|pid| waitpid(Some(pid), WaitOptions::empty()));
                if !matches!(status, Some(Ok(Some((_, s)))) if s.exit_status() == Some(0)) {
                    let _ = writeln!(ready, "ERROR cannot detach from the caller");
                    libc::_exit(1);
                }
                libc::_exit(0)
            }
        }
    }
}
