//! The login state of an installed harness.
//!
//! The command `[auth] status` of the manifest runs in the login environment, with no stdin and a
//! limit of ten seconds. The rules `logged_in` and `logged_out` read its exit status, its output or
//! a field of its JSON. The result is `logged_in`, `logged_out` or `unknown`: `unknown` when the
//! program failed, timed out, or gave an output that no rule matches. The method of the login comes
//! from `method` when the probe gives it. The output itself is not kept, because it can have the
//! name of the account.

use std::{path::Path, time::Duration};

use nh_proto::api::model::{LoginMethod, LoginState};
use regex::Regex;
use serde_json::Value;

use crate::{
    manifest::schema::{Auth, AuthRule, MethodRule},
    version::{Output, find_exe, path_of, run_output},
};

/// How long the probe may run.
pub const TIMEOUT: Duration = Duration::from_secs(10);

/// The names that `[auth] method.values` can give.
pub const METHODS: &[&str] = &["subscription", "api_key", "token"];

/// What a probe found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Verdict {
    pub state: LoginState,
    /// Only for `logged_in`.
    pub method: Option<LoginMethod>,
}

impl Verdict {
    pub const UNKNOWN: Self = Self {
        state: LoginState::Unknown,
        method: None,
    };
}

/// The method that a name of `values` means.
pub fn method_named(name: &str) -> Option<LoginMethod> {
    match name {
        "subscription" => Some(LoginMethod::Subscription),
        "api_key" => Some(LoginMethod::ApiKey),
        "token" => Some(LoginMethod::Token),
        _ => None,
    }
}

/// The standard output as JSON, when it is JSON.
fn json_of(output: &Output) -> Option<Value> {
    serde_json::from_str(output.stdout.trim()).ok()
}

/// Both outputs, for a regular expression.
fn text_of(output: &Output) -> String {
    format!("{}{}", output.stdout, output.stderr)
}

/// Whether a rule matches. A rule without a key never matches: the check of the manifest refuses
/// it, and a rule that says nothing must not say `logged_in`.
pub fn matches(rule: &AuthRule, output: &Output, json: Option<&Value>) -> bool {
    if rule.exit.is_none() && rule.regex.is_none() && rule.json.is_none() {
        return false;
    }
    if let Some(exit) = rule.exit
        && output.exit != Some(exit)
    {
        return false;
    }
    if let Some(regex) = &rule.regex {
        let found = Regex::new(regex).is_ok_and(|regex| regex.is_match(&text_of(output)));
        if !found {
            return false;
        }
    }
    if let Some(pointer) = &rule.json {
        let Some(field) = json.and_then(|json| json.pointer(pointer)) else {
            return false;
        };
        let good = match &rule.equals {
            Some(wanted) => field == wanted,
            None => !matches!(field, Value::Null | Value::Bool(false)),
        };
        if !good {
            return false;
        }
    }
    true
}

/// The method of the login that the output gives.
pub fn method_of(rule: &MethodRule, output: &Output, json: Option<&Value>) -> Option<LoginMethod> {
    let text = if let Some(pointer) = &rule.json {
        json?.pointer(pointer)?.as_str()?.to_owned()
    } else if let Some(regex) = &rule.regex {
        let regex = Regex::new(regex).ok()?;
        let all = text_of(output);
        let captures = regex.captures(&all)?;
        captures
            .name("m")
            .or_else(|| captures.get(0))?
            .as_str()
            .to_owned()
    } else {
        return None;
    };
    rule.values.get(&text).and_then(|name| method_named(name))
}

/// The verdict of the rules on what the probe printed.
pub fn evaluate(auth: &Auth, output: &Output) -> Verdict {
    let json = json_of(output);
    let json = json.as_ref();
    if auth
        .logged_in
        .as_ref()
        .is_some_and(|rule| matches(rule, output, json))
    {
        return Verdict {
            state: LoginState::LoggedIn,
            method: auth
                .method
                .as_ref()
                .and_then(|rule| method_of(rule, output, json)),
        };
    }
    if auth
        .logged_out
        .as_ref()
        .is_some_and(|rule| matches(rule, output, json))
    {
        return Verdict {
            state: LoginState::LoggedOut,
            method: None,
        };
    }
    Verdict::UNKNOWN
}

/// Runs `[auth] status` and reads its result. `env` is the login environment: the program
/// is looked for in its `PATH`, and it runs with it. A probe that cannot run, or that runs longer
/// than `limit`, is `unknown`.
pub fn probe_with_limit(auth: &Auth, env: &[(String, String)], limit: Duration) -> Verdict {
    let Some(program) = auth.status.first() else {
        return Verdict::UNKNOWN;
    };
    let Some(exe) = find_exe(program, &path_of(env)) else {
        return Verdict::UNKNOWN;
    };
    probe_exe(auth, &exe, env, limit)
}

/// [`probe_with_limit`] with a program that the caller found already.
pub fn probe_exe(auth: &Auth, exe: &Path, env: &[(String, String)], limit: Duration) -> Verdict {
    run_output(exe, auth.status.get(1..).unwrap_or_default(), env, limit)
        .map_or(Verdict::UNKNOWN, |output| evaluate(auth, &output))
}

/// [`probe_with_limit`] with the limit of ten seconds.
pub fn probe(auth: &Auth, env: &[(String, String)]) -> Verdict {
    probe_with_limit(auth, env, TIMEOUT)
}

#[cfg(test)]
mod tests;
