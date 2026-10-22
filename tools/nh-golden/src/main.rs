//! The `nh-golden` binary. See the library for what it does.
//!
//! ```text
//! nh-golden record <claude|codex> --scenario <file.toml> (--out <dir> | --out-root <dir>) [options]
//! nh-golden record-all <claude|codex> --dir <scenarios dir> --out-root <dir> [options]
//! nh-golden compare <dir-a> <dir-b>
//! nh-golden scrub <dir>
//! nh-golden screen <file.journal> [events]
//! ```

use std::{
    fs::{self, File},
    io::BufReader,
    path::{Path, PathBuf},
    process::ExitCode,
};

use nh_golden::{
    GoldenError, chain, compare,
    driver::{self, Request},
    guard,
    recipe::{self, Harness},
    scrub,
};
use nh_proto::holder::{HolderEvent, Marker, StreamId};
use nh_record::{holder::locate_nh, journal, signals};
use nh_term::{AlacrittyEngine, MIN_HISTORY, Replaying, ScreenDump, VtEngine, xterm_palette};

const USAGE: &str = "\
usage:
  nh-golden record <claude|codex> --scenario <file.toml> (--out <dir> | --out-root <dir>) [options]
  nh-golden record-all <claude|codex> --dir <scenarios dir> --out-root <dir> [options]
  nh-golden compare <dir-a> <dir-b>
  nh-golden scrub <dir>
  nh-golden screen <file.journal> [events]
  nh-golden mcp-elicit                     (an MCP server for a scenario; it reads standard input)

record-all records every *.toml file of the directory, in name order, and stops at the first
failure. It takes the options of record, except --scenario, --out and --name.

record options:
  --out <dir>        write the recording into this directory
  --out-root <dir>   write it to <dir>/<harness>/<version>/<name>
  --name <name>      the name of the scenario (default: the name of the file, without .toml)
  --nh <path>        the nh binary (default: $NEOHAR_NH, else next to this executable)
  --binary <path>    the harness binary (default: the first one on PATH)
  --root <dir>       the scratch tree (default: /tmp/nhg-<harness>)

record needs macOS: the harness runs under sandbox-exec. compare exits with 1 when the
recordings differ. scrub removes the text of the harness (request bodies, transcript attachments)
from every recording under the directory, as record does. screen prints the screen at the end of a journal of nh-record.";

struct RecordArgs {
    harness: Harness,
    scenario: PathBuf,
    out: Option<PathBuf>,
    out_root: Option<PathBuf>,
    name: Option<String>,
    nh: Option<PathBuf>,
    binary: Option<PathBuf>,
    root: Option<PathBuf>,
}

/// The arguments of `record-all`, as a `record` for each scenario file.
fn parse_record_all(args: &[String]) -> Result<Vec<RecordArgs>, String> {
    let mut dir = None;
    let mut rest = Vec::new();
    let mut it = args.iter();
    let harness = it.next().ok_or("record-all needs a harness")?.clone();
    while let Some(arg) = it.next() {
        if arg == "--dir" {
            dir = Some(PathBuf::from(it.next().ok_or("--dir needs a value")?));
        } else {
            rest.push(arg.clone());
        }
    }
    let dir = dir.ok_or("--dir is required")?;
    if rest
        .iter()
        .any(|a| a == "--scenario" || a == "--out" || a == "--name")
    {
        return Err("record-all does not take --scenario, --out or --name".into());
    }
    let mut files: Vec<PathBuf> = fs::read_dir(&dir)
        .map_err(|e| format!("cannot read {}: {e}", dir.display()))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "toml"))
        .collect();
    files.sort();
    if files.is_empty() {
        return Err(format!("{} has no .toml file", dir.display()));
    }
    files
        .into_iter()
        .map(|file| {
            let mut one = vec![
                harness.clone(),
                "--scenario".to_owned(),
                file.display().to_string(),
            ];
            one.extend(rest.iter().cloned());
            parse_record(&one)
        })
        .collect()
}

fn parse_record(args: &[String]) -> Result<RecordArgs, String> {
    let (name, rest) = args.split_first().ok_or("record needs a harness")?;
    let harness = Harness::parse(name)
        .ok_or_else(|| format!("{name:?} is not a harness (claude or codex)"))?;
    let mut parsed = RecordArgs {
        harness,
        scenario: PathBuf::new(),
        out: None,
        out_root: None,
        name: None,
        nh: None,
        binary: None,
        root: None,
    };
    let mut scenario = None;
    let mut it = rest.iter();
    while let Some(flag) = it.next() {
        let mut value = || {
            it.next()
                .cloned()
                .ok_or_else(|| format!("{flag} needs a value"))
        };
        match flag.as_str() {
            "--scenario" => scenario = Some(PathBuf::from(value()?)),
            "--out" => parsed.out = Some(PathBuf::from(value()?)),
            "--out-root" => parsed.out_root = Some(PathBuf::from(value()?)),
            "--name" => parsed.name = Some(value()?),
            "--nh" => parsed.nh = Some(PathBuf::from(value()?)),
            "--binary" => parsed.binary = Some(PathBuf::from(value()?)),
            "--root" => parsed.root = Some(PathBuf::from(value()?)),
            other => return Err(format!("unknown argument {other:?}")),
        }
    }
    parsed.scenario = scenario.ok_or("--scenario is required")?;
    if parsed.out.is_some() == parsed.out_root.is_some() {
        return Err("give exactly one of --out and --out-root".into());
    }
    Ok(parsed)
}

fn record(args: &RecordArgs) -> Result<(), GoldenError> {
    let chain = chain::load_chain(&args.scenario, args.name.as_deref())?;
    let name = chain.last().map(|l| l.name.clone()).unwrap_or_default();
    let nh = locate_nh(args.nh.as_deref())?;
    // The hooks call it from the working directory of the harness.
    let nh = fs::canonicalize(&nh).map_err(GoldenError::io(format!("resolve {}", nh.display())))?;
    let binary = match &args.binary {
        Some(path) => path.clone(),
        None => recipe::find_binary(args.harness, std::env::var_os("PATH").as_deref()).ok_or_else(
            || {
                GoldenError::Setup(format!(
                    "{} is not on PATH. Install it, or pass --binary <path>",
                    args.harness.name()
                ))
            },
        )?,
    };
    let binary = fs::canonicalize(&binary).unwrap_or(binary);
    let root = args
        .root
        .clone()
        .unwrap_or_else(|| PathBuf::from(format!("/tmp/nhg-{}", args.harness.name())));
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| GoldenError::Setup("HOME is not set".into()))?;

    // The check that does not trust the sandbox: the real settings are as before afterwards.
    let before = guard::take(args.harness, &home);
    let requests: Vec<Request<'_>> = chain
        .iter()
        .map(|loaded| Request {
            harness: args.harness,
            scenario: &loaded.scenario,
            scenario_text: &loaded.text,
            scenario_name: &loaded.name,
            nh: &nh,
            binary: &binary,
            root: &root,
        })
        .collect();
    let refs: Vec<&Request<'_>> = requests.iter().collect();
    let result = driver::record_chain(&refs);
    guard::verify(args.harness, &home, &before)?;
    let recording = result?;

    let out = match (&args.out, &args.out_root) {
        (Some(out), _) => out.clone(),
        (None, Some(root)) => root
            .join(args.harness.name())
            .join(&recording.meta.harness_version)
            .join(&name),
        (None, None) => unreachable!("parse_record checked this"),
    };
    fs::create_dir_all(&out).map_err(GoldenError::io(format!("create {}", out.display())))?;
    recording.write(&out)?;
    let meta = &recording.meta;
    println!(
        "recorded {} {} {} to {}: {} screens, {} hooks, {} requests, exit: {}",
        meta.harness,
        meta.harness_version,
        meta.scenario,
        out.display(),
        meta.screens.len(),
        meta.hook_calls,
        meta.llm_requests,
        meta.exit
    );
    Ok(())
}

fn screen(path: &str, upto: usize) -> Result<String, String> {
    let file = File::open(path).map_err(|e| format!("cannot open {path}: {e}"))?;
    let events = journal::read_journal(&mut BufReader::new(file)).map_err(|e| e.to_string())?;
    let (cols, rows) = journal::resizes(&events)
        .first()
        .map_or((120, 40), |(_, c, r)| (*c, *r));
    let mut engine = AlacrittyEngine::new(cols, rows, MIN_HISTORY, xterm_palette());
    for event in events.iter().take(upto) {
        match event {
            HolderEvent::Output {
                stream: StreamId::Pty,
                bytes,
                ..
            } => engine.advance(bytes, &mut Replaying::default()),
            HolderEvent::Marker {
                marker: Marker::Resize { cols, rows },
                ..
            } => engine.resize(*cols, *rows),
            _ => {}
        }
    }
    Ok(ScreenDump::of(&engine, &xterm_palette()).text().join("\n"))
}

fn run(args: &[String]) -> Result<ExitCode, String> {
    match args.split_first() {
        Some((cmd, rest)) if cmd == "record" => {
            let parsed = parse_record(rest)?;
            record(&parsed).map_err(|e| e.to_string())?;
            Ok(ExitCode::SUCCESS)
        }
        Some((cmd, rest)) if cmd == "record-all" => {
            for parsed in parse_record_all(rest)? {
                record(&parsed).map_err(|e| format!("{}: {e}", parsed.scenario.display()))?;
            }
            Ok(ExitCode::SUCCESS)
        }
        Some((cmd, _)) if cmd == "mcp-elicit" => {
            nh_golden::mcp::serve(std::io::stdin().lock(), std::io::stdout().lock())
                .map_err(|e| format!("mcp-elicit: {e}"))?;
            Ok(ExitCode::SUCCESS)
        }
        Some((cmd, rest)) if cmd == "compare" => {
            let [a, b] = rest else {
                return Err("compare needs two directories".into());
            };
            let differences =
                compare::compare(Path::new(a), Path::new(b)).map_err(|e| e.to_string())?;
            if differences.is_empty() {
                println!("the recordings are the same");
                return Ok(ExitCode::SUCCESS);
            }
            for difference in &differences {
                println!("{}: {}", difference.what, difference.detail);
            }
            println!("{} differences", differences.len());
            Ok(ExitCode::from(1))
        }
        Some((cmd, rest)) if cmd == "scrub" => {
            let [dir] = rest else {
                return Err("scrub needs a directory".into());
            };
            let changed = scrub::tree(Path::new(dir)).map_err(|e| e.to_string())?;
            println!("{changed} files changed");
            Ok(ExitCode::SUCCESS)
        }
        Some((cmd, rest)) if cmd == "screen" => {
            let (path, upto) = match rest {
                [path] => (path, usize::MAX),
                [path, n] => (
                    path,
                    n.parse().map_err(|_| format!("{n:?} is not a number"))?,
                ),
                _ => return Err("screen needs a journal file".into()),
            };
            println!("{}", screen(path, upto)?);
            Ok(ExitCode::SUCCESS)
        }
        _ => Err("give a command".into()),
    }
}

fn main() -> ExitCode {
    signals::install();
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(code) => code,
        Err(message) => {
            eprintln!("nh-golden: {message}\n\n{USAGE}");
            ExitCode::from(2)
        }
    }
}
