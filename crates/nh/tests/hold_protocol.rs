//! The holder protocol: the lease, status, pings and misuse.

mod support;

use std::time::Duration;

use nh_proto::holder::{
    HoldMode, HolderEvent, HolderRequest, Marker, RefuseReason, SignalKind, SignalTarget,
};
use support::{TestHolder, hello};

#[test]
fn a_lower_or_equal_fencing_token_is_refused_and_a_higher_one_takes_the_lease() {
    let holder = TestHolder::spawn(&["/bin/sh", "-c", "sleep 30"]);
    let secret = holder.config.secret;

    let mut first = holder.connect();
    first.send(&hello(&secret, 10));
    assert!(matches!(
        first.recv(),
        HolderEvent::HelloOk {
            fencing_token: 10,
            ..
        }
    ));

    for stale in [10, 9] {
        let mut late = holder.connect();
        late.send(&hello(&secret, stale));
        assert!(
            matches!(
                late.recv(),
                HolderEvent::Refused {
                    reason: RefuseReason::StaleFence
                }
            ),
            "token {stale}"
        );
    }

    let mut second = holder.connect();
    second.send(&hello(&secret, 11));
    assert!(matches!(
        second.recv(),
        HolderEvent::HelloOk {
            fencing_token: 11,
            ..
        }
    ));
    assert!(matches!(
        first.recv(),
        HolderEvent::LeaseLost {
            by_fencing_token: 11
        }
    ));
    assert!(first.closed_within(Duration::from_secs(5)));

    // The old client cannot write any more, and the new one can.
    second.send(&HolderRequest::Ping { nonce: 7 });
    assert!(matches!(second.recv(), HolderEvent::Pong { nonce: 7 }));
}

#[test]
fn a_protocol_version_other_than_2_is_refused() {
    let holder = TestHolder::spawn(&["/bin/sh", "-c", "sleep 30"]);
    let mut client = holder.connect();
    client.send(&HolderRequest::Hello {
        proto: 3,
        secret: holder.config.secret,
        fencing_token: 1,
        client: "test".into(),
    });
    assert!(matches!(
        client.recv(),
        HolderEvent::Refused {
            reason: RefuseReason::ProtoUnsupported
        }
    ));
}

#[test]
fn a_request_before_hello_closes_the_connection() {
    let holder = TestHolder::spawn(&["/bin/sh", "-c", "sleep 30"]);
    let mut client = holder.connect();
    client.send(&HolderRequest::Status);
    assert!(client.closed_within(Duration::from_secs(5)));
}

#[test]
fn status_reports_the_child_the_size_and_the_journal() {
    let holder = TestHolder::spawn(&["/bin/sh", "-c", "printf abc; sleep 30"]);
    let mut client = holder.client();
    client.send(&HolderRequest::Ping { nonce: 1 });
    client.wait_for(|e| matches!(e, HolderEvent::Pong { nonce: 1 }).then_some(()));
    let status = loop {
        client.send(&HolderRequest::Status);
        let status = client.wait_for(|e| match e {
            HolderEvent::StatusReply(s) => Some(s.clone()),
            _ => None,
        });
        if status.journal.end >= 3 {
            break status;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    assert!(status.child_pid > 1);
    assert_eq!((status.cols, status.rows), (80, 24));
    assert_eq!(status.exited, None);
    assert_eq!((status.journal.start, status.journal.end), (0, 3));
    assert_eq!(status.journal.capacity, 1 << 20);
    assert_eq!(status.holder_version, env!("CARGO_PKG_VERSION"));
    assert!(matches!(holder.config.mode, HoldMode::Pty { .. }));
}

#[test]
fn a_signal_reaches_the_child() {
    let holder = TestHolder::spawn(&["/bin/sh", "-c", "sleep 30"]);
    let mut client = holder.attach(0);
    client.send(&HolderRequest::Signal {
        target: SignalTarget::Child,
        signal: SignalKind::Term,
    });
    let status = client.wait_for(|e| match e {
        HolderEvent::ChildExited { status } => Some(*status),
        _ => None,
    });
    assert_eq!(status, nh_proto::holder::ExitStatus::Signal(15));
}

#[test]
fn a_signal_to_the_foreground_group_reaches_the_program_that_runs_there() {
    let holder = TestHolder::spawn(&["/bin/sh", "-c", "echo ready; exec sleep 30"]);
    let mut client = holder.attach(0);
    client.output_containing("ready");
    client.send(&HolderRequest::Signal {
        target: SignalTarget::Foreground,
        signal: SignalKind::Int,
    });
    let status = client.wait_for(|e| match e {
        HolderEvent::ChildExited { status } => Some(*status),
        _ => None,
    });
    assert_eq!(status, nh_proto::holder::ExitStatus::Signal(2));
}

/// A cut point is never inside an escape sequence, in a live stream with reads in the middle of it.
#[test]
fn cut_point_markers_are_placed_between_sequences() {
    // The first print ends inside a CSI sequence (`ESC [ 3 1`). A second print finishes it.
    let script = "printf 'ab\\033[31'; sleep 0.3; printf 'mcd'; sleep 30";
    let holder = TestHolder::spawn(&["/bin/sh", "-c", script]);
    let mut client = holder.attach(0);
    let mut others = Vec::new();
    let (out, _) = client.output_until(|o| o == b"ab\x1b[31mcd", &mut others);
    assert_eq!(out, b"ab\x1b[31mcd");
    // The markers come in the same stream. Read until the one after the last byte arrives.
    let mut cuts: Vec<u64> = others
        .iter()
        .filter_map(|e| match e {
            HolderEvent::Marker {
                offset,
                marker: Marker::CutPoint,
            } => Some(*offset),
            _ => None,
        })
        .collect();
    while cuts.last() != Some(&9) {
        match client.try_recv() {
            Ok(HolderEvent::Marker {
                offset,
                marker: Marker::CutPoint,
            }) => cuts.push(offset),
            Ok(_) => {}
            Err(e) => panic!("no cut point at the end of the stream: {e} (cuts: {cuts:?})"),
        }
    }
    assert!(cuts.contains(&2), "a cut point after `ab`: {cuts:?}");
    assert!(
        cuts.iter().all(|c| !(3..=6).contains(c)),
        "a cut point inside the sequence: {cuts:?}"
    );
    assert_eq!(cuts.last(), Some(&9));
}
