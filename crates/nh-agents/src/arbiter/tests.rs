//! The arbiter, one table of cases for each of its rules.

use super::*;

fn id(n: u8) -> InteractionId {
    // A fixed ULID for each number, so that the tests read the same each time.
    format!("01M4F1W7EM2GN3EV6NHXZJEG{n:02}")
        .parse()
        .expect("an id")
}

fn at(seconds: i64) -> Timestamp {
    Timestamp(1_000_000 + seconds * 1_000)
}

fn input(source: Source, seconds: i64, observation: Observation) -> Input {
    Input::new(source, at(seconds), observation)
}

fn structured(seconds: i64, observation: Observation) -> Input {
    input(Source::Structured, seconds, observation)
}

fn screen(seconds: i64, confidence: f32, observation: Observation) -> Input {
    input(Source::Screen, seconds, observation).with_confidence(confidence)
}

/// A run that is idle by the word of the harness.
fn idle_run() -> Arbiter {
    let mut arbiter = Arbiter::new(at(0), Source::Process);
    arbiter.apply(structured(1, Observation::Execution(Execution::Idle)));
    arbiter
}

fn working_run() -> Arbiter {
    let mut arbiter = idle_run();
    arbiter.apply(structured(2, Observation::Execution(Execution::Working)));
    arbiter
}

fn states(changes: &[Change], facet: Facet) -> Vec<(Value, Value)> {
    changes
        .iter()
        .filter_map(|change| match change {
            Change::State {
                facet: f, from, to, ..
            } if *f == facet => Some((from.clone(), to.clone())),
            _ => None,
        })
        .collect()
}

fn state(name: &str) -> Value {
    json!({ "state": name })
}

// --- the start and the plain flow -------------------------------------------------------------

#[test]
fn a_new_run_is_alive_and_starting_and_healthy() {
    let arbiter = Arbiter::new(at(0), Source::Process);
    let facets = arbiter.facets(ReadState::Seen);
    assert_eq!(facets.liveness.value, Liveness::Alive);
    assert_eq!(facets.execution.value, Execution::Starting);
    assert_eq!(facets.adapter_health.value, AdapterHealth::Healthy);
    assert!(facets.pending.is_empty());
    assert!(
        arbiter.inferred(),
        "a state that only the process told is inferred"
    );
}

#[test]
fn the_word_of_the_harness_sets_the_execution_state() {
    let mut arbiter = Arbiter::new(at(0), Source::Process);
    let changes = arbiter.apply(structured(1, Observation::Execution(Execution::Idle)));
    assert_eq!(
        states(&changes, Facet::Execution),
        vec![(state("starting"), state("idle"))]
    );
    let changes = arbiter.apply(structured(2, Observation::Execution(Execution::Working)));
    assert_eq!(
        states(&changes, Facet::Execution),
        vec![(state("idle"), state("working"))]
    );
    assert!(!arbiter.inferred());
    assert_eq!(arbiter.execution().provenance.source, Source::Structured);
}

#[test]
fn the_same_state_again_makes_no_change() {
    let mut arbiter = working_run();
    let changes = arbiter.apply(structured(3, Observation::Execution(Execution::Working)));
    assert!(changes.is_empty());
}

#[test]
fn rate_limited_with_a_new_time_is_a_change() {
    let mut arbiter = working_run();
    let first = Execution::RateLimited {
        resets_at: Some(Timestamp(5_000_000)),
    };
    let changes = arbiter.apply(structured(3, Observation::Execution(first)));
    assert_eq!(states(&changes, Facet::Execution).len(), 1);
    let later = Execution::RateLimited {
        resets_at: Some(Timestamp(6_000_000)),
    };
    let changes = arbiter.apply(structured(4, Observation::Execution(later)));
    assert_eq!(states(&changes, Facet::Execution).len(), 1);
}

#[test]
fn the_process_source_settles_a_state_that_nobody_else_told() {
    let mut arbiter = Arbiter::new(at(0), Source::Process);
    let changes = arbiter.apply(input(Source::Process, 30, Observation::Settled));
    assert_eq!(
        states(&changes, Facet::Execution),
        vec![(state("starting"), state("unknown"))]
    );
    // A run that has a word from the harness is not settled by the process.
    let mut arbiter = idle_run();
    assert!(
        arbiter
            .apply(input(Source::Process, 30, Observation::Settled))
            .is_empty()
    );
}

// --- rule 1: the end of the process overrides at once -----------------------------------------

#[test]
fn rule_1_the_end_of_the_process_overrides_a_working_run_and_cancels_the_interactions() {
    let mut arbiter = working_run();
    arbiter.apply(structured(
        3,
        Observation::Opened {
            id: id(1),
            kind: InteractionKind::Approval,
        },
    ));
    let changes = arbiter.apply(input(
        Source::Holder,
        4,
        Observation::Exited { code: Some(0) },
    ));
    assert_eq!(arbiter.liveness().value, Liveness::Exited { code: Some(0) });
    assert_eq!(arbiter.execution().value, Execution::Exited);
    assert!(arbiter.pending().is_empty());
    assert!(changes.contains(&Change::Cancel {
        ids: vec![id(1)],
        reason: CancelReason::ProcessExited
    }));
    assert_eq!(
        states(&changes, Facet::Execution),
        vec![(state("working"), state("exited"))]
    );
}

#[test]
fn rule_1_nothing_that_comes_after_the_end_changes_the_run() {
    let mut arbiter = working_run();
    arbiter.apply(input(Source::Holder, 3, Observation::Exited { code: None }));
    for late in [
        structured(4, Observation::Execution(Execution::Working)),
        structured(
            4,
            Observation::Opened {
                id: id(2),
                kind: InteractionKind::Approval,
            },
        ),
        screen(4, 0.9, Observation::Execution(Execution::Idle)),
        input(Source::Process, 4, Observation::Alive),
        input(Source::Holder, 5, Observation::Exited { code: Some(1) }),
    ] {
        assert!(arbiter.apply(late).is_empty());
    }
    assert_eq!(arbiter.liveness().value, Liveness::Exited { code: None });
}

#[test]
fn rule_1_the_process_source_ends_a_run_that_the_holder_did_not() {
    // The harness quits and the shell stays: the holder sees nothing, and the walk sees no process.
    let mut arbiter = idle_run();
    let changes = arbiter.apply(input(
        Source::Process,
        9,
        Observation::Exited { code: None },
    ));
    assert_eq!(arbiter.execution().value, Execution::Exited);
    assert_eq!(states(&changes, Facet::Liveness).len(), 1);
}

// --- rule 2: open interactions are authoritative ----------------------------------------------

#[test]
fn rule_2_an_interaction_stays_open_through_silence_and_the_screen() {
    let mut arbiter = working_run();
    arbiter.apply(structured(
        3,
        Observation::Opened {
            id: id(1),
            kind: InteractionKind::Approval,
        },
    ));
    assert_eq!(arbiter.pending(), vec![id(1)]);
    // The screen says idle and shows no dialog; a process signal says alive. None closes it.
    arbiter.apply(screen(60, 0.9, Observation::Execution(Execution::Idle)));
    arbiter.apply(screen(61, 0.9, Observation::Closed { id: id(1) }));
    arbiter.apply(input(Source::Process, 62, Observation::Alive));
    arbiter.apply(input(
        Source::Process,
        62,
        Observation::Closed { id: id(1) },
    ));
    assert_eq!(arbiter.pending(), vec![id(1)]);
}

#[test]
fn rule_2_the_paths_of_the_spec_close_it() {
    // An answer delivered, a native resolution and a dismissal all arrive as `Closed`.
    for source in [Source::Structured, Source::User, Source::Derived] {
        let mut arbiter = working_run();
        arbiter.apply(structured(
            3,
            Observation::Opened {
                id: id(1),
                kind: InteractionKind::Approval,
            },
        ));
        let changes = arbiter.apply(input(source, 4, Observation::Closed { id: id(1) }));
        assert!(arbiter.pending().is_empty(), "{source:?}");
        assert_eq!(
            states(&changes, Facet::Pending),
            vec![(json!([id(1)]), json!([]))]
        );
    }
}

#[test]
fn rule_2_the_end_of_a_turn_closes_every_interaction() {
    let mut arbiter = working_run();
    for n in [1, 2] {
        arbiter.apply(structured(
            3,
            Observation::Opened {
                id: id(n),
                kind: if n == 1 {
                    InteractionKind::Approval
                } else {
                    InteractionKind::Question
                },
            },
        ));
    }
    let changes = arbiter.apply(structured(5, Observation::TurnEnded));
    assert_eq!(arbiter.execution().value, Execution::Idle);
    assert!(arbiter.pending().is_empty());
    assert!(changes.contains(&Change::Cancel {
        ids: vec![id(1), id(2)],
        reason: CancelReason::TurnEnded
    }));
}

#[test]
fn rule_2_a_turn_that_ended_before_the_state_is_late_and_closes_nothing() {
    let mut arbiter = working_run();
    arbiter.apply(structured(
        9,
        Observation::Opened {
            id: id(1),
            kind: InteractionKind::Approval,
        },
    ));
    arbiter.apply(structured(10, Observation::Execution(Execution::Working)));
    assert!(
        arbiter
            .apply(structured(5, Observation::TurnEnded))
            .is_empty()
    );
    assert_eq!(arbiter.pending(), vec![id(1)]);
}

#[test]
fn rule_2_a_working_run_can_have_an_open_approval() {
    // A subagent goes on while the main agent waits: `working` and an open interaction at once.
    let mut arbiter = working_run();
    arbiter.apply(structured(
        3,
        Observation::Opened {
            id: id(1),
            kind: InteractionKind::Approval,
        },
    ));
    arbiter.apply(structured(4, Observation::Execution(Execution::Working)));
    assert_eq!(arbiter.execution().value, Execution::Working);
    assert_eq!(arbiter.pending(), vec![id(1)]);
}

#[test]
fn rule_2_the_end_of_the_session_closes_them_and_ends_the_state() {
    let mut arbiter = idle_run();
    arbiter.apply(structured(
        3,
        Observation::Opened {
            id: id(1),
            kind: InteractionKind::Question,
        },
    ));
    let changes = arbiter.apply(structured(4, Observation::SessionEnded));
    assert_eq!(arbiter.execution().value, Execution::Exited);
    assert_eq!(arbiter.liveness().value, Liveness::Alive);
    assert!(changes.contains(&Change::Cancel {
        ids: vec![id(1)],
        reason: CancelReason::SessionEnded
    }));
}

// --- rule 3: silence is not staleness, and a lower source adds information --------------------

#[test]
fn rule_3_a_long_silence_changes_nothing() {
    let mut arbiter = working_run();
    // Two hours with no input but the process: still working.
    let changes = arbiter.apply(input(Source::Process, 7_200, Observation::Alive));
    assert!(changes.is_empty());
    assert_eq!(arbiter.execution().value, Execution::Working);
    assert_eq!(arbiter.execution().provenance.source, Source::Structured);
}

#[test]
fn rule_3_a_screen_idle_while_structured_says_working_only_counts() {
    let mut arbiter = working_run();
    let changes = arbiter.apply(screen(10, 0.9, Observation::Execution(Execution::Idle)));
    assert!(changes.is_empty(), "{changes:?}");
    assert_eq!(arbiter.execution().value, Execution::Working);
    assert_eq!(
        arbiter.counters(),
        Counters {
            observations: 1,
            disagreements: 1
        }
    );
    // A screen that agrees is an observation and no disagreement.
    arbiter.apply(screen(11, 0.9, Observation::Execution(Execution::Working)));
    assert_eq!(
        arbiter.counters(),
        Counters {
            observations: 2,
            disagreements: 1
        }
    );
}

#[test]
fn rule_3_a_screen_approval_while_structured_says_working_opens_a_provisional_one() {
    let mut arbiter = working_run();
    let changes = arbiter.apply(screen(
        10,
        0.85,
        Observation::Opened {
            id: id(7),
            kind: InteractionKind::Approval,
        },
    ));
    assert_eq!(arbiter.pending(), vec![id(7)]);
    assert!(arbiter.is_provisional(id(7)));
    assert_eq!(arbiter.execution().value, Execution::Working);
    assert!(changes.contains(&Change::Disagreement {
        facet: Facet::Execution,
        structured: state("working"),
        other: json!({ "pending": "approval" }),
    }));
    let pending = changes
        .iter()
        .find_map(|change| match change {
            Change::State {
                facet: Facet::Pending,
                source,
                confidence,
                ..
            } => Some((*source, *confidence)),
            _ => None,
        })
        .expect("a change of pending");
    assert_eq!(pending, (Source::Screen, 0.85));
}

#[test]
fn rule_3_the_screen_does_not_open_a_second_one_for_the_dialog_that_is_open() {
    let mut arbiter = working_run();
    arbiter.apply(structured(
        3,
        Observation::Opened {
            id: id(1),
            kind: InteractionKind::Approval,
        },
    ));
    let changes = arbiter.apply(screen(
        4,
        0.9,
        Observation::Opened {
            id: id(2),
            kind: InteractionKind::Approval,
        },
    ));
    assert!(changes.is_empty());
    assert_eq!(arbiter.pending(), vec![id(1)]);
}

#[test]
fn rule_3_a_hook_that_comes_later_takes_the_place_of_the_provisional_interaction() {
    let mut arbiter = working_run();
    arbiter.apply(screen(
        3,
        0.9,
        Observation::Opened {
            id: id(7),
            kind: InteractionKind::Approval,
        },
    ));
    let changes = arbiter.apply(structured(
        4,
        Observation::Opened {
            id: id(8),
            kind: InteractionKind::Approval,
        },
    ));
    assert_eq!(arbiter.pending(), vec![id(8)]);
    assert!(!arbiter.is_provisional(id(8)));
    assert!(changes.contains(&Change::Cancel {
        ids: vec![id(7)],
        reason: CancelReason::Superseded
    }));
}

#[test]
fn rule_3_the_screen_closes_only_the_interaction_that_it_opened() {
    let mut arbiter = working_run();
    arbiter.apply(screen(
        3,
        0.9,
        Observation::Opened {
            id: id(7),
            kind: InteractionKind::Approval,
        },
    ));
    let changes = arbiter.apply(screen(5, 0.9, Observation::Closed { id: id(7) }));
    assert!(arbiter.pending().is_empty());
    assert!(changes.contains(&Change::Cancel {
        ids: vec![id(7)],
        reason: CancelReason::DialogGone
    }));
}

#[test]
fn rule_3_without_a_structured_channel_the_screen_tells_the_execution_state() {
    let mut arbiter = Arbiter::new(at(0), Source::Process);
    let changes = arbiter.apply(screen(5, 0.8, Observation::Execution(Execution::Working)));
    assert_eq!(
        states(&changes, Facet::Execution),
        vec![(state("starting"), state("working"))]
    );
    assert!(arbiter.inferred());
    // No structured state to compare with, so nothing counts as a disagreement.
    assert_eq!(arbiter.counters(), Counters::default());
    // And a screen approval opens its interaction with no disagreement.
    let changes = arbiter.apply(screen(
        6,
        0.8,
        Observation::Opened {
            id: id(3),
            kind: InteractionKind::Approval,
        },
    ));
    assert_eq!(arbiter.pending(), vec![id(3)]);
    assert!(
        !changes
            .iter()
            .any(|change| matches!(change, Change::Disagreement { .. }))
    );
}

// --- rule 4: a structured channel that is gone ------------------------------------------------

#[test]
fn rule_4_a_lost_channel_hands_the_state_to_the_screen_and_marks_it_inferred() {
    let mut arbiter = working_run();
    arbiter.apply(screen(10, 0.9, Observation::Execution(Execution::Idle)));
    assert_eq!(arbiter.execution().value, Execution::Working);
    let changes = arbiter.apply(structured(20, Observation::TransportLost));
    assert_eq!(arbiter.execution().value, Execution::Idle);
    assert_eq!(arbiter.execution().provenance.source, Source::Screen);
    assert!(arbiter.inferred());
    assert_eq!(arbiter.health().value, AdapterHealth::Disconnected);
    assert!(changes.contains(&Change::Reconcile));
    assert_eq!(
        states(&changes, Facet::AdapterHealth),
        vec![(state("healthy"), state("disconnected"))]
    );
}

#[test]
fn rule_4_without_a_screen_state_the_process_source_says_unknown() {
    let mut arbiter = working_run();
    arbiter.apply(structured(20, Observation::TransportLost));
    assert_eq!(arbiter.execution().value, Execution::Unknown);
    assert_eq!(arbiter.execution().provenance.source, Source::Process);
}

#[test]
fn rule_4_after_the_loss_the_screen_follows_and_a_structured_signal_heals() {
    let mut arbiter = working_run();
    arbiter.apply(structured(20, Observation::TransportLost));
    // The screen tells the state now.
    let changes = arbiter.apply(screen(25, 0.9, Observation::Execution(Execution::Working)));
    assert_eq!(states(&changes, Facet::Execution).len(), 1);
    assert_eq!(arbiter.execution().provenance.source, Source::Screen);
    // The channel comes back, and the next structured signal makes the state structured again.
    arbiter.apply(structured(30, Observation::TransportHealed));
    assert_eq!(arbiter.health().value, AdapterHealth::Healthy);
    let changes = arbiter.apply(structured(31, Observation::Execution(Execution::Idle)));
    assert_eq!(arbiter.execution().provenance.source, Source::Structured);
    assert_eq!(arbiter.execution().value, Execution::Idle);
    assert!(!arbiter.inferred());
    assert_eq!(states(&changes, Facet::Execution).len(), 1);
}

#[test]
fn rule_4_a_structured_signal_heals_the_health_without_a_healed_message() {
    let mut arbiter = working_run();
    arbiter.apply(structured(20, Observation::TransportLost));
    let changes = arbiter.apply(structured(21, Observation::Execution(Execution::Working)));
    assert_eq!(arbiter.health().value, AdapterHealth::Healthy);
    assert_eq!(
        states(&changes, Facet::AdapterHealth),
        vec![(state("disconnected"), state("healthy"))]
    );
}

#[test]
fn rule_4_losing_the_channel_twice_asks_to_reconcile_once() {
    let mut arbiter = working_run();
    let first = arbiter.apply(structured(20, Observation::TransportLost));
    let second = arbiter.apply(structured(21, Observation::TransportLost));
    assert!(first.contains(&Change::Reconcile));
    assert!(second.is_empty());
}

#[test]
fn rule_4_the_open_interactions_stay_open_when_the_channel_is_lost() {
    let mut arbiter = working_run();
    arbiter.apply(structured(
        3,
        Observation::Opened {
            id: id(1),
            kind: InteractionKind::Approval,
        },
    ));
    arbiter.apply(structured(20, Observation::TransportLost));
    assert_eq!(arbiter.pending(), vec![id(1)]);
}

// --- rule 5: precedence ----------------------------------------------------------------------

#[test]
fn rule_5_a_late_structured_signal_does_not_overwrite_a_newer_one() {
    let mut arbiter = idle_run();
    arbiter.apply(structured(10, Observation::Execution(Execution::Working)));
    let changes = arbiter.apply(structured(5, Observation::Execution(Execution::Idle)));
    assert!(changes.is_empty());
    assert_eq!(arbiter.execution().value, Execution::Working);
}

#[test]
fn rule_5_signals_with_the_same_time_apply_in_the_order_they_arrive() {
    let mut arbiter = idle_run();
    arbiter.apply(structured(10, Observation::Execution(Execution::Working)));
    arbiter.apply(structured(10, Observation::Execution(Execution::Error)));
    assert_eq!(arbiter.execution().value, Execution::Error);
}

#[test]
fn rule_5_the_screen_beats_the_process_and_the_process_does_not_tell_the_state() {
    let mut arbiter = Arbiter::new(at(0), Source::Process);
    arbiter.apply(input(
        Source::Process,
        1,
        Observation::Execution(Execution::Idle),
    ));
    assert_eq!(arbiter.execution().value, Execution::Starting);
    arbiter.apply(screen(2, 0.7, Observation::Execution(Execution::Idle)));
    assert_eq!(arbiter.execution().value, Execution::Idle);
    assert_eq!(arbiter.execution().provenance.source, Source::Screen);
}

#[test]
fn rule_5_an_older_screen_signal_does_not_overwrite_a_newer_one() {
    let mut arbiter = Arbiter::new(at(0), Source::Process);
    arbiter.apply(screen(10, 0.7, Observation::Execution(Execution::Working)));
    let changes = arbiter.apply(screen(5, 0.7, Observation::Execution(Execution::Idle)));
    assert!(changes.is_empty());
    assert_eq!(arbiter.execution().value, Execution::Working);
}

#[test]
fn rule_5_a_structured_signal_beats_a_newer_screen_state() {
    let mut arbiter = Arbiter::new(at(0), Source::Process);
    arbiter.apply(screen(20, 0.7, Observation::Execution(Execution::Working)));
    // The hook is older than the screen match, and the hook wins (rule 5).
    arbiter.apply(structured(10, Observation::Execution(Execution::Idle)));
    assert_eq!(arbiter.execution().value, Execution::Idle);
    assert_eq!(arbiter.execution().provenance.source, Source::Structured);
}

// --- rule 6: unvalidated versions --------------------------------------------------------------

#[test]
fn rule_6_an_unvalidated_version_caps_every_confidence_at_0_8() {
    let mut arbiter = Arbiter::new(at(0), Source::Process);
    let changes = arbiter.apply(input(
        Source::Derived,
        0,
        Observation::Version { validated: false },
    ));
    assert_eq!(arbiter.health().value, AdapterHealth::UnvalidatedVersion);
    assert_eq!(
        states(&changes, Facet::AdapterHealth),
        vec![(state("healthy"), state("unvalidated_version"))]
    );
    let changes = arbiter.apply(structured(1, Observation::Execution(Execution::Working)));
    assert_eq!(arbiter.execution().provenance.confidence, UNVALIDATED_CAP);
    assert!(changes.iter().all(|change| match change {
        Change::State { confidence, .. } => *confidence <= UNVALIDATED_CAP,
        _ => true,
    }));
    // The rules 1 to 5 still run.
    arbiter.apply(input(Source::Holder, 2, Observation::Exited { code: None }));
    assert_eq!(arbiter.execution().value, Execution::Exited);
    assert!(arbiter.execution().provenance.confidence <= UNVALIDATED_CAP);
    assert!(arbiter.liveness().provenance.confidence <= UNVALIDATED_CAP);
}

#[test]
fn rule_6_a_state_that_was_set_before_is_capped_when_the_version_turns_out_to_be_unvalidated() {
    let mut arbiter = working_run();
    assert_eq!(arbiter.execution().provenance.confidence, 1.0);
    arbiter.apply(input(
        Source::Derived,
        3,
        Observation::Version { validated: false },
    ));
    assert_eq!(arbiter.execution().provenance.confidence, UNVALIDATED_CAP);
    // And a validated version lifts the cap for what comes next.
    arbiter.apply(input(
        Source::Derived,
        4,
        Observation::Version { validated: true },
    ));
    assert_eq!(arbiter.health().value, AdapterHealth::Healthy);
    arbiter.apply(structured(5, Observation::Execution(Execution::Idle)));
    assert_eq!(arbiter.execution().provenance.confidence, 1.0);
}

#[test]
fn the_health_shows_the_worst_reason() {
    let mut arbiter = working_run();
    arbiter.apply(input(
        Source::Derived,
        3,
        Observation::Version { validated: false },
    ));
    arbiter.apply(input(
        Source::Derived,
        4,
        Observation::Degraded(Some("the shim did not answer".to_owned())),
    ));
    assert_eq!(
        arbiter.health().value,
        AdapterHealth::Degraded {
            reason: "the shim did not answer".to_owned()
        }
    );
    arbiter.apply(structured(5, Observation::TransportLost));
    assert_eq!(arbiter.health().value, AdapterHealth::Disconnected);
    arbiter.apply(structured(6, Observation::TransportHealed));
    assert!(matches!(
        arbiter.health().value,
        AdapterHealth::Degraded { .. }
    ));
    arbiter.apply(input(Source::Derived, 7, Observation::Degraded(None)));
    assert_eq!(arbiter.health().value, AdapterHealth::UnvalidatedVersion);
}

#[test]
fn the_facets_carry_the_pending_ids_and_the_read_state_that_the_caller_gives() {
    let mut arbiter = working_run();
    arbiter.apply(structured(
        3,
        Observation::Opened {
            id: id(4),
            kind: InteractionKind::PlanReview,
        },
    ));
    let facets = arbiter.facets(ReadState::Unseen);
    assert_eq!(facets.pending, vec![id(4)]);
    assert_eq!(facets.read_state, ReadState::Unseen);
}
