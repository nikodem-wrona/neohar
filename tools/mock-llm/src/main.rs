//! `mock-llm`: a scripted model server on 127.0.0.1.
//!
//! ```text
//! mock-llm --scenario <file.toml> [--port N] [--log <file.jsonl>]
//! ```
//!
//! It prints `LISTENING http://127.0.0.1:<port>` on standard output when it is ready, and it runs
//! until it is killed. The format of a scenario is in [`mock_llm::scenario`].

use std::{net::SocketAddr, path::PathBuf, process::ExitCode};

use mock_llm::{LogSink, Scenario, serve};

const USAGE: &str = "usage: mock-llm --scenario <file.toml> [--port N] [--log <file.jsonl>]";

struct Args {
    scenario: PathBuf,
    port: u16,
    log: Option<PathBuf>,
}

fn parse(args: &[String]) -> Result<Args, String> {
    let (mut scenario, mut port, mut log) = (None, 0u16, None);
    let mut it = args.iter();
    while let Some(flag) = it.next() {
        let mut value = || {
            it.next()
                .cloned()
                .ok_or_else(|| format!("{flag} needs a value"))
        };
        match flag.as_str() {
            "--scenario" => scenario = Some(PathBuf::from(value()?)),
            "--port" => {
                let text = value()?;
                port = text
                    .parse()
                    .map_err(|_| format!("{text:?} is not a port"))?;
            }
            "--log" => log = Some(PathBuf::from(value()?)),
            "-h" | "--help" => return Err(USAGE.to_owned()),
            other => return Err(format!("unknown argument {other:?}")),
        }
    }
    Ok(Args {
        scenario: scenario.ok_or("--scenario is required")?,
        port,
        log,
    })
}

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args = match parse(&args) {
        Ok(args) => args,
        Err(message) => {
            eprintln!("mock-llm: {message}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    let result = async {
        let text = std::fs::read_to_string(&args.scenario)
            .map_err(|e| format!("cannot read {}: {e}", args.scenario.display()))?;
        let scenario = Scenario::parse(&text).map_err(|e| e.to_string())?;
        let log = match &args.log {
            Some(path) => {
                LogSink::file(path).map_err(|e| format!("cannot write {}: {e}", path.display()))?
            }
            None => LogSink::Discard,
        };
        let bind = SocketAddr::from(([127, 0, 0, 1], args.port));
        serve(scenario, log, bind)
            .await
            .map_err(|e| format!("cannot listen: {e}"))
    }
    .await;
    match result {
        Ok(server) => {
            println!("LISTENING {}", server.url());
            // Run until the process is killed.
            std::future::pending::<()>().await;
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("mock-llm: {message}");
            ExitCode::from(1)
        }
    }
}
