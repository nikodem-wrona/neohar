//! A core without a window, for the measurement of the hook shim (`scripts/bench-shim.sh`).
//!
//! It starts the real `nh-core` with its API socket under `<runtime dir>` and waits to be killed.
//! The first line on stdout is `READY <path of api.sock>`. Then it writes `SIGNAL <harness> <event>`
//! each time the newest hook signal changes, so that a measurement can check that the core
//! received what the shim sent.
//!
//! Usage: `api_core <runtime dir> <session>`

use std::{io::Write, path::PathBuf};

use nh_core::{Core, CoreConfig};
use nh_proto::paths::Env;

fn main() {
    let mut args = std::env::args().skip(1);
    let (Some(runtime), Some(session)) = (args.next(), args.next()) else {
        eprintln!("usage: api_core <runtime dir> <session>");
        std::process::exit(2);
    };
    let mut paths = Env::from_process();
    paths.runtime_override = Some(PathBuf::from(runtime));
    let socket = paths
        .api_socket(&session)
        .expect("the session name and the runtime directory make a socket path");
    let config = CoreConfig {
        session,
        paths,
        nh_path: PathBuf::from("/nonexistent/nh"),
        base_env: Vec::new(),
        tuning: nh_core::Tuning::default(),
        // A core with no host in its process, as `nh daemon`.
        host: None,
        // The token that `scripts/shim-bench.ts` puts in the environment of the shim.
        pane_tokens: vec![(ulid::Ulid::from(1u128), "bench-token".to_owned())],
    };
    let core = Core::start(config).expect("the core starts");
    let handle = core.handle();

    let mut out = std::io::stdout();
    writeln!(out, "READY {}", socket.display()).expect("stdout is open");
    out.flush().expect("stdout is open");
    let mut last = None;
    loop {
        std::thread::sleep(std::time::Duration::from_millis(20));
        let newest = handle.recent_signals().pop();
        if newest != last {
            if let Some(signal) = &newest {
                writeln!(out, "SIGNAL {} {}", signal.harness, signal.event)
                    .expect("stdout is open");
                out.flush().expect("stdout is open");
            }
            last = newest;
        }
    }
}
