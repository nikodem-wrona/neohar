//! A real holder under a real PTY: output, input, resize, reconnect and loss of the spawner.

mod support;

use std::time::{Duration, Instant};

use nh_proto::holder::{ExitStatus, HolderEvent, HolderRequest, InputResult, Marker, RefuseReason};
use rustix::process::{Pid, Signal, WaitOptions, kill_process, waitpid};
use support::{TestHolder, count, hello};

#[test]
fn a_client_that_attaches_at_zero_receives_the_output() {
    let holder = TestHolder::spawn(&["/bin/sh", "-c", "printf hello; sleep 30"]);
    let mut client = holder.attach(0);
    let (out, end) = client.output_containing("hello");
    assert_eq!(out, b"hello");
    assert_eq!(end, 5);
}

#[test]
fn input_is_written_once_and_a_repeated_id_is_a_duplicate() {
    let holder = TestHolder::spawn(&["/bin/sh", "-c", "stty -echo; echo ready; cat"]);
    let mut client = holder.attach(0);
    client.output_containing("ready");
    client.send(&HolderRequest::Input {
        input_id: 1,
        bytes: b"abc\n".to_vec(),
    });
    let (offset, result) = client.wait_for(|e| match e {
        HolderEvent::InputAck {
            input_id: 1,
            offset,
            result,
        } => Some((*offset, *result)),
        _ => None,
    });
    assert_eq!(result, InputResult::Written);
    client.send(&HolderRequest::Input {
        input_id: 1,
        bytes: b"abc\n".to_vec(),
    });
    let duplicate = client.wait_for(|e| match e {
        HolderEvent::InputAck {
            input_id: 1,
            offset,
            result: InputResult::Duplicate,
        } => Some(*offset),
        _ => None,
    });
    assert_eq!(
        duplicate, offset,
        "a duplicate reports the offset of the first ack"
    );
    // Another id, as a barrier: when its output comes back, the duplicate has had its chance.
    client.send(&HolderRequest::Input {
        input_id: 2,
        bytes: b"end\n".to_vec(),
    });
    let (out, _) = client.output_containing("end");
    // `ready` is in the journal. After it comes `abc` once, then `end`.
    let text = String::from_utf8_lossy(&out).into_owned();
    assert_eq!(count(&out, "abc"), 1, "output: {text:?}");
}

#[test]
fn a_reconnecting_client_loses_no_byte_and_repeats_none() {
    let go = format!("/tmp/nhh-go-{}", std::process::id());
    let _ = std::fs::remove_file(&go);
    let script = format!("echo one; while [ ! -f {go} ]; do sleep 0.05; done; echo two; sleep 30");
    let holder = TestHolder::spawn(&["/bin/sh", "-c", &script]);
    let mut first = holder.attach(0);
    // The whole line: a PTY can deliver the text and its line ending in two reads.
    let (out, end) = first.output_containing("one\r\n");
    assert_eq!(out, b"one\r\n");
    drop(first);
    std::fs::write(&go, "").expect("the trigger file is written");
    // Give the child time to print while nobody is attached.
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut second = holder.attach(end);
    let (more, more_end) = second.output_containing("two\r\n");
    assert!(Instant::now() < deadline);
    assert_eq!(more, b"two\r\n");
    assert_eq!(more_end, end + 5);
    let _ = std::fs::remove_file(&go);
}

#[test]
fn a_ring_overflow_reports_the_new_start() {
    let mut config = support::config(&[
        "/bin/sh",
        "-c",
        "head -c 3000 /dev/zero | tr '\\0' x; echo done; sleep 30",
    ]);
    config.ring_bytes = 256;
    let holder = TestHolder::spawn_with(config, None);
    // Wait until the output is complete: attach again and again is not needed. Poll the status.
    let mut probe = holder.client();
    let end = loop {
        probe.send(&HolderRequest::Status);
        let status = probe.wait_for(|e| match e {
            HolderEvent::StatusReply(s) => Some(s.clone()),
            _ => None,
        });
        if status.journal.end >= 3000 {
            break status.journal.end;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let mut client = holder.attach(0);
    let mut others = Vec::new();
    let (out, out_end) = client.output_until(|o| o.len() >= 256, &mut others);
    let start = others
        .iter()
        .find_map(|e| match e {
            HolderEvent::Truncated {
                requested: 0,
                start,
            } => Some(*start),
            _ => None,
        })
        .expect("a Truncated event");
    assert_eq!(start, end - 256);
    assert_eq!((out.len() as u64, out_end), (256, end));
}

#[test]
fn a_wrong_secret_is_refused_and_the_connection_closes() {
    let holder = TestHolder::spawn(&["/bin/sh", "-c", "sleep 30"]);
    let mut client = holder.connect();
    client.send(&hello(&[0; 32], 1));
    assert!(matches!(
        client.recv(),
        HolderEvent::Refused {
            reason: RefuseReason::BadSecret
        }
    ));
    assert!(client.closed_within(Duration::from_secs(5)));
}

#[test]
fn a_client_that_says_nothing_is_closed() {
    let holder = TestHolder::spawn(&["/bin/sh", "-c", "sleep 30"]);
    let mut client = holder.connect();
    assert!(client.closed_within(Duration::from_secs(5)));
}

#[test]
fn a_resize_reaches_the_child() {
    let holder = TestHolder::spawn(&["/bin/sh", "-c", "read x; stty size; sleep 30"]);
    let mut client = holder.attach(0);
    client.send(&HolderRequest::Resize {
        cols: 100,
        rows: 30,
        px_width: 0,
        px_height: 0,
    });
    client.send(&HolderRequest::Input {
        input_id: 1,
        bytes: b"\n".to_vec(),
    });
    let (out, _) = client.output_containing("30 100");
    assert!(count(&out, "30 100") >= 1);
    // The resize is a marker in the journal, before the bytes that the child printed after it.
    let mut replay = holder.attach(0);
    let mut others = Vec::new();
    replay.output_until(|o| count(o, "30 100") >= 1, &mut others);
    assert!(others.iter().any(|e| matches!(
        e,
        HolderEvent::Marker {
            marker: Marker::Resize {
                cols: 100,
                rows: 30
            },
            ..
        }
    )));
}

#[test]
fn the_holder_keeps_serving_after_its_spawner_is_killed() {
    // The shell is the spawner. It stays alive after it starts the holder, like the core does.
    let config = support::config(&["/bin/sh", "-c", "echo up; sleep 30"]);
    let holder = TestHolder::spawn_with(config, Some("HOLD; exec sleep 600"));
    let spawner = holder.spawner_pid.expect("the test has a spawner");
    assert_ne!(spawner, holder.holder_pid);
    let mut client = holder.attach(0);
    client.output_containing("up");
    drop(client);
    let spawner = Pid::from_raw(spawner).expect("a pid");
    kill_process(spawner, Signal::KILL).expect("the spawner is killed");
    // The spawner is a child of this test. Collect it, so that it is not a zombie.
    waitpid(Some(spawner), WaitOptions::empty()).expect("the spawner is collected");
    let mut again = holder.attach(0);
    let (out, _) = again.output_containing("up\r\n");
    assert_eq!(out, b"up\r\n");
}

#[test]
fn a_missing_program_is_reported_as_an_exec_failure() {
    let holder = TestHolder::spawn(&["/nonexistent/program"]);
    let mut client = holder.attach(0);
    let status = client.wait_for(|e| match e {
        HolderEvent::ChildExited { status } => Some(*status),
        _ => None,
    });
    assert_eq!(status, ExitStatus::ExecFailed(2));
}

#[test]
fn the_exit_of_a_child_comes_after_its_output_and_the_holder_leaves_on_ack() {
    let holder = TestHolder::spawn(&["/bin/sh", "-c", "printf bye; exit 3"]);
    let mut client = holder.attach(0);
    let mut others = Vec::new();
    let (out, _) = client.output_until(|o| o == b"bye", &mut others);
    assert_eq!(out, b"bye");
    let status = client.wait_for(|e| match e {
        HolderEvent::ChildExited { status } => Some(*status),
        _ => None,
    });
    assert_eq!(status, ExitStatus::Code(3));
    client.send(&HolderRequest::ExitAck);
    let pid = rustix::process::Pid::from_raw(holder.holder_pid).expect("a pid");
    let deadline = Instant::now() + Duration::from_secs(5);
    while rustix::process::test_kill_process(pid).is_ok() {
        assert!(
            Instant::now() < deadline,
            "the holder is still running after ExitAck"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn the_socket_and_its_directory_are_private() {
    use std::os::unix::fs::PermissionsExt;
    let holder = TestHolder::spawn(&["/bin/sh", "-c", "sleep 30"]);
    let mode =
        |p: &std::path::Path| std::fs::metadata(p).expect("metadata").permissions().mode() & 0o7777;
    assert_eq!(mode(&holder.socket), 0o600);
    assert_eq!(mode(holder.socket.parent().expect("a directory")), 0o700);
}
