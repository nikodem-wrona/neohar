//! The system calls of the process walk on macOS: `proc_pidinfo` for the ids and the
//! start time, `proc_pidpath` for the executable, `proc_listchildpids` for the children, and
//! `sysctl(KERN_PROCARGS2)` for the command line and the environment. The table of all processes
//! adds `proc_listallpids` and `getsid`.
#![allow(unsafe_code)]

use std::{ffi::OsStr, os::unix::ffi::OsStrExt, path::PathBuf};

use super::{Args, Basic, Kin};

/// `SZOMB` of `sys/proc.h`.
const ZOMBIE: u32 = 5;

pub(super) fn basic(pid: i32) -> Option<Basic> {
    // SAFETY: `proc_bsdinfo` is a plain struct of integers and arrays of integers, so the all-zero
    // bytes are a value of it.
    let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
    let size = i32::try_from(std::mem::size_of::<libc::proc_bsdinfo>()).ok()?;
    // SAFETY: `info` is valid for `size` bytes and the call writes at most that many.
    let written = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDTBSDINFO,
            0,
            std::ptr::addr_of_mut!(info).cast(),
            size,
        )
    };
    if written != size {
        return None;
    }
    Some(Basic {
        ppid: i32::try_from(info.pbi_ppid).ok()?,
        pgid: i32::try_from(info.pbi_pgid).ok()?,
        start: info
            .pbi_start_tvsec
            .saturating_mul(1_000)
            .saturating_add(info.pbi_start_tvusec / 1_000),
        zombie: info.pbi_status == ZOMBIE,
    })
}

pub(super) fn exe(pid: i32) -> Option<PathBuf> {
    let mut buffer = vec![0_u8; usize::try_from(libc::PROC_PIDPATHINFO_MAXSIZE).ok()?];
    let capacity = u32::try_from(buffer.len()).ok()?;
    // SAFETY: `buffer` is valid for `capacity` bytes and the call writes at most that many.
    let length = unsafe { libc::proc_pidpath(pid, buffer.as_mut_ptr().cast(), capacity) };
    let length = usize::try_from(length).ok().filter(|length| *length > 0)?;
    buffer.truncate(length);
    Some(PathBuf::from(OsStr::from_bytes(&buffer)))
}

pub(super) fn cwd(pid: i32) -> Option<PathBuf> {
    // SAFETY: `proc_vnodepathinfo` is a plain struct of integers and arrays of integers, so the
    // all-zero bytes are a value of it.
    let mut info: libc::proc_vnodepathinfo = unsafe { std::mem::zeroed() };
    let size = i32::try_from(std::mem::size_of::<libc::proc_vnodepathinfo>()).ok()?;
    // SAFETY: `info` is valid for `size` bytes and the call writes at most that many.
    let written = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDVNODEPATHINFO,
            0,
            std::ptr::addr_of_mut!(info).cast(),
            size,
        )
    };
    if written != size {
        return None;
    }
    // The path ends at the first NUL of a buffer of `MAXPATHLEN` bytes.
    let path: Vec<u8> = info
        .pvi_cdir
        .vip_path
        .as_flattened()
        .iter()
        .map(|byte| u8::from_ne_bytes(byte.to_ne_bytes()))
        .take_while(|byte| *byte != 0)
        .collect();
    (!path.is_empty()).then(|| PathBuf::from(OsStr::from_bytes(&path)))
}

pub(super) fn children(pid: i32) -> Vec<i32> {
    // A list of 64 children is enough for a pane. A bigger list is cut, as the walk is bounded.
    // The buffer starts as zeros and the call fills the first entries, so the entries that are
    // not zero are the children. (The value that the call gives is a number of bytes on some
    // versions of the system, and a number of entries on others, so the walk does not use it.)
    let mut pids = [0_i32; 64];
    let size = i32::try_from(std::mem::size_of_val(&pids)).unwrap_or(0);
    // SAFETY: `pids` is valid for `size` bytes and the call writes at most that many.
    let written = unsafe { libc::proc_listchildpids(pid, pids.as_mut_ptr().cast(), size) };
    if written <= 0 {
        return Vec::new();
    }
    pids.into_iter().filter(|child| *child > 0).collect()
}

/// Every process that the system lists, with its parent and its session.
pub(super) fn all() -> Vec<Kin> {
    // SAFETY: with no buffer, the call only gives the number of processes.
    let count = unsafe { libc::proc_listallpids(std::ptr::null_mut(), 0) };
    let Ok(count) = usize::try_from(count) else {
        return Vec::new();
    };
    // Processes start between the two calls. The room for more keeps them in the list.
    let mut pids = vec![0_i32; count + 64];
    let Ok(size) = i32::try_from(std::mem::size_of_val(pids.as_slice())) else {
        return Vec::new();
    };
    // SAFETY: `pids` is valid for `size` bytes and the call writes at most that many.
    let written = unsafe { libc::proc_listallpids(pids.as_mut_ptr().cast(), size) };
    let Ok(written) = usize::try_from(written) else {
        return Vec::new();
    };
    pids.truncate(written.min(pids.len()));
    pids.into_iter()
        .filter(|pid| *pid > 0)
        .filter_map(|pid| {
            let basic = basic(pid)?;
            // SAFETY: `getsid` reads the session of a process and changes nothing.
            let sid = unsafe { libc::getsid(pid) };
            (sid >= 0).then_some(Kin {
                pid,
                ppid: basic.ppid,
                sid,
            })
        })
        .collect()
}

/// The command line and, when asked, the environment of the process.
pub(super) fn args(pid: i32, want_env: bool) -> Option<Args> {
    let raw = procargs(pid)?;
    parse_procargs(&raw, want_env)
}

fn procargs(pid: i32) -> Option<Vec<u8>> {
    let mut mib = [libc::CTL_KERN, libc::KERN_PROCARGS2, pid];
    let mut size: libc::size_t = 64 * 1024;
    let mut buffer = vec![0_u8; size];
    for _ in 0..2 {
        // SAFETY: `mib` has three entries; `buffer` is valid for `size` bytes; the call writes at
        // most `size` bytes and sets `size` to the number it wrote, or to the number it needs.
        let result = unsafe {
            libc::sysctl(
                mib.as_mut_ptr(),
                3,
                buffer.as_mut_ptr().cast(),
                std::ptr::addr_of_mut!(size),
                std::ptr::null_mut(),
                0,
            )
        };
        if result == 0 {
            buffer.truncate(size);
            return Some(buffer);
        }
        if std::io::Error::last_os_error().raw_os_error() != Some(libc::ENOMEM) {
            return None;
        }
        // The arguments are longer than the buffer. Ask for the size, and read again.
        size = max_args()?;
        buffer = vec![0_u8; size];
    }
    None
}

/// `kern.argmax`: the longest command line and environment together.
fn max_args() -> Option<usize> {
    let mut mib = [libc::CTL_KERN, libc::KERN_ARGMAX];
    let mut value: libc::c_int = 0;
    let mut size: libc::size_t = std::mem::size_of::<libc::c_int>();
    // SAFETY: `mib` has two entries and `value` is valid for `size` bytes.
    let result = unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            2,
            std::ptr::addr_of_mut!(value).cast(),
            std::ptr::addr_of_mut!(size),
            std::ptr::null_mut(),
            0,
        )
    };
    if result != 0 {
        return None;
    }
    usize::try_from(value).ok().filter(|value| *value > 0)
}

/// The layout of `KERN_PROCARGS2`: `argc` as an `int`, the path of the executable, NULs, `argc`
/// strings of arguments, and then the strings of the environment.
fn parse_procargs(raw: &[u8], want_env: bool) -> Option<Args> {
    let argc = usize::try_from(i32::from_ne_bytes(raw.get(..4)?.try_into().ok()?)).ok()?;
    let mut rest = raw.get(4..)?;
    // The path of the executable, then the NULs that pad it.
    let end = rest.iter().position(|byte| *byte == 0)?;
    rest = &rest[end..];
    let skip = rest
        .iter()
        .position(|byte| *byte != 0)
        .unwrap_or(rest.len());
    rest = &rest[skip..];
    let mut strings = rest.split(|byte| *byte == 0);
    let argv: Vec<String> = strings
        .by_ref()
        .take(argc)
        .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
        .collect();
    let env = want_env.then(|| {
        strings
            .filter(|bytes| !bytes.is_empty())
            .filter_map(|bytes| {
                let text = String::from_utf8_lossy(bytes);
                let (key, value) = text.split_once('=')?;
                Some((key.to_owned(), value.to_owned()))
            })
            .collect()
    });
    Some((argv, env))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_layout_of_the_arguments_is_read() {
        let mut raw = 3_i32.to_ne_bytes().to_vec();
        raw.extend_from_slice(b"/usr/local/bin/node\0\0\0\0");
        raw.extend_from_slice(b"node\0--flag\0script.js\0");
        raw.extend_from_slice(b"HOME=/Users/x\0CLAUDECODE=1\0\0");
        let (argv, env) = parse_procargs(&raw, true).expect("a layout");
        assert_eq!(argv, ["node", "--flag", "script.js"]);
        let env = env.expect("an environment");
        assert_eq!(env.get("CLAUDECODE").map(String::as_str), Some("1"));
        assert_eq!(env.len(), 2);
        let (_, none) = parse_procargs(&raw, false).expect("a layout");
        assert!(none.is_none());
        assert!(parse_procargs(&[1, 2], false).is_none());
    }
}
