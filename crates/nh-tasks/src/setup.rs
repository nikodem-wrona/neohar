//! The setup commands of a task: they run in the worktree, one after the other, before
//! the agent starts. The output goes into a blob, and the first failure stops the run.

use std::{path::Path, time::Duration};

use tokio::process::Command;

/// The most output that is kept.
const OUTPUT_LIMIT: usize = 1 << 20;
/// How long one command may run.
const COMMAND_LIMIT: Duration = Duration::from_secs(10 * 60);

/// What the setup did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Outcome {
    /// Every command ended with status 0.
    pub success: bool,
    /// The output of the commands, with a line for each command that tells its line.
    pub output: String,
    /// The command that failed, if one did.
    pub failed: Option<String>,
}

fn keep_end(text: &mut String) {
    if text.len() > OUTPUT_LIMIT {
        let cut = text.len() - OUTPUT_LIMIT;
        let at = text
            .char_indices()
            .map(|(i, _)| i)
            .find(|i| *i >= cut)
            .unwrap_or(0);
        text.drain(..at);
    }
}

/// Runs the commands with `shell -c`, in `cwd`, with `env` over the environment of the process.
pub async fn run(
    shell: &str,
    commands: &[String],
    cwd: &Path,
    env: &[(String, String)],
) -> Outcome {
    let mut output = String::new();
    for line in commands {
        output.push_str(&format!("$ {line}\n"));
        let mut command = Command::new(shell);
        // The error output joins the output, so that the blob reads in order.
        command
            .arg("-c")
            .arg(format!("({line}) 2>&1"))
            .current_dir(cwd)
            .envs(env.iter().map(|(k, v)| (k, v)))
            .stdin(std::process::Stdio::null())
            .kill_on_drop(true);
        let ran = tokio::time::timeout(COMMAND_LIMIT, command.output()).await;
        match ran {
            Ok(Ok(done)) => {
                output.push_str(&String::from_utf8_lossy(&done.stdout));
                if !done.status.success() {
                    output.push_str(&format!("(exit status {})\n", done.status));
                    keep_end(&mut output);
                    return Outcome {
                        success: false,
                        output,
                        failed: Some(line.clone()),
                    };
                }
            }
            Ok(Err(error)) => {
                output.push_str(&format!("(cannot run: {error})\n"));
                return Outcome {
                    success: false,
                    output,
                    failed: Some(line.clone()),
                };
            }
            Err(_) => {
                output.push_str("(it took more than 10 minutes and was stopped)\n");
                return Outcome {
                    success: false,
                    output,
                    failed: Some(line.clone()),
                };
            }
        }
    }
    keep_end(&mut output);
    Outcome {
        success: true,
        output,
        failed: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn the_commands_run_in_order_and_the_output_is_kept() {
        let dir = std::env::temp_dir();
        let env = [("NEOHAR_TASK_ID".to_owned(), "k7".to_owned())];
        let done = run(
            "/bin/sh",
            &[
                "echo one $NEOHAR_TASK_ID".to_owned(),
                "echo two >&2".to_owned(),
            ],
            &dir,
            &env,
        )
        .await;
        assert!(done.success, "{done:?}");
        assert!(
            done.output.contains("$ echo one $NEOHAR_TASK_ID\none k7\n"),
            "{}",
            done.output
        );
        assert!(done.output.contains("two\n"), "{}", done.output);
    }

    #[tokio::test]
    async fn the_first_failure_stops_the_run() {
        let dir = std::env::temp_dir();
        let done = run(
            "/bin/sh",
            &[
                "echo before".to_owned(),
                "exit 3".to_owned(),
                "echo never".to_owned(),
            ],
            &dir,
            &[],
        )
        .await;
        assert!(!done.success);
        assert_eq!(done.failed.as_deref(), Some("exit 3"));
        assert!(done.output.contains("before") && !done.output.contains("never"));
        assert!(done.output.contains("exit status"), "{}", done.output);
    }

    #[test]
    fn a_long_output_keeps_its_end() {
        let mut text = "x".repeat(OUTPUT_LIMIT + 100);
        text.push_str("END");
        keep_end(&mut text);
        assert!(text.len() <= OUTPUT_LIMIT + 3);
        assert!(text.ends_with("END"));
    }
}
