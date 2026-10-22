//! The log of a host of the core. Both hosts use it: the app and `nh daemon`.
//!
//! A file log is JSON lines in `<data>/logs/core.log`, a new file each day, and the last seven are
//! kept. Each line loses its secrets before it reaches the file or the terminal.
//! `NEOHAR_LOG` is an `EnvFilter`, for example `NEOHAR_LOG=nh_core=debug`.

use std::path::PathBuf;

use nh_proto::paths::Env;
use tracing_subscriber::{EnvFilter, fmt::MakeWriter};

/// A logger writer whose lines lose their secrets on the way.
pub struct Redacting<M>(pub M);

impl<'a, M: MakeWriter<'a>> MakeWriter<'a> for Redacting<M> {
    type Writer = crate::redact::RedactWriter<M::Writer>;

    fn make_writer(&'a self) -> Self::Writer {
        crate::redact::RedactWriter::new(self.0.make_writer())
    }
}

/// Where the log goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogTo {
    /// Plain lines on stderr. The level is `warn` in a debug build and `info` in a release.
    Stderr,
    /// JSON lines in `<data>/logs/core.log`, at the level `info`. When the directory cannot be
    /// made, the log goes to stderr.
    File,
}

/// Starts the log of the process. Only the first call counts. Returns the directory of the file
/// log, or `None` when the log goes to stderr.
pub fn init(paths: &Env, session: &str, to: LogTo) -> Option<PathBuf> {
    let default = if to == LogTo::File || !cfg!(debug_assertions) {
        "info"
    } else {
        "warn"
    };
    let filter = EnvFilter::try_from_env("NEOHAR_LOG").unwrap_or_else(|_| EnvFilter::new(default));
    if to == LogTo::File
        && let Some((dir, writer)) = log_file(paths, session)
    {
        let _ = tracing_subscriber::fmt()
            .json()
            .with_env_filter(filter)
            .with_writer(Redacting(writer))
            .with_ansi(false)
            .try_init();
        return Some(dir);
    }
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(Redacting(std::io::stderr))
        .try_init();
    None
}

/// The daily file of the log and its directory, or `None` when the directory cannot be made.
fn log_file(
    paths: &Env,
    session: &str,
) -> Option<(PathBuf, tracing_appender::rolling::RollingFileAppender)> {
    let dir = paths.data_dir(session).ok()?.join("logs");
    std::fs::create_dir_all(&dir).ok()?;
    let writer = tracing_appender::rolling::Builder::new()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix("core")
        .filename_suffix("log")
        .max_log_files(7)
        .build(&dir)
        .ok()?;
    Some((dir, writer))
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex, PoisonError};

    use super::*;

    /// A writer that keeps what it gets, for a test of the logger.
    #[derive(Clone, Default)]
    struct Shared(Arc<Mutex<Vec<u8>>>);

    impl std::io::Write for Shared {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl<'a> MakeWriter<'a> for Shared {
        type Writer = Shared;

        fn make_writer(&'a self) -> Shared {
            self.clone()
        }
    }

    #[test]
    fn a_logged_secret_does_not_reach_the_log() {
        let sink = Shared::default();
        let subscriber = tracing_subscriber::fmt()
            .json()
            .with_writer(Redacting(sink.clone()))
            .finish();
        let key = format!("{}{}", "sk-ant-api03-", "x9".repeat(20));
        tracing::subscriber::with_default(subscriber, || {
            tracing::warn!(command = %format!("curl -H 'x-api-key: {key}'"), "a request");
            tracing::info!(password = "hunter2hunter2", "a login");
        });
        let text = String::from_utf8(
            sink.0
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone(),
        )
        .expect("text");
        assert!(!text.contains(&key) && !text.contains("hunter2"), "{text}");
        assert!(
            text.contains("a request") && text.contains("a login"),
            "{text}"
        );
    }
}
