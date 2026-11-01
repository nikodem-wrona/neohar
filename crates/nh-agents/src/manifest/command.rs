//! The command lines that a manifest makes for a launch and a resume.
//!
//! The core types a command line into the shell of a pane, so the line must be one that the shell
//! reads as the argument list. [`shell_line`] quotes each argument for that.

use super::schema::{Launch, Resume};

/// What a launch chooses besides the command: the model and the effort level.
#[derive(Clone, Copy, Debug, Default)]
pub struct Choice<'a> {
    pub model: Option<&'a str>,
    pub effort: Option<&'a str>,
}

/// The argument list that starts the harness. With a session id and a manifest that has
/// `argv_with_session`, the id is pre-assigned. The model and the effort level follow,
/// and `args` come last.
pub fn launch_argv(
    launch: &Launch,
    session: Option<&str>,
    choice: Choice<'_>,
    args: &[String],
) -> Vec<String> {
    let mut argv = match (&launch.argv_with_session, session) {
        (Some(template), Some(id)) => fill(template, "session_id", id),
        _ => launch.argv.clone(),
    };
    if let (Some(model_arg), Some(model)) = (&launch.model_arg, choice.model) {
        argv.extend(fill(model_arg, "model", model));
    }
    if let (Some(effort_arg), Some(effort)) = (&launch.effort_arg, choice.effort) {
        argv.extend(fill(effort_arg, "effort", effort));
    }
    argv.extend(args.iter().cloned());
    argv
}

/// The arguments that let the agent read one more folder, or none when the manifest has no
/// `add_dir_arg`.
pub fn add_dir_args(launch: &Launch, dir: &str) -> Vec<String> {
    launch
        .add_dir_arg
        .as_ref()
        .map(|template| fill(template, "add_dir", dir))
        .unwrap_or_default()
}

/// Whether the manifest can pre-assign a session id.
pub fn preassigns(launch: &Launch) -> bool {
    launch.argv_with_session.is_some()
}

/// The argument list that starts the harness again with the handle of a session. `None` when the
/// manifest has no resume command line.
pub fn resume_argv(resume: &Resume, handle: &str) -> Option<Vec<String>> {
    (!resume.argv.is_empty()).then(|| fill(&resume.argv, "resume_id", handle))
}

fn fill(template: &[String], name: &str, value: &str) -> Vec<String> {
    let marker = format!("{{{name}}}");
    template
        .iter()
        .map(|part| part.replace(&marker, value))
        .collect()
}

/// The arguments as one line for a POSIX shell. An argument of safe characters stays as it is, and
/// any other is in single quotes.
pub fn shell_line(argv: &[String]) -> String {
    argv.iter()
        .map(|arg| quote(arg))
        .collect::<Vec<_>>()
        .join(" ")
}

/// One argument for a POSIX shell.
pub fn quote(arg: &str) -> String {
    let safe = !arg.is_empty()
        && arg
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./=:@%+,".contains(c));
    if safe {
        arg.to_owned()
    } else {
        format!("'{}'", arg.replace('\'', "'\\''"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn launch() -> Launch {
        Launch {
            argv: vec!["claude".to_owned()],
            argv_with_session: Some(vec![
                "claude".to_owned(),
                "--session-id".to_owned(),
                "{session_id}".to_owned(),
            ]),
            model_arg: Some(vec!["--model".to_owned(), "{model}".to_owned()]),
            effort_arg: Some(vec!["--effort".to_owned(), "{effort}".to_owned()]),
            ..Launch::default()
        }
    }

    #[test]
    fn a_session_id_model_effort_and_arguments_join_in_order() {
        let argv = launch_argv(
            &launch(),
            Some("abc-123"),
            Choice {
                model: Some("opus"),
                effort: Some("high"),
            },
            &["--verbose".to_owned()],
        );
        assert_eq!(
            argv,
            [
                "claude",
                "--session-id",
                "abc-123",
                "--model",
                "opus",
                "--effort",
                "high",
                "--verbose"
            ]
        );
    }

    #[test]
    fn without_a_session_the_plain_command_line_is_used() {
        assert_eq!(
            launch_argv(&launch(), None, Choice::default(), &[]),
            ["claude"]
        );
        let plain = Launch {
            argv: vec!["codex".to_owned()],
            ..Launch::default()
        };
        assert!(!preassigns(&plain));
        assert_eq!(
            launch_argv(&plain, Some("x"), Choice::default(), &[]),
            ["codex"]
        );
    }

    #[test]
    fn the_effort_fills_its_place_inside_an_argument() {
        let codex = Launch {
            argv: vec!["codex".to_owned()],
            effort_arg: Some(vec![
                "-c".to_owned(),
                "model_reasoning_effort=\"{effort}\"".to_owned(),
            ]),
            ..Launch::default()
        };
        let choice = Choice {
            model: Some("gpt-5.5"),
            effort: Some("xhigh"),
        };
        // The manifest has no `model_arg`, so the model goes nowhere.
        let argv = launch_argv(&codex, None, choice, &[]);
        assert_eq!(argv, ["codex", "-c", "model_reasoning_effort=\"xhigh\""]);
        assert_eq!(
            shell_line(&argv),
            r#"codex -c 'model_reasoning_effort="xhigh"'"#
        );
    }

    #[test]
    fn the_resume_handle_fills_the_command_line() {
        let resume = Resume {
            argv: vec![
                "claude".to_owned(),
                "--resume".to_owned(),
                "{resume_id}".to_owned(),
            ],
            ..Resume::default()
        };
        assert_eq!(
            resume_argv(&resume, "s-1").expect("a command line"),
            ["claude", "--resume", "s-1"]
        );
        assert_eq!(resume_argv(&Resume::default(), "s-1"), None);
    }

    #[test]
    fn the_shell_reads_the_line_as_the_arguments() {
        let argv = vec![
            "claude".to_owned(),
            "--model".to_owned(),
            "it's a model".to_owned(),
            String::new(),
            "$(rm -rf /)".to_owned(),
            "a b".to_owned(),
        ];
        assert_eq!(
            shell_line(&argv),
            r#"claude --model 'it'\''s a model' '' '$(rm -rf /)' 'a b'"#
        );
    }
}
