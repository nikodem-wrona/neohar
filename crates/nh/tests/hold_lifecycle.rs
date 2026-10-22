//! The life of a holder: the exit of its child, signals, its socket file and the foreground group.

mod support;

use std::{
    os::unix::{fs::MetadataExt, net::UnixStream},
    time::{Duration, Instant},
};

use nh_proto::holder::{ExitStatus, HolderEvent, HolderRequest};
use rustix::process::{Pid, Signal, kill_process, test_kill_process};
use support::{TestHolder, config};

fn pid(holder: &TestHolder) -> Pid {
    Pid::from_raw(holder.holder_pid).expect("a pid")
}

fn gone(pid: Pid) -> bool {
    test_kill_process(pid).is_err()
}

#[test]
fn without_an_ack_the_holder_keeps_serving_after_the_child_exited() {
    let holder = TestHolder::spawn(&["/bin/sh", "-c", "printf bye"]);
    let mut client = holder.attach(0);
    client.wait_for(|e| matches!(e, HolderEvent::ChildExited { .. }).then_some(()));
    std::thread::sleep(Duration::from_millis(500));
    assert!(!gone(pid(&holder)), "the holder left without an ExitAck");
    client.send(&HolderRequest::Status);
    let status = client.wait_for(|e| match e {
        HolderEvent::StatusReply(s) => Some(s.clone()),
        _ => None,
    });
    assert_eq!(status.exited, Some(ExitStatus::Code(0)));
    // A later client still hears of the exit, after the output that it asks for.
    let mut later = holder.attach(0);
    later.wait_for(|e| matches!(e, HolderEvent::ChildExited { .. }).then_some(()));
}

#[test]
fn sigterm_hangs_up_the_child_and_the_holder_leaves() {
    let holder = TestHolder::spawn(&["/bin/sh", "-c", "sleep 30"]);
    kill_process(pid(&holder), Signal::TERM).expect("the holder gets the signal");
    let deadline = Instant::now() + Duration::from_secs(5);
    while !gone(pid(&holder)) {
        assert!(
            Instant::now() < deadline,
            "the holder is still there after SIGTERM"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(!holder.socket.exists(), "the socket file is left behind");
}

#[test]
fn the_holder_binds_its_socket_again_when_the_file_is_gone() {
    let holder = TestHolder::spawn(&["/bin/sh", "-c", "echo up; sleep 30"]);
    let inode = std::fs::metadata(&holder.socket)
        .expect("the socket exists")
        .ino();
    std::fs::remove_file(&holder.socket).expect("the socket is removed");
    assert!(UnixStream::connect(&holder.socket).is_err());
    kill_process(pid(&holder), Signal::USR1).expect("the holder gets the signal");
    let deadline = Instant::now() + Duration::from_secs(5);
    while !holder.socket.exists() {
        assert!(Instant::now() < deadline, "the socket is not back");
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_ne!(
        std::fs::metadata(&holder.socket)
            .expect("the socket exists")
            .ino(),
        inode
    );
    let mut client = holder.attach(0);
    client.output_containing("up");
}

#[test]
fn a_socket_path_that_holds_another_kind_of_file_is_left_alone() {
    let holder = TestHolder::spawn(&["/bin/sh", "-c", "sleep 30"]);
    std::fs::remove_file(&holder.socket).expect("the socket is removed");
    std::fs::write(&holder.socket, "not a socket").expect("a file takes its place");
    kill_process(pid(&holder), Signal::USR1).expect("the holder gets the signal");
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(
        std::fs::read(&holder.socket).expect("the file is there"),
        b"not a socket"
    );
    assert!(!gone(pid(&holder)), "the holder is still running");
}

#[test]
fn a_change_of_the_foreground_process_group_is_reported() {
    // With job control, `fg` gives the terminal to the group of `sleep`.
    let script = "echo ready; sleep 1; set -m; sleep 3 & fg %1 >/dev/null; sleep 30";
    let holder = TestHolder::spawn_with(config(&["/bin/bash", "-c", script]), None);
    let mut client = holder.attach(0);
    client.output_containing("ready");
    let pgid = client.wait_for(|e| match e {
        HolderEvent::ForegroundChanged { pgid } => Some(*pgid),
        _ => None,
    });
    assert!(pgid > 1);
    client.send(&HolderRequest::Status);
    let _ = client.events_within(Duration::from_millis(50));
}

#[test]
fn a_client_that_is_gone_does_not_stop_the_child() {
    let holder = TestHolder::spawn(&[
        "/bin/sh",
        "-c",
        "while true; do echo tick; sleep 0.05; done",
    ]);
    let mut first = holder.attach(0);
    first.output_containing("tick");
    drop(first);
    std::thread::sleep(Duration::from_millis(300));
    let mut second = holder.attach(0);
    // The first client saw one or two ticks. The rest came while nobody was attached.
    let (output, _) = second.output_until(|o| support::count(o, "tick") >= 5, &mut Vec::new());
    assert!(support::count(&output, "tick") >= 5);
}
