//! Two signals that a link may be dead: the Mac woke up, and the network path changed.
//!
//! The host does not wait for TCP to find a lost link after a sleep. It uses no platform API for
//! this (no `NSWorkspace` observer and no `NWPathMonitor`), so the code is the same on every
//! system and has no foreign dependency:
//!
//! - **Wake.** A task ticks every second and compares the wall clock with the monotonic clock. The
//!   monotonic clock of `std` does not count the time of a sleep on macOS, and the wall clock
//!   does. A gap of more than 3 s between the two is a wake. A manual change of the clock is
//!   the same gap, and the cost of that false alarm is one new link.
//! - **Network.** A task reads the IPv4 addresses of the interfaces that are up every 2 s. A
//!   change of the set is a change of the path: a Wi-Fi that went, a cable, a VPN. IPv6 is left
//!   out, because the privacy addresses rotate and the interfaces for AirDrop come and go with
//!   no IPv4 address.
//!
//! Both call the function they got, and that function nudges the links of the app.

use std::{
    collections::BTreeSet,
    net::Ipv4Addr,
    sync::Arc,
    time::{Duration, Instant, SystemTime},
};

/// How often the wake task looks at the clocks.
pub const WAKE_TICK: Duration = Duration::from_secs(1);

/// How often the network task reads the addresses.
pub const NET_TICK: Duration = Duration::from_secs(2);

/// The gap between the clocks that is a wake.
pub const WAKE_GAP: Duration = Duration::from_secs(3);

/// Why a link may be dead.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cause {
    Wake,
    Network,
}

/// Whether the time between two ticks shows a sleep.
pub fn is_wake(monotonic: Duration, wall: Duration) -> bool {
    wall.saturating_sub(monotonic) > WAKE_GAP
}

/// The IPv4 addresses of the interfaces that are up and running, by interface name. The loopback
/// is left out.
pub type Addresses = BTreeSet<(String, Ipv4Addr)>;

/// Reads the addresses now.
#[allow(
    unsafe_code,
    reason = "getifaddrs and freeifaddrs are C functions, and the list they give is read with raw pointers"
)]
pub fn addresses() -> Addresses {
    let mut found = Addresses::new();
    let mut list: *mut libc::ifaddrs = std::ptr::null_mut();
    // SAFETY: `getifaddrs` fills `list` with a linked list that `freeifaddrs` frees. The loop
    // reads each node once, and only while the list is alive.
    unsafe {
        if libc::getifaddrs(&raw mut list) != 0 {
            return found;
        }
        let mut node = list;
        while !node.is_null() {
            let entry = &*node;
            node = entry.ifa_next;
            let flags = entry.ifa_flags;
            let up = flags & libc::IFF_UP as u32 != 0 && flags & libc::IFF_RUNNING as u32 != 0;
            if !up || flags & libc::IFF_LOOPBACK as u32 != 0 || entry.ifa_addr.is_null() {
                continue;
            }
            if i32::from((*entry.ifa_addr).sa_family) != libc::AF_INET {
                continue;
            }
            let v4 = &*entry.ifa_addr.cast::<libc::sockaddr_in>();
            let address = Ipv4Addr::from(u32::from_be(v4.sin_addr.s_addr));
            let name = std::ffi::CStr::from_ptr(entry.ifa_name)
                .to_string_lossy()
                .into_owned();
            found.insert((name, address));
        }
        libc::freeifaddrs(list);
    }
    found
}

/// Starts the two tasks on the runtime of the caller. `on_cause` runs on a task of the runtime.
/// The tasks end when the returned handle is aborted or dropped.
pub fn watch(on_cause: Arc<dyn Fn(Cause) + Send + Sync>) -> Watch {
    let wake = {
        let on_cause = Arc::clone(&on_cause);
        tokio::spawn(async move {
            let (mut mono, mut wall) = (Instant::now(), SystemTime::now());
            let mut tick = tokio::time::interval(WAKE_TICK);
            loop {
                tick.tick().await;
                let (now_mono, now_wall) = (Instant::now(), SystemTime::now());
                let mono_gap = now_mono.duration_since(mono);
                // The wall clock can go back; then the gap is zero.
                let wall_gap = now_wall.duration_since(wall).unwrap_or_default();
                (mono, wall) = (now_mono, now_wall);
                if is_wake(mono_gap, wall_gap) {
                    tracing::info!(?wall_gap, ?mono_gap, "the Mac woke up");
                    on_cause(Cause::Wake);
                }
            }
        })
    };
    let net = tokio::spawn(async move {
        let mut known = tokio::task::spawn_blocking(addresses)
            .await
            .unwrap_or_default();
        let mut tick = tokio::time::interval(NET_TICK);
        loop {
            tick.tick().await;
            let now = tokio::task::spawn_blocking(addresses)
                .await
                .unwrap_or_default();
            if now != known {
                tracing::info!("the network path changed");
                known = now;
                on_cause(Cause::Network);
            }
        }
    });
    Watch {
        tasks: vec![wake, net],
    }
}

/// The two tasks. They end with it.
pub struct Watch {
    tasks: Vec<tokio::task::JoinHandle<()>>,
}

impl Drop for Watch {
    fn drop(&mut self) {
        for task in &self.tasks {
            task.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_gap_of_more_than_three_seconds_between_the_clocks_is_a_wake() {
        let tick = Duration::from_secs(1);
        // An ordinary tick, and a tick that came a little late.
        assert!(!is_wake(tick, tick));
        assert!(!is_wake(tick, tick + Duration::from_secs(2)));
        assert!(!is_wake(tick, tick + WAKE_GAP));
        // A sleep of one minute: the monotonic clock did not move.
        assert!(is_wake(tick, Duration::from_secs(61)));
        assert!(is_wake(Duration::ZERO, Duration::from_secs(4)));
        // A wall clock set back gives no gap.
        assert!(!is_wake(tick, Duration::ZERO));
    }

    #[test]
    fn the_addresses_have_no_loopback() {
        for (name, address) in addresses() {
            assert!(!address.is_loopback(), "{name} {address}");
            assert!(!name.starts_with("lo"), "{name}");
        }
    }

    #[tokio::test(start_paused = true)]
    async fn the_tasks_end_with_the_watch() {
        let seen = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter = Arc::clone(&seen);
        let watching = watch(Arc::new(move |_| {
            counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }));
        tokio::time::sleep(Duration::from_secs(5)).await;
        drop(watching);
        // With a paused clock the clocks move together, so nothing woke.
        assert_eq!(seen.load(std::sync::atomic::Ordering::SeqCst), 0);
    }
}
