//! The life of a run: start, end, replace, identity, binding and read state.

use nh_proto::api::{common::Harness, model::ReadState};

use super::*;

fn detection(pid: i32, start: u64) -> Detection {
    Detection {
        manifest: "claude".to_owned(),
        harness: Harness::Claude,
        priority: 100,
        remote_loop: None,
        pid,
        start,
        place: 0,
    }
}

#[test]
fn a_process_that_appears_starts_a_run_once() {
    let mut tracker = Tracker::default();
    assert_eq!(tracker.observe(None, |_| true), Step::Nothing);
    let step = tracker.observe(Some(detection(10, 100)), |_| true);
    assert_eq!(step, Step::Start(detection(10, 100)));
    // The same process again, on every walk: no new run.
    for _ in 0..5 {
        assert_eq!(
            tracker.observe(Some(detection(10, 100)), |_| true),
            Step::Nothing
        );
    }
    assert_eq!(tracker.current(), Some((10, 100)));
}

#[test]
fn a_process_that_is_gone_ends_the_run() {
    let mut tracker = Tracker::default();
    tracker.observe(Some(detection(10, 100)), |_| true);
    let step = tracker.observe(None, |key| {
        assert_eq!(key, (10, 100));
        false
    });
    assert_eq!(
        step,
        Step::End {
            pid: 10,
            start: 100
        }
    );
    assert_eq!(tracker.current(), None);
    assert_eq!(tracker.observe(None, |_| false), Step::Nothing);
}

#[test]
fn a_process_that_lives_but_left_the_foreground_keeps_the_run() {
    // The harness opened an editor, which is the foreground group for a time.
    let mut tracker = Tracker::default();
    tracker.observe(Some(detection(10, 100)), |_| true);
    assert_eq!(tracker.observe(None, |_| true), Step::Nothing);
    assert_eq!(tracker.current(), Some((10, 100)));
    // It comes back as the same run.
    assert_eq!(
        tracker.observe(Some(detection(10, 100)), |_| true),
        Step::Nothing
    );
}

#[test]
fn another_process_takes_the_place_of_the_run() {
    let mut tracker = Tracker::default();
    tracker.observe(Some(detection(10, 100)), |_| true);
    let mut codex = detection(22, 200);
    codex.manifest = "codex".to_owned();
    codex.harness = Harness::Codex;
    let step = tracker.observe(Some(codex.clone()), |_| panic!("no check is needed"));
    assert_eq!(
        step,
        Step::Replace {
            ended: (10, 100),
            started: codex
        }
    );
    assert_eq!(tracker.current(), Some((22, 200)));
}

#[test]
fn a_pid_that_comes_again_with_a_new_start_time_is_a_new_process() {
    let mut tracker = Tracker::default();
    tracker.observe(Some(detection(10, 100)), |_| true);
    let step = tracker.observe(Some(detection(10, 900)), |_| true);
    assert!(matches!(
        step,
        Step::Replace {
            ended: (10, 100),
            ..
        }
    ));
}

#[test]
fn the_holder_can_end_the_run_without_a_walk() {
    let mut tracker = Tracker::default();
    assert_eq!(tracker.gone(), None);
    tracker.observe(Some(detection(10, 100)), |_| true);
    assert_eq!(tracker.gone(), Some((10, 100)));
    assert_eq!(tracker.current(), None);
}

#[test]
fn a_run_that_the_core_loaded_continues_after_a_restart() {
    let mut tracker = Tracker::default();
    tracker.resume((10, 100));
    assert_eq!(
        tracker.observe(Some(detection(10, 100)), |_| true),
        Step::Nothing
    );
}

#[test]
fn a_detected_run_is_suggested_and_authorizes_nothing() {
    let binding = binding_for_detection(false);
    assert!(matches!(binding, Binding::Suggested { confidence } if confidence < 1.0));
    assert!(!authorizes(&binding));
    assert!(!authorizes(&Binding::Unlinked));
    assert!(authorizes(&binding_for_detection(true)));
    assert!(authorizes(&promote()));
}

#[test]
fn a_new_session_id_in_the_same_process_keeps_the_run() {
    let mut sessions = Sessions::default();
    assert_eq!(sessions.current(), None);
    assert_eq!(sessions.identify("s1"), Identified::First);
    assert_eq!(sessions.identify("s1"), Identified::Same);
    // `/clear` starts a new session in the same process.
    assert_eq!(sessions.identify("s2"), Identified::Changed);
    assert_eq!(sessions.current(), Some("s2"));
    assert_eq!(sessions.previous(), ["s1".to_owned()]);
    // `/resume` goes back to the first one.
    assert_eq!(sessions.identify("s1"), Identified::Changed);
    assert_eq!(sessions.current(), Some("s1"));
    assert_eq!(sessions.previous(), ["s2".to_owned()]);
}

#[test]
fn a_run_that_nobody_has_seen_is_unseen_until_a_client_looks() {
    let mut read = ReadStates::<&str>::new();
    read.joined("a");
    read.joined("b");
    assert_eq!(read.get(&"a"), ReadState::Seen);
    // The turn ends while window a is looking at the pane.
    read.changed(["a"]);
    assert_eq!(read.get(&"a"), ReadState::Seen);
    assert_eq!(read.get(&"b"), ReadState::Unseen);
    // Window b focuses the pane after the change.
    read.viewed("b");
    assert_eq!(read.get(&"b"), ReadState::Seen);
    // The next change makes it unseen for the window that is not looking.
    read.changed(["b"]);
    assert_eq!(read.get(&"a"), ReadState::Unseen);
    assert_eq!(read.get(&"b"), ReadState::Seen);
}

#[test]
fn a_window_that_opens_later_has_seen_what_happened_before() {
    let mut read = ReadStates::<&str>::new();
    read.changed([]);
    assert_eq!(
        read.get(&"late"),
        ReadState::Seen,
        "unknown clients have seen it"
    );
    read.joined("late");
    assert_eq!(read.get(&"late"), ReadState::Seen);
    read.changed([]);
    assert_eq!(read.get(&"late"), ReadState::Unseen);
    read.left(&"late");
    assert_eq!(read.get(&"late"), ReadState::Seen);
}

#[test]
fn a_view_after_a_change_counts_and_a_join_does_not_undo_a_view() {
    let mut read = ReadStates::<&str>::new();
    read.joined("a");
    read.changed([]);
    read.viewed("a");
    assert_eq!(read.get(&"a"), ReadState::Seen);
    // A second join of the same window changes nothing.
    read.changed([]);
    read.joined("a");
    assert_eq!(read.get(&"a"), ReadState::Unseen);
}

#[test]
fn changes_in_the_same_moment_keep_their_order() {
    // The count orders them, not a clock.
    let mut read = ReadStates::<&str>::new();
    read.joined("a");
    read.changed([]);
    read.changed([]);
    assert_eq!(read.get(&"a"), ReadState::Unseen);
    read.viewed("a");
    read.changed([]);
    assert_eq!(read.get(&"a"), ReadState::Unseen);
}
