//! Ports for a task: the first free ones of a range, found with a bind probe.

use std::{collections::HashSet, net::TcpListener};

/// Whether something can listen on the port of the loopback address now.
pub fn is_free(port: u16) -> bool {
    TcpListener::bind(("127.0.0.1", port)).is_ok()
}

/// The first `wanted` ports of the range `[low, high]` that no task leases and that are free now.
/// `None` when the range has fewer.
pub fn pick(wanted: usize, range: (u16, u16), leased: &HashSet<u16>) -> Option<Vec<u16>> {
    pick_with(wanted, range, leased, is_free)
}

/// Like [`pick`], with the probe given.
pub fn pick_with(
    wanted: usize,
    range: (u16, u16),
    leased: &HashSet<u16>,
    free: impl Fn(u16) -> bool,
) -> Option<Vec<u16>> {
    let mut found = Vec::new();
    for port in range.0..=range.1 {
        if found.len() == wanted {
            break;
        }
        if !leased.contains(&port) && free(port) {
            found.push(port);
        }
    }
    (found.len() == wanted).then_some(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_free_ports_that_nobody_leases_are_taken() {
        let leased: HashSet<u16> = [41_000, 41_002].into_iter().collect();
        let busy = |port: u16| port != 41_001;
        assert_eq!(
            pick_with(2, (41_000, 41_999), &leased, busy),
            Some(vec![41_003, 41_004])
        );
        assert_eq!(
            pick_with(1, (41_000, 41_999), &leased, |_| true),
            Some(vec![41_001])
        );
    }

    #[test]
    fn a_range_with_too_few_ports_gives_none() {
        let leased = HashSet::new();
        assert_eq!(pick_with(3, (50_000, 50_001), &leased, |_| true), None);
        assert_eq!(
            pick_with(0, (50_000, 50_001), &leased, |_| true),
            Some(Vec::new())
        );
    }

    #[test]
    fn the_probe_sees_a_port_that_is_in_use() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("a port");
        let port = listener.local_addr().expect("an address").port();
        assert!(!is_free(port));
        drop(listener);
        assert!(is_free(port));
    }
}
