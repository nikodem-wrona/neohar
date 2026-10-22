//! What the host tells the core when it starts it: directories, the sidecar and the environment.

use std::path::PathBuf;

use nh_core::CoreConfig;
use nh_proto::paths::Env;

/// The variables that move the three roots of the directories.
const ROOT_VARIABLES: [&str; 3] = ["NEOHAR_CONFIG_DIR", "NEOHAR_DATA_DIR", "NEOHAR_RUNTIME_DIR"];

/// Logging. A debug build writes to stderr. A product build writes JSON lines to
/// `<data>/logs/core.log`, a new file each day, and keeps the last seven. `nh daemon`
/// shares the setup ([`nh_core::logging`]).
pub fn init_logging(config: &CoreConfig) {
    let to = if cfg!(debug_assertions) {
        nh_core::logging::LogTo::Stderr
    } else {
        nh_core::logging::LogTo::File
    };
    nh_core::logging::init(&config.paths, &config.session, to);
}

/// The configuration of the core.
///
/// A debug build refuses to run unless all three directory variables are set, so that a
/// development run never touches the real config, data and sockets. It uses the session
/// `dev`, and it never creates or changes `~/.local/bin/nh`.
pub fn core_config() -> Result<CoreConfig, String> {
    if cfg!(debug_assertions) {
        let missing = missing_roots(|name| std::env::var_os(name));
        if !missing.is_empty() {
            return Err(format!(
                "a debug build needs {missing:?}. Run it with `mise run app:dev`"
            ));
        }
    }
    let session = std::env::var("NEOHAR_SESSION").unwrap_or_else(|_| {
        if cfg!(debug_assertions) {
            "dev"
        } else {
            "default"
        }
        .to_owned()
    });
    Ok(CoreConfig {
        session,
        paths: Env::from_process(),
        nh_path: sidecar()?,
        base_env: nh_core::login_env::capture(),
        tuning: nh_core::Tuning::default(),
        // The app puts its own host and its device here, after the configuration is made
        // (`crate::host`).
        host: None,
        pane_tokens: Vec::new(),
    })
}

/// The client device of this app: the id in `<data>/device-id`, made at the
/// first launch with mode 0600, and the host name.
pub fn device(config: &CoreConfig) -> Result<nh_core::ClientInfo, String> {
    let dir = config
        .paths
        .data_dir(&config.session)
        .map_err(|error| format!("no data directory: {error}"))?;
    nh_core::device::this_device(&dir)
        .map_err(|error| format!("cannot keep the device id in {}: {error}", dir.display()))
}

/// Whether this run keeps `~/.local/bin/nh` pointing at the sidecar.
///
/// Only a product build does, and only when it started without the directory overrides and without
/// `NEOHAR_NH`. A debug build, a test and a run with another `nh` must never touch the real
/// symlink of the user.
fn manages_symlink(debug_build: bool, paths: &Env, nh_overridden: bool) -> bool {
    !debug_build && !paths.has_overrides() && !nh_overridden
}

/// Makes `~/.local/bin/nh` a symlink to the sidecar when this run should.
///
/// A path that holds a file or a directory of the user is left alone and logged as a warning.
/// The log is the record, and `nh doctor` checks the symlink.
pub fn link_sidecar(config: &CoreConfig) {
    use nh_core::sidecar::{LinkOutcome, ensure_symlink};

    let nh_overridden = std::env::var_os("NEOHAR_NH").is_some();
    if !manages_symlink(cfg!(debug_assertions), &config.paths, nh_overridden) {
        return;
    }
    let link = match config.paths.nh_symlink() {
        Ok(link) => link,
        Err(error) => {
            tracing::warn!(%error, "no ~/.local/bin/nh: the home directory is unknown");
            return;
        }
    };
    match ensure_symlink(&link, &config.nh_path) {
        Ok(LinkOutcome::Created) => tracing::info!(link = %link.display(), "made the nh symlink"),
        Ok(LinkOutcome::Replaced { previous }) => tracing::info!(
            link = %link.display(),
            previous = %previous.display(),
            "pointed the nh symlink at this app"
        ),
        Ok(LinkOutcome::Unchanged) => {}
        Ok(LinkOutcome::Skipped(reason)) => tracing::warn!(
            link = %link.display(),
            target = %config.nh_path.display(),
            ?reason,
            "health: the nh symlink is not in place, so agent hooks cannot find nh"
        ),
        Err(error) => tracing::warn!(
            link = %link.display(),
            %error,
            "health: cannot make the nh symlink"
        ),
    }
}

/// The `nh` binary: `NEOHAR_NH`, else the file next to this executable. In development that is
/// `target/debug/nh`. A bundled app keeps it in `Contents/MacOS`.
fn sidecar() -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os("NEOHAR_NH") {
        return Ok(PathBuf::from(path));
    }
    let exe = std::env::current_exe().map_err(|e| format!("cannot find this executable: {e}"))?;
    let dir = exe
        .parent()
        .ok_or_else(|| "this executable has no directory".to_owned())?;
    Ok(dir.join("nh"))
}

/// The directory variables that `get` does not set, or sets to nothing.
fn missing_roots(get: impl Fn(&str) -> Option<std::ffi::OsString>) -> Vec<&'static str> {
    ROOT_VARIABLES
        .into_iter()
        .filter(|name| get(name).is_none_or(|value| value.is_empty()))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::*;

    fn env<'a>(set: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<OsString> + 'a {
        move |name| {
            set.iter()
                .find(|(k, _)| *k == name)
                .map(|(_, v)| OsString::from(v))
        }
    }

    #[test]
    fn a_development_run_needs_all_three_roots() {
        let all = [
            ("NEOHAR_CONFIG_DIR", "/tmp/c"),
            ("NEOHAR_DATA_DIR", "/tmp/d"),
            ("NEOHAR_RUNTIME_DIR", "/tmp/r"),
        ];
        assert!(missing_roots(env(&all)).is_empty());
        assert_eq!(
            missing_roots(env(&all[..2])),
            ["NEOHAR_RUNTIME_DIR"],
            "the runtime root is missing"
        );
        assert_eq!(missing_roots(env(&[])), ROOT_VARIABLES);
    }

    #[test]
    fn only_a_product_build_without_overrides_manages_the_symlink() {
        let plain = Env::from_process();
        let plain = Env {
            config_override: None,
            data_override: None,
            runtime_override: None,
            ..plain
        };
        assert!(manages_symlink(false, &plain, false));
        assert!(!manages_symlink(true, &plain, false), "a debug build");
        assert!(!manages_symlink(false, &plain, true), "NEOHAR_NH is set");
        for with in [
            Env {
                config_override: Some("/c".into()),
                ..plain.clone()
            },
            Env {
                data_override: Some("/d".into()),
                ..plain.clone()
            },
            Env {
                runtime_override: Some("/r".into()),
                ..plain.clone()
            },
        ] {
            assert!(!manages_symlink(false, &with, false), "an override is set");
        }
    }

    #[test]
    fn an_empty_root_counts_as_missing() {
        let set = [
            ("NEOHAR_CONFIG_DIR", ""),
            ("NEOHAR_DATA_DIR", "/tmp/d"),
            ("NEOHAR_RUNTIME_DIR", "/tmp/r"),
        ];
        assert_eq!(missing_roots(env(&set)), ["NEOHAR_CONFIG_DIR"]);
    }
}
