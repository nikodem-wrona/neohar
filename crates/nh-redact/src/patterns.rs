//! The patterns. Each has a kind, which names the mark that replaces the secret.

use std::{borrow::Cow, sync::LazyLock};

use regex::{Captures, Regex};

/// The mark for a secret of a kind.
fn mark(kind: &str) -> String {
    format!("[redacted:{kind}]")
}

fn re(pattern: &str) -> Regex {
    Regex::new(pattern).unwrap_or_else(|e| panic!("the pattern {pattern:?} is bad: {e}"))
}

/// A secret that is the whole match.
struct Whole {
    kind: &'static str,
    regex: Regex,
    /// A check on the match. A match that fails it stays as it is.
    accept: fn(&str) -> bool,
}

fn always(_: &str) -> bool {
    true
}

/// A word that looks like a key of OpenAI needs a digit; a long hyphenated word does not.
fn has_digit(text: &str) -> bool {
    text.bytes().any(|b| b.is_ascii_digit())
}

static WHOLE: LazyLock<Vec<Whole>> = LazyLock::new(|| {
    let whole = |kind, pattern: &str, accept| Whole {
        kind,
        regex: re(pattern),
        accept,
    };
    vec![
        // A key block, whole or cut short by a log line.
        whole(
            "private_key",
            r"(?s)-----BEGIN [A-Z0-9 ]*PRIVATE KEY(?: BLOCK)?-----.*?-----END [A-Z0-9 ]*PRIVATE KEY(?: BLOCK)?-----",
            always,
        ),
        whole(
            "private_key",
            r"-----BEGIN [A-Z0-9 ]*PRIVATE KEY(?: BLOCK)?-----[A-Za-z0-9+/=\s]{40,}",
            always,
        ),
        whole(
            "jwt",
            r"\beyJ[A-Za-z0-9_\-]{8,}\.eyJ[A-Za-z0-9_\-]{8,}\.[A-Za-z0-9_\-]{8,}",
            always,
        ),
        whole("anthropic_key", r"\bsk-ant-[A-Za-z0-9_\-]{20,}", always),
        whole(
            "openai_key",
            r"\bsk-(?:proj-|svcacct-|admin-)?[A-Za-z0-9_\-]{32,}",
            has_digit,
        ),
        whole(
            "github_token",
            r"\b(?:gh[pousr]_[A-Za-z0-9]{36,}|github_pat_[A-Za-z0-9_]{22,})",
            always,
        ),
        whole("slack_token", r"\bxox[abprs]-[A-Za-z0-9\-]{10,}", always),
        whole(
            "aws_key",
            r"\b(?:AKIA|ASIA|AGPA|AIDA|AROA|ANPA|ANVA|AIPA)[0-9A-Z]{16}\b",
            always,
        ),
        whole("google_key", r"\bAIza[0-9A-Za-z_\-]{35}\b", always),
        // The cookie of a paired phone and the code of a pairing.
        whole("phone_token", r"\bnh_device=[0-9A-Fa-f]{16,}", always),
        whole("pairing_code", r"[?&]code=[A-Za-z0-9_\-]{8,}", always),
    ]
});

/// `scheme://user:password@host`: the user and the password go.
static URL_CREDENTIALS: LazyLock<Regex> =
    LazyLock::new(|| re(r#"\b([a-z][a-z0-9+.\-]{1,20}://)[^\s/:@'"]+:[^\s/@'"]+@"#));

/// `Bearer <token>`, where the token is long enough to be one.
static BEARER: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)\b(bearer)\s+[A-Za-z0-9._~+/=\-]{16,}"));

/// The names of the keys that hold a secret, in text and in JSON.
const SECRET_WORDS: &str = r"password|passwd|secret|token|api[_\-]?key|apikey|access[_\-]?key|private[_\-]?key|credential|authorization|cookie";

/// `name = value`, `name: value` and `"name": "value"`, where the name has a word of
/// [`SECRET_WORDS`]. A quoted value must have 8 characters. A value without quotes must have a
/// digit and 8 characters, so that `max_tokens = 4096` and `token = next_token` stay.
static KEY_VALUE: LazyLock<Regex> = LazyLock::new(|| {
    re(&format!(
        r#"(?i)(["']?)\b([A-Za-z0-9_.\-]*(?:{SECRET_WORDS})[A-Za-z0-9_.\-]*)(["']?\s*[=:]\s*)(?:"([^"\n]{{8,}})"|'([^'\n]{{8,}})'|([^\s"',;&)(<>{{}}\[\]]{{8,}}))"#
    ))
});

static SECRET_KEY: LazyLock<Regex> = LazyLock::new(|| re(&format!(r"(?i)(?:{SECRET_WORDS})")));

/// Whether the name of a JSON key says that its value is a secret.
pub(crate) fn secret_key(key: &str) -> bool {
    SECRET_KEY.is_match(key)
}

fn replace_key_value<'a>(text: Cow<'a, str>) -> Cow<'a, str> {
    if !KEY_VALUE.is_match(&text) {
        return text;
    }
    let replaced = KEY_VALUE.replace_all(&text, |caps: &Captures<'_>| {
        // An unquoted value without a digit is a name in code, such as `token = next_token`.
        if let Some(bare) = caps.get(6)
            && !has_digit(bare.as_str())
        {
            return caps[0].to_owned();
        }
        // A value that is a mark already stays.
        let value = caps
            .get(4)
            .or_else(|| caps.get(5))
            .or_else(|| caps.get(6))
            .map_or("", |m| m.as_str());
        if value.starts_with("[redacted") {
            return caps[0].to_owned();
        }
        let quote = if caps.get(4).is_some() {
            "\""
        } else if caps.get(5).is_some() {
            "'"
        } else {
            ""
        };
        format!(
            "{}{}{}{quote}{}{quote}",
            &caps[1],
            &caps[2],
            &caps[3],
            mark("value")
        )
    });
    Cow::Owned(replaced.into_owned())
}

/// Applies every pattern, the specific ones first.
pub(crate) fn apply(text: &str) -> Cow<'_, str> {
    let mut current: Cow<'_, str> = Cow::Borrowed(text);
    for rule in WHOLE.iter() {
        if rule.regex.is_match(&current) {
            let replaced = rule.regex.replace_all(&current, |caps: &Captures<'_>| {
                if (rule.accept)(&caps[0]) {
                    mark(rule.kind)
                } else {
                    caps[0].to_owned()
                }
            });
            if let Cow::Owned(done) = replaced {
                current = Cow::Owned(done);
            }
        }
    }
    if URL_CREDENTIALS.is_match(&current) {
        let replaced = URL_CREDENTIALS
            .replace_all(&current, |caps: &Captures<'_>| {
                format!("{}{}@", &caps[1], mark("url_credentials"))
            })
            .into_owned();
        current = Cow::Owned(replaced);
    }
    if BEARER.is_match(&current) {
        let replaced = BEARER
            .replace_all(&current, |caps: &Captures<'_>| {
                format!("{} {}", &caps[1], mark("bearer"))
            })
            .into_owned();
        current = Cow::Owned(replaced);
    }
    replace_key_value(current)
}
