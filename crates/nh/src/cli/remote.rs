//! `nh --machine <label> …` and handles with a machine prefix.
//!
//! The command runs on the machine: `ssh <options> <target> -- <remote nh> <args…>`, with each
//! argument quoted for the shell of the machine. The ssh options are those of the link, so a link
//! that is up carries the command through its control socket. The standard streams of ssh are
//! the ones of this process, so output, input and the exit code pass through. A failure of ssh
//! (exit status 255) is exit code 1 with the message that ssh wrote.
//!
//! [`plan`] finds the machine in the arguments. It comes from `--machine`, or from a handle with a
//! prefix (`studio/w3:p5`, `studio/i42`) in the place of a handle. The prefix is removed from the
//! arguments that go to the machine, because the core there does not know its label.

use std::process::{Command, Stdio};

use clap::Command as ClapCommand;
use nh_core::config::ConfigStore;
use nh_proto::api::ids::Handle;
use nh_remote::{
    machines::DEFAULT_REMOTE_NH,
    ssh::{SshError, SshSpec},
};

use super::{Environment, output::CliError};

/// The label of this machine. A command for it runs here.
pub const LOCAL: &str = "local";

/// The positionals of the verbs that hold a handle. The other positionals are text, and the
/// client does not read them.
const HANDLE_POSITIONALS: &[&str] = &["workspace", "tab", "pane", "agent", "interaction"];

/// Where a command line runs, and the arguments that go there.
#[derive(Debug, PartialEq, Eq)]
pub struct Plan {
    /// The machine of `--machine` or of the prefix of a handle. `None` means this machine, and
    /// then `args` are the arguments as they came.
    pub machine: Option<String>,
    /// The arguments without `--machine <label>` and without the prefixes of handles.
    pub args: Vec<String>,
    /// The command line has `--session` or `--socket`, so the session of the machine is not added.
    pub session_given: bool,
}

/// The handle with its prefix, if the text is one and has a prefix.
fn prefixed(text: &str) -> Option<(String, String)> {
    let handle: Handle = text.parse().ok()?;
    let machine = handle.machine.clone()?;
    let bare = Handle {
        machine: None,
        ..handle
    };
    Some((machine, bare.to_string()))
}

/// Finds the machine of a command line. `root` is the whole command (`args::command`) and `args`
/// are the arguments after the program name. It returns a usage error when two machines
/// appear. The command line has passed `clap` already, so it is well formed.
pub fn plan(root: &ClapCommand, args: &[String]) -> Result<Plan, CliError> {
    let mut machine: Option<String> = None;
    let mut session_given = false;
    let mut out: Vec<String> = Vec::with_capacity(args.len());
    let mut note = |label: &str, machine: &mut Option<String>| -> Result<(), CliError> {
        match machine {
            Some(known) if known != label => Err(CliError::Usage(format!(
                "this command names two machines, {known} and {label}"
            ))),
            _ => {
                *machine = Some(label.to_owned());
                Ok(())
            }
        }
    };
    // The command that holds the arguments now: the root, then the noun, then the verb.
    let mut scope: Vec<&ClapCommand> = vec![root];
    let mut positional = 0usize;
    let mut rest_is_positional = false;
    let mut i = 0;
    while i < args.len() {
        let token = &args[i];
        i += 1;
        if rest_is_positional {
            out.push(token.clone());
            continue;
        }
        if token == "--" {
            rest_is_positional = true;
            out.push(token.clone());
            continue;
        }
        if let Some(long) = token.strip_prefix("--") {
            let (name, inline) = match long.split_once('=') {
                Some((name, value)) => (name, Some(value)),
                None => (long, None),
            };
            let takes_value = scope
                .iter()
                .rev()
                .find_map(|c| c.get_arguments().find(|a| a.get_long() == Some(name)))
                .is_some_and(|a| a.get_action().takes_values());
            if name == "machine" {
                let label = match inline {
                    Some(value) => value.to_owned(),
                    None => {
                        let value = args.get(i).cloned().unwrap_or_default();
                        i += 1;
                        value
                    }
                };
                note(&label, &mut machine)?;
                continue;
            }
            if name == "session" || name == "socket" {
                session_given = true;
            }
            if !takes_value {
                out.push(token.clone());
                continue;
            }
            match inline {
                Some(value) => out.push(format!(
                    "--{name}={}",
                    strip(value, &mut machine, &mut note)?
                )),
                None => {
                    out.push(token.clone());
                    if let Some(value) = args.get(i) {
                        out.push(strip(value, &mut machine, &mut note)?);
                        i += 1;
                    }
                }
            }
            continue;
        }
        if token.len() > 1 && token.starts_with('-') {
            // A short option. Only `-f` exists, and it takes no value.
            out.push(token.clone());
            continue;
        }
        // A word: the noun, the verb, or a positional.
        match scope.len() {
            1 => {
                if let Some(noun) = scope[0].find_subcommand(token) {
                    scope.push(noun);
                }
                out.push(token.clone());
            }
            2 => {
                if let Some(verb) = scope[1].find_subcommand(token) {
                    scope.push(verb);
                }
                out.push(token.clone());
            }
            _ => {
                let verb = scope[2];
                let positionals: Vec<_> = verb.get_positionals().collect();
                let name = positionals
                    .get(positional)
                    .or(positionals.last())
                    .map(|a| a.get_id().as_str());
                positional += 1;
                if name.is_some_and(|n| HANDLE_POSITIONALS.contains(&n)) {
                    out.push(strip(token, &mut machine, &mut note)?);
                } else {
                    out.push(token.clone());
                }
            }
        }
    }
    Ok(Plan {
        machine,
        args: out,
        session_given,
    })
}

/// The text without the prefix of a handle. A prefix gives the machine of the command.
fn strip(
    text: &str,
    machine: &mut Option<String>,
    note: &mut impl FnMut(&str, &mut Option<String>) -> Result<(), CliError>,
) -> Result<String, CliError> {
    match prefixed(text) {
        Some((label, bare)) => {
            note(&label, machine)?;
            Ok(bare)
        }
        None => Ok(text.to_owned()),
    }
}

/// The failure of a command on a machine that cannot start, as the CLI reports it.
fn failed(error: SshError) -> CliError {
    CliError::Failed(error.to_string())
}

/// Runs `args` on the machine `label` of `config.toml`. It returns the exit code of the remote
/// command. A failure of ssh gives 1.
pub fn run(
    label: &str,
    plan: &Plan,
    env: &Environment,
    out: &mut dyn std::io::Write,
) -> Result<u8, CliError> {
    let usage = |e: nh_proto::paths::PathsError| CliError::Usage(e.to_string());
    let config = env.paths.config_dir().map_err(usage)?.join("config.toml");
    let store = ConfigStore::load(config);
    let machine = store
        .machines()
        .into_iter()
        .find(|m| m.label == label)
        .ok_or_else(|| {
            CliError::Usage(format!(
                "no machine {label} in config.toml\n  add a table [machines.{label}] with an ssh target"
            ))
        })?
        .machine;
    // ssh puts its ControlPath socket under `<runtime>/ssh` and connects to whatever is there, so
    // the root must be a directory of this user that nobody else can change.
    let runtime = env
        .paths
        .ensure_runtime_root()
        .map_err(|e| CliError::Failed(e.to_string()))?;
    let spec = SshSpec::new(
        machine.ssh.clone(),
        machine.session.clone(),
        machine
            .remote_nh
            .clone()
            .unwrap_or_else(|| DEFAULT_REMOTE_NH.to_owned()),
        runtime,
    );
    let mut remote_args = Vec::with_capacity(plan.args.len() + 2);
    if !plan.session_given && machine.session != super::DEFAULT_SESSION {
        remote_args.extend(["--session".to_owned(), machine.session.clone()]);
    }
    remote_args.extend(plan.args.iter().cloned());
    let ssh_args = spec.command_args(&remote_args).map_err(failed)?;
    spec.prepare_control_dir().map_err(failed)?;
    out.flush()?;
    let status = Command::new(&spec.program)
        .args(ssh_args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .map_err(|e| CliError::Failed(format!("cannot run ssh: {e}")))?;
    Ok(match status.code() {
        // ssh uses 255 for its own failures. It has written the message.
        Some(255) | None => 1,
        Some(code) => u8::try_from(code).unwrap_or(1),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::args;

    fn plan_of(words: &[&str]) -> Result<Plan, CliError> {
        let doc = nh_proto::api::schema::document();
        let words: Vec<String> = words.iter().map(|w| (*w).to_owned()).collect();
        plan(&args::command(&doc), &words)
    }

    fn strings(words: &[&str]) -> Vec<String> {
        words.iter().map(|w| (*w).to_owned()).collect()
    }

    #[test]
    fn a_command_without_a_machine_stays_as_it_is() {
        let p = plan_of(&["pane", "show", "w1:p1"]).unwrap();
        assert_eq!(p.machine, None);
        assert_eq!(p.args, strings(&["pane", "show", "w1:p1"]));
        assert!(!p.session_given);
    }

    #[test]
    fn the_flag_names_the_machine_and_goes_away() {
        for words in [
            &["--machine", "studio", "pane", "list", "--json"][..],
            &["pane", "list", "--machine", "studio", "--json"],
            &["--machine=studio", "pane", "list", "--json"],
        ] {
            let p = plan_of(words).unwrap();
            assert_eq!(p.machine.as_deref(), Some("studio"), "{words:?}");
            assert_eq!(p.args, strings(&["pane", "list", "--json"]), "{words:?}");
        }
    }

    #[test]
    fn a_prefix_names_the_machine_and_is_removed() {
        let p = plan_of(&["pane", "show", "studio/w1:p1"]).unwrap();
        assert_eq!(p.machine.as_deref(), Some("studio"));
        assert_eq!(p.args, strings(&["pane", "show", "w1:p1"]));
        let p = plan_of(&["interaction", "show", "studio/i42"]).unwrap();
        assert_eq!(p.machine.as_deref(), Some("studio"));
        assert_eq!(p.args, strings(&["interaction", "show", "i42"]));
        // The same machine twice, as flag and prefix, is fine.
        let p = plan_of(&["--machine", "studio", "pane", "show", "studio/w1:p1"]).unwrap();
        assert_eq!(p.args, strings(&["pane", "show", "w1:p1"]));
        // A prefix in the value of an option.
        let p = plan_of(&["agent", "start", "claude", "--pane", "studio/w1:p2"]).unwrap();
        assert_eq!(p.machine.as_deref(), Some("studio"));
        assert_eq!(
            p.args,
            strings(&["agent", "start", "claude", "--pane", "w1:p2"])
        );
        let p = plan_of(&["agent", "start", "claude", "--pane=studio/w1:p2"]).unwrap();
        assert_eq!(
            p.args,
            strings(&["agent", "start", "claude", "--pane=w1:p2"])
        );
    }

    #[test]
    fn two_machines_are_a_usage_error() {
        for words in [
            &["pane", "move", "studio/w1:p1", "mac/w1:t1"][..],
            &["--machine", "studio", "pane", "show", "mac/w1:p1"],
            &["--machine", "studio", "--machine", "mac", "pane", "list"],
        ] {
            let error = plan_of(words).expect_err("two machines");
            assert!(matches!(error, CliError::Usage(_)), "{words:?}");
            assert_eq!(error.exit_code(), 2);
        }
    }

    #[test]
    fn text_that_looks_like_a_handle_is_left_alone() {
        let p = plan_of(&["pane", "send", "w1:p1", "studio/w3"]).unwrap();
        assert_eq!(p.machine, None);
        assert_eq!(p.args, strings(&["pane", "send", "w1:p1", "studio/w3"]));
        let p = plan_of(&["workspace", "create", "studio/w3"]).unwrap();
        assert_eq!(p.machine, None);
        // After `--` nothing is read.
        let p = plan_of(&["pane", "send-keys", "w1:p1", "--", "studio/w3"]).unwrap();
        assert_eq!(p.machine, None);
    }

    #[test]
    fn a_session_option_is_noted() {
        let p = plan_of(&["--machine", "studio", "--session", "work", "pane", "list"]).unwrap();
        assert!(p.session_given);
        assert_eq!(p.args, strings(&["--session", "work", "pane", "list"]));
    }
}
