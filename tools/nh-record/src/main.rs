//! The `nh-record` binary. See the library for what it does and `tests/vt-corpus/README.md` for
//! how the corpus is recorded.

use std::{fs, fs::File, io::BufReader, process::ExitCode};

use nh_record::{
    RecordError,
    cli::{self, Command, USAGE},
    holder::locate_nh,
    journal,
    recorder::{self, Options},
    script, signals,
};

fn main() -> ExitCode {
    signals::install();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = match cli::parse_args(&args) {
        Ok(command) => command,
        Err(message) => {
            eprintln!("nh-record: {message}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    match run(command) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("nh-record: {e}");
            ExitCode::from(1)
        }
    }
}

fn run(command: Command) -> Result<(), RecordError> {
    match command {
        Command::Dump { file } => {
            let open = RecordError::io(format!("open {}", file.display()));
            let events =
                journal::read_journal(&mut BufReader::new(File::open(&file).map_err(open)?))?;
            print!("{}", journal::dump(&events));
            Ok(())
        }
        Command::Record {
            out,
            script,
            cols,
            rows,
            nh,
            env,
            setup,
            timeout,
            argv,
        } => {
            let text = fs::read_to_string(&script)
                .map_err(RecordError::io(format!("read {}", script.display())))?;
            let steps = script::parse_script(&text)?;
            let options = Options {
                script: steps,
                argv,
                env,
                cols,
                rows,
                nh: locate_nh(nh.as_deref())?,
                setup,
                timeout,
            };
            let recording = recorder::record_to_file(&options, &out)?;
            let ended = recording
                .exit
                .map_or("stopped by the tool".to_owned(), |s| format!("{s:?}"));
            println!(
                "recorded {} bytes of output and {} markers to {} (program: {ended})",
                recording.output_len(),
                recording.marker_count(),
                out.display()
            );
            Ok(())
        }
    }
}
