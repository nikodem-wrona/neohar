//! Starting the child under a PTY.
//!
//! Everything the child needs is built before `fork`. Between `fork` and `exec` the child calls
//! only async-signal-safe functions and allocates nothing. If `exec` fails, the child writes its
//! `errno` to a close-on-exec pipe and exits, so the holder can report `ExecFailed`.

#![allow(unsafe_code)]

use std::{
    ffi::{CString, c_char},
    io,
    os::{
        fd::{AsRawFd, OwnedFd, RawFd},
        unix::ffi::OsStringExt,
    },
    path::Path,
    ptr,
};

use nh_proto::holder::{ExitStatus, HoldConfig};
use rustix::{
    fs::{Access, Mode, access},
    io::{FdFlags, fcntl_setfd, read},
    pipe::pipe,
    process::{Pid, WaitOptions, WaitStatus, waitpid},
};

use crate::Error;

/// Where the child's standard streams come from.
pub enum ChildIo<'a> {
    /// The slave side of a PTY. The child gets it as its controlling terminal.
    Pty(&'a OwnedFd),
    /// The ends of three pipes that the child gets as stdin, stdout and stderr.
    Pipes {
        stdin: &'a OwnedFd,
        stdout: &'a OwnedFd,
        stderr: &'a OwnedFd,
    },
}

/// The result of starting a child.
pub struct Spawned {
    pub pid: Pid,
    /// Set when `exec` failed. The holder has already collected the child.
    pub exec_failed: Option<ExitStatus>,
}

/// A pipe whose two ends close on `exec`.
pub fn cloexec_pipe() -> io::Result<(OwnedFd, OwnedFd)> {
    let (read_end, write_end) = pipe()?;
    fcntl_setfd(&read_end, FdFlags::CLOEXEC)?;
    fcntl_setfd(&write_end, FdFlags::CLOEXEC)?;
    Ok((read_end, write_end))
}

/// The command, environment and directory as C strings, ready for `execve`.
struct Exec {
    prog: Option<CString>,
    argv: Vec<*const c_char>,
    envp: Vec<*const c_char>,
    cwd: CString,
    /// The owners of the pointers in `argv` and `envp`.
    _strings: Vec<CString>,
}

/// Starts `config.argv` with `config.env` in `config.cwd`, with the file mode mask `umask`.
pub fn spawn(config: &HoldConfig, io: &ChildIo<'_>, umask: Mode) -> Result<Spawned, Error> {
    let exec = prepare(config)?;
    let (err_read, err_write) = cloexec_pipe().map_err(|e| Error::new("error pipe", e))?;
    let (stdio, ctty): ([RawFd; 3], Option<RawFd>) = match io {
        ChildIo::Pty(slave) => ([slave.as_raw_fd(); 3], Some(slave.as_raw_fd())),
        ChildIo::Pipes {
            stdin,
            stdout,
            stderr,
        } => (
            [stdin.as_raw_fd(), stdout.as_raw_fd(), stderr.as_raw_fd()],
            None,
        ),
    };
    // SAFETY: an all-zero `sigset_t` is a valid value, and `sigemptyset` then makes it empty.
    let mut no_signals: libc::sigset_t = unsafe { std::mem::zeroed() };
    // SAFETY: `no_signals` is valid for writes.
    unsafe { libc::sigemptyset(&mut no_signals) };

    // SAFETY: the holder has one thread at this point, so the child gets a consistent copy of the
    // process. The child runs `run_child` and never returns.
    let pid = unsafe { libc::fork() };
    match pid {
        -1 => Err(Error::new("fork", io::Error::last_os_error())),
        0 => run_child(
            &exec,
            &stdio,
            ctty,
            err_write.as_raw_fd(),
            &no_signals,
            umask.bits(),
        ),
        pid => {
            drop(err_write);
            let pid = Pid::from_raw(pid).ok_or_else(|| Error::new("fork", "pid 0"))?;
            let mut errno = [0u8; 4];
            let exec_failed = match read(&err_read, &mut errno) {
                Ok(4) => {
                    // The child exits at once. Collect it so that it is not a zombie.
                    let _ = waitpid(Some(pid), WaitOptions::empty());
                    Some(ExitStatus::ExecFailed(i32::from_ne_bytes(errno)))
                }
                _ => None,
            };
            Ok(Spawned { pid, exec_failed })
        }
    }
}

fn prepare(config: &HoldConfig) -> Result<Exec, Error> {
    let c = |s: &str| CString::new(s).map_err(|e| Error::new("command", e));
    let mut strings = Vec::new();
    for arg in &config.argv {
        strings.push(c(arg)?);
    }
    for (key, value) in &config.env {
        strings.push(c(&format!("{key}={value}"))?);
    }
    let (args, vars) = strings.split_at(config.argv.len());
    let pointers = |list: &[CString]| {
        list.iter()
            .map(|s| s.as_ptr())
            .chain([ptr::null()])
            .collect()
    };
    Ok(Exec {
        prog: resolve(&config.argv[0], &config.env),
        argv: pointers(args),
        envp: pointers(vars),
        cwd: c(&config.cwd)?,
        _strings: strings,
    })
}

/// Finds the program like `execvp` does, with the `PATH` of the child's environment.
fn resolve(argv0: &str, env: &[(String, String)]) -> Option<CString> {
    if argv0.contains('/') {
        return CString::new(argv0).ok();
    }
    let path = env
        .iter()
        .find(|(k, _)| k == "PATH")
        .map_or("/usr/local/bin:/usr/bin:/bin", |(_, v)| v.as_str());
    path.split(':')
        .map(|dir| Path::new(if dir.is_empty() { "." } else { dir }).join(argv0))
        .find(|p| {
            std::fs::metadata(p).is_ok_and(|m| m.is_file()) && access(p, Access::EXEC_OK).is_ok()
        })
        .and_then(|p| CString::new(p.into_os_string().into_vec()).ok())
}

/// The code of the child between `fork` and `exec`.
fn run_child(
    exec: &Exec,
    stdio: &[RawFd; 3],
    ctty: Option<RawFd>,
    err_fd: RawFd,
    no_signals: &libc::sigset_t,
    umask: libc::mode_t,
) -> ! {
    // SAFETY: this runs in the child of a single-threaded process. It calls only async-signal-safe
    // functions, it allocates nothing, and every pointer comes from `exec`, which `fork` copied.
    unsafe {
        libc::setsid();
        // Only a session leader can take a controlling terminal, so `setsid` comes first.
        if let Some(ctty) = ctty {
            libc::ioctl(ctty, libc::TIOCSCTTY as _, 0);
        }
        for (target, &fd) in stdio.iter().enumerate() {
            if libc::dup2(fd, target as libc::c_int) < 0 {
                fail(err_fd);
            }
        }
        // Every descriptor is close-on-exec. This loop is the fallback for one that is not.
        for fd in 3..1024 {
            if fd != err_fd {
                libc::close(fd);
            }
        }
        // An ignored signal stays ignored across `exec`. Give the child the default of each.
        for signal in 1..32 {
            libc::signal(signal, libc::SIG_DFL);
        }
        libc::sigprocmask(libc::SIG_SETMASK, no_signals, ptr::null_mut());
        // The holder runs with the mask 077 for its own files. The child gets the caller's mask.
        libc::umask(umask);
        if libc::chdir(exec.cwd.as_ptr()) != 0 {
            fail(err_fd);
        }
        match &exec.prog {
            Some(prog) => {
                libc::execve(prog.as_ptr(), exec.argv.as_ptr(), exec.envp.as_ptr());
            }
            None => *errno_location() = libc::ENOENT,
        }
        fail(err_fd)
    }
}

/// Writes `errno` to the error pipe and exits.
///
/// # Safety
/// Call it only in the child between `fork` and `exec`.
unsafe fn fail(err_fd: RawFd) -> ! {
    // SAFETY: the caller is in the child. `write` and `_exit` are async-signal-safe.
    unsafe {
        let errno = (*errno_location()).to_ne_bytes();
        libc::write(err_fd, errno.as_ptr().cast(), errno.len());
        libc::_exit(127)
    }
}

/// The address of this thread's `errno`.
///
/// # Safety
/// The pointer is valid for the calling thread only.
#[cfg(target_os = "macos")]
pub(crate) unsafe fn errno_location() -> *mut libc::c_int {
    // SAFETY: `__error` has no preconditions.
    unsafe { libc::__error() }
}

/// The address of this thread's `errno`.
///
/// # Safety
/// The pointer is valid for the calling thread only.
#[cfg(not(target_os = "macos"))]
pub(crate) unsafe fn errno_location() -> *mut libc::c_int {
    // SAFETY: `__errno_location` has no preconditions.
    unsafe { libc::__errno_location() }
}

/// How a collected child ended.
pub fn exit_status(status: WaitStatus) -> ExitStatus {
    match (status.exit_status(), status.terminating_signal()) {
        (Some(code), _) => ExitStatus::Code(code),
        (None, Some(signal)) => ExitStatus::Signal(signal),
        (None, None) => ExitStatus::Code(-1),
    }
}
