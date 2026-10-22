//! Codex.

pub mod hooks;
pub mod models;

pub use hooks::CodexHooks;

/// The flag of Codex that keeps the app-server in the process of the interface.
const NO_DAEMON: &str = "--no-daemon";

/// Whether the arguments of a Codex process keep it off the shared app-server daemon:
/// `--no-daemon`, which the shim of neohar adds. `--disable daemon_auto_start`
/// is not enough: Codex 0.163 does not start the daemon then, but it attaches to a daemon that
/// runs already. On the daemon the hooks run in the environment of the daemon, not in the pane.
pub fn daemon_off(argv: &[String]) -> bool {
    argv.iter().skip(1).any(|arg| arg == NO_DAEMON)
}

#[cfg(test)]
mod tests {
    use super::daemon_off;

    fn argv(args: &[&str]) -> Vec<String> {
        std::iter::once("codex")
            .chain(args.iter().copied())
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn the_flag_of_the_shim_turns_the_daemon_off() {
        for args in [
            &["--disable", "daemon_auto_start", "--no-daemon"][..],
            &["--no-daemon"],
            &["--model", "x", "--no-daemon", "resume"],
            &["resume", "--no-daemon", "--last"],
        ] {
            assert!(daemon_off(&argv(args)), "{args:?}");
        }
    }

    #[test]
    fn the_feature_off_alone_leaves_the_daemon_on() {
        for args in [
            &[][..],
            &["resume", "--last"],
            // Codex attaches to a daemon that runs already.
            &["--disable", "daemon_auto_start"],
            &["-c", "features.daemon_auto_start=false"],
            &["--enable", "daemon_auto_start"],
            // A prompt that names the flag is not the flag.
            &["--no-daemon please"],
        ] {
            assert!(!daemon_off(&argv(args)), "{args:?}");
        }
        // The name of the program is not an argument.
        assert!(!daemon_off(&["--no-daemon".to_owned()]));
    }
}
