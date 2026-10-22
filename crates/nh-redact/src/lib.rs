//! Secret redaction for logs, events and debug bundles.
//!
//! The functions find the secrets that tools usually leak: the keys of cloud services and Git
//! hosts, the tokens of the harnesses, private keys, JSON web tokens, `Bearer` headers, URLs with
//! a password, and the pairs `password = value`. A secret becomes `[redacted:<kind>]`, and the
//! text around it stays. The patterns would rather miss a secret that has no mark than damage
//! ordinary code, so each one looks for a mark: a prefix, a shape or a name.
//!
//! - [`redact`] works on text.
//! - [`redact_value`] works on a JSON value, and also masks the value of a key that is named like
//!   a secret.
//! - [`RedactWriter`] puts the redaction on the output of a logger. A host that has the logger
//!   wraps its writers with it.
//!
//! The environment of a process is never written to a log: [`mask_env`] is for the code that
//! must show a variable at all.

mod patterns;
mod writer;

use std::borrow::Cow;

use serde_json::Value;

pub use writer::RedactWriter;

/// Replaces every secret in `text`. The text comes back as it is when it has none.
pub fn redact(text: &str) -> Cow<'_, str> {
    patterns::apply(text)
}

/// Replaces the secrets in the strings of a JSON value, and the value of each key that is named
/// like a secret (`password`, `token`, `authorization`, …). Returns whether anything changed.
pub fn redact_value(value: &mut Value) -> bool {
    match value {
        Value::String(text) => match redact(text) {
            Cow::Borrowed(_) => false,
            Cow::Owned(clean) => {
                *text = clean;
                true
            }
        },
        Value::Array(items) => items
            .iter_mut()
            .fold(false, |changed, item| redact_value(item) | changed),
        Value::Object(map) => {
            let mut changed = false;
            for (key, item) in map.iter_mut() {
                if patterns::secret_key(key) && item.is_string() {
                    let text = item.as_str().unwrap_or_default();
                    if !text.is_empty() && !text.starts_with("[redacted") {
                        *item = Value::String("[redacted:key]".to_owned());
                        changed = true;
                        continue;
                    }
                }
                changed |= redact_value(item);
            }
            changed
        }
        _ => false,
    }
}

/// The text of an environment variable for a place that must name it: the name stays, the value
/// does not. Use it where a program would print `NAME=value`.
pub fn mask_env(name: &str) -> String {
    format!("{name}=[redacted:env]")
}

#[cfg(test)]
mod tests;
