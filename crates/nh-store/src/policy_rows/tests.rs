use serde_json::json;

use super::*;

fn rule(id: &str, scope: &str, at: i64) -> PolicyRuleRow {
    PolicyRuleRow {
        id: id.to_owned(),
        scope: scope.to_owned(),
        rule: json!({"decision": "allow", "tool": "Bash", "command": "pnpm test:*"}),
        source: "api".to_owned(),
        created_at: at,
    }
}

fn fingerprint(hash: &str, decision: &str, at: i64) -> FingerprintRow {
    FingerprintRow {
        fingerprint: hash.to_owned(),
        decision: decision.to_owned(),
        // One interaction has one row, so each fingerprint of the tests has its own interaction.
        interaction: Some(Ulid::from(u128::from(at.unsigned_abs()))),
        at,
        harness: Some("claude".to_owned()),
        tool: Some("Bash".to_owned()),
        pattern: Some("pnpm test:*".to_owned()),
        workspace: Some("/work".to_owned()),
        eligible: true,
    }
}

#[test]
fn rules_come_back_in_the_order_of_their_creation() {
    let mut store = Store::open_in_memory().expect("a store");
    store
        .tx(10, |tx| {
            tx.put_policy_rule(&rule("b", "config", 20))?;
            tx.put_policy_rule(&rule("a", "session", 10))
        })
        .expect("rules");
    let ids: Vec<String> = store
        .policy_rules()
        .expect("a read")
        .into_iter()
        .map(|r| r.id)
        .collect();
    assert_eq!(ids, ["a", "b"]);
}

#[test]
fn a_rule_is_removed_by_its_id_and_the_session_rules_go_together() {
    let mut store = Store::open_in_memory().expect("a store");
    store
        .tx(10, |tx| {
            tx.put_policy_rule(&rule("kept", "config", 1))?;
            tx.put_policy_rule(&rule("s1", "session", 2))?;
            tx.put_policy_rule(&rule("s2", "session", 3))
        })
        .expect("rules");
    let removed = store
        .tx(11, |tx| tx.delete_policy_rule("kept"))
        .expect("a delete")
        .0;
    assert!(removed);
    let again = store
        .tx(12, |tx| tx.delete_policy_rule("kept"))
        .expect("a delete")
        .0;
    assert!(!again, "there was no such rule");
    let mut ids = store
        .tx(13, |tx| tx.delete_session_policy_rules())
        .expect("a delete")
        .0;
    ids.sort();
    assert_eq!(ids, ["s1", "s2"]);
    assert!(store.policy_rules().expect("a read").is_empty());
}

#[test]
fn the_fingerprints_since_a_time_come_back_with_their_details() {
    let mut store = Store::open_in_memory().expect("a store");
    store
        .tx(10, |tx| {
            tx.put_fingerprint(&fingerprint("old", "allow", 100))?;
            tx.put_fingerprint(&fingerprint("new", "deny", 900))
        })
        .expect("rows");
    let found = store.fingerprints_since(500).expect("a read");
    assert_eq!(found, [fingerprint("new", "deny", 900)]);
    assert_eq!(store.fingerprints_since(0).expect("a read").len(), 2);
}
