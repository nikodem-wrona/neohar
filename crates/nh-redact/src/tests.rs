use std::io::Write;

use serde_json::json;

use super::*;

/// A secret is built at run time, so that this file holds no string that a scanner of secrets
/// takes for a real key.
fn fake(prefix: &str, tail_char: char, tail_len: usize) -> String {
    let mut out = prefix.to_owned();
    // A mix of letters and digits, so that the check for a digit holds.
    for i in 0..tail_len {
        out.push(if i % 7 == 3 { '7' } else { tail_char });
    }
    out
}

#[track_caller]
fn hides(secret: &str, kind: &str) {
    let text = format!("before {secret} after");
    let clean = redact(&text);
    assert!(!clean.contains(secret), "{kind}: still there in {clean}");
    assert!(
        clean.contains(&format!("[redacted:{kind}]")),
        "{kind}: no mark in {clean}"
    );
    assert!(
        clean.starts_with("before ") && clean.ends_with(" after"),
        "{clean}"
    );
}

#[test]
fn the_cookie_of_a_phone_and_the_code_of_a_pairing_are_hidden() {
    hides(&format!("nh_device={}", "a1".repeat(32)), "phone_token");
    hides(&format!("/pair?code={}", fake("", 'x', 43)), "pairing_code");
    let clean = redact("exit code=1, status code=200");
    assert_eq!(
        clean, "exit code=1, status code=200",
        "a code without ? or & stays"
    );
}

#[test]
fn every_kind_of_key_is_hidden() {
    hides("AKIAIOSFODNN7EXAMPLE", "aws_key");
    hides(&fake("ghp_", 'a', 36), "github_token");
    hides(&fake("gho_", 'B', 40), "github_token");
    hides(&fake("ghs_", 'c', 36), "github_token");
    hides(&fake("github_pat_", 'd', 40), "github_token");
    hides(&fake("sk-ant-api03-", 'e', 40), "anthropic_key");
    hides(&fake("sk-", 'f', 48), "openai_key");
    hides(&fake("sk-proj-", 'g', 60), "openai_key");
    hides(&fake("xoxb-", '1', 30), "slack_token");
    hides(&fake("AIza", 'h', 35), "google_key");
}

#[test]
fn a_jwt_a_bearer_header_and_a_url_with_a_password_are_hidden() {
    let jwt = format!(
        "eyJ{}.eyJ{}.{}",
        "a".repeat(20),
        "b".repeat(20),
        "c".repeat(30)
    );
    hides(&jwt, "jwt");
    let header = redact("Authorization: Bearer abcdefghijklmnop0123456789");
    assert!(!header.contains("abcdefghijklmnop"), "{header}");
    let url = redact("clone https://ada:hunter2pass@example.com/repo.git now");
    assert_eq!(
        url,
        "clone https://[redacted:url_credentials]@example.com/repo.git now"
    );
}

#[test]
fn a_private_key_block_is_hidden_whole_or_cut_short() {
    let body = "MIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQC7".repeat(3);
    let block = format!("-----BEGIN PRIVATE KEY-----\n{body}\n-----END PRIVATE KEY-----");
    let text = format!("key:\n{block}\nend");
    assert_eq!(redact(&text), "key:\n[redacted:private_key]\nend");
    let cut = format!("-----BEGIN RSA PRIVATE KEY-----\n{body}");
    assert_eq!(redact(&cut), "[redacted:private_key]");
}

#[test]
fn a_pair_with_a_secret_name_loses_its_value_and_keeps_its_name() {
    assert_eq!(
        redact("export OPENAI_API_KEY=abc123def456ghi"),
        "export OPENAI_API_KEY=[redacted:value]"
    );
    assert_eq!(
        redact(r#"{"password": "correct horse battery"}"#),
        r#"{"password": "[redacted:value]"}"#
    );
    assert_eq!(
        redact("db_password = 'p4ssw0rd-long'"),
        "db_password = '[redacted:value]'"
    );
    assert_eq!(
        redact("GITHUB_TOKEN: ghx1234567890abcdef"),
        "GITHUB_TOKEN: [redacted:value]"
    );
}

#[test]
fn common_code_and_prose_stay_as_they_are() {
    for text in [
        "let token = lexer.next_token();",
        "max_tokens = 4096",
        "token = next_token",
        "const secretary = hire(\"Ada\");",
        "the author of this authority file",
        "pip install scikit-learn sk-learn-compatible",
        "disk-usage: 91%  task-runner: ok",
        "git clone git@github.com:acme/widgets.git",
        "ssh://git@github.com/acme/widgets",
        "http://localhost:8080/path?x=1@2",
        "password: required",
        "passwordless login is on",
        "Bearer token auth is described in the docs",
        "let basic = 1; // basic configuration of the internationalization layer",
        "cookie: yes",
        "SELECT token_count FROM usage WHERE day = '2026-10-10'",
        "ghp_ is the prefix of a personal access token",
        "see AKIA keys in the IAM console",
        "eyJ is the start of a JSON web token",
        "-----BEGIN CERTIFICATE-----\nMIIB\n-----END CERTIFICATE-----",
        "a -> b, c: d, e = f",
        "fn auth_header(&self) -> String { format!(\"{}\", self.name) }",
    ] {
        assert_eq!(redact(text), text, "changed: {text}");
    }
}

#[test]
fn a_clean_text_is_not_copied() {
    assert!(matches!(redact("nothing here"), Cow::Borrowed(_)));
}

#[test]
fn a_json_value_loses_secrets_in_strings_and_under_secret_keys() {
    let key = fake("ghp_", 'a', 36);
    let mut value = json!({
        "command": format!("curl -H 'Authorization: Bearer abcdefghijklmnop0123' {key}"),
        "password": "hunter2hunter2",
        "usage": {"input_tokens": 12, "max_tokens": 4096},
        "list": ["fine", format!("token {key}")],
        "empty": {"secret": ""},
    });
    assert!(redact_value(&mut value));
    let text = value.to_string();
    assert!(
        !text.contains(&key) && !text.contains("hunter2") && !text.contains("abcdefghijklmnop"),
        "{text}"
    );
    assert_eq!(
        value["usage"],
        json!({"input_tokens": 12, "max_tokens": 4096})
    );
    assert_eq!(value["password"], "[redacted:key]");
    assert_eq!(value["empty"]["secret"], "");
    assert!(!redact_value(&mut value), "a second pass changes nothing");
}

#[test]
fn the_writer_redacts_lines_that_arrive_in_pieces() {
    let key = fake("sk-ant-api03-", 'z', 40);
    let mut out = Vec::new();
    {
        let mut writer = RedactWriter::new(&mut out);
        let line = format!("{{\"msg\":\"key {key}\"}}\n");
        let (a, b) = line.split_at(14);
        writer.write_all(a.as_bytes()).expect("a piece");
        writer.write_all(b.as_bytes()).expect("a piece");
        writer
            .write_all(b"last line without an end")
            .expect("a piece");
    }
    let text = String::from_utf8(out).expect("text");
    assert!(!text.contains(&key), "{text}");
    assert!(text.contains("[redacted:anthropic_key]"));
    assert!(text.ends_with("last line without an end"));
}

#[test]
fn an_environment_variable_shows_its_name_only() {
    assert_eq!(mask_env("HOME"), "HOME=[redacted:env]");
}
