//! Frozen fixtures of the `holder/2` protocol.
//!
//! These files are the base of the N−1 compatibility test: when `holder/3` appears, the holder/2
//! types move to `holder::v2_frozen` and this suite keeps running against them.

mod common;

use nh_proto::{
    answers::QueryKind,
    holder::{
        CheckpointReason, ExitStatus, HoldConfig, HoldMode, HoldModeKind, HolderEvent,
        HolderRequest, HolderStatus, InputResult, JournalInfo, Marker, QueuedQuery, RefuseReason,
        SignalKind, SignalTarget, StreamId, TrackedModes,
    },
};

const SECRET: [u8; 32] = [7; 32];
const PANE: [u8; 16] = *b"0123456789abcdef";

fn journal() -> JournalInfo {
    JournalInfo {
        start: 100,
        end: 5_000,
        capacity: 16 << 20,
        last_checkpoint: 2_000,
        last_cut_point: 4_990,
    }
}

/// Each variant of `HolderRequest` once, in declaration order, with its postcard discriminant.
fn request_variants() -> Vec<(u8, &'static str, HolderRequest)> {
    vec![
        (
            0,
            "hello",
            HolderRequest::Hello {
                proto: 2,
                secret: SECRET,
                fencing_token: 41,
                client: "core".into(),
            },
        ),
        (1, "status", HolderRequest::Status),
        (2, "attach", HolderRequest::Attach { from_offset: 1_234 }),
        (
            3,
            "input",
            HolderRequest::Input {
                input_id: 77,
                bytes: b"ls\r".to_vec(),
            },
        ),
        (
            4,
            "resize",
            HolderRequest::Resize {
                cols: 120,
                rows: 40,
                px_width: 960,
                px_height: 800,
            },
        ),
        (
            5,
            "checkpoint_taken",
            HolderRequest::CheckpointTaken { offset: 4_990 },
        ),
        (
            6,
            "queries_answered",
            HolderRequest::QueriesAnswered { up_to_seq: 9 },
        ),
        (
            7,
            "signal_child_hup",
            HolderRequest::Signal {
                target: SignalTarget::Child,
                signal: SignalKind::Hup,
            },
        ),
        (8, "close_stdin", HolderRequest::CloseStdin),
        (9, "exit_ack", HolderRequest::ExitAck),
        (10, "detach", HolderRequest::Detach),
        (11, "ping", HolderRequest::Ping { nonce: 0xdead_beef }),
    ]
}

/// The cases that cover the nested enums of the requests.
fn request_extras() -> Vec<(&'static str, HolderRequest)> {
    use SignalKind::*;
    use SignalTarget::*;
    [
        ("signal_child_int", Child, Int),
        ("signal_child_term", Child, Term),
        ("signal_child_kill", Child, Kill),
        ("signal_child_usr1", Child, Usr1),
        ("signal_foreground_term", Foreground, Term),
    ]
    .into_iter()
    .map(|(name, target, signal)| (name, HolderRequest::Signal { target, signal }))
    .collect()
}

fn queued(seq: u64, kind: QueryKind, raw: &[u8]) -> QueuedQuery {
    QueuedQuery {
        seq,
        offset: 4_000 + seq,
        received_at_ms: 1_760_000_000_000 + seq as i64,
        kind,
        raw: raw.to_vec(),
    }
}

fn status() -> HolderStatus {
    HolderStatus {
        child_pid: 4_242,
        fg_pgid: 4_250,
        exited: None,
        cols: 120,
        rows: 40,
        journal: journal(),
        tracked: TrackedModes {
            bracketed_paste: true,
            focus_events: false,
            kitty_flags_stack: vec![1, 5],
        },
        queued_queries: vec![
            queued(1, QueryKind::Da1, b"\x1b[c"),
            queued(2, QueryKind::Da2, b"\x1b[>c"),
            queued(3, QueryKind::Da3, b"\x1b[=c"),
            queued(4, QueryKind::XtVersion, b"\x1b[>q"),
            queued(
                5,
                QueryKind::Decrqm {
                    private: true,
                    mode: 2004,
                },
                b"\x1b[?2004$p",
            ),
            queued(6, QueryKind::CursorPosition, b"\x1b[6n"),
            queued(7, QueryKind::OscColor { code: 11 }, b"\x1b]11;?\x07"),
            queued(8, QueryKind::WindowSize { op: 14 }, b"\x1b[14t"),
        ],
        holder_version: "0.1.0".into(),
        started_at_ms: 1_760_000_000_000,
    }
}

/// Each variant of `HolderEvent` once, in declaration order, with its postcard discriminant.
fn event_variants() -> Vec<(u8, &'static str, HolderEvent)> {
    vec![
        (
            0,
            "hello_ok_pty",
            HolderEvent::HelloOk {
                proto: 2,
                pane: PANE,
                mode: HoldModeKind::Pty,
                child_pid: 4_242,
                journal: journal(),
                fencing_token: 41,
            },
        ),
        (
            1,
            "refused_bad_secret",
            HolderEvent::Refused {
                reason: RefuseReason::BadSecret,
            },
        ),
        (2, "status_reply", HolderEvent::StatusReply(status())),
        (
            3,
            "output_pty",
            HolderEvent::Output {
                offset: 100,
                stream: StreamId::Pty,
                bytes: b"hello\r\n".to_vec(),
            },
        ),
        (
            4,
            "marker_resize",
            HolderEvent::Marker {
                offset: 107,
                marker: Marker::Resize {
                    cols: 100,
                    rows: 30,
                },
            },
        ),
        (
            5,
            "truncated",
            HolderEvent::Truncated {
                requested: 10,
                start: 100,
            },
        ),
        (
            6,
            "input_ack_written",
            HolderEvent::InputAck {
                input_id: 77,
                offset: 120,
                result: InputResult::Written,
            },
        ),
        (
            7,
            "checkpoint_wanted_ring_half",
            HolderEvent::CheckpointWanted {
                offset: 4_990,
                reason: CheckpointReason::RingHalf,
            },
        ),
        (
            8,
            "foreground_changed",
            HolderEvent::ForegroundChanged { pgid: 4_250 },
        ),
        (
            9,
            "child_exited_code",
            HolderEvent::ChildExited {
                status: ExitStatus::Code(1),
            },
        ),
        (
            10,
            "lease_lost",
            HolderEvent::LeaseLost {
                by_fencing_token: 42,
            },
        ),
        (11, "pong", HolderEvent::Pong { nonce: 0xdead_beef }),
    ]
}

/// The cases that cover the nested enums of the events.
fn event_extras() -> Vec<(&'static str, HolderEvent)> {
    vec![
        (
            "hello_ok_pipe",
            HolderEvent::HelloOk {
                proto: 2,
                pane: PANE,
                mode: HoldModeKind::Pipe,
                child_pid: 9,
                journal: journal(),
                fencing_token: 1,
            },
        ),
        (
            "refused_stale_fence",
            HolderEvent::Refused {
                reason: RefuseReason::StaleFence,
            },
        ),
        (
            "refused_proto_unsupported",
            HolderEvent::Refused {
                reason: RefuseReason::ProtoUnsupported,
            },
        ),
        (
            "refused_wrong_user",
            HolderEvent::Refused {
                reason: RefuseReason::WrongUser,
            },
        ),
        (
            "status_reply_exited",
            HolderEvent::StatusReply(HolderStatus {
                exited: Some(ExitStatus::Signal(9)),
                ..status()
            }),
        ),
        (
            "output_stdin",
            HolderEvent::Output {
                offset: 1,
                stream: StreamId::Stdin,
                bytes: b"{}\n".to_vec(),
            },
        ),
        (
            "output_stdout",
            HolderEvent::Output {
                offset: 2,
                stream: StreamId::Stdout,
                bytes: b"{}\n".to_vec(),
            },
        ),
        (
            "output_stderr",
            HolderEvent::Output {
                offset: 3,
                stream: StreamId::Stderr,
                bytes: b"warn\n".to_vec(),
            },
        ),
        (
            "marker_input_ack",
            HolderEvent::Marker {
                offset: 120,
                marker: Marker::InputAck { input_id: 77 },
            },
        ),
        (
            "marker_cut_point",
            HolderEvent::Marker {
                offset: 130,
                marker: Marker::CutPoint,
            },
        ),
        (
            "marker_stream_stderr",
            HolderEvent::Marker {
                offset: 140,
                marker: Marker::Stream {
                    id: StreamId::Stderr,
                },
            },
        ),
        (
            "input_ack_duplicate",
            HolderEvent::InputAck {
                input_id: 77,
                offset: 120,
                result: InputResult::Duplicate,
            },
        ),
        (
            "checkpoint_wanted_idle",
            HolderEvent::CheckpointWanted {
                offset: 4_990,
                reason: CheckpointReason::Idle,
            },
        ),
        (
            "checkpoint_wanted_interval",
            HolderEvent::CheckpointWanted {
                offset: 4_990,
                reason: CheckpointReason::Interval,
            },
        ),
        (
            "child_exited_signal",
            HolderEvent::ChildExited {
                status: ExitStatus::Signal(15),
            },
        ),
        (
            "child_exited_exec_failed",
            HolderEvent::ChildExited {
                status: ExitStatus::ExecFailed(2),
            },
        ),
    ]
}

fn configs() -> Vec<(&'static str, HoldConfig)> {
    let base = HoldConfig {
        proto: 2,
        pane: PANE,
        secret: SECRET,
        mode: HoldMode::Pty {
            cols: 120,
            rows: 40,
            px_width: 960,
            px_height: 800,
        },
        argv: vec!["/bin/zsh".into(), "-l".into()],
        env: vec![
            ("TERM".into(), "xterm-256color".into()),
            ("NEOHAR_PANE".into(), "w1:p1".into()),
        ],
        cwd: "/Users/ada/code".into(),
        socket_path: "/tmp/neohar-501/default/holders/01J9ZZZZZZZZZZZZZZZZZZZZZZ.sock".into(),
        log_path: "/Users/ada/Library/Application Support/dev.neohar.app/default/logs/holder.log"
            .into(),
        ring_bytes: 16 << 20,
        checkpoint_idle_ms: 2_000,
        checkpoint_max_interval_ms: 30_000,
        query_fresh_ms: 5_000,
    };
    vec![
        ("config_pty", base.clone()),
        (
            "config_pipe",
            HoldConfig {
                mode: HoldMode::Pipe,
                argv: vec!["claude".into()],
                ..base
            },
        ),
    ]
}

/// The postcard discriminant of each request variant. The `match` has no wildcard, so a new
/// variant does not compile until it is listed here and in `request_variants`.
fn request_discriminant(request: &HolderRequest) -> u8 {
    match request {
        HolderRequest::Hello { .. } => 0,
        HolderRequest::Status => 1,
        HolderRequest::Attach { .. } => 2,
        HolderRequest::Input { .. } => 3,
        HolderRequest::Resize { .. } => 4,
        HolderRequest::CheckpointTaken { .. } => 5,
        HolderRequest::QueriesAnswered { .. } => 6,
        HolderRequest::Signal { .. } => 7,
        HolderRequest::CloseStdin => 8,
        HolderRequest::ExitAck => 9,
        HolderRequest::Detach => 10,
        HolderRequest::Ping { .. } => 11,
    }
}

/// The postcard discriminant of each event variant. See `request_discriminant`.
fn event_discriminant(event: &HolderEvent) -> u8 {
    match event {
        HolderEvent::HelloOk { .. } => 0,
        HolderEvent::Refused { .. } => 1,
        HolderEvent::StatusReply(_) => 2,
        HolderEvent::Output { .. } => 3,
        HolderEvent::Marker { .. } => 4,
        HolderEvent::Truncated { .. } => 5,
        HolderEvent::InputAck { .. } => 6,
        HolderEvent::CheckpointWanted { .. } => 7,
        HolderEvent::ForegroundChanged { .. } => 8,
        HolderEvent::ChildExited { .. } => 9,
        HolderEvent::LeaseLost { .. } => 10,
        HolderEvent::Pong { .. } => 11,
    }
}

#[test]
fn every_request_variant_keeps_its_discriminant() {
    let variants = request_variants();
    assert_eq!(variants.len(), 12, "a variant has no case");
    for (index, (disc, name, value)) in variants.iter().enumerate() {
        assert_eq!(
            usize::from(*disc),
            index,
            "{name} is not listed in declaration order"
        );
        assert_eq!(request_discriminant(value), *disc, "{name}");
        let bytes = postcard::to_stdvec(value).expect("the request serializes");
        assert_eq!(bytes[0], *disc, "the wire discriminant of {name} changed");
    }
}

#[test]
fn every_event_variant_keeps_its_discriminant() {
    let variants = event_variants();
    assert_eq!(variants.len(), 12, "a variant has no case");
    for (index, (disc, name, value)) in variants.iter().enumerate() {
        assert_eq!(
            usize::from(*disc),
            index,
            "{name} is not listed in declaration order"
        );
        assert_eq!(event_discriminant(value), *disc, "{name}");
        let bytes = postcard::to_stdvec(value).expect("the event serializes");
        assert_eq!(bytes[0], *disc, "the wire discriminant of {name} changed");
    }
}

#[test]
fn request_fixtures_are_frozen() {
    let cases: Vec<_> = request_variants()
        .into_iter()
        .map(|(_, name, value)| (name, value))
        .chain(request_extras())
        .collect();
    common::check_fixtures("holder2", "request", &cases);
}

#[test]
fn event_fixtures_are_frozen() {
    let cases: Vec<_> = event_variants()
        .into_iter()
        .map(|(_, name, value)| (name, value))
        .chain(event_extras())
        .collect();
    common::check_fixtures("holder2", "event", &cases);
}

#[test]
fn config_fixtures_are_frozen() {
    common::check_fixtures("holder2", "config", &configs());
}
