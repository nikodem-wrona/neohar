//! `nh-bench`: tools for the render tests.
//!
//! ```text
//! nh-bench gen-frames --scenario screen|redraw|spinner|scroll|glyphs --out FILE
//!                     [--panes N] [--cols C] [--rows R] [--frames F] [--seed S]
//! ```

use std::process::ExitCode;

use nh_bench::{
    generate,
    scenes::{Params, Scenario},
    write,
};

fn usage() -> String {
    "usage: nh-bench gen-frames --scenario screen|redraw|spinner|scroll|glyphs --out FILE \
     [--panes N] [--cols C] [--rows R] [--frames F] [--seed S]"
        .to_owned()
}

fn parse_gen_frames(args: &[String]) -> Result<(Params, String), String> {
    let mut scenario = None;
    let mut out = None;
    let mut params = Params {
        scenario: Scenario::Screen,
        panes: 1,
        cols: 200,
        rows: 60,
        frames: 120,
        seed: 1,
    };
    let mut rest = args.iter();
    while let Some(flag) = rest.next() {
        let value = rest.next().ok_or_else(|| format!("{flag} needs a value"))?;
        let number = || {
            value
                .parse::<u64>()
                .map_err(|_| format!("{flag}: {value} is not a number"))
        };
        match flag.as_str() {
            "--scenario" => {
                scenario = Some(
                    Scenario::parse(value).ok_or_else(|| format!("unknown scenario {value}"))?,
                );
            }
            "--out" => out = Some(value.clone()),
            "--panes" => params.panes = number()? as u32,
            "--cols" => params.cols = number()? as u16,
            "--rows" => params.rows = number()? as u16,
            "--frames" => params.frames = number()? as u32,
            "--seed" => params.seed = number()?,
            other => return Err(format!("unknown flag {other}\n{}", usage())),
        }
    }
    params.scenario = scenario.ok_or_else(usage)?;
    if params.panes == 0 || params.cols < 20 || params.rows < 6 {
        return Err("the grid must have at least 20 columns and 6 rows, and one pane".into());
    }
    if params.scenario == Scenario::Screen && (params.cols < 64 || params.rows < 20) {
        return Err("the screen scenario needs at least 64 columns and 20 rows".into());
    }
    Ok((params, out.ok_or_else(usage)?))
}

fn gen_frames(args: &[String]) -> Result<(), String> {
    let (params, out) = parse_gen_frames(args)?;
    let container = generate(&params);
    let bytes = write(&container);
    std::fs::write(&out, &bytes).map_err(|e| format!("cannot write {out}: {e}"))?;
    eprintln!(
        "{out}: {} frames, {} bytes",
        container.records.len(),
        bytes.len()
    );
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some((command, rest)) = args.split_first() else {
        eprintln!("{}", usage());
        return ExitCode::from(2);
    };
    let result = match command.as_str() {
        "gen-frames" => gen_frames(rest),
        _ => Err(usage()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(2)
        }
    }
}
