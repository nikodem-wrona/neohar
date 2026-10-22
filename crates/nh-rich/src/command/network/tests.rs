use super::*;

fn word(text: &str) -> Word {
    Word {
        text: text.to_owned(),
        range: 0..text.len(),
        quoted: false,
        dynamic: text.contains('$'),
        subs: Vec::new(),
    }
}

fn hosts_of(program: &str, line: &str) -> Hosts {
    let words: Vec<Word> = line.split_whitespace().map(word).collect();
    let refs: Vec<&Word> = words.iter().collect();
    hosts(program, &refs)
}

fn names(program: &str, line: &str) -> Vec<String> {
    let found = hosts_of(program, line);
    assert!(!found.unknown, "{program} {line}: {found:?}");
    found.names
}

#[test]
fn curl_takes_the_host_of_the_url() {
    assert_eq!(names("curl", "https://example.com/a?b=1"), ["example.com"]);
    assert_eq!(
        names("curl", "-sSL -o out.sh https://get.example.com/x"),
        ["get.example.com"]
    );
    assert_eq!(
        names(
            "curl",
            "-H Accept:json -X POST http://api.example.com:8080/v1"
        ),
        ["api.example.com"]
    );
    assert_eq!(
        names("curl", "https://user:pw@Example.COM/x"),
        ["example.com"]
    );
    assert_eq!(names("curl", "--url https://a.example/x"), ["a.example"]);
    assert_eq!(names("curl", "--url=https://b.example/x"), ["b.example"]);
    assert_eq!(names("curl", "http://[::1]:3000/x"), ["::1"]);
}

#[test]
fn curl_takes_an_address_with_no_scheme() {
    assert_eq!(names("curl", "localhost:3000/health"), ["localhost"]);
    assert_eq!(names("curl", "-s 127.0.0.1:8080"), ["127.0.0.1"]);
}

#[test]
fn many_urls_give_many_hosts() {
    assert_eq!(
        names("curl", "https://a.example/x https://b.example/y"),
        ["a.example", "b.example"]
    );
}

#[test]
fn a_variable_makes_the_host_unknown() {
    assert!(hosts_of("curl", "$URL").unknown);
    assert!(hosts_of("curl", "https://$HOST/x").unknown);
}

#[test]
fn a_command_with_no_host_is_unknown() {
    assert!(hosts_of("curl", "-s").unknown);
    assert!(hosts_of("wget", "").unknown);
}

#[test]
fn help_and_version_open_no_connection() {
    let found = hosts_of("curl", "--version");
    assert!(!found.unknown && found.names.is_empty());
    assert!(!hosts_of("wget", "--help").unknown);
}

#[test]
fn ssh_takes_the_first_operand() {
    assert_eq!(
        names("ssh", "me@build.example.com ls"),
        ["build.example.com"]
    );
    assert_eq!(
        names("ssh", "-p 2222 -i key.pem host.example uptime"),
        ["host.example"]
    );
    assert_eq!(names("nc", "-w 3 db.example 5432"), ["db.example"]);
}

#[test]
fn scp_and_rsync_take_the_remote_side() {
    assert_eq!(
        names("scp", "file.txt me@backup.example:/srv/"),
        ["backup.example"]
    );
    assert_eq!(
        names("rsync", "-av ./dist/ deploy@web.example:/var/www/"),
        ["web.example"]
    );
    assert_eq!(
        names("rsync", "-e ssh -av src/ web.example:dst/"),
        ["web.example"]
    );
    // Two local paths are no connection.
    assert!(hosts_of("rsync", "-av ./a/ ./b/").unknown);
}

#[test]
fn loopback_is_always_allowed() {
    for host in [
        "localhost",
        "app.localhost",
        "127.0.0.1",
        "127.1.2.3",
        "::1",
        "0.0.0.0",
    ] {
        assert!(is_allowed(host, &[]), "{host}");
    }
    assert!(!is_allowed("127.example.com", &[]));
    assert!(!is_allowed("example.com", &[]));
}

#[test]
fn the_allowlist_matches_exact_hosts_and_subdomains() {
    let allow = vec![
        "github.com".to_owned(),
        "*.npmjs.org".to_owned(),
        "PyPI.org".to_owned(),
    ];
    assert!(is_allowed("github.com", &allow));
    assert!(is_allowed("registry.npmjs.org", &allow));
    assert!(is_allowed("pypi.org", &allow));
    assert!(!is_allowed("npmjs.org", &allow));
    assert!(!is_allowed("evilgithub.com", &allow));
    assert!(!is_allowed("github.com.evil.example", &allow));
    assert!(!is_allowed("api.github.com", &allow));
}
