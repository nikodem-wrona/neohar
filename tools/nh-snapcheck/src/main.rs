//! `nh-snapcheck`: checks that a restored VT engine continues like the engine that was never
//! stopped. See the module documentation of the library.

use std::{
    path::{Path, PathBuf},
    process::ExitCode,
};

use nh_snapcheck::{Options, Report, Stream, run};

const USAGE: &str = "\
usage: nh-snapcheck (--seed N [--bytes M] | --journal FILE) [options]
       nh-snapcheck corpus [--seeds N] [--bytes M] [--dir DIR] [options]

options:
  --every N       take every Nth cut point (default 10; 1 takes all)
  --window BYTES  bytes that each restored engine reads (default 262144)
  --max-live N    restored engines alive at the same time (default 128)
  --max-failures N  stop a stream after N differences (default 5)
  --normalize     rebuild each engine from its snapshot at a checkpoint (AlacrittyEngine::checkpoint)
  --report-only   exit 0 unless a restore was not exact right after it
  --quiet         print only the totals";

struct Args {
    seed: Option<u64>,
    bytes: usize,
    journal: Option<PathBuf>,
    corpus: bool,
    seeds: u64,
    dir: PathBuf,
    quiet: bool,
    report_only: bool,
    options: Options,
}

fn parse() -> Result<Args, String> {
    let mut args = Args {
        seed: None,
        bytes: 1 << 20,
        journal: None,
        corpus: false,
        seeds: 50,
        dir: PathBuf::from("tests/vt-corpus"),
        quiet: false,
        report_only: false,
        options: Options::default(),
    };
    let mut it = std::env::args().skip(1).peekable();
    if it.peek().map(String::as_str) == Some("corpus") {
        args.corpus = true;
        it.next();
    }
    while let Some(arg) = it.next() {
        let mut value = |name: &str| it.next().ok_or_else(|| format!("{name} needs a value"));
        let number = |text: String, name: &str| {
            text.parse::<usize>()
                .map_err(|_| format!("{name} needs a number"))
        };
        match arg.as_str() {
            "--seed" => args.seed = Some(number(value("--seed")?, "--seed")? as u64),
            "--bytes" => args.bytes = number(value("--bytes")?, "--bytes")?,
            "--journal" => args.journal = Some(PathBuf::from(value("--journal")?)),
            "--seeds" => args.seeds = number(value("--seeds")?, "--seeds")? as u64,
            "--dir" => args.dir = PathBuf::from(value("--dir")?),
            "--every" => args.options.every = number(value("--every")?, "--every")?,
            "--window" => args.options.window = number(value("--window")?, "--window")?,
            "--max-live" => args.options.max_live = number(value("--max-live")?, "--max-live")?,
            "--max-failures" => {
                args.options.max_failures = number(value("--max-failures")?, "--max-failures")?
            }
            "--quiet" => args.quiet = true,
            "--normalize" => args.options.normalize = true,
            "--report-only" => args.report_only = true,
            "-h" | "--help" => return Err(String::new()),
            other => return Err(format!("unknown argument {other}")),
        }
    }
    if !args.corpus && args.seed.is_none() && args.journal.is_none() {
        return Err("give --seed, --journal or the command corpus".to_owned());
    }
    Ok(args)
}

fn journals(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut found: Vec<_> = std::fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "journal"))
        .collect();
    found.sort();
    Ok(found)
}

fn main() -> ExitCode {
    let args = match parse() {
        Ok(args) => args,
        Err(message) => {
            if !message.is_empty() {
                eprintln!("nh-snapcheck: {message}");
            }
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    let mut streams = Vec::new();
    if args.corpus {
        match journals(&args.dir) {
            Ok(found) => {
                for path in found {
                    match Stream::from_journal(&path) {
                        Ok(stream) => streams.push(stream),
                        Err(message) => {
                            eprintln!("nh-snapcheck: {message}");
                            return ExitCode::from(2);
                        }
                    }
                }
            }
            Err(message) => {
                eprintln!("nh-snapcheck: {message}");
                return ExitCode::from(2);
            }
        }
        streams.extend((0..args.seeds).map(|seed| Stream::from_seed(seed, args.bytes)));
    } else {
        if let Some(seed) = args.seed {
            streams.push(Stream::from_seed(seed, args.bytes));
        }
        if let Some(path) = &args.journal {
            match Stream::from_journal(path) {
                Ok(stream) => streams.push(stream),
                Err(message) => {
                    eprintln!("nh-snapcheck: {message}");
                    return ExitCode::from(2);
                }
            }
        }
    }
    let mut total = Report {
        stream: format!("all {} streams", streams.len()),
        ..Report::default()
    };
    for stream in &streams {
        let report = run(stream, &args.options);
        if !args.quiet || !report.exact() {
            print!("{}", report.summary());
        }
        total.absorb(report);
    }
    print!("{}", total.summary());
    // A difference of the style of a blank cell can show later in an engine that was not rebuilt
    // (see the library documentation). Any other difference fails the run.
    let pass = total.only_blank_cells_differ()
        && if args.report_only {
            total.instant_exact()
        } else {
            total.exact()
        };
    if pass {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
