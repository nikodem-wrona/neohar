//! The hosts that a command talks to, for the rule "network to non-allowlisted hosts".

use super::lex::Word;

/// Programs that open a connection to a host that their arguments name.
pub(super) fn is_network_program(program: &str) -> bool {
    matches!(
        program,
        "curl"
            | "wget"
            | "http"
            | "https"
            | "xh"
            | "nc"
            | "ncat"
            | "netcat"
            | "telnet"
            | "ssh"
            | "scp"
            | "sftp"
            | "rsync"
            | "ftp"
            | "nmap"
            | "aria2c"
    )
}

/// What the arguments of a network program say about its hosts.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Hosts {
    pub names: Vec<String>,
    /// The command connects somewhere that the text does not name: a variable, or no host at all.
    pub unknown: bool,
}

/// Flags that take a value, so the word after one is not a host.
fn takes_value(program: &str, flag: &str) -> bool {
    let flags: &[&str] = match program {
        "curl" => &[
            "-o",
            "--output",
            "-H",
            "--header",
            "-d",
            "--data",
            "--data-raw",
            "--data-binary",
            "--data-urlencode",
            "-X",
            "--request",
            "-u",
            "--user",
            "-A",
            "--user-agent",
            "-e",
            "--referer",
            "-b",
            "--cookie",
            "-c",
            "--cookie-jar",
            "-F",
            "--form",
            "-T",
            "--upload-file",
            "-w",
            "--write-out",
            "-m",
            "--max-time",
            "--connect-timeout",
            "-x",
            "--proxy",
            "-K",
            "--config",
            "--cacert",
            "--cert",
            "--key",
            "-r",
            "--range",
            "-D",
            "--dump-header",
            "--retry",
            "-C",
            "--continue-at",
            "-E",
            "-y",
        ],
        "wget" => &[
            "-O",
            "-o",
            "-a",
            "-P",
            "-i",
            "-U",
            "--header",
            "--post-data",
            "--post-file",
            "--user",
            "--password",
            "-e",
            "-t",
            "-T",
            "-w",
            "--limit-rate",
            "-B",
            "--output-document",
            "--directory-prefix",
            "--user-agent",
            "--tries",
            "--timeout",
        ],
        "ssh" | "sftp" => &[
            "-p", "-i", "-l", "-o", "-F", "-J", "-L", "-R", "-D", "-b", "-c", "-m", "-E", "-S",
            "-W", "-w", "-B", "-e", "-O", "-Q", "-P",
        ],
        "scp" => &["-P", "-i", "-l", "-o", "-F", "-J", "-c", "-S", "-D"],
        "rsync" => &[
            "-e",
            "--rsh",
            "--port",
            "--exclude",
            "--include",
            "--filter",
            "-f",
        ],
        "nc" | "ncat" | "netcat" => &["-p", "-s", "-w", "-i", "-x", "-X", "-I", "-O", "-T", "-q"],
        _ => &[],
    };
    flags.contains(&flag)
}

/// The host of an address such as `https://user@host:8080/path`, `host:8080/path` or `[::1]:80`.
fn host_of(address: &str) -> Option<String> {
    let rest = address.split_once("://").map_or(address, |(_, rest)| rest);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    let authority = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    let host = if let Some(inner) = authority.strip_prefix('[') {
        inner.split(']').next()?
    } else {
        authority.split(':').next()?
    };
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    (!host.is_empty()).then_some(host)
}

/// A word such as `user@host:path` or `host:path` of `scp` and `rsync`.
fn scp_host(word: &str) -> Option<String> {
    if word.starts_with(['-', '/', '.', '~']) || word.contains("://") {
        return None;
    }
    let (before, _) = word.split_once(':')?;
    // `C:` is a drive, and a single letter before the colon is not a host.
    if before.len() < 2 {
        return None;
    }
    host_of(before)
}

/// The hosts that a network program connects to.
pub(super) fn hosts(program: &str, args: &[&Word]) -> Hosts {
    let mut out = Hosts::default();
    let info_only = args.iter().any(|a| {
        matches!(
            a.text.as_str(),
            "--help" | "-h" | "--version" | "-V" | "-help"
        )
    });
    if info_only {
        return out;
    }
    let mut skip_next = false;
    let mut url_next = false;
    let mut positional = 0usize;
    let mut found_any = false;
    for arg in args {
        let text = arg.text.as_str();
        if skip_next {
            skip_next = false;
            continue;
        }
        let url = std::mem::take(&mut url_next);
        if !url && let Some((flag, _)) = text.split_once('=').filter(|_| text.starts_with("--")) {
            // `--flag=value` carries its value. Only `--url=` names a host.
            if flag == "--url" {
                add_address(
                    &mut out,
                    &text["--url=".len()..],
                    arg.dynamic,
                    &mut found_any,
                );
            }
            continue;
        }
        if text == "--url" {
            url_next = true;
            continue;
        }
        if text.starts_with('-') && !url {
            if takes_value(program, text) {
                skip_next = true;
            }
            continue;
        }
        if text.contains("://") || url {
            add_address(&mut out, text, arg.dynamic, &mut found_any);
            continue;
        }
        positional += 1;
        match program {
            "scp" | "rsync" => {
                if let Some(host) = scp_host(text) {
                    out.names.push(host);
                    found_any = true;
                } else if arg.dynamic {
                    out.unknown = true;
                }
            }
            // `git@host:path` is a remote. A name such as `origin` is not an address.
            "git" => {
                if let Some(host) = scp_host(text) {
                    out.names.push(host);
                    found_any = true;
                }
            }
            // The first operand is the host.
            "ssh" | "sftp" | "telnet" | "nc" | "ncat" | "netcat" | "ftp" | "nmap" => {
                if positional == 1 {
                    add_address(&mut out, text, arg.dynamic, &mut found_any);
                }
            }
            // `curl localhost:8080`: an operand with no scheme is an address.
            _ => add_address(&mut out, text, arg.dynamic, &mut found_any),
        }
    }
    // `git push origin` names no host, and the remote is in the configuration of the repository.
    if !found_any && program != "git" {
        out.unknown = true;
    }
    out
}

fn add_address(out: &mut Hosts, text: &str, dynamic: bool, found: &mut bool) {
    if dynamic && host_of(text).is_none_or(|host| host.contains('$') || host.contains('`')) {
        out.unknown = true;
        *found = true;
        return;
    }
    match host_of(text) {
        Some(host) => {
            out.names.push(host);
            *found = true;
        }
        None => out.unknown = true,
    }
}

/// A host that is never a risk: this machine.
pub(super) fn is_loopback(host: &str) -> bool {
    host == "localhost"
        || host.ends_with(".localhost")
        || host == "::1"
        || host == "0.0.0.0"
        || host.starts_with("127.") && host.bytes().all(|b| b.is_ascii_digit() || b == b'.')
}

/// Whether the allowlist has the host. An entry `*.example.com` has every subdomain of
/// `example.com` and not `example.com` itself.
pub(super) fn is_allowed(host: &str, allow: &[String]) -> bool {
    if is_loopback(host) {
        return true;
    }
    allow.iter().any(|entry| {
        let entry = entry.trim().to_ascii_lowercase();
        match entry.strip_prefix("*.") {
            Some(suffix) => host.len() > suffix.len() + 1 && host.ends_with(&format!(".{suffix}")),
            None => entry == host,
        }
    })
}

#[cfg(test)]
mod tests;
