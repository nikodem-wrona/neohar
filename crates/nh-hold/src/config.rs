//! The command line and the config that the core sends over `--config-fd`.

#![allow(unsafe_code)]

use std::{
    fs::File,
    os::fd::{FromRawFd, RawFd},
};

use nh_proto::{
    framing,
    holder::{HoldConfig, HoldMode},
    version::HOLDER_PROTO,
};
use ulid::Ulid;

use crate::Error;

/// The arguments of `nh hold`.
pub struct Args {
    pub pane: String,
    pub config_fd: RawFd,
}

/// Parses `--pane <ulid> --config-fd <n>`.
pub fn parse_args(args: &[String]) -> Result<Args, Error> {
    let (mut pane, mut fd) = (None, None);
    let mut it = args.iter();
    while let Some(flag) = it.next() {
        let value = it
            .next()
            .ok_or_else(|| Error::new("arguments", format!("{flag} needs a value")))?;
        match flag.as_str() {
            "--pane" => pane = Some(value.clone()),
            "--config-fd" => {
                fd = Some(
                    value
                        .parse::<RawFd>()
                        .map_err(|e| Error::new("--config-fd", e))?,
                );
            }
            other => return Err(Error::new("arguments", format!("unknown flag {other}"))),
        }
    }
    match (pane, fd) {
        (Some(pane), Some(config_fd)) => Ok(Args { pane, config_fd }),
        _ => Err(Error::new(
            "arguments",
            "usage: nh hold --pane <ulid> --config-fd <n>",
        )),
    }
}

/// Reads the one `HoldConfig` message from the config pipe and checks it. The pipe closes.
pub fn read_config(args: &Args) -> Result<HoldConfig, Error> {
    // SAFETY: the core passes this descriptor to the holder and nothing else in the process uses
    // it. The `File` takes ownership and closes it.
    let mut pipe = unsafe { File::from_raw_fd(args.config_fd) };
    let config: HoldConfig =
        framing::read_msg(&mut pipe).map_err(|e| Error::new("read config", e))?;
    check(&config, &args.pane)?;
    Ok(config)
}

fn check(config: &HoldConfig, pane: &str) -> Result<(), Error> {
    let bad = |what: &str| Err(Error::new("config", what));
    if config.proto != HOLDER_PROTO {
        return bad(&format!(
            "protocol {} is not holder/{HOLDER_PROTO}",
            config.proto
        ));
    }
    if Ulid::from_bytes(config.pane).to_string() != pane {
        return bad("the pane in the config is not the --pane argument");
    }
    if config.argv.is_empty()
        || config.argv.iter().any(|a| a.contains('\0'))
        || config.cwd.contains('\0')
    {
        return bad("the command is empty or has a NUL byte");
    }
    if config
        .env
        .iter()
        .any(|(k, v)| k.is_empty() || k.contains(['=', '\0']) || v.contains('\0'))
    {
        return bad("an environment entry is not valid");
    }
    if !(1..=1 << 30).contains(&config.ring_bytes) {
        return bad("ring_bytes must be between 1 and 1 GiB");
    }
    if let HoldMode::Pty { cols, rows, .. } = config.mode
        && (cols == 0 || rows == 0)
    {
        return bad("the PTY size is zero");
    }
    Ok(())
}
