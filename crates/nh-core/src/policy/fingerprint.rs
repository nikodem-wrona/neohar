//! The fingerprint of an approval.
//!
//! `hash(harness, tool, normalized command prefix | path glob, workspace)`. Every answered approval
//! stores one with its decision. A fingerprint also says whether a rule may be suggested for it,
//! and "Allow always" made from it: not for a high risk, a line that is not plain (many commands,
//! a substitution, a glob), a program that destroys data or runs any code, a path that touches
//! `.git`, `.env*`, keys or credentials, or a rule that would not allow the request itself.

use std::path::Path;

use nh_proto::api::common::{Harness, Risk};
use nh_rich::command::{is_credential_path, rule_prefix};
use serde::{Deserialize, Serialize};

use super::matcher::{self, CommandPattern, Subject};

/// What the core remembers about an approval, from the moment it opens.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Facts {
    /// The hash. Two approvals of the same kind in the same workspace have the same one.
    pub fingerprint: String,
    pub harness: Harness,
    pub tool: String,
    /// The command prefix such as `pnpm test:*`, or the glob of the paths. `None` when no rule
    /// should be made for it.
    pub pattern: Option<String>,
    pub workspace: String,
    /// A rule may be suggested for it.
    pub eligible: bool,
}

/// Whether a path is one that no suggested rule may cover: `.git`, `.env*`, keys, credentials.
fn sensitive(path: &str) -> bool {
    let p = Path::new(path);
    let in_git = p.components().any(|c| c.as_os_str() == ".git");
    let env = p
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.starts_with(".env"));
    in_git || env || is_credential_path(path)
}

/// The glob for a set of relative paths: the directory that holds them all, with `/**`. A single
/// file in the root of the workspace names itself.
fn glob_of(paths: &[String]) -> Option<String> {
    let mut common: Option<Vec<&str>> = None;
    for path in paths {
        let parts: Vec<&str> = path.split('/').collect();
        let dir = &parts[..parts.len().saturating_sub(1)];
        common = Some(match common {
            None => dir.to_vec(),
            Some(known) => known
                .iter()
                .zip(dir)
                .take_while(|(a, b)| a == b)
                .map(|(a, _)| *a)
                .collect(),
        });
    }
    let common = common?;
    if common.is_empty() {
        return match paths {
            [only] if !only.contains('/') => Some(only.clone()),
            _ => None,
        };
    }
    Some(format!("{}/**", common.join("/")))
}

/// Whether a rule with `pattern` would allow the request itself, as the matcher reads it. A rule
/// that does not (`cargo test:*` for `FOO=1 cargo test`) is no rule for it.
fn covers(pattern: &str, subject: &Subject) -> bool {
    match &subject.command {
        Some(line) => CommandPattern::parse(pattern)
            .is_ok_and(|p| matcher::is_plain(line) && p.fits(&matcher::normalize(line))),
        None => subject
            .paths
            .iter()
            .all(|path| matcher::glob_matches(pattern, path)),
    }
}

/// The facts of a request. `argv` is the first command of a shell line, as the card shows it.
pub fn facts_of(subject: &Subject, risk: Risk, argv: &[String]) -> Facts {
    let (pattern, key, mut eligible) = if let Some(line) = &subject.command {
        // A new line is white space, so the check of the characters reads the raw text.
        let prefix = rule_prefix(argv).filter(|p| covers(p, subject));
        let eligible = prefix.is_some();
        let key = prefix.clone().unwrap_or_else(|| matcher::normalize(line));
        (prefix, key, eligible)
    } else if !subject.paths.is_empty() {
        let inside = subject.paths.iter().all(|p| !p.starts_with('/'));
        let clean = subject.paths.iter().all(|p| !sensitive(p));
        let glob = if inside {
            glob_of(&subject.paths)
        } else {
            None
        };
        let key = glob.clone().unwrap_or_else(|| subject.paths.join("\n"));
        let eligible = inside && clean && glob.as_deref().is_some_and(|g| covers(g, subject));
        (glob, key, eligible)
    } else {
        (None, String::new(), false)
    };
    if risk == Risk::High {
        eligible = false;
    }
    let text = format!(
        "{:?}\n{}\n{}\n{}",
        subject.harness, subject.tool, key, subject.workspace
    );
    let hash = blake3::hash(text.as_bytes()).to_hex();
    Facts {
        fingerprint: hash.as_str()[..16].to_owned(),
        harness: subject.harness,
        tool: subject.tool.clone(),
        pattern: pattern.filter(|_| eligible),
        workspace: subject.workspace.clone(),
        eligible,
    }
}

#[cfg(test)]
mod tests;
