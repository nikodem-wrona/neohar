//! The command line of `nh-record`.

use std::{path::PathBuf, time::Duration};

/// The usage text.
pub const USAGE: &str = "\
usage:
  nh-record --out <file.journal> --script <steps.txt> [options] -- <cmd> [args...]
  nh-record --dump <file.journal>

options:
  --cols N         initial columns (default 100)
  --rows N         initial rows (default 30)
  --nh PATH        the nh binary (default: $NEOHAR_NH, else next to this executable)
  --env K=V        add or replace a variable of the program's environment; repeatable
  --setup FILE     a shell script that makes the files the program needs; it runs in the
                   working directory of the program
  --timeout SECS   the longest wait of one expect step (default 15)

In --env values, the command and the arguments, {scratch}, {home}, {tmp} and {work} stand for
the scratch directories. The program starts in {work}.";

/// What the command line asks for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    Record {
        out: PathBuf,
        script: PathBuf,
        cols: u16,
        rows: u16,
        nh: Option<PathBuf>,
        env: Vec<(String, String)>,
        setup: Option<PathBuf>,
        timeout: Duration,
        argv: Vec<String>,
    },
    /// Print a journal file as text.
    Dump { file: PathBuf },
}

/// Parses the arguments after the program name. The error is a message for the user.
pub fn parse_args(args: &[String]) -> Result<Command, String> {
    let (flags, argv) = match args.iter().position(|a| a == "--") {
        Some(at) => (&args[..at], args[at + 1..].to_vec()),
        None => (args, Vec::new()),
    };
    let (mut out, mut script, mut nh, mut setup, mut dump) = (None, None, None, None, None);
    let (mut cols, mut rows, mut timeout) = (100u16, 30u16, 15u64);
    let mut env = Vec::new();
    let mut it = flags.iter();
    while let Some(flag) = it.next() {
        let mut value = || {
            it.next()
                .cloned()
                .ok_or_else(|| format!("{flag} needs a value"))
        };
        let number = |text: String| {
            text.parse::<u16>()
                .ok()
                .filter(|n| *n > 0)
                .ok_or_else(|| format!("{flag} needs a number from 1 to 65535, not {text:?}"))
        };
        match flag.as_str() {
            "--out" => out = Some(PathBuf::from(value()?)),
            "--script" => script = Some(PathBuf::from(value()?)),
            "--nh" => nh = Some(PathBuf::from(value()?)),
            "--setup" => setup = Some(PathBuf::from(value()?)),
            "--dump" => dump = Some(PathBuf::from(value()?)),
            "--cols" => cols = number(value()?)?,
            "--rows" => rows = number(value()?)?,
            "--timeout" => {
                let text = value()?;
                timeout =
                    text.parse::<u64>().ok().filter(|n| *n > 0).ok_or_else(|| {
                        format!("--timeout needs a number of seconds, not {text:?}")
                    })?;
            }
            "--env" => {
                let text = value()?;
                let (key, val) = text
                    .split_once('=')
                    .filter(|(key, _)| !key.is_empty())
                    .ok_or_else(|| format!("--env needs K=V, not {text:?}"))?;
                env.push((key.to_owned(), val.to_owned()));
            }
            other => return Err(format!("unknown argument {other:?}")),
        }
    }
    if let Some(file) = dump {
        if out.is_some() || script.is_some() || !argv.is_empty() {
            return Err("--dump cannot be combined with a recording".into());
        }
        return Ok(Command::Dump { file });
    }
    let out = out.ok_or("--out is required")?;
    let script = script.ok_or("--script is required")?;
    if argv.is_empty() {
        return Err("give the program after --".into());
    }
    Ok(Command::Record {
        out,
        script,
        cols,
        rows,
        nh,
        env,
        setup,
        timeout: Duration::from_secs(timeout),
        argv,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(text: &str) -> Vec<String> {
        text.split_whitespace().map(str::to_owned).collect()
    }

    #[test]
    fn a_recording_command_line_parses() {
        let command = parse_args(&args(
            "--out a.journal --script a.steps --cols 80 --rows 24 --env A=b=c --env X= \
             --setup s.sh --timeout 5 -- sh -c exit",
        ))
        .expect("valid");
        assert_eq!(
            command,
            Command::Record {
                out: "a.journal".into(),
                script: "a.steps".into(),
                cols: 80,
                rows: 24,
                nh: None,
                env: vec![("A".into(), "b=c".into()), ("X".into(), String::new())],
                setup: Some("s.sh".into()),
                timeout: Duration::from_secs(5),
                argv: args("sh -c exit"),
            }
        );
    }

    #[test]
    fn the_defaults_are_100_by_30_and_15_seconds() {
        let Command::Record {
            cols,
            rows,
            timeout,
            ..
        } = parse_args(&args("--out a --script b -- vim")).expect("valid")
        else {
            panic!("not a recording");
        };
        assert_eq!((cols, rows, timeout), (100, 30, Duration::from_secs(15)));
    }

    #[test]
    fn the_arguments_of_the_program_are_not_flags() {
        let Command::Record { argv, .. } =
            parse_args(&args("--out a --script b -- less --out -- x")).expect("valid")
        else {
            panic!("not a recording");
        };
        assert_eq!(argv, args("less --out -- x"));
    }

    #[test]
    fn dump_takes_one_file() {
        assert_eq!(
            parse_args(&args("--dump x.journal")).expect("valid"),
            Command::Dump {
                file: "x.journal".into()
            }
        );
        assert!(parse_args(&args("--dump x --out y")).is_err());
    }

    #[test]
    fn mistakes_are_errors() {
        for bad in [
            "--script b -- vim",
            "--out a -- vim",
            "--out a --script b",
            "--out a --script b --cols 0 -- vim",
            "--out a --script b --cols many -- vim",
            "--out a --script b --env NOEQUALS -- vim",
            "--out a --script b --env =v -- vim",
            "--out a --script b --timeout 0 -- vim",
            "--out a --script b --wat -- vim",
            "--out",
        ] {
            assert!(parse_args(&args(bad)).is_err(), "{bad}");
        }
    }
}
