//! The policy: rules that decide an approval before a person is asked.
//!
//! A rule comes from `config.toml` (`[[policy.rule]]`), from `policy.add` (the table
//! `policy_rules`, scope `config`), or from "Allow always" (the same table, scope `session`). A
//! session rule lasts until the core starts again. The matcher is pure ([`matcher`]). The
//! fingerprints of the answered approvals ([`fingerprint`]) feed `policy.suggest` ([`suggest`]).

use std::path::Path;
use std::sync::Arc;

use nh_proto::api::{
    calls::system::{PolicyDecision, PolicyRule, PolicyScope},
    common::Harness,
    time::Timestamp,
};
use nh_store::PolicyRuleRow;
use serde_json::Value;

use crate::config::Config;

pub(crate) mod calls;
pub mod fingerprint;
pub mod matcher;
pub mod suggest;

pub use matcher::{CommandPattern, Subject};

/// The most paths and the longest text that a rule can have.
const MAX_PATHS: usize = 64;
const MAX_TEXT: usize = 512;

/// A rule that the matcher can use.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rule {
    pub id: String,
    pub decision: PolicyDecision,
    pub harness: Vec<Harness>,
    pub tool: String,
    pub command: Option<CommandPattern>,
    pub paths: Vec<String>,
    pub workspace: Option<String>,
    pub reason: Option<String>,
    pub scope: PolicyScope,
    /// Where the rule came from, for the list: `config.toml`, `api` or `allow_always`.
    pub source: &'static str,
    pub created_at: Timestamp,
}

/// The id of a rule that has none: the hash of its text, so the same rule has the same id.
fn generated_id(spec: &PolicyRule) -> String {
    let text = serde_json::to_string(&PolicyRule {
        id: None,
        ..spec.clone()
    })
    .unwrap_or_default();
    let hash = blake3::hash(text.as_bytes()).to_hex();
    format!("r-{}", &hash.as_str()[..8])
}

/// Turns the text of a rule into a rule, or says what is wrong with it.
pub fn compile(
    spec: &PolicyRule,
    scope: PolicyScope,
    source: &'static str,
    created_at: Timestamp,
    home: Option<&Path>,
) -> Result<Rule, String> {
    let id = spec.id.clone().unwrap_or_else(|| generated_id(spec));
    let fail = |why: &str| Err(format!("the policy rule {id}: {why}"));
    if id.is_empty() || id.len() > 64 || !id.chars().all(|c| c.is_ascii_graphic()) {
        return fail("the id must have 1 to 64 characters without spaces");
    }
    let tool = spec.tool.trim().to_owned();
    if tool.is_empty() || tool.len() > MAX_TEXT {
        return fail("the tool is empty or too long");
    }
    if spec.command.is_some() && !spec.paths.is_empty() {
        return fail("a rule has a command or paths, not both");
    }
    let command = match &spec.command {
        Some(text) if text.len() <= MAX_TEXT => match CommandPattern::parse(text) {
            Ok(pattern) => Some(pattern),
            Err(error) => return fail(&error),
        },
        Some(_) => return fail("the command is too long"),
        None => None,
    };
    if spec.paths.len() > MAX_PATHS {
        return fail("too many paths");
    }
    for glob in &spec.paths {
        if glob.is_empty() || glob.len() > MAX_TEXT {
            return fail("a path glob is empty or too long");
        }
        if glob.starts_with('/') || glob.split('/').any(|part| part == "..") {
            return fail(&format!(
                "`{glob}`: a path glob is relative to the workspace and has no `..`"
            ));
        }
    }
    if spec.decision == PolicyDecision::Allow
        && tool == "*"
        && command.is_none()
        && spec.paths.is_empty()
    {
        return fail("an allow for every tool needs a command or paths");
    }
    let workspace = spec.workspace.as_deref().map(|text| expand(text, home));
    let spec = PolicyRule {
        id: Some(id.clone()),
        tool: tool.clone(),
        ..spec.clone()
    };
    Ok(Rule {
        id,
        decision: spec.decision,
        harness: spec.harness.clone(),
        tool,
        command,
        paths: spec.paths.clone(),
        workspace,
        reason: spec.reason.clone(),
        scope,
        source,
        created_at,
    })
}

/// `~` is the home of the user.
fn expand(text: &str, home: Option<&Path>) -> String {
    match (text.strip_prefix("~/"), home) {
        (Some(rest), Some(home)) => home.join(rest).display().to_string(),
        _ if text == "~" => home.map_or_else(|| text.to_owned(), |h| h.display().to_string()),
        _ => text.to_owned(),
    }
}

impl Rule {
    /// The rule as the API shows it.
    pub fn spec(&self) -> PolicyRule {
        PolicyRule {
            id: Some(self.id.clone()),
            decision: self.decision,
            harness: self.harness.clone(),
            tool: self.tool.clone(),
            command: self.command.as_ref().map(CommandPattern::text),
            paths: self.paths.clone(),
            workspace: self.workspace.clone(),
            reason: self.reason.clone(),
        }
    }
}

/// The keys of a rule in the file.
const RULE_KEYS: [&str; 8] = [
    "id",
    "decision",
    "harness",
    "tool",
    "command",
    "paths",
    "workspace",
    "reason",
];

/// The rules of `config.toml`, and what is wrong with the others.
pub fn compile_config(config: &Config, home: Option<&Path>) -> (Vec<Rule>, Vec<String>) {
    let mut rules = Vec::new();
    let mut problems = Vec::new();
    for value in &config.policy.rule {
        // A key that the rule does not know is a mistake in the file, and it can widen the rule:
        // `comand = "pnpm test:*"` would be an allow for every shell command.
        if let Some(object) = value.as_object()
            && let Some(unknown) = object.keys().find(|key| !RULE_KEYS.contains(&key.as_str()))
        {
            problems.push(format!(
                "config.toml: a [[policy.rule]] has the key `{unknown}`, which no rule has"
            ));
            continue;
        }
        let spec: Result<PolicyRule, _> = serde_json::from_value(value.clone());
        match spec {
            Ok(spec) => match compile(
                &spec,
                PolicyScope::Config,
                "config.toml",
                Timestamp(0),
                home,
            ) {
                Ok(rule) => rules.push(rule),
                Err(error) => problems.push(format!("config.toml: {error}")),
            },
            Err(error) => problems.push(format!(
                "config.toml: a [[policy.rule]] is not valid: {error}"
            )),
        }
    }
    (rules, problems)
}

/// The rule of a row of the table.
pub fn from_row(row: &PolicyRuleRow, home: Option<&Path>) -> Result<Rule, String> {
    let spec: PolicyRule = serde_json::from_value(row.rule.clone())
        .map_err(|e| format!("the policy rule {}: {e}", row.id))?;
    let spec = PolicyRule {
        id: Some(row.id.clone()),
        ..spec
    };
    let (scope, source) = match row.scope.as_str() {
        "session" => (PolicyScope::Session, "allow_always"),
        _ => (PolicyScope::Config, "api"),
    };
    compile(&spec, scope, source, Timestamp(row.created_at), home)
}

/// The rules that the last core left in the table. The session rules end with the core that made
/// them, so they go now. A row that this version cannot read is skipped.
pub fn load(store: &mut nh_store::Store) -> Policy {
    let now = crate::persist::now_ms();
    if let Err(error) = store.tx(now, |tx| tx.delete_session_policy_rules()) {
        tracing::warn!(%error, "the session rules of the policy are not removed");
    }
    let rows = store.policy_rules().unwrap_or_else(|error| {
        tracing::warn!(%error, "the policy rules are not read");
        Vec::new()
    });
    let home = calls::home();
    let rules = rows
        .iter()
        .filter_map(|row| match from_row(row, home.as_deref()) {
            Ok(rule) => Some(rule),
            Err(error) => {
                tracing::warn!(%error, "a policy rule of the table is skipped");
                None
            }
        })
        .collect();
    Policy::new(rules)
}

/// The session rule for "Allow always": the pattern of the approval, for this harness
/// and this workspace. An approval that no rule may cover gives none.
pub fn session_rule(facts: &fingerprint::Facts, at: Timestamp) -> Option<Rule> {
    let pattern = facts.pattern.as_deref().filter(|_| facts.eligible)?;
    let spec = PolicyRule {
        id: Some(format!("session-{}", &facts.fingerprint[..8])),
        ..suggest::rule_for(
            facts.harness,
            &facts.tool,
            pattern,
            Some(&facts.workspace),
            Some("Allow always".to_owned()),
        )
    };
    compile(&spec, PolicyScope::Session, "allow_always", at, None).ok()
}

/// The row of a rule, for the table.
pub fn to_row(rule: &Rule) -> PolicyRuleRow {
    PolicyRuleRow {
        id: rule.id.clone(),
        scope: match rule.scope {
            PolicyScope::Session => "session",
            PolicyScope::Config => "config",
        }
        .to_owned(),
        rule: serde_json::to_value(rule.spec()).unwrap_or(Value::Null),
        source: rule.source.to_owned(),
        created_at: rule.created_at.0,
    }
}

/// The rules in force: those of the file, and those of the table. The core keeps the table in
/// memory, and the rules of the file follow the configuration.
#[derive(Clone, Default)]
pub struct Policy {
    table: Vec<Rule>,
    file: Vec<Rule>,
    /// The configuration that `file` was made from.
    file_of: Option<Arc<Config>>,
    combined: Vec<Rule>,
    dirty: bool,
}

impl Policy {
    pub fn new(table: Vec<Rule>) -> Self {
        Self {
            table,
            dirty: true,
            ..Self::default()
        }
    }

    /// The rules of the table, for the list.
    pub fn table(&self) -> &[Rule] {
        &self.table
    }

    /// Adds a rule to the table, or replaces the one with its id.
    pub fn put(&mut self, rule: Rule) {
        self.table.retain(|r| r.id != rule.id);
        self.table.push(rule);
        self.dirty = true;
    }

    /// Removes a rule of the table. Says whether there was one.
    pub fn remove(&mut self, id: &str) -> bool {
        let before = self.table.len();
        self.table.retain(|r| r.id != id);
        self.dirty = true;
        self.table.len() != before
    }

    fn refresh(&mut self, config: &Arc<Config>, home: Option<&Path>) {
        let same = self
            .file_of
            .as_ref()
            .is_some_and(|known| Arc::ptr_eq(known, config));
        if !same {
            self.file = compile_config(config, home).0;
            self.file_of = Some(Arc::clone(config));
            self.dirty = true;
        }
        if self.dirty {
            self.combined = self.file.iter().chain(&self.table).cloned().collect();
            self.dirty = false;
        }
    }

    /// All the rules, the file first.
    pub fn rules(&mut self, config: &Arc<Config>, home: Option<&Path>) -> &[Rule] {
        self.refresh(config, home);
        &self.combined
    }
}

#[cfg(test)]
mod tests;
