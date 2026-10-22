//! The holder's log: plain lines in one file, with one rotation. No `tracing` (`README.md`).

use std::{
    fmt,
    fs::{self, File, OpenOptions},
    io::Write,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

/// A rotated file is at most this many bytes.
const MAX_BYTES: u64 = 1 << 20;

/// A log file that rotates once at 1 MiB. A failed write is dropped: logging never stops the holder.
pub struct Log {
    path: PathBuf,
    file: Option<File>,
    size: u64,
}

impl Log {
    /// Opens `path` for appending. An empty path or an open error gives a log that drops lines.
    pub fn open(path: &str) -> Self {
        let path = PathBuf::from(path);
        let file = open_append(&path);
        let size = file
            .as_ref()
            .and_then(|f| f.metadata().ok())
            .map_or(0, |m| m.len());
        Self { path, file, size }
    }

    /// Appends one line with a Unix-millisecond timestamp.
    pub fn line(&mut self, args: fmt::Arguments<'_>) {
        if self.size >= MAX_BYTES {
            self.rotate();
        }
        let ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_millis());
        let line = format!("{ms} {args}\n");
        if let Some(file) = &mut self.file
            && file.write_all(line.as_bytes()).is_ok()
        {
            self.size += line.len() as u64;
        }
    }

    fn rotate(&mut self) {
        let mut old = self.path.clone().into_os_string();
        old.push(".1");
        let _ = fs::rename(&self.path, old);
        self.file = open_append(&self.path);
        self.size = 0;
    }
}

fn open_append(path: &PathBuf) -> Option<File> {
    OpenOptions::new().create(true).append(true).open(path).ok()
}

/// `hlog!(log, "text {}", value)`: appends one line to a [`Log`].
#[macro_export]
macro_rules! hlog {
    ($log:expr, $($arg:tt)*) => { $log.line(format_args!($($arg)*)) };
}
