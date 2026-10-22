//! `nh-chaos`: the kill and relaunch test of the core.
//!
//! ```text
//! nh-chaos run  [--cycles N] [--seed N] [--dir DIR] [--nh PATH] [--fixture PATH] [--keep] ...
//! nh-chaos host --info FILE --nh PATH [--session NAME] [--ring-bytes N] ...
//! nh-chaos clean --dir DIR
//! ```
//!
//! `run` is the test. It starts `host` as a child process, kills it with `SIGKILL` and starts it
//! again. See [`chaos`] for what it checks. `clean` kills the holders of a run that ended without
//! cleaning up.

mod chaos;
mod host;
mod journal;
mod panes;
mod rng;
mod ws;

use std::{path::PathBuf, process::ExitCode};

use anyhow::{Context, Result, bail};

const USAGE: &str = "\
Usage:
  nh-chaos run   [--cycles N] [--seed N] [--dir DIR] [--nh PATH] [--fixture PATH] [--keep]
                 [--ring-mib N] [--checkpoint-idle-ms N] [--checkpoint-interval-ms N]
                 [--load-min-ms N] [--load-max-ms N] [--out FILE]
  nh-chaos host  --info FILE --nh PATH [--session NAME] [--ring-bytes N]
                 [--checkpoint-idle-ms N] [--checkpoint-interval-ms N]
  nh-chaos clean (--dir DIR | --data-dir DIR --session NAME)

'run' starts 20 panes (vim, less, htop, a tui and echo-seq) under a host, kills the host with
SIGKILL at random moments, starts it again, and compares each restored screen with a reference
engine that read the whole journal. It exits 1 when a cell differs or an input is lost or applied
twice. The defaults: 100 cycles, a random seed, /tmp/nhch-<pid>, nh and nh-fixture next to this
program, a ring of 16 MiB, checkpoints after 1 s of quiet and at most every 3 s, a load of 0.5 to 5 s.
";

/// The options of a command, as `--name value` pairs and flags.
struct Args {
    rest: Vec<String>,
}

impl Args {
    fn take(&mut self, name: &str) -> Result<Option<String>> {
        let Some(at) = self.rest.iter().position(|a| a == name) else {
            return Ok(None);
        };
        if at + 1 >= self.rest.len() {
            bail!("{name} needs a value");
        }
        self.rest.remove(at);
        Ok(Some(self.rest.remove(at)))
    }

    fn flag(&mut self, name: &str) -> bool {
        match self.rest.iter().position(|a| a == name) {
            Some(at) => {
                self.rest.remove(at);
                true
            }
            None => false,
        }
    }

    fn number(&mut self, name: &str, default: u64) -> Result<u64> {
        match self.take(name)? {
            Some(text) => text
                .parse()
                .with_context(|| format!("{name}: {text:?} is not a number")),
            None => Ok(default),
        }
    }

    fn done(&self) -> Result<()> {
        match self.rest.first() {
            Some(extra) => bail!("unknown argument {extra:?}"),
            None => Ok(()),
        }
    }
}

/// The `nh` binary next to this program.
fn sibling(name: &str) -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join(name)))
        .unwrap_or_else(|| PathBuf::from(name))
}

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args[0] == "-h" || args[0] == "--help" {
        print!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    let command = args.remove(0);
    match dispatch(&command, Args { rest: args }) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("nh-chaos: {error:#}");
            ExitCode::from(2)
        }
    }
}

fn dispatch(command: &str, mut args: Args) -> Result<ExitCode> {
    match command {
        "host" => {
            let host_args = host::HostArgs {
                info_file: args.take("--info")?.context("--info is required")?.into(),
                nh: args.take("--nh")?.context("--nh is required")?.into(),
                session: args
                    .take("--session")?
                    .unwrap_or_else(|| chaos::SESSION.to_owned()),
                ring_bytes: args.number("--ring-bytes", 16 << 20)?,
                checkpoint_idle_ms: args.number("--checkpoint-idle-ms", 1_000)?,
                checkpoint_interval_ms: args.number("--checkpoint-interval-ms", 3_000)?,
            };
            args.done()?;
            host::run(&host_args)?;
            Ok(ExitCode::SUCCESS)
        }
        "clean" => {
            let run = args.take("--dir")?.map(PathBuf::from);
            let data = args.take("--data-dir")?.map(PathBuf::from);
            let session = args.take("--session")?;
            args.done()?;
            match (run, data, session) {
                (Some(dir), None, None) => chaos::kill_holders(&dir),
                (None, Some(data), Some(session)) => chaos::kill_holders_of(&data.join(session)),
                _ => bail!("give --dir, or --data-dir with --session"),
            }
            Ok(ExitCode::SUCCESS)
        }
        "run" => run(args),
        other => bail!("unknown command {other:?}; run 'nh-chaos --help'"),
    }
}

fn run(mut args: Args) -> Result<ExitCode> {
    let seed = match args.take("--seed")? {
        Some(text) => text.parse().context("--seed is not a number")?,
        None => rng::Rng::random_seed(),
    };
    let dir: PathBuf = args.take("--dir")?.map_or_else(
        || PathBuf::from(format!("/tmp/nhch-{}", std::process::id())),
        PathBuf::from,
    );
    // A socket path must stay below 104 bytes on macOS, and the longest path under the directory
    // is about 60 bytes: `/run/chaos/holders/<26 characters>.sock`.
    if dir.as_os_str().len() > 36 {
        bail!("--dir is too long: the holder sockets under it would not fit in a socket path");
    }
    let out: Option<PathBuf> = args.take("--out")?.map(Into::into);
    let options = chaos::Options {
        cycles: u32::try_from(args.number("--cycles", 100)?)?,
        seed,
        dir: dir.clone(),
        nh: args
            .take("--nh")?
            .map_or_else(|| sibling("nh"), PathBuf::from),
        fixture: args.take("--fixture")?.map(PathBuf::from),
        ring_bytes: args.number("--ring-mib", 16)? << 20,
        checkpoint_idle_ms: args.number("--checkpoint-idle-ms", 1_000)?,
        checkpoint_interval_ms: args.number("--checkpoint-interval-ms", 3_000)?,
        load_ms: (
            args.number("--load-min-ms", 500)?,
            args.number("--load-max-ms", 5_000)?,
        ),
        keep: args.flag("--keep"),
    };
    args.done()?;
    if !options.nh.is_file() {
        bail!(
            "{} does not exist; build it with `cargo build -p nh`",
            options.nh.display()
        );
    }
    if options.load_ms.0 > options.load_ms.1 {
        bail!("--load-min-ms is above --load-max-ms");
    }
    eprintln!(
        "nh-chaos: seed {seed}, {} cycles, directory {}",
        options.cycles,
        dir.display()
    );
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let summary = runtime.block_on(chaos::run(options))?;
    let json = serde_json::to_string_pretty(&summary)?;
    let path = out.unwrap_or_else(|| dir.join("summary.json"));
    std::fs::write(&path, &json)?;
    print_summary(&summary, &path);
    Ok(if summary.passed() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}

fn print_summary(s: &chaos::Summary, path: &std::path::Path) {
    let stat = |label: &str, v: &chaos::Stats| {
        println!(
            "  {label:<22} n {:>3}  min {:>4}  median {:>4}  p95 {:>4}  max {:>4}",
            v.n, v.min, v.median, v.p95, v.max
        );
    };
    println!("\nnh-chaos: seed {}", s.seed);
    println!(
        "  {} cycles, {} restores, {} kills during a restore, {} panes (20 in the gate)",
        s.cycles, s.restores, s.kills_during_restore, s.panes
    );
    println!(
        "  {} screens compared, {} with differences, {} cells differ ({} are blank cells that differ in style, {} are before the nudge)",
        s.screens_checked,
        s.screens_with_differences,
        s.cell_differences,
        s.blank_style_only,
        s.cell_differences_before_nudge
    );
    println!(
        "  the pane that does not redraw (outside the verdict): {} screens, {} cells differ",
        s.informational_screens_with_differences, s.informational_cell_differences
    );
    println!(
        "  recovery: {:?}, {} bytes replayed",
        s.recoveries, s.replay_bytes_total
    );
    stat("restore, wall (ms)", &s.restore_wall_ms);
    stat("restore, core (ms)", &s.restore_core_ms);
    stat("settled (ms)", &s.settle_ms);
    println!(
        "  inputs: {} sent, {} acked before a kill, {} sent again, {} given up, {} missing, {} applied twice, {} stray bytes",
        s.inputs_sent,
        s.inputs_acked_before_kill,
        s.inputs_resent,
        s.inputs_abandoned,
        s.inputs_missing.len(),
        s.inputs_applied_twice.len(),
        s.inputs_unknown_bytes
    );
    println!(
        "  {}: {}",
        if s.passed() { "PASS" } else { "FAIL" },
        path.display()
    );
}
