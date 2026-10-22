//! Records small programs under a real holder. These tests need the `nh` binary, which
//! `cargo nextest run --workspace` builds. With `-p nh-record` alone, build it first:
//! `mise exec -- cargo build -p nh`.

use std::{
    fs::{self, File},
    io::BufReader,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use nh_proto::holder::{ExitStatus, HolderEvent, Marker, StreamId};
use nh_record::{
    holder::locate_nh,
    journal::{output_bytes, read_journal, resizes},
    recorder::{Options, record, record_to_file},
    script::parse_script,
};
use rustix::process::{Pid, test_kill_process};

/// The `nh` binary. The test fails, and says why, when it is not built.
fn nh() -> PathBuf {
    locate_nh(None).unwrap_or_else(|e| {
        panic!("{e}\nThe integration tests of nh-record need the holder: run `mise exec -- cargo build -p nh` first.")
    })
}

fn options(script: &str, argv: &[&str]) -> Options {
    Options {
        script: parse_script(script).expect("the script parses"),
        argv: argv.iter().map(|a| (*a).to_owned()).collect(),
        env: Vec::new(),
        cols: 100,
        rows: 30,
        nh: nh(),
        setup: None,
        timeout: Duration::from_secs(15),
    }
}

/// A file name in `/tmp` that no other test process uses.
fn temp_file(name: &str) -> PathBuf {
    PathBuf::from(format!("/tmp/nhr-test-{}-{name}", std::process::id()))
}

#[test]
fn a_recording_has_the_output_and_the_initial_size() {
    let out = temp_file("hello.journal");
    let recording = record_to_file(
        &options(
            "expect \"hello\"\nexpect_exit\n",
            &["printf", "hello\\r\\n"],
        ),
        &out,
    )
    .expect("the recording works");
    assert_eq!(recording.exit, Some(ExitStatus::Code(0)));

    let events = read_journal(&mut BufReader::new(
        File::open(&out).expect("the file exists"),
    ))
    .expect("the journal reads");
    let _ = fs::remove_file(&out);
    assert_eq!(
        events.first(),
        Some(&HolderEvent::Marker {
            offset: 0,
            marker: Marker::Resize {
                cols: 100,
                rows: 30
            }
        }),
        "the journal starts with the initial size"
    );
    let output = output_bytes(&events);
    assert!(
        String::from_utf8_lossy(&output).contains("hello"),
        "output: {output:?}"
    );
    // Offsets are those of the holder: output events follow each other without a gap.
    let mut next = 0;
    for event in &events {
        if let HolderEvent::Output {
            offset,
            stream,
            bytes,
        } = event
        {
            assert_eq!(*stream, StreamId::Pty);
            assert_eq!(*offset, next);
            next += bytes.len() as u64;
        }
    }
    assert_eq!(next as usize, output.len());
}

#[test]
fn input_and_resizes_reach_the_program_and_the_journal() {
    // `read` waits for a line. `stty size` then prints the size that the PTY has by then.
    let recording = record(&options(
        "expect \"ready\"\nresize 90 20\nsend \"go\\r\"\nexpect \"20 90\"\nexpect_exit\n",
        &["sh", "-c", "echo ready; read line; stty size"],
    ))
    .expect("the recording works");
    assert_eq!(recording.exit, Some(ExitStatus::Code(0)));
    let sizes: Vec<(u16, u16)> = resizes(&recording.events)
        .into_iter()
        .map(|(_, cols, rows)| (cols, rows))
        .collect();
    assert_eq!(sizes, vec![(100, 30), (90, 20)]);
    assert!(
        recording.events.iter().any(|e| matches!(
            e,
            HolderEvent::Marker {
                marker: Marker::InputAck { .. },
                ..
            }
        )),
        "the input was acknowledged in the journal"
    );
}

#[test]
fn a_program_that_is_still_running_is_stopped_at_the_end() {
    let recording = record(&options(
        "expect \"up\"\n",
        &["sh", "-c", "echo up; sleep 60"],
    ))
    .expect("the recording works");
    assert_eq!(recording.exit, None);
}

#[test]
fn a_failed_expect_names_the_step_and_stops_the_program() {
    let pid_file = temp_file("sleeper.pid");
    let mut options = options(
        "expect \"never appears\"\n",
        &[
            "sh",
            "-c",
            "echo $$ > \"$1\"; echo something else; exec sleep 60",
            "sh",
            &pid_file.display().to_string(),
        ],
    );
    options.timeout = Duration::from_secs(1);
    let error = record(&options).expect_err("the text never appears");
    let message = error.to_string();
    assert!(message.contains("step 1"), "{message}");
    assert!(message.contains("never appears"), "{message}");
    assert!(
        message.contains("something else"),
        "the tail of the output is shown: {message}"
    );

    let pid = fs::read_to_string(&pid_file)
        .expect("the program wrote its pid")
        .trim()
        .parse::<i32>()
        .expect("a pid");
    let _ = fs::remove_file(&pid_file);
    assert!(
        gone(pid),
        "the program {pid} is still running after the error"
    );
}

#[test]
fn an_expect_fails_at_once_when_the_program_has_exited() {
    let started = Instant::now();
    let error = record(&options("expect \"never\"\n", &["true"])).expect_err("the program ends");
    assert!(error.to_string().contains("exited"), "{error}");
    assert!(started.elapsed() < Duration::from_secs(10));
}

#[test]
fn an_expect_goes_forward_through_the_output() {
    // The second `expect` must not match the text before the first match.
    let error = record(&{
        let mut o = options("expect \"two\"\nexpect \"one\"\n", &["printf", "one two"]);
        o.timeout = Duration::from_secs(1);
        o
    })
    .expect_err("one is before two");
    assert!(error.to_string().contains("step 2"), "{error}");
}

#[test]
fn a_program_that_is_not_there_is_an_error() {
    let recording = record(&options("expect_exit\n", &["/no/such/program"]))
        .expect("the holder reports the failed start as an exit");
    assert!(matches!(recording.exit, Some(ExitStatus::ExecFailed(_))));
}

#[test]
fn nh_is_found_only_where_it_is() {
    assert!(locate_nh(Some(Path::new("/no/such/nh"))).is_err());
    assert!(locate_nh(Some(&nh())).is_ok());
}

/// Whether the process is gone, within a few seconds. A killed child can stay for a moment as a
/// zombie until the holder or `init` collects it.
fn gone(pid: i32) -> bool {
    let Some(pid) = Pid::from_raw(pid) else {
        return true;
    };
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if test_kill_process(pid).is_err() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}
