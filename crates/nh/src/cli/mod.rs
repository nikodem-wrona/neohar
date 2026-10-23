//! The command line of `nh`.
//!
//! The CLI is a client of the API socket. It speaks the same methods as the UI, and it has no
//! logic of its own beyond reading arguments and printing results.
//!
//! - **Which core.** `--socket` names the socket. Else `--session` names the session. Else
//!   `$NEOHAR_SOCKET` (which every pane has), and else `$NEOHAR_SESSION` or the session `default`.
//! - **Output.** JSON when stdout is not a terminal or with `--json`, else text for people.
//! - **Exit codes.** 0 ok, 1 error, 2 usage, 3 not found, 4 already answered.
//! - **No pane token.** The CLI does not send the token of the pane that it runs in, so a command
//!   that a person types in a pane has the scope of the CLI.
//! - **Verbs.** A verb for each method that a person can call is in [`verbs`].

mod args;
pub(crate) mod client;
mod commands;
mod output;
mod remote;
mod render;
#[cfg(test)]
mod testing;
mod verbs;

use std::{
    io::{self, IsTerminal, Read, Write},
    path::PathBuf,
    process::ExitCode,
    time::Duration,
};

use clap::ArgMatches;
use nh_proto::paths::Env;
use serde_json::Value;

use self::{
    args::Globals,
    client::{CallError, Client},
    output::{CliError, Output, exit},
};

/// The session that the CLI talks to when nothing says otherwise.
pub const DEFAULT_SESSION: &str = "default";

/// What the CLI reads from the environment of the process.
pub struct Environment {
    pub paths: Env,
    /// `$NEOHAR_SESSION`.
    pub session: Option<String>,
    /// `$NEOHAR_SOCKET`.
    pub socket: Option<PathBuf>,
}

impl Environment {
    pub fn from_process() -> Self {
        let var = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty());
        Self {
            paths: Env::from_process(),
            session: var("NEOHAR_SESSION").and_then(|v| v.into_string().ok()),
            socket: var("NEOHAR_SOCKET").map(PathBuf::from),
        }
    }
}

/// The socket to connect to.
fn socket_path(globals: &Globals, env: &Environment) -> Result<PathBuf, CliError> {
    let for_session = |session: &str| {
        env.paths
            .api_socket(session)
            .map_err(|e| CliError::Usage(e.to_string()))
    };
    if let Some(socket) = &globals.socket {
        return Ok(socket.clone());
    }
    if let Some(session) = &globals.session {
        return for_session(session);
    }
    if let Some(socket) = &env.socket {
        return Ok(socket.clone());
    }
    for_session(env.session.as_deref().unwrap_or(DEFAULT_SESSION))
}

/// Runs the CLI as the process: arguments after the program name.
pub fn main(args: Vec<String>) -> ExitCode {
    let stdout = io::stdout();
    let code = run(
        &args,
        &Environment::from_process(),
        &mut stdout.lock(),
        &mut io::stderr().lock(),
        &mut io::stdin().lock(),
        stdout.is_terminal(),
    );
    ExitCode::from(code)
}

/// Runs a command line and returns the exit code.
pub fn run(
    args: &[String],
    env: &Environment,
    out: &mut dyn Write,
    err: &mut dyn Write,
    stdin: &mut dyn Read,
    stdout_is_terminal: bool,
) -> u8 {
    let doc = nh_proto::api::schema::document();
    let command = args::command(&doc);
    let parse = |words: &[String]| {
        command
            .clone()
            .try_get_matches_from(std::iter::once("nh").chain(words.iter().map(String::as_str)))
    };
    let matches = match parse(args) {
        Ok(matches) => matches,
        Err(error) => {
            // Help and version go to stdout and are not errors.
            let rendered = error.render().to_string();
            return if error.use_stderr() {
                let _ = write!(err, "{rendered}");
                exit::USAGE
            } else {
                let _ = write!(out, "{rendered}");
                exit::OK
            };
        }
    };
    let globals = args::globals(&matches);
    let mut output = Output {
        out,
        err,
        json: globals.json || !stdout_is_terminal,
        pretty: stdout_is_terminal,
    };
    // `--machine` and a prefix in a handle send the command to another machine.
    let plan = match remote::plan(&command, args) {
        Ok(plan) => plan,
        Err(error) => {
            output.error(&error);
            return error.exit_code();
        }
    };
    let matches = match plan.machine.as_deref() {
        Some(label) if label != remote::LOCAL => {
            return match remote::run(label, &plan, env, output.out) {
                Ok(code) => code,
                Err(error) => {
                    output.error(&error);
                    error.exit_code()
                }
            };
        }
        // The label of this machine: run here, without the prefixes.
        Some(_) => match parse(&plan.args) {
            Ok(matches) => matches,
            Err(error) => {
                let _ = write!(output.err, "{}", error.render());
                return exit::USAGE;
            }
        },
        None => matches,
    };
    match dispatch(&matches, &globals, &doc, env, &mut output, stdin) {
        Ok(()) => exit::OK,
        Err(error) => {
            // A closed pipe (`nh ... | head`) is not worth a message.
            if !matches!(&error, CliError::Output(e) if e.kind() == io::ErrorKind::BrokenPipe) {
                output.error(&error);
            }
            error.exit_code()
        }
    }
}

fn connect(globals: &Globals, env: &Environment) -> Result<Client, CliError> {
    let socket = socket_path(globals, env)?;
    Ok(Client::connect(
        &socket,
        Duration::from_secs(globals.timeout.max(1)),
    )?)
}

fn dispatch(
    matches: &ArgMatches,
    globals: &Globals,
    doc: &Value,
    env: &Environment,
    output: &mut Output<'_>,
    stdin: &mut dyn Read,
) -> Result<(), CliError> {
    let Some((name, sub)) = matches.subcommand() else {
        return Ok(());
    };
    let sub_name = sub.subcommand_name().unwrap_or("");
    let sub_matches = sub.subcommand().map(|(_, m)| m);
    match (name, sub_name) {
        // The schema is in the binary, and so are the completions. They need no core.
        ("api", "schema") => return commands::api_schema(output),
        ("completions", _) => {
            let shell = sub
                .get_one::<clap_complete::Shell>("shell")
                .copied()
                .ok_or_else(|| CliError::Usage("name a shell".to_owned()))?;
            return commands::completions(shell, &args::command(doc), output);
        }
        ("api", "call") => {
            let call = sub_matches.ok_or_else(|| CliError::Usage("name a method".to_owned()))?;
            // The params are checked before the connection, so wrong input is a usage error
            // also when no core runs.
            let params = commands::parse_params(
                call.get_one::<String>("params").map(String::as_str),
                stdin,
            )?;
            let method = call
                .get_one::<String>("method")
                .cloned()
                .unwrap_or_default();
            let mut client = connect(globals, env)?;
            return commands::api_call(&mut client, output, &method, params);
        }
        ("doctor", _) => return doctor(globals, env, output),
        _ => {}
    }
    let mut client = connect(globals, env)?;
    match (name, sub_name) {
        ("session", "info") => commands::session_info(&mut client, output),
        ("debug", "bundle") => {
            let bundle = sub_matches.ok_or_else(|| CliError::Usage("name a command".to_owned()))?;
            let path = match bundle.get_one::<PathBuf>("out") {
                Some(path) => std::path::absolute(path).unwrap_or_else(|_| path.clone()),
                None => std::env::current_dir()
                    .unwrap_or_default()
                    .join(format!("neohar-debug-{}.zip", unix_ms())),
            };
            commands::debug_bundle(
                &mut client,
                output,
                &path,
                bundle.get_flag("include-scrollback"),
            )
        }
        ("events", "tail") => {
            let tail = sub_matches.ok_or_else(|| CliError::Usage("name a command".to_owned()))?;
            commands::events_tail(&mut client, output, tail)
        }
        ("config", "edit") => commands::config_edit(&mut client, output, env),
        (noun, verb) if verbs::is_noun(noun) => {
            let found = verbs::VERBS
                .iter()
                .find(|v| v.noun == noun && v.name == verb)
                .ok_or_else(|| CliError::Usage(format!("no command {noun} {verb}")))?;
            let verb_matches =
                sub_matches.ok_or_else(|| CliError::Usage("name a command".to_owned()))?;
            let cwd = std::env::current_dir().unwrap_or_default();
            let params = verbs::params(doc, found, verb_matches, &cwd)?;
            let result = client.call(found.method, params)?;
            render::result(output, &result)?;
            Ok(())
        }
        _ => Err(CliError::Usage(format!("no command {name}"))),
    }
}

/// `nh doctor`: with a core the checks run there, and without one they are the checks of the
/// install.
fn doctor(globals: &Globals, env: &Environment, output: &mut Output<'_>) -> Result<(), CliError> {
    let socket = socket_path(globals, env)?;
    let timeout = Duration::from_secs(globals.timeout.max(1));
    let mut connected = Client::connect(&socket, timeout);
    match &mut connected {
        Ok(client) => commands::doctor(Some(client), || Value::Null, output),
        Err(CallError::NotRunning(_)) => {
            let session = globals
                .session
                .as_deref()
                .or(env.session.as_deref())
                .unwrap_or(DEFAULT_SESSION);
            let nh = std::env::current_exe().unwrap_or_default();
            commands::doctor(
                None,
                || {
                    serde_json::to_value(nh_proto::doctor::offline(&env.paths, session, &nh))
                        .unwrap_or(Value::Null)
                },
                output,
            )
        }
        Err(_) => connected.map(|_| ()).map_err(CliError::from),
    }
}

fn unix_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis())
}

#[cfg(test)]
mod tests;
