//! Runs a script against a program under a holder and collects the journal.

use std::{
    fs::File,
    io::BufWriter,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use nh_proto::holder::{ExitStatus, HolderEvent, HolderRequest, Marker, StreamId};

use crate::{
    RecordError,
    holder::{self, Connection, HolderProcess, HolderSpec, Scratch},
    journal::{self, escape_bytes},
    script::Step,
    signals,
};

/// How long the output must be quiet at the end of a script before the recording stops.
const QUIET: Duration = Duration::from_millis(300);
/// The longest wait for that quiet.
const SETTLE_MAX: Duration = Duration::from_secs(3);

/// What to record.
#[derive(Clone, Debug)]
pub struct Options {
    pub script: Vec<Step>,
    /// The program and its arguments. `{scratch}`, `{home}`, `{tmp}` and `{work}` in them stand
    /// for the scratch directories.
    pub argv: Vec<String>,
    /// Variables to add to the minimal environment, or to replace in it. The values may use the
    /// same placeholders as `argv`.
    pub env: Vec<(String, String)>,
    pub cols: u16,
    pub rows: u16,
    /// The `nh` binary.
    pub nh: PathBuf,
    /// A shell script that makes the files the program needs, run in the working directory.
    pub setup: Option<PathBuf>,
    /// The longest wait of one `expect` or `expect_exit` step.
    pub timeout: Duration,
}

/// The result of a recording.
#[derive(Clone, Debug)]
pub struct Recording {
    /// The journal: the synthetic initial `Resize` marker, then every `Output` event and every
    /// `Marker` event, in the order the holder sent them.
    pub events: Vec<HolderEvent>,
    /// How the program ended, or `None` when the script ended first and the tool stopped it.
    pub exit: Option<ExitStatus>,
}

impl Recording {
    /// The number of PTY output bytes.
    pub fn output_len(&self) -> usize {
        journal::output_bytes(&self.events).len()
    }

    /// The number of markers, the initial one included.
    pub fn marker_count(&self) -> usize {
        self.events
            .iter()
            .filter(|e| matches!(e, HolderEvent::Marker { .. }))
            .count()
    }
}

/// Records the program and returns the journal. The holder and the program are stopped, and the
/// scratch directory is removed, whether the recording works or not.
pub fn record(options: &Options) -> Result<Recording, RecordError> {
    let scratch = Scratch::create()?;
    let env = scratch.child_env(&options.env);
    let argv: Vec<String> = options.argv.iter().map(|a| scratch.expand(a)).collect();
    if argv.is_empty() {
        return Err(RecordError::Holder("there is no program to run".into()));
    }
    if let Some(setup) = &options.setup {
        holder::run_setup(setup, &scratch, &env)?;
    }
    let mut holder = HolderProcess::new(scratch)?;
    holder.launch(&HolderSpec {
        nh: &options.nh,
        argv: &argv,
        env: &env,
        cols: options.cols,
        rows: options.rows,
    })?;
    let mut conn = holder.connect()?;
    conn.send(&HolderRequest::Attach { from_offset: 0 })?;

    let mut session = Session {
        conn,
        holder: &mut holder,
        journal: vec![HolderEvent::Marker {
            offset: 0,
            marker: Marker::Resize {
                cols: options.cols,
                rows: options.rows,
            },
        }],
        output: Vec::new(),
        cursor: 0,
        exit: None,
        next_input_id: 1,
        timeout: options.timeout,
    };
    for (index, step) in options.script.iter().enumerate() {
        session.run_step(index + 1, step)?;
    }
    session.settle()?;
    if session.exit.is_some() {
        // The holder may exit now. The guard kills it if it does not.
        let _ = session.conn.send(&HolderRequest::ExitAck);
    }
    Ok(Recording {
        events: session.journal,
        exit: session.exit,
    })
}

/// Records and saves the journal file.
pub fn record_to_file(options: &Options, out: &Path) -> Result<Recording, RecordError> {
    let recording = record(options)?;
    let file = File::create(out).map_err(RecordError::io(format!("create {}", out.display())))?;
    let mut writer = BufWriter::new(file);
    journal::write_journal(&mut writer, &recording.events)?;
    std::io::Write::flush(&mut writer)
        .map_err(RecordError::io(format!("write {}", out.display())))?;
    Ok(recording)
}

struct Session<'a> {
    conn: Connection,
    holder: &'a mut HolderProcess,
    journal: Vec<HolderEvent>,
    /// All PTY output so far.
    output: Vec<u8>,
    /// Where the next `expect` starts to look: the end of the previous match.
    cursor: usize,
    exit: Option<ExitStatus>,
    next_input_id: u64,
    timeout: Duration,
}

impl Session<'_> {
    fn run_step(&mut self, number: usize, step: &Step) -> Result<(), RecordError> {
        match step {
            Step::Send(bytes) => {
                let input_id = self.next_input_id;
                self.next_input_id += 1;
                self.conn.send(&HolderRequest::Input {
                    input_id,
                    bytes: bytes.clone(),
                })
            }
            Step::Expect(needle) => self.expect(number, needle),
            Step::Wait(time) => {
                let deadline = Instant::now() + *time;
                while let Some(left) = deadline.checked_duration_since(Instant::now()) {
                    self.pump(left)?;
                }
                Ok(())
            }
            Step::Resize { cols, rows } => self.conn.send(&HolderRequest::Resize {
                cols: *cols,
                rows: *rows,
                px_width: 0,
                px_height: 0,
            }),
            Step::ExpectExit => {
                let deadline = Instant::now() + self.timeout;
                while self.exit.is_none() {
                    let left = deadline.saturating_duration_since(Instant::now());
                    if left.is_zero() {
                        return Err(self.failure(
                            number,
                            "expect_exit",
                            "the program is still running",
                        ));
                    }
                    self.pump(left)?;
                }
                Ok(())
            }
        }
    }

    fn expect(&mut self, number: usize, needle: &[u8]) -> Result<(), RecordError> {
        let deadline = Instant::now() + self.timeout;
        loop {
            if let Some(at) = find(&self.output[self.cursor..], needle) {
                self.cursor += at + needle.len();
                return Ok(());
            }
            if let Some(status) = self.exit {
                return Err(self.failure(
                    number,
                    &format!("expect \"{}\"", escape_bytes(needle)),
                    &format!("the program exited ({status:?}) first"),
                ));
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(self.failure(
                    number,
                    &format!("expect \"{}\"", escape_bytes(needle)),
                    &format!("the text did not appear within {:?}", self.timeout),
                ));
            }
            self.pump(left.min(Duration::from_millis(200)))?;
        }
    }

    /// An error for a step, with the output that the step searched.
    fn failure(&self, number: usize, step: &str, why: &str) -> RecordError {
        let rest = &self.output[self.cursor..];
        let tail = &rest[rest.len().saturating_sub(800)..];
        RecordError::Step(format!(
            "step {number} ({step}) failed: {why}.\n\
             Output since the last match ({} bytes), last {} bytes:\n{}",
            rest.len(),
            tail.len(),
            escape_bytes(tail)
        ))
    }

    /// Reads until the output has been quiet for [`QUIET`], for at most [`SETTLE_MAX`].
    fn settle(&mut self) -> Result<(), RecordError> {
        let start = Instant::now();
        let mut last_activity = start;
        while self.exit.is_none() {
            let before = self.journal.len();
            self.pump(Duration::from_millis(50))?;
            let now = Instant::now();
            if self.journal.len() != before {
                last_activity = now;
            }
            if now - last_activity >= QUIET || now - start >= SETTLE_MAX {
                break;
            }
        }
        Ok(())
    }

    /// Reads at most one event, waiting at most `within`.
    fn pump(&mut self, within: Duration) -> Result<(), RecordError> {
        if signals::interrupted() {
            return Err(RecordError::Interrupted);
        }
        let Some(event) = self.conn.recv(within)? else {
            return Ok(());
        };
        match event {
            HolderEvent::Output {
                stream: StreamId::Pty,
                ref bytes,
                ..
            } => {
                self.output.extend_from_slice(bytes);
                self.journal.push(event);
            }
            HolderEvent::Marker { .. } => self.journal.push(event),
            HolderEvent::ChildExited { status } => {
                self.exit = Some(status);
                self.holder.child_exited = true;
            }
            HolderEvent::Truncated { requested, start } => {
                return Err(RecordError::Holder(format!(
                    "the holder's ring dropped output the tool had not read (asked for {requested}, \
                     ring starts at {start})"
                )));
            }
            HolderEvent::LeaseLost { by_fencing_token } => {
                return Err(RecordError::Holder(format!(
                    "another client took the holder (fencing token {by_fencing_token})"
                )));
            }
            HolderEvent::Refused { reason } => {
                return Err(RecordError::Holder(format!(
                    "the holder refused: {reason:?}"
                )));
            }
            // Output of other streams (pipe mode), acknowledgements, checkpoint requests and the
            // other replies do not belong in the journal file.
            _ => {}
        }
        Ok(())
    }
}

/// The index of the first occurrence of `needle` in `haystack`.
fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_returns_the_first_match() {
        assert_eq!(find(b"abcabc", b"bc"), Some(1));
        assert_eq!(find(b"abc", b"abcd"), None);
        assert_eq!(find(b"", b"a"), None);
        assert_eq!(find(b"\x1b[1mhi", b"hi"), Some(4));
    }
}
