//! Links to a loopback address from a window of a remote machine.
//!
//! `http://localhost:8765/` in a pane of a remote machine is a service of that machine, so the
//! system browser of this Mac cannot reach it. The host adds a forward with `ssh -O forward`,
//! through the control socket of the link, and opens the address with the port of the forward.
//! This module holds the pure parts: the rule for the address and the choice of the local port.

use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpListener};

/// An address that names a service of the machine of the window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Loopback {
    /// `http` or `https`.
    scheme: String,
    /// The port in the address, or the default port of the scheme.
    pub port: u16,
    /// What follows the authority: the path, the query and the fragment.
    rest: String,
}

impl Loopback {
    /// Reads an address. Only `http` and `https` addresses to `localhost`, `127.0.0.1`, `[::1]`
    /// or `0.0.0.0` count, with no user name. Any other address is `None` and opens as it is.
    pub fn parse(address: &str) -> Option<Self> {
        let (scheme, after) = address.split_once("://")?;
        let scheme = scheme.to_ascii_lowercase();
        let default = match scheme.as_str() {
            "http" => 80,
            "https" => 443,
            _ => return None,
        };
        let end = after.find(['/', '?', '#']).unwrap_or(after.len());
        let (authority, rest) = after.split_at(end);
        if authority.contains('@') {
            return None;
        }
        // The host ends at the last colon that is outside the brackets of an IPv6 address.
        let (host, port) = match authority.rfind(':') {
            Some(at) if !authority[at..].contains(']') => {
                (&authority[..at], Some(&authority[at + 1..]))
            }
            _ => (authority, None),
        };
        let loopback = host.eq_ignore_ascii_case("localhost")
            || ["127.0.0.1", "[::1]", "0.0.0.0"].contains(&host);
        if !loopback {
            return None;
        }
        let port = match port {
            None => default,
            Some(text) => text.parse::<u16>().ok().filter(|port| *port != 0)?,
        };
        Some(Self {
            scheme,
            port,
            rest: rest.to_owned(),
        })
    }

    /// The same address with the port of the forward on this Mac. The host is `localhost`, also
    /// for `0.0.0.0`, which a browser does not take as a place to connect to.
    pub fn with_port(&self, local_port: u16) -> String {
        format!("{}://localhost:{local_port}{}", self.scheme, self.rest)
    }
}

/// The port on this Mac for a forward to `remote_port`: the same number when it is free on the
/// loopback address, else a free one that the system picks. Another program can take the port
/// before `ssh` binds it, and `ssh` then fails: the caller reports that.
pub fn pick_local_port(remote_port: u16) -> Option<u16> {
    let free = |port: u16| {
        TcpListener::bind(SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port))).ok()
    };
    if let Some(listener) = free(remote_port) {
        drop(listener);
        return Some(remote_port);
    }
    let listener = free(0)?;
    let port = listener.local_addr().ok()?.port();
    drop(listener);
    Some(port)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_http_addresses_to_a_loopback_host_are_loopback() {
        for (address, port) in [
            ("http://localhost:8765/", 8765),
            ("http://LOCALHOST:3000", 3000),
            ("https://127.0.0.1:8443/a/b?c=d#e", 8443),
            ("http://[::1]:5173/", 5173),
            ("http://0.0.0.0:3000/x?y", 3000),
            ("http://localhost/", 80),
            ("https://localhost", 443),
        ] {
            let found = Loopback::parse(address).unwrap_or_else(|| panic!("{address}"));
            assert_eq!(found.port, port, "{address}");
        }
        for address in [
            "http://example.com:80/",
            "http://localhost.example.com:80/",
            "http://127.0.0.2:80/",
            "http://user@localhost:80/",
            "http://localhost:0/",
            "http://localhost:99999/",
            "http://localhost:x/",
            "ftp://localhost:21/",
            "file:///etc/passwd",
            "mailto:a@example.com",
            "localhost:8000",
            "",
        ] {
            assert!(Loopback::parse(address).is_none(), "{address}");
        }
    }

    #[test]
    fn the_address_gets_the_port_of_the_forward_and_keeps_the_rest() {
        let found = Loopback::parse("http://0.0.0.0:3000/x?y#z").expect("loopback");
        assert_eq!(found.with_port(3000), "http://localhost:3000/x?y#z");
        assert_eq!(found.with_port(41000), "http://localhost:41000/x?y#z");
        let found = Loopback::parse("https://[::1]/").expect("loopback");
        assert_eq!(found.with_port(9), "https://localhost:9/");
        let found = Loopback::parse("http://localhost:8080").expect("loopback");
        assert_eq!(found.with_port(8080), "http://localhost:8080");
    }

    #[test]
    fn a_free_port_stays_and_a_busy_port_gets_another() {
        let held = TcpListener::bind("127.0.0.1:0").expect("a port");
        let busy = held.local_addr().expect("an address").port();
        let other = pick_local_port(busy).expect("a free port");
        assert_ne!(other, busy);
        drop(held);
        assert_eq!(pick_local_port(busy), Some(busy));
    }
}
