//! `nh-fixture`: the generators of test input for neohar, on the command line.
//!
//! To add a command: add a row to [`COMMANDS`], a variant to [`Command`], an arm to [`parse`] and
//! an arm to [`run`].

use std::fmt;
use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use nh_fixture::{Op, vt};

/// The commands and what they do, for `--help`.
const COMMANDS: &[(&str, &str)] = &[
    ("gen-vt", "Write a synthetic terminal-output corpus"),
    (
        "tui",
        "Run a full-screen program that animates and redraws on resize",
    ),
    (
        "echo-seq",
        "Log every byte of input to a file with a sequence number",
    ),
];

const USAGE: &str = "\
Usage: nh-fixture <command> [options]

Commands:
";

const USAGE_END: &str = "
Run 'nh-fixture <command> --help' for the options of a command.
";

const GEN_VT_HELP: &str = "\
Usage: nh-fixture gen-vt --seed <N> --bytes <M> [--out <FILE>]

Writes a synthetic terminal-output corpus. It covers every piece of terminal state (colours,
cursor, margins, modes, tab stops, charsets, titles, palette, hyperlinks, keyboard stacks, both
screens), with valid sequences and, now and then, sequences that are out of range. The same seed
and size give the same bytes on every platform.

Options:
  --seed <N>     The seed: an unsigned 64-bit integer.
  --bytes <M>    The size. The output has at least M bytes and fewer than M + 4096 bytes. It
                 ends after a complete sequence.
  --out <FILE>   Write to FILE. The default is standard output.
  -h, --help     Show this text.

The corpus also has resizes between the chunks of output. This command writes the bytes only and
skips the resizes. The library function `nh_fixture::vt::corpus` gives both.
";

const TUI_HELP: &str = "\
Usage: nh-fixture tui [--interval-ms <N>]

Runs a full-screen program on the alternate screen: a border, colored bands (truecolor and
256-color), a title, a log in a scroll region and a status line. It animates on a timer and
draws everything again when the size of the terminal changes. It exits when it reads a 'q'.

Options:
  --interval-ms <N>   The time between two ticks of the animation. The default is 100.
  -h, --help          Show this text.
";

const ECHO_SEQ_HELP: &str = "\
Usage: nh-fixture echo-seq --log <FILE> [--plain]

Puts the terminal in raw mode and appends one line to FILE for every byte that it reads:
'<number> <hex byte>'. The numbers start at 0 and have no gaps. It shows a status screen on the
alternate screen and draws it again when the size changes. It exits when its input ends.

Options:
  --log <FILE>   The file for the log. The program makes it when it is not there.
  --plain        Write the bytes as hex to the main screen instead. The text scrolls and wraps,
                 and the program never draws it again.
  -h, --help     Show this text.
";

#[derive(Debug, PartialEq, Eq)]
enum Command {
    /// Show the help of the tool, or of one command.
    Help(Option<&'static str>),
    GenVt(GenVt),
    Tui {
        interval_ms: u64,
    },
    EchoSeq {
        log: PathBuf,
        plain: bool,
    },
}

#[derive(Debug, PartialEq, Eq)]
struct GenVt {
    seed: u64,
    bytes: usize,
    out: Option<PathBuf>,
}

/// A mistake in the arguments. It ends the program with exit code 2.
#[derive(Debug, PartialEq, Eq)]
enum UsageError {
    NoCommand,
    UnknownCommand(String),
    UnknownOption {
        command: &'static str,
        option: String,
    },
    MissingValue(&'static str),
    BadValue {
        option: &'static str,
        value: String,
    },
    Required(&'static str),
    Repeated(&'static str),
    NotUtf8,
}

impl fmt::Display for UsageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoCommand => write!(f, "no command given"),
            Self::UnknownCommand(c) => write!(f, "unknown command '{c}'"),
            Self::UnknownOption { command, option } => {
                write!(f, "unknown option '{option}' for '{command}'")
            }
            Self::MissingValue(o) => write!(f, "option '{o}' needs a value"),
            Self::BadValue { option, value } => {
                write!(f, "'{value}' is not a valid value for '{option}'")
            }
            Self::Required(o) => write!(f, "option '{o}' is required"),
            Self::Repeated(o) => write!(f, "option '{o}' is given more than once"),
            Self::NotUtf8 => write!(f, "an argument is not valid UTF-8"),
        }
    }
}

impl std::error::Error for UsageError {}

/// Reads the arguments (without the program name).
fn parse(args: &[String]) -> Result<Command, UsageError> {
    let Some((command, rest)) = args.split_first() else {
        return Err(UsageError::NoCommand);
    };
    match command.as_str() {
        "-h" | "--help" | "help" => Ok(Command::Help(None)),
        "gen-vt" => parse_gen_vt(rest),
        "tui" => parse_tui(rest),
        "echo-seq" => parse_echo_seq(rest),
        other => Err(UsageError::UnknownCommand(other.to_owned())),
    }
}

fn parse_gen_vt(args: &[String]) -> Result<Command, UsageError> {
    let mut seed = None;
    let mut bytes = None;
    let mut out = None;
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        if arg == "-h" || arg == "--help" {
            return Ok(Command::Help(Some("gen-vt")));
        }
        // `--option value` and `--option=value`.
        let (name, inline) = match arg.split_once('=') {
            Some((name, value)) if name.starts_with("--") => (name, Some(value.to_owned())),
            _ => (arg.as_str(), None),
        };
        let option: &'static str = match name {
            "--seed" => "--seed",
            "--bytes" => "--bytes",
            "--out" => "--out",
            _ => {
                return Err(UsageError::UnknownOption {
                    command: "gen-vt",
                    option: arg.clone(),
                });
            }
        };
        let value = match inline {
            Some(value) => value,
            None => match args.next() {
                Some(value) if !value.starts_with("--") => value.clone(),
                _ => return Err(UsageError::MissingValue(option)),
            },
        };
        let bad = || UsageError::BadValue {
            option,
            value: value.clone(),
        };
        match option {
            "--seed" => set_once(&mut seed, option, value.parse().map_err(|_| bad())?)?,
            "--bytes" => set_once(&mut bytes, option, value.parse().map_err(|_| bad())?)?,
            _ => set_once(&mut out, option, PathBuf::from(&value))?,
        }
    }
    Ok(Command::GenVt(GenVt {
        seed: seed.ok_or(UsageError::Required("--seed"))?,
        bytes: bytes.ok_or(UsageError::Required("--bytes"))?,
        out,
    }))
}

/// Reads `--name value` and `--name=value` pairs. `Err(Ok(()))` is not used: a request for help is
/// an `Ok(None)`.
fn named_values(
    command: &'static str,
    args: &[String],
    allowed: &[&'static str],
) -> Result<Option<Vec<(&'static str, String)>>, UsageError> {
    let mut found: Vec<(&'static str, String)> = Vec::new();
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        if arg == "-h" || arg == "--help" {
            return Ok(None);
        }
        let (name, inline) = match arg.split_once('=') {
            Some((name, value)) if name.starts_with("--") => (name, Some(value.to_owned())),
            _ => (arg.as_str(), None),
        };
        let Some(option) = allowed.iter().copied().find(|a| *a == name) else {
            return Err(UsageError::UnknownOption {
                command,
                option: arg.clone(),
            });
        };
        let value = match inline {
            Some(value) => value,
            None => match args.next() {
                Some(value) if !value.starts_with("--") => value.clone(),
                _ => return Err(UsageError::MissingValue(option)),
            },
        };
        if found.iter().any(|(name, _)| *name == option) {
            return Err(UsageError::Repeated(option));
        }
        found.push((option, value));
    }
    Ok(Some(found))
}

fn parse_tui(args: &[String]) -> Result<Command, UsageError> {
    let Some(values) = named_values("tui", args, &["--interval-ms"])? else {
        return Ok(Command::Help(Some("tui")));
    };
    let interval_ms = match values.first() {
        Some((option, value)) => {
            value
                .parse()
                .ok()
                .filter(|n| *n > 0)
                .ok_or_else(|| UsageError::BadValue {
                    option,
                    value: value.clone(),
                })?
        }
        None => 100,
    };
    Ok(Command::Tui { interval_ms })
}

fn parse_echo_seq(args: &[String]) -> Result<Command, UsageError> {
    let plain = args.iter().any(|a| a == "--plain");
    let rest: Vec<String> = args.iter().filter(|a| *a != "--plain").cloned().collect();
    let Some(values) = named_values("echo-seq", &rest, &["--log"])? else {
        return Ok(Command::Help(Some("echo-seq")));
    };
    let (_, log) = values
        .into_iter()
        .next()
        .ok_or(UsageError::Required("--log"))?;
    Ok(Command::EchoSeq {
        log: PathBuf::from(log),
        plain,
    })
}

fn set_once<T>(slot: &mut Option<T>, option: &'static str, value: T) -> Result<(), UsageError> {
    if slot.is_some() {
        return Err(UsageError::Repeated(option));
    }
    *slot = Some(value);
    Ok(())
}

fn help(command: Option<&str>) -> String {
    match command {
        Some("gen-vt") => GEN_VT_HELP.to_owned(),
        Some("tui") => TUI_HELP.to_owned(),
        Some("echo-seq") => ECHO_SEQ_HELP.to_owned(),
        _ => {
            let mut text = USAGE.to_owned();
            for (name, about) in COMMANDS {
                text.push_str(&format!("  {name:<10} {about}\n"));
            }
            text.push_str(USAGE_END);
            text
        }
    }
}

/// Writes the bytes of the `Op::Bytes` operations, one after the other. Resizes have no bytes.
fn write_bytes(ops: &[Op], out: &mut impl Write) -> io::Result<()> {
    for op in ops {
        if let Op::Bytes(bytes) = op {
            out.write_all(bytes)?;
        }
    }
    out.flush()
}

fn run(command: Command) -> io::Result<()> {
    match command {
        Command::Help(which) => io::stdout().write_all(help(which).as_bytes()),
        Command::Tui { interval_ms } => {
            nh_fixture::tui::run(std::time::Duration::from_millis(interval_ms))
        }
        Command::EchoSeq { log, plain } => nh_fixture::echo_seq::run(&log, plain),
        Command::GenVt(GenVt { seed, bytes, out }) => {
            let ops = vt::corpus(seed, bytes);
            match out {
                Some(path) => write_bytes(&ops, &mut BufWriter::new(File::create(path)?)),
                None => write_bytes(&ops, &mut BufWriter::new(io::stdout().lock())),
            }
        }
    }
}

fn main() -> ExitCode {
    let args: Result<Vec<String>, UsageError> = std::env::args_os()
        .skip(1)
        .map(|a| a.into_string().map_err(|_| UsageError::NotUtf8))
        .collect();
    let command = match args.and_then(|a| parse(&a)) {
        Ok(command) => command,
        Err(error) => {
            eprintln!("nh-fixture: {error}");
            eprintln!("Run 'nh-fixture --help' for the usage.");
            return ExitCode::from(2);
        }
    };
    match run(command) {
        Ok(()) => ExitCode::SUCCESS,
        // The reader of a pipe (`| head`) closed it. That is not a failure.
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("nh-fixture: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    fn gen_vt(list: &[&str]) -> Result<GenVt, UsageError> {
        let mut all = vec!["gen-vt"];
        all.extend_from_slice(list);
        match parse(&args(&all))? {
            Command::GenVt(g) => Ok(g),
            other => panic!("not gen-vt: {other:?}"),
        }
    }

    #[test]
    fn gen_vt_takes_a_seed_and_a_size() {
        let g = gen_vt(&["--seed", "7", "--bytes", "1048576"]).unwrap();
        assert_eq!(
            g,
            GenVt {
                seed: 7,
                bytes: 1 << 20,
                out: None
            }
        );
    }

    #[test]
    fn gen_vt_takes_a_file_and_the_equals_form() {
        let g = gen_vt(&[
            "--bytes=10",
            "--out=/tmp/x.vt",
            "--seed=18446744073709551615",
        ])
        .unwrap();
        assert_eq!(g.seed, u64::MAX);
        assert_eq!(g.bytes, 10);
        assert_eq!(g.out, Some(PathBuf::from("/tmp/x.vt")));
        let g = gen_vt(&["--out", "a b", "--seed", "0", "--bytes", "0"]).unwrap();
        assert_eq!(g.out, Some(PathBuf::from("a b")));
    }

    #[test]
    fn gen_vt_needs_both_numbers() {
        assert_eq!(
            gen_vt(&["--bytes", "5"]),
            Err(UsageError::Required("--seed"))
        );
        assert_eq!(
            gen_vt(&["--seed", "5"]),
            Err(UsageError::Required("--bytes"))
        );
        assert_eq!(gen_vt(&[]), Err(UsageError::Required("--seed")));
    }

    #[test]
    fn gen_vt_rejects_bad_numbers_and_options() {
        assert!(matches!(
            gen_vt(&["--seed", "-1", "--bytes", "5"]),
            Err(UsageError::BadValue {
                option: "--seed",
                ..
            })
        ));
        assert!(matches!(
            gen_vt(&["--seed", "1", "--bytes", "1.5"]),
            Err(UsageError::BadValue {
                option: "--bytes",
                ..
            })
        ));
        assert!(matches!(
            gen_vt(&["--seed", "1", "--bytes", "5", "--size", "3"]),
            Err(UsageError::UnknownOption { .. })
        ));
        assert!(matches!(
            gen_vt(&["--seed", "1", "--bytes", "5", "extra"]),
            Err(UsageError::UnknownOption { .. })
        ));
        assert_eq!(
            gen_vt(&["--seed", "1", "--bytes"]),
            Err(UsageError::MissingValue("--bytes"))
        );
        assert_eq!(
            gen_vt(&["--seed", "--bytes", "5"]),
            Err(UsageError::MissingValue("--seed"))
        );
        assert_eq!(
            gen_vt(&["--seed", "1", "--seed", "2", "--bytes", "5"]),
            Err(UsageError::Repeated("--seed"))
        );
    }

    #[test]
    fn the_tool_has_help_and_rejects_unknown_commands() {
        assert_eq!(parse(&args(&["--help"])), Ok(Command::Help(None)));
        assert_eq!(parse(&args(&["-h"])), Ok(Command::Help(None)));
        assert_eq!(
            parse(&args(&["gen-vt", "--seed", "1", "--help"])),
            Ok(Command::Help(Some("gen-vt")))
        );
        assert_eq!(parse(&args(&[])), Err(UsageError::NoCommand));
        assert_eq!(
            parse(&args(&["nope"])),
            Err(UsageError::UnknownCommand("nope".to_owned()))
        );
        for (name, _) in COMMANDS {
            assert!(help(None).contains(name), "the help lists {name}");
        }
        assert!(help(Some("gen-vt")).contains("skips the resizes"));
    }

    #[test]
    fn tui_takes_an_interval_and_has_a_default() {
        assert_eq!(
            parse(&args(&["tui"])),
            Ok(Command::Tui { interval_ms: 100 })
        );
        assert_eq!(
            parse(&args(&["tui", "--interval-ms", "25"])),
            Ok(Command::Tui { interval_ms: 25 })
        );
        assert_eq!(
            parse(&args(&["tui", "--interval-ms=7"])),
            Ok(Command::Tui { interval_ms: 7 })
        );
        for bad in ["0", "-3", "x"] {
            assert!(matches!(
                parse(&args(&["tui", "--interval-ms", bad])),
                Err(UsageError::BadValue { .. } | UsageError::MissingValue(_))
            ));
        }
        assert_eq!(parse(&args(&["tui", "-h"])), Ok(Command::Help(Some("tui"))));
    }

    #[test]
    fn echo_seq_needs_a_log_file() {
        assert_eq!(
            parse(&args(&["echo-seq", "--log", "/tmp/x"])),
            Ok(Command::EchoSeq {
                log: PathBuf::from("/tmp/x"),
                plain: false
            })
        );
        assert_eq!(
            parse(&args(&["echo-seq", "--plain", "--log", "/tmp/x"])),
            Ok(Command::EchoSeq {
                log: PathBuf::from("/tmp/x"),
                plain: true
            })
        );
        assert_eq!(
            parse(&args(&["echo-seq"])),
            Err(UsageError::Required("--log"))
        );
        assert!(matches!(
            parse(&args(&["echo-seq", "--file", "x"])),
            Err(UsageError::UnknownOption { .. })
        ));
        assert_eq!(
            parse(&args(&["echo-seq", "--log", "a", "--log", "b"])),
            Err(UsageError::Repeated("--log"))
        );
    }

    #[test]
    fn the_output_is_the_bytes_of_the_chunks_without_the_resizes() {
        let ops = vec![
            Op::Bytes(b"ab".to_vec()),
            Op::Resize { cols: 3, rows: 4 },
            Op::Bytes(b"\x1b[m".to_vec()),
        ];
        let mut out = Vec::new();
        write_bytes(&ops, &mut out).unwrap();
        assert_eq!(out, b"ab\x1b[m");
    }
}
