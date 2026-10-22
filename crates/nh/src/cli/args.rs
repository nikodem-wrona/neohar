//! The arguments of the command line (`clap`, builder API).
//!
//! The commands that are not a verb of the table are built here: `api`, `session info`, `doctor`,
//! `debug`, `events` and `completions`. The nouns come from [`super::verbs`].

use std::path::PathBuf;

use clap::{Arg, ArgAction, ArgMatches, Command};
use serde_json::Value;

use super::verbs;

/// The options that every command has.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Globals {
    /// The session to talk to.
    pub session: Option<String>,
    /// The API socket to use. It wins over the session.
    pub socket: Option<PathBuf>,
    /// Print JSON.
    pub json: bool,
    /// How long to wait for the core to answer, in seconds.
    pub timeout: u64,
}

pub fn globals(matches: &ArgMatches) -> Globals {
    Globals {
        session: matches.get_one::<String>("session").cloned(),
        socket: matches.get_one::<PathBuf>("socket").cloned(),
        json: matches.get_flag("json"),
        timeout: matches.get_one::<u64>("timeout").copied().unwrap_or(30),
    }
}

/// The whole command line. `doc` is the contract of the API, which gives the options of the verbs.
pub fn command(doc: &Value) -> Command {
    let mut root = Command::new("nh")
        .about("The command line of neohar. It talks to the core of a session over the API socket")
        .version(env!("CARGO_PKG_VERSION"))
        .disable_help_subcommand(true)
        .arg_required_else_help(true)
        .subcommand_required(true)
        .arg(
            Arg::new("session")
                .long("session")
                .global(true)
                .value_name("NAME")
                .help("The session to talk to. The default is $NEOHAR_SESSION, or `default`"),
        )
        .arg(
            Arg::new("socket")
                .long("socket")
                .global(true)
                .value_name("PATH")
                .value_parser(clap::value_parser!(PathBuf))
                .help("The API socket to use. It wins over --session and $NEOHAR_SOCKET"),
        )
        .arg(
            // The id is not `machine`, which is the name of a positional of `machine connect`.
            Arg::new("via_machine")
                .long("machine")
                .global(true)
                .value_name("LABEL")
                .help("Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same"),
        )
        .arg(
            Arg::new("json")
                .long("json")
                .global(true)
                .action(ArgAction::SetTrue)
                .help("Print JSON. This is the default when stdout is not a terminal"),
        )
        .arg(
            Arg::new("timeout")
                .long("timeout")
                .global(true)
                .value_name("SECONDS")
                .default_value("30")
                .value_parser(clap::value_parser!(u64))
                .help("How long to wait for the core to answer, in seconds"),
        )
        .subcommand(
            Command::new("api")
                .about("Call the API directly")
                .subcommand_required(true)
                .arg_required_else_help(true)
                .subcommand(
                    Command::new("call")
                        .about("Call a method and print its result as JSON")
                        .long_about(
                            "Call a method and print its result as JSON.\n\nThe params are a JSON object, in the argument or, with `-`, on stdin. Without them the call has no params.",
                        )
                        .arg(Arg::new("method").required(true).value_name("METHOD").help("The name of the method, for example `session.info`"))
                        .arg(Arg::new("params").value_name("PARAMS").help("The params as JSON, or `-` to read them from stdin")),
                )
                .subcommand(Command::new("schema").about("Print the contract of the API as one JSON document")),
        )
        .subcommand(Command::new("doctor").about(
            "Check the install and the core: the symlink, the sockets, the integrations of the harnesses, the storage. It works also when neohar does not run",
        ))
        .subcommand(
            Command::new("debug")
                .about("Tools for a bug report")
                .subcommand_required(true)
                .arg_required_else_help(true)
                .subcommand(
                    Command::new("bundle")
                        .about("Write a zip with the logs, the doctor, the configuration and the recent events. Secrets are removed from every text")
                        .arg(Arg::new("out").long("out").value_name("FILE").value_parser(clap::value_parser!(PathBuf)).help("The file to write. The default is neohar-debug-<time>.zip in this directory"))
                        .arg(Arg::new("include-scrollback").long("include-scrollback").action(ArgAction::SetTrue).help("Add the text of the panes")),
                ),
        )
        .subcommand(
            Command::new("events")
                .about("The log of events")
                .subcommand_required(true)
                .arg_required_else_help(true)
                .subcommand(
                    Command::new("tail")
                        .about("Print events as JSON lines")
                        .arg(Arg::new("since").long("since").value_name("CURSOR").help("Start at this cursor, as JSON. Without it the log starts at its end"))
                        .arg(Arg::new("follow").long("follow").short('f').action(ArgAction::SetTrue).help("Go on when the log is read to its end"))
                        .arg(Arg::new("kind").long("kind").action(ArgAction::Append).value_name("KIND").help("Only this kind of event. A trailing `*` is a prefix, as in `interaction.*`"))
                        .arg(Arg::new("subject").long("subject").value_name("ID").help("Only events about this object")),
                ),
        )
        .subcommand(
            Command::new("completions")
                .about("Print the completions of a shell")
                .arg(Arg::new("shell").required(true).value_parser(clap::value_parser!(clap_complete::Shell)).value_name("SHELL")),
        );
    for noun in verbs::nouns(doc) {
        let noun = match noun.get_name() {
            "config" => noun.subcommand(
                Command::new("edit").about("Open config.toml in $EDITOR, and read it again"),
            ),
            "session" => {
                noun.subcommand(Command::new("info").about("Show the session and its core"))
            }
            _ => noun,
        };
        root = root.subcommand(noun);
    }
    root
}
