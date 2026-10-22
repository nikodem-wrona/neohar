//! `policy.suggest`: the rules that the answers of the user ask for.
//!
//! A fingerprint that was approved at least [`MIN_APPROVALS`] times with no denial inside
//! [`WINDOW_DAYS`] days becomes a rule for the user to paste into `config.toml`. Nothing is
//! applied. The fingerprints that no rule may cover (`fingerprint`) are not eligible, and never
//! come out.

use std::collections::BTreeMap;

use nh_proto::api::{
    calls::system::{PolicyDecision, PolicyRule, PolicySuggestion},
    common::Harness,
    time::Timestamp,
};
use nh_store::FingerprintRow;

/// How many approvals a rule needs.
pub const MIN_APPROVALS: u32 = 5;
/// The days that count.
pub const WINDOW_DAYS: i64 = 30;
/// The window in milliseconds.
pub const WINDOW_MS: i64 = WINDOW_DAYS * 86_400_000;

fn harness_of(name: &str) -> Option<Harness> {
    match name {
        "claude" => Some(Harness::Claude),
        "codex" => Some(Harness::Codex),
        _ => None,
    }
}

/// The rule for a pattern: `pnpm test:*` is a command, and a glob is a set of paths.
pub fn rule_for(
    harness: Harness,
    tool: &str,
    pattern: &str,
    workspace: Option<&str>,
    reason: Option<String>,
) -> PolicyRule {
    // A command prefix ends with `:*` (`nh_rich::command::rule_prefix`). Anything else is a glob.
    let command = pattern.ends_with(":*").then(|| pattern.to_owned());
    PolicyRule {
        id: None,
        decision: PolicyDecision::Allow,
        harness: vec![harness],
        tool: tool.to_owned(),
        paths: if command.is_none() {
            vec![pattern.to_owned()]
        } else {
            Vec::new()
        },
        command,
        workspace: workspace.map(str::to_owned),
        reason,
    }
}

/// A rule as a `[[policy.rule]]` block, ready to paste.
pub fn toml_block(rule: &PolicyRule) -> String {
    let quote = |text: &str| toml::Value::String(text.to_owned()).to_string();
    let mut out = String::from("[[policy.rule]]\n");
    if let Some(id) = &rule.id {
        out.push_str(&format!("id = {}\n", quote(id)));
    }
    out.push_str(&format!(
        "decision = {}\n",
        quote(match rule.decision {
            PolicyDecision::Allow => "allow",
            PolicyDecision::Deny => "deny",
        })
    ));
    if !rule.harness.is_empty() {
        let names: Vec<String> = rule
            .harness
            .iter()
            .map(|h| {
                quote(match h {
                    Harness::Claude => "claude",
                    Harness::Codex => "codex",
                })
            })
            .collect();
        out.push_str(&format!("harness = [{}]\n", names.join(", ")));
    }
    out.push_str(&format!("tool = {}\n", quote(&rule.tool)));
    if let Some(command) = &rule.command {
        out.push_str(&format!("command = {}\n", quote(command)));
    }
    if !rule.paths.is_empty() {
        let globs: Vec<String> = rule.paths.iter().map(|p| quote(p)).collect();
        out.push_str(&format!("paths = [{}]\n", globs.join(", ")));
    }
    if let Some(workspace) = &rule.workspace {
        out.push_str(&format!("workspace = {}\n", quote(workspace)));
    }
    if let Some(reason) = &rule.reason {
        out.push_str(&format!("reason = {}\n", quote(reason)));
    }
    out
}

#[derive(Default)]
struct Group<'a> {
    approvals: u32,
    denials: u32,
    eligible: bool,
    last_at: i64,
    sample: Option<&'a FingerprintRow>,
}

/// The suggestions for the answers since `now - WINDOW_MS`. The most approved first.
pub fn suggest(rows: &[FingerprintRow], now_ms: i64) -> Vec<PolicySuggestion> {
    let mut groups: BTreeMap<&str, Group<'_>> = BTreeMap::new();
    for row in rows.iter().filter(|r| r.at >= now_ms - WINDOW_MS) {
        let group = groups.entry(row.fingerprint.as_str()).or_insert(Group {
            eligible: true,
            ..Group::default()
        });
        match row.decision.as_str() {
            "allow" => group.approvals += 1,
            _ => group.denials += 1,
        }
        group.eligible &= row.eligible;
        group.last_at = group.last_at.max(row.at);
        group.sample = Some(row);
    }
    let mut out: Vec<PolicySuggestion> = groups
        .into_iter()
        .filter(|(_, g)| g.approvals >= MIN_APPROVALS && g.denials == 0 && g.eligible)
        .filter_map(|(fingerprint, g)| {
            let row = g.sample?;
            let harness = harness_of(row.harness.as_deref()?)?;
            let pattern = row.pattern.as_deref()?;
            let tool = row.tool.as_deref()?;
            let rule = rule_for(
                harness,
                tool,
                pattern,
                row.workspace.as_deref(),
                Some(format!(
                    "approved {} times in {WINDOW_DAYS} days",
                    g.approvals
                )),
            );
            Some(PolicySuggestion {
                toml: toml_block(&rule),
                rule,
                fingerprint: fingerprint.to_owned(),
                approvals: g.approvals,
                last_at: Timestamp(g.last_at),
            })
        })
        .collect();
    out.sort_by(|a, b| {
        b.approvals
            .cmp(&a.approvals)
            .then(b.last_at.cmp(&a.last_at))
            .then(a.fingerprint.cmp(&b.fingerprint))
    });
    out
}

#[cfg(test)]
mod tests;
