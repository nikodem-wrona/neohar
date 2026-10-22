//! The headless host: the core without a window (`nh-chaos host`).
//!
//! It starts a core over its own three directories (`NEOHAR_*_DIR`), opens the dev WebSocket,
//! writes where it listens to a file, and waits. The orchestrator kills it with `SIGKILL` and starts it again.

use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context, Result, bail};
use nh_core::{Core, CoreConfig, Tuning};
use nh_proto::paths::Env;

use crate::ws::HostInfo;

/// What the orchestrator tells the host.
pub struct HostArgs {
    pub info_file: PathBuf,
    pub nh: PathBuf,
    pub session: String,
    pub ring_bytes: u64,
    pub checkpoint_idle_ms: u64,
    pub checkpoint_interval_ms: u64,
}

/// Starts the core and waits for ever.
pub fn run(args: &HostArgs) -> Result<()> {
    let paths = Env::from_process();
    if !paths.has_overrides()
        || ["NEOHAR_CONFIG_DIR", "NEOHAR_DATA_DIR", "NEOHAR_RUNTIME_DIR"]
            .iter()
            .any(|name| std::env::var_os(name).is_none())
    {
        bail!("the host needs NEOHAR_CONFIG_DIR, NEOHAR_DATA_DIR and NEOHAR_RUNTIME_DIR");
    }
    let base_env: Vec<(String, String)> = ["PATH", "HOME", "SHELL", "LANG"]
        .iter()
        .filter_map(|name| std::env::var(name).ok().map(|v| ((*name).to_owned(), v)))
        .collect();
    let config = CoreConfig {
        session: args.session.clone(),
        paths,
        nh_path: args.nh.clone(),
        base_env,
        tuning: Tuning {
            ring_bytes: args.ring_bytes,
            checkpoint_idle: Duration::from_millis(args.checkpoint_idle_ms),
            checkpoint_interval: Duration::from_millis(args.checkpoint_interval_ms),
            ..Tuning::default()
        },
        host: Some(nh_core::InProcessHost::noop()),
        pane_tokens: Vec::new(),
    };
    let core = Core::start(config).context("the core does not start")?;
    let handle = core.handle();
    let ws = handle
        .dev_ws_serve()
        .context("the dev WebSocket does not start")?;
    write_info(
        &args.info_file,
        &HostInfo {
            pid: std::process::id(),
            port: ws.port,
            token: ws.token,
        },
    )?;
    // The core lives on its own threads. This thread only waits for the signal that ends it.
    loop {
        std::thread::sleep(Duration::from_secs(3600));
    }
}

/// Writes the file in one step, so that a reader never sees half of it.
fn write_info(path: &Path, info: &HostInfo) -> Result<()> {
    let temp = path.with_extension("tmp");
    std::fs::write(&temp, serde_json::to_vec(info)?)?;
    std::fs::rename(&temp, path)?;
    Ok(())
}
