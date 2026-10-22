//! `nh-keytest`: serves the keyboard fidelity test page and the encoder to a browser.
//! See the library for what a case is.

use std::{net::TcpListener, path::PathBuf, process::ExitCode};

use nh_keytest::{
    load_cases,
    server::{Config, serve},
};

const USAGE: &str = "\
usage: nh-keytest --web DIR --cases DIR [--port N]
       nh-keytest --cases DIR --check

options:
  --web DIR    the built test page (test.html, test.js, capture.html, capture.js)
  --cases DIR  the directory of the *.toml cases (tests/keyboard/cases)
  --port N     the port on 127.0.0.1 (default 4174)
  --check      load the cases, print how many there are, and exit";

fn main() -> ExitCode {
    let mut web = None;
    let mut cases = None;
    let mut port = 4174u16;
    let mut check = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let value = match arg.as_str() {
            "--check" => {
                check = true;
                continue;
            }
            "--web" | "--cases" | "--port" => args.next(),
            _ => None,
        };
        match (arg.as_str(), value) {
            ("--web", Some(v)) => web = Some(PathBuf::from(v)),
            ("--cases", Some(v)) => cases = Some(PathBuf::from(v)),
            ("--port", Some(v)) => match v.parse() {
                Ok(p) => port = p,
                Err(_) => return usage("--port needs a number"),
            },
            _ => return usage(&format!("unknown or incomplete argument {arg}")),
        }
    }
    let Some(cases) = cases else {
        return usage("--cases is required");
    };
    if check {
        return match load_cases(&cases) {
            Ok(list) => {
                println!("{} cases", list.len());
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("nh-keytest: {e}");
                ExitCode::FAILURE
            }
        };
    }
    let Some(web) = web else {
        return usage("--web is required");
    };
    let listener = match TcpListener::bind(("127.0.0.1", port)) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("nh-keytest: cannot listen on 127.0.0.1:{port}: {e}");
            return ExitCode::FAILURE;
        }
    };
    eprintln!("nh-keytest on http://127.0.0.1:{port}");
    serve(listener, Config { web, cases });
    ExitCode::SUCCESS
}

fn usage(message: &str) -> ExitCode {
    eprintln!("nh-keytest: {message}\n{USAGE}");
    ExitCode::from(2)
}
