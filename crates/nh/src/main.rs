//! The `nh` binary: `hold`, `hook`, the CLI, `daemon`, `bridge` and `askpass`.
//!
//! The role is the first argument. It is read before anything else starts, because a hook has a
//! 5 ms budget and a holder must not run any other setup. Every other first argument
//! is a command of the CLI, which parses its arguments with `clap`.

mod askpass;
mod bridge;
mod cli;
mod daemon;
mod peer;
mod shim;

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args_os()
        .map(|a| a.to_string_lossy().into_owned())
        .skip(1)
        .collect();
    match args.first().map(String::as_str) {
        Some("hold") => nh_hold::main(args[1..].to_vec()),
        // The shim reads only the environment and stdin, and it exits 0 whatever happens.
        Some("hook") => shim::main(args[1..].to_vec()),
        Some("daemon") => daemon::main(&args[1..]),
        // The end of a link on a remote machine: it copies bytes and parses nothing.
        Some("bridge") => bridge::main(&args[1..]),
        // The answer to a question of ssh, asked of the app that started the link.
        Some("askpass") => askpass::main(&args[1..]),
        _ => cli::main(args),
    }
}
