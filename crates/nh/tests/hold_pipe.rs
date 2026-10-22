//! Pipe mode: a headless process on three pipes, one journal for all of them.

mod support;

use std::time::Duration;

use nh_proto::holder::{
    ExitStatus, HoldConfig, HoldMode, HoldModeKind, HolderEvent, HolderRequest, Marker, StreamId,
};
use support::{TestHolder, config, hello};

fn pipe_config(script: &str) -> HoldConfig {
    let mut config = config(&["/bin/sh", "-c", script]);
    config.mode = HoldMode::Pipe;
    config
}

/// The output events as `(stream, bytes)`, with neighbours of one stream joined.
fn runs(events: &[HolderEvent]) -> Vec<(StreamId, String)> {
    let mut out: Vec<(StreamId, String)> = Vec::new();
    for event in events {
        if let HolderEvent::Output { stream, bytes, .. } = event {
            let text = String::from_utf8_lossy(bytes).into_owned();
            match out.last_mut() {
                Some((last, joined)) if last == stream => joined.push_str(&text),
                _ => out.push((*stream, text)),
            }
        }
    }
    out
}

fn markers(events: &[HolderEvent], keep: impl Fn(&Marker) -> bool) -> Vec<(u64, Marker)> {
    events
        .iter()
        .filter_map(|e| match e {
            HolderEvent::Marker { offset, marker } if keep(marker) => Some((*offset, *marker)),
            _ => None,
        })
        .collect()
}

/// Reads events until the output has `text` in the last stream run.
fn until_run_ends_with(
    client: &mut support::Client,
    stream: StreamId,
    text: &str,
) -> Vec<HolderEvent> {
    let mut events = Vec::new();
    loop {
        events.push(client.recv());
        if runs(&events)
            .last()
            .is_some_and(|(s, t)| *s == stream && t.ends_with(text))
        {
            return events;
        }
    }
}

const SCRIPT: &str = "echo out1; sleep 0.3; echo err1 >&2; sleep 0.3; read line; echo got:$line; \
                      sleep 0.3; echo err2 >&2; sleep 30";

#[test]
fn stdin_stdout_and_stderr_replay_in_the_order_they_happened() {
    let holder = TestHolder::spawn_with(pipe_config(SCRIPT), None);
    let mut client = holder.attach(0);
    let mut events = until_run_ends_with(&mut client, StreamId::Stderr, "err1\n");
    client.send(&HolderRequest::Input {
        input_id: 1,
        bytes: b"x\n".to_vec(),
    });
    events.extend(until_run_ends_with(&mut client, StreamId::Stderr, "err2\n"));
    // The marker of the last cut point follows the last bytes.
    events.extend(client.events_within(Duration::from_millis(300)));

    assert_eq!(
        runs(&events),
        [
            (StreamId::Stdout, "out1\n".to_owned()),
            (StreamId::Stderr, "err1\n".to_owned()),
            (StreamId::Stdin, "x\n".to_owned()),
            (StreamId::Stdout, "got:x\n".to_owned()),
            (StreamId::Stderr, "err2\n".to_owned()),
        ]
    );
    let switches = markers(&events, |m| matches!(m, Marker::Stream { .. }));
    let expected: Vec<_> = [
        (0, StreamId::Stdout),
        (5, StreamId::Stderr),
        (10, StreamId::Stdin),
        (12, StreamId::Stdout),
        (18, StreamId::Stderr),
    ]
    .into_iter()
    .map(|(offset, id)| (offset, Marker::Stream { id }))
    .collect();
    assert_eq!(switches, expected);
    // Cut points are at line ends only, and there is one at the end of each line.
    let cuts: Vec<u64> = markers(&events, |m| *m == Marker::CutPoint)
        .into_iter()
        .map(|(o, _)| o)
        .collect();
    assert_eq!(cuts, [5, 10, 12, 18, 23]);
    // The ack marker sits right after the bytes that were written.
    assert!(
        markers(&events, |m| matches!(m, Marker::InputAck { .. }))
            .contains(&(12, Marker::InputAck { input_id: 1 }))
    );
}

#[test]
fn a_client_that_attaches_in_the_middle_knows_the_stream_of_its_first_byte() {
    let holder = TestHolder::spawn_with(
        pipe_config("echo out1; sleep 0.3; echo err1 >&2; sleep 30"),
        None,
    );
    let mut first = holder.attach(0);
    until_run_ends_with(&mut first, StreamId::Stderr, "err1\n");
    // Offset 7 is inside `err1\n`, which started at offset 5.
    let mut client = holder.attach(7);
    match client.recv() {
        HolderEvent::Output {
            offset: 7,
            stream: StreamId::Stderr,
            bytes,
        } => assert_eq!(bytes, b"r1\n"),
        other => panic!("expected stderr bytes at offset 7, got {other:?}"),
    }
}

#[test]
fn a_partial_line_is_not_a_cut_point() {
    let holder = TestHolder::spawn_with(pipe_config("printf ab; sleep 0.3; echo; sleep 30"), None);
    let mut client = holder.attach(0);
    let mut events = until_run_ends_with(&mut client, StreamId::Stdout, "ab\n");
    events.extend(client.events_within(Duration::from_millis(300)));
    let cuts: Vec<u64> = markers(&events, |m| *m == Marker::CutPoint)
        .into_iter()
        .map(|(o, _)| o)
        .collect();
    assert!(!cuts.contains(&2), "a cut point inside a line: {cuts:?}");
    assert!(cuts.contains(&3));
}

#[test]
fn closing_stdin_ends_a_program_that_reads_it() {
    let holder = TestHolder::spawn_with(pipe_config("cat"), None);
    let mut client = holder.attach(0);
    client.send(&HolderRequest::Input {
        input_id: 1,
        bytes: b"hi\n".to_vec(),
    });
    let events = until_run_ends_with(&mut client, StreamId::Stdout, "hi\n");
    assert!(runs(&events).contains(&(StreamId::Stdin, "hi\n".to_owned())));
    client.send(&HolderRequest::CloseStdin);
    let status = client.wait_for(|e| match e {
        HolderEvent::ChildExited { status } => Some(*status),
        _ => None,
    });
    assert_eq!(status, ExitStatus::Code(0));
}

#[test]
fn a_resize_is_ignored_and_the_status_has_no_terminal() {
    let holder = TestHolder::spawn_with(pipe_config("sleep 30"), None);
    let mut client = holder.connect();
    client.send(&hello(&holder.config.secret, 1));
    assert!(matches!(
        client.recv(),
        HolderEvent::HelloOk {
            mode: HoldModeKind::Pipe,
            ..
        }
    ));
    client.send(&HolderRequest::Resize {
        cols: 100,
        rows: 30,
        px_width: 0,
        px_height: 0,
    });
    client.send(&HolderRequest::Status);
    let status = client.wait_for(|e| match e {
        HolderEvent::StatusReply(s) => Some(s.clone()),
        _ => None,
    });
    assert_eq!((status.cols, status.rows, status.fg_pgid), (0, 0, 0));
    assert!(client.events_within(Duration::from_millis(200)).is_empty());
}

#[test]
fn the_exit_code_comes_after_all_output() {
    let holder = TestHolder::spawn_with(pipe_config("echo bye; echo oops >&2; exit 4"), None);
    let mut client = holder.attach(0);
    let mut events = Vec::new();
    loop {
        let event = client.recv();
        let done = matches!(event, HolderEvent::ChildExited { .. });
        events.push(event);
        if done {
            break;
        }
    }
    assert_eq!(
        events.last(),
        Some(&HolderEvent::ChildExited {
            status: ExitStatus::Code(4)
        })
    );
    let text: String = runs(&events).into_iter().map(|(_, t)| t).collect();
    assert!(
        text.contains("bye\n") && text.contains("oops\n"),
        "{text:?}"
    );
}
