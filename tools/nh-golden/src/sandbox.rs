//! The macOS sandbox profile of a recording.
//!
//! The harness runs under `sandbox-exec -f <profile>`. The profile makes the isolation a fact that
//! the operating system enforces, not a hope about environment variables:
//!
//! | Rule | Effect |
//! |---|---|
//! | `deny network-outbound`, then allow `localhost:*` and Unix sockets | The harness can reach the mock model and the API socket, and nothing else |
//! | `deny file-write*`, then allow the scratch tree and `/dev` | The harness cannot write outside the scratch tree |
//! | `deny file-read* file-write*` on the real harness directories | The harness cannot read or change the real `~/.claude`, `~/.claude.json` or `~/.codex`, so no recording depends on them |
//!
//! SBPL uses the last rule that matches, so the denies for the real directories come last.
//!
//! A denied write does not print anything. The harness fails or ignores it. To see denials while
//! you debug a recording: `log stream --style compact --predicate 'sender == "Sandbox"'`.

use std::path::{Path, PathBuf};

/// The system tool that applies a profile.
pub const SANDBOX_EXEC: &str = "/usr/bin/sandbox-exec";

/// Whether this machine can run a recording. The profile needs macOS.
pub fn available() -> bool {
    cfg!(target_os = "macos") && Path::new(SANDBOX_EXEC).is_file()
}

/// A path as an SBPL string.
fn quote(path: &Path) -> String {
    let text = path.to_string_lossy();
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        if c == '"' || c == '\\' {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('"');
    out
}

/// The real places where the harnesses keep their configuration and state, under `home`.
pub fn real_harness_paths(home: &Path) -> Vec<PathBuf> {
    vec![
        home.join(".claude"),
        home.join(".claude.json"),
        home.join(".codex"),
    ]
}

/// The text of the profile. `writable` is the scratch tree (a path with symbolic links resolved:
/// the sandbox compares real paths). `extra_writable` are more places where writes are allowed,
/// which a scenario asks for. `hidden` are the paths that the harness must not touch.
pub fn profile(writable: &Path, extra_writable: &[PathBuf], hidden: &[PathBuf]) -> String {
    let mut text = String::from(
        "(version 1)\n\
         (allow default)\n\
         (deny network-outbound)\n\
         (allow network-outbound (remote ip \"localhost:*\"))\n\
         (allow network-outbound (remote unix-socket))\n\
         (deny file-write*)\n",
    );
    text.push_str(&format!(
        "(allow file-write* (subpath {}) (subpath \"/dev\"))\n",
        quote(writable)
    ));
    for path in extra_writable {
        text.push_str(&format!("(allow file-write* (subpath {}))\n", quote(path)));
    }
    for path in hidden {
        text.push_str(&format!(
            "(deny file-read* file-write* (subpath {}))\n",
            quote(path)
        ));
    }
    text
}

/// The command that runs `argv` under the profile in the file `profile`.
pub fn wrap(profile: &Path, argv: &[String]) -> Vec<String> {
    let mut wrapped = vec![
        SANDBOX_EXEC.to_owned(),
        "-f".to_owned(),
        profile.display().to_string(),
    ];
    wrapped.extend(argv.iter().cloned());
    wrapped
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_profile_denies_what_it_should_and_the_real_directories_come_last() {
        let text = profile(
            Path::new("/private/tmp/nhg-claude"),
            &[PathBuf::from("/private/tmp/codex-daemon-501")],
            &real_harness_paths(Path::new("/Users/someone")),
        );
        assert!(text.contains("(allow file-write* (subpath \"/private/tmp/codex-daemon-501\"))"));
        assert!(text.contains("(deny network-outbound)"));
        assert!(text.contains("(allow network-outbound (remote ip \"localhost:*\"))"));
        assert!(text.contains("(allow file-write* (subpath \"/private/tmp/nhg-claude\")"));
        let deny_write = text.find("(deny file-write*)").expect("a deny rule");
        let allow_write = text.find("(allow file-write*").expect("an allow rule");
        let hide_claude = text.find("/Users/someone/.claude\"").expect("a hide rule");
        assert!(deny_write < allow_write && allow_write < hide_claude);
        for hidden in ["/.claude\"", "/.claude.json\"", "/.codex\""] {
            assert!(text.contains(&format!(
                "(deny file-read* file-write* (subpath \"/Users/someone{hidden}))"
            )));
        }
    }

    #[test]
    fn a_path_with_a_quote_or_a_backslash_is_escaped() {
        assert_eq!(quote(Path::new("/a\"b\\c")), "\"/a\\\"b\\\\c\"");
    }

    #[test]
    fn wrapping_puts_the_profile_before_the_program() {
        let argv = vec!["claude".to_owned(), "--version".to_owned()];
        assert_eq!(
            wrap(Path::new("/tmp/p.sb"), &argv),
            [
                "/usr/bin/sandbox-exec",
                "-f",
                "/tmp/p.sb",
                "claude",
                "--version"
            ]
        );
    }
}
