//! Checkpoint requests: each trigger fires once, and stops after `CheckpointTaken`.

mod support;

use std::time::Duration;

use nh_proto::holder::{CheckpointReason, HolderEvent, HolderRequest};
use support::{TestHolder, config};

fn wants(events: &[HolderEvent]) -> Vec<(u64, CheckpointReason)> {
    events
        .iter()
        .filter_map(|e| match e {
            HolderEvent::CheckpointWanted { offset, reason } => Some((*offset, *reason)),
            _ => None,
        })
        .collect()
}

/// A holder whose child prints one line of `n` bytes and waits, with these checkpoint settings.
fn holder(n: usize, ring: u64, idle_ms: u32, interval_ms: u32) -> TestHolder {
    let script = format!("head -c {n} /dev/zero | tr '\\0' x; echo; sleep 30");
    let mut config = config(&["/bin/sh", "-c", &script]);
    config.ring_bytes = ring;
    config.checkpoint_idle_ms = idle_ms;
    config.checkpoint_max_interval_ms = interval_ms;
    TestHolder::spawn_with(config, None)
}

#[test]
fn a_half_full_ring_asks_for_a_checkpoint_once_and_stops_after_it_is_taken() {
    let holder = holder(600, 1000, 60_000, 300_000);
    let mut client = holder.attach(0);
    let (offset, reason) = client.wait_for(|e| wants(std::slice::from_ref(e)).first().copied());
    assert_eq!(reason, CheckpointReason::RingHalf);
    // The request comes after the read of the 600 bytes. The `\r\n` can already be in the journal.
    assert!([600, 602].contains(&offset), "offset {offset}");
    // No second request while the first is open.
    assert!(wants(&client.events_within(Duration::from_millis(700))).is_empty());
    client.send(&HolderRequest::CheckpointTaken { offset });
    assert!(wants(&client.events_within(Duration::from_millis(700))).is_empty());
}

#[test]
fn quiet_output_asks_for_a_checkpoint_after_the_idle_time() {
    let holder = holder(20, 1 << 20, 300, 300_000);
    let mut client = holder.attach(0);
    let (offset, reason) = client.wait_for(|e| wants(std::slice::from_ref(e)).first().copied());
    assert_eq!((offset, reason), (22, CheckpointReason::Idle));
    assert!(wants(&client.events_within(Duration::from_millis(700))).is_empty());
}

#[test]
fn an_old_checkpoint_asks_for_a_new_one_after_the_interval() {
    let holder = holder(20, 1 << 20, 60_000, 300);
    let mut client = holder.attach(0);
    let (offset, reason) = client.wait_for(|e| wants(std::slice::from_ref(e)).first().copied());
    assert_eq!((offset, reason), (22, CheckpointReason::Interval));
}

#[test]
fn a_client_that_attaches_later_gets_its_own_request() {
    let holder = holder(20, 1 << 20, 300, 300_000);
    let mut first = holder.attach(0);
    first.wait_for(|e| wants(std::slice::from_ref(e)).first().copied());
    drop(first);
    // A new core has made no checkpoint, so the holder does not wait for the repeat time.
    let mut second = holder.attach(0);
    let (_, reason) = second.wait_for(|e| wants(std::slice::from_ref(e)).first().copied());
    assert_eq!(reason, CheckpointReason::Idle);
}
