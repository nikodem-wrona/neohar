//! The endpoints that a push goes to: `https` on port 443 to the push services of
//! Apple, Google and Mozilla. The core sends a POST to the endpoint of a subscription, so without
//! this list a paired phone could make the Mac send requests to any host.
//!
//! A development build also takes the endpoints of one test origin, `http://127.0.0.1:<port>`: the
//! fake push service of the tests. A release build never does.

/// The hosts of the push services.
pub const HOSTS: [&str; 3] = [
    "web.push.apple.com",
    "fcm.googleapis.com",
    "updates.push.services.mozilla.com",
];

/// The longest endpoint, in bytes.
const MAX_LEN: usize = 2048;

/// Where a push goes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    /// `https`, or `http` for the test origin.
    pub tls: bool,
    pub host: String,
    pub port: u16,
    /// The path and the query, from the first `/`.
    pub path: String,
}

impl Target {
    /// The origin of the endpoint: the claim `aud` of the JWT (RFC 8292).
    pub fn origin(&self) -> String {
        match (self.tls, self.port) {
            (true, 443) => format!("https://{}", self.host),
            (true, port) => format!("https://{}:{port}", self.host),
            (false, port) => format!("http://{}:{port}", self.host),
        }
    }

    /// The value of the header `Host`.
    pub fn host_header(&self) -> String {
        match (self.tls, self.port) {
            (true, 443) => self.host.clone(),
            (_, port) => format!("{}:{port}", self.host),
        }
    }
}

/// Checks an endpoint. `test` is the test origin of a development build, `http://127.0.0.1:<port>`.
/// `Err` has the reason.
pub fn check(endpoint: &str, test: Option<&str>) -> Result<Target, String> {
    if endpoint.len() > MAX_LEN {
        return Err(format!("the endpoint is longer than {MAX_LEN} bytes"));
    }
    if endpoint
        .chars()
        .any(|c| c.is_whitespace() || c.is_control() || !c.is_ascii())
    {
        return Err("the endpoint has a space or a character that is not ASCII".to_owned());
    }
    if endpoint.contains('#') {
        return Err("the endpoint has a fragment".to_owned());
    }
    if let Some(test) = test.filter(|_| cfg!(debug_assertions))
        && let Some(target) = test_target(endpoint, test)
    {
        return Ok(target);
    }
    let rest = endpoint
        .strip_prefix("https://")
        .ok_or("the endpoint is not https")?;
    let (authority, path) = match rest.find(['/', '?']) {
        Some(at) => rest.split_at(at),
        None => (rest, ""),
    };
    if authority.contains('@') {
        return Err("the endpoint has a user name".to_owned());
    }
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, "443")) => (host, 443),
        Some(_) => return Err("the endpoint is not on port 443".to_owned()),
        None => (authority, 443),
    };
    let host = host.to_ascii_lowercase();
    if !HOSTS.contains(&host.as_str()) {
        return Err(format!(
            "the push service {host} is not on the list: {}",
            HOSTS.join(", ")
        ));
    }
    Ok(Target {
        tls: true,
        host,
        port,
        path: path_of(path),
    })
}

/// The endpoint as a target of the test origin, or `None` when it is not one.
fn test_target(endpoint: &str, test: &str) -> Option<Target> {
    let port: u16 = test.strip_prefix("http://127.0.0.1:")?.parse().ok()?;
    let path = endpoint.strip_prefix(test)?;
    if !(path.is_empty() || path.starts_with('/') || path.starts_with('?')) {
        return None;
    }
    Some(Target {
        tls: false,
        host: "127.0.0.1".to_owned(),
        port,
        path: path_of(path),
    })
}

fn path_of(path: &str) -> String {
    if path.starts_with('/') {
        path.to_owned()
    } else {
        format!("/{path}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_push_services_on_port_443_pass() {
        let target = check("https://web.push.apple.com/QGhP9m2Aw", None).expect("Apple");
        assert_eq!(
            target,
            Target {
                tls: true,
                host: "web.push.apple.com".to_owned(),
                port: 443,
                path: "/QGhP9m2Aw".to_owned(),
            }
        );
        assert_eq!(target.origin(), "https://web.push.apple.com");
        assert_eq!(target.host_header(), "web.push.apple.com");
        for endpoint in [
            "https://fcm.googleapis.com/fcm/send/abc:def",
            "https://updates.push.services.mozilla.com/wpush/v2/gAAAA",
            "https://Web.Push.Apple.com:443/x",
        ] {
            assert!(check(endpoint, None).is_ok(), "{endpoint}");
        }
    }

    #[test]
    fn other_schemes_hosts_ports_and_address_literals_fail() {
        for endpoint in [
            "http://web.push.apple.com/x",
            "https://example.com/x",
            "https://web.push.apple.com.example.com/x",
            "https://push.apple.com/x",
            "https://web.push.apple.com:8443/x",
            "https://web.push.apple.com:/x",
            "https://17.253.144.10/x",
            "https://[2620:149::1]/x",
            "https://127.0.0.1/x",
            "https://user@web.push.apple.com/x",
            "https://web.push.apple.com/x y",
            "https://web.push.apple.com/x#y",
            "ftp://web.push.apple.com/x",
            "",
        ] {
            assert!(check(endpoint, None).is_err(), "{endpoint}");
        }
        let long = format!("https://web.push.apple.com/{}", "a".repeat(MAX_LEN));
        assert!(check(&long, None).is_err());
    }

    #[test]
    fn a_development_build_takes_the_test_origin_only() {
        let test = Some("http://127.0.0.1:5123");
        let target = check("http://127.0.0.1:5123/push/1", test).expect("the test origin");
        assert!(!target.tls);
        assert_eq!(target.port, 5123);
        assert_eq!(target.path, "/push/1");
        assert_eq!(target.origin(), "http://127.0.0.1:5123");
        assert!(
            check("http://127.0.0.1:5124/push/1", test).is_err(),
            "another port"
        );
        assert!(
            check("http://127.0.0.1:51234/push", test).is_err(),
            "a longer port"
        );
        assert!(
            check("http://127.0.0.1:5123/push/1", None).is_err(),
            "no test origin"
        );
        assert!(
            check("http://localhost:5123/push", Some("http://localhost:5123")).is_err(),
            "only 127.0.0.1"
        );
    }
}
