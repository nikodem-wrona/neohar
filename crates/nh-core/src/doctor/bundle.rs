//! The debug bundle (`nh debug bundle`).
//!
//! A zip of what a person needs to send with a bug report: the logs of the last days, the report
//! of the doctor, the versions, the configuration, statistics of the database (the number of rows
//! and the size of the files, never the content), and the last events. The text of the panes
//! comes only with `include_scrollback`. Every text passes through [`nh_redact`] before it goes in,
//! and the bundle says so in its README.

use std::{
    io::Write,
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    sync::Arc,
};

use nh_proto::api::calls::system::{CheckStatus, DebugBundled, DoctorReport};
use serde_json::{Value, json};

use super::zipfile::ZipWriter;
use crate::{ErrorCode, error::RpcError, persist::now_ms, runtime::Inner};

/// How many log files go in, the newest first.
const LOG_FILES: usize = 7;
/// How much of the end of a log file goes in.
const LOG_TAIL: u64 = 2 << 20;
/// How many events go in.
const EVENTS: usize = 1000;
/// How many lines of each pane go in with `include_scrollback`.
const PANE_LINES: u64 = 3000;

const README: &str = "This is a debug bundle of neohar.\n\
\n\
- doctor.txt, doctor.json: the checks of `nh doctor`.\n\
- versions.json: the versions of neohar and of the harnesses.\n\
- config.toml: your configuration.\n\
- db-stats.json: how many rows each table has, and the size of the files. No content.\n\
- events.jsonl: the last events of the log.\n\
- logs/: the logs of the last days.\n\
- scrollback/: the text of the panes. Only when you asked for it.\n\
\n\
Every text passed through the redaction of secrets (keys of services, tokens, private keys,\n\
passwords in pairs and in URLs). The redaction can miss a secret that has no mark. Read the files\n\
before you send them.\n";

fn internal(error: impl std::fmt::Display) -> RpcError {
    RpcError::internal(error.to_string())
}

/// A text with its secrets removed, as the bytes of a file.
fn clean(text: &str) -> Vec<u8> {
    nh_redact::redact(text).into_owned().into_bytes()
}

/// The text of the report for a person.
pub(crate) fn report_text(report: &DoctorReport) -> String {
    let mut out = String::new();
    for check in &report.checks {
        let mark = match check.status {
            CheckStatus::Ok => "ok  ",
            CheckStatus::Info => "info",
            CheckStatus::Warn => "warn",
            CheckStatus::Fail => "FAIL",
        };
        out.push_str(&format!("{mark}  {}: {}\n", check.title, check.detail));
        if let Some(hint) = &check.hint {
            out.push_str(&format!("      {hint}\n"));
        }
    }
    out
}

/// The end of a file, as text. A cut at the start of a line keeps whole lines.
fn tail(path: &Path, limit: u64) -> Option<String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = std::fs::File::open(path).ok()?;
    let size = file.metadata().ok()?.len();
    let start = size.saturating_sub(limit);
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).ok()?;
    let text = String::from_utf8_lossy(&bytes).into_owned();
    Some(if start > 0 {
        text.split_once('\n')
            .map_or(String::new(), |(_, rest)| rest.to_owned())
    } else {
        text
    })
}

/// The newest log files of the core.
fn logs(dir: &Path) -> Vec<(String, String)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|t| t.is_file()))
        .filter_map(|entry| Some((entry.metadata().ok()?.modified().ok()?, entry.path())))
        .collect();
    files.sort_by(|a, b| b.cmp(a));
    files
        .into_iter()
        .take(LOG_FILES)
        .filter_map(|(_, path)| {
            let name = path.file_name()?.to_string_lossy().into_owned();
            Some((name, tail(&path, LOG_TAIL)?))
        })
        .collect()
}

fn file_sizes(dir: &Path) -> Value {
    let mut sizes = serde_json::Map::new();
    for name in ["state.db", "state.db-wal", "state.db-shm"] {
        if let Ok(meta) = std::fs::metadata(dir.join(name)) {
            sizes.insert(name.to_owned(), json!(meta.len()));
        }
    }
    Value::Object(sizes)
}

/// Where the bundle goes when the call names no file.
fn default_path(data_dir: &Path) -> PathBuf {
    data_dir.join(format!("neohar-debug-{}.zip", now_ms()))
}

pub(crate) async fn write(
    inner: &Arc<Inner>,
    out: Option<&str>,
    include_scrollback: bool,
) -> Result<DebugBundled, RpcError> {
    let factory = &inner.factory;
    let data_dir = factory.paths.data_dir(&factory.session).map_err(internal)?;
    let path = out.map_or_else(|| default_path(&data_dir), PathBuf::from);
    let report = super::report(inner).await;

    let (events, tables) = inner
        .actor
        .store(|store| Ok((store.recent_events(EVENTS)?, store.table_counts()?)))
        .await
        .map_err(internal)?;
    let versions = inner
        .actor
        .update(|parts| {
            parts
                .state
                .runs
                .by_id
                .values()
                .filter(|run| run.live())
                .map(|run| {
                    json!({
                        "harness": nh_agents::harness::name(run.harness),
                        "version": run.version.as_ref().map(ToString::to_string),
                    })
                })
                .collect::<Vec<_>>()
        })
        .await
        .map_err(internal)?;

    let mut files: Vec<(String, Vec<u8>)> = vec![
        ("README.txt".to_owned(), README.as_bytes().to_vec()),
        ("doctor.txt".to_owned(), clean(&report_text(&report))),
    ];
    let mut doctor = serde_json::to_value(&report).map_err(internal)?;
    nh_redact::redact_value(&mut doctor);
    files.push((
        "doctor.json".to_owned(),
        serde_json::to_vec_pretty(&doctor).map_err(internal)?,
    ));
    let mut versions = json!({
        "neohar": env!("CARGO_PKG_VERSION"),
        "session": factory.session,
        "nh": factory.nh_path.display().to_string(),
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "running": versions,
    });
    nh_redact::redact_value(&mut versions);
    files.push((
        "versions.json".to_owned(),
        serde_json::to_vec_pretty(&versions).map_err(internal)?,
    ));
    if let Ok(dir) = factory.paths.config_dir()
        && let Ok(text) = std::fs::read_to_string(dir.join("config.toml"))
    {
        files.push(("config.toml".to_owned(), clean(&text)));
    }
    let stats = json!({
        "tables": tables.iter().map(|(name, rows)| json!({"name": name, "rows": rows})).collect::<Vec<_>>(),
        "files": file_sizes(&data_dir),
    });
    files.push((
        "db-stats.json".to_owned(),
        serde_json::to_vec_pretty(&stats).map_err(internal)?,
    ));
    let mut lines = String::new();
    for event in &events {
        let mut line = serde_json::to_value(event).map_err(internal)?;
        nh_redact::redact_value(&mut line);
        lines.push_str(&line.to_string());
        lines.push('\n');
    }
    files.push(("events.jsonl".to_owned(), lines.into_bytes()));
    for (name, text) in logs(&data_dir.join("logs")) {
        files.push((format!("logs/{name}"), clean(&text)));
    }
    if include_scrollback {
        let handles = inner
            .actor
            .update(|parts| {
                parts
                    .state
                    .layout
                    .panes
                    .values()
                    .map(|pane| (pane.id.ulid(), pane.handle.clone()))
                    .collect::<Vec<_>>()
            })
            .await
            .map_err(internal)?;
        for (id, handle) in handles {
            let Some(pane) = crate::pane::lock(&inner.panes).get(&id).cloned() else {
                continue;
            };
            let total = pane.viewport().total_lines;
            let (text, _) = pane
                .read_lines(total.saturating_sub(PANE_LINES), total)
                .await?;
            files.push((
                format!("scrollback/{}.txt", handle.replace(':', "-")),
                clean(&text),
            ));
        }
    }

    let mut zip = ZipWriter::new(Vec::new());
    for (name, bytes) in &files {
        zip.add(name, bytes).map_err(internal)?;
    }
    let (bytes, written) = zip.finish().map_err(internal)?;
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(internal)?;
    }
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
        .map_err(|e| {
            RpcError::new(
                ErrorCode::IoError,
                format!("cannot write {}: {e}", path.display()),
            )
        })?;
    file.write_all(&bytes).map_err(internal)?;
    Ok(DebugBundled {
        path: path.display().to_string(),
        bytes: written,
        files: u32::try_from(files.len()).unwrap_or(u32::MAX),
    })
}
