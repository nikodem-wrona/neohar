use semver::Version;

use super::*;

const MINIMAL: &str = r#"
schema = 1
id = "t"
harness = "claude"
versions = ">=1.0.0, <2.0.0"

[[rules]]
id = "working"
state = "working"
any = ['esc to interrupt']
confidence = 0.8
"#;

fn parse(text: &str) -> Result<ScreenManifest, ScreenError> {
    ScreenManifest::parse(text)
}

fn why(text: &str) -> String {
    parse(text).expect_err("refused").to_string()
}

#[test]
fn a_small_manifest_compiles_and_knows_its_versions() {
    let manifest = parse(MINIMAL).expect("a manifest");
    assert_eq!(manifest.id, "t");
    assert_eq!(manifest.rules.len(), 1);
    assert!(manifest.applies_to(&Version::new(1, 5, 0)));
    assert!(!manifest.applies_to(&Version::new(2, 0, 0)));
    assert!(manifest.rule("working").is_some());
    assert!(manifest.rule("other").is_none());
}

#[test]
fn the_built_in_manifests_compile() {
    for id in ["claude", "codex"] {
        let manifest = builtin(id).unwrap_or_else(|| panic!("{id} does not compile"));
        assert_eq!(manifest.id, id);
        assert!(
            manifest.rule("input_box").is_some(),
            "{id} names the ready rule"
        );
        assert!(manifest.rule("approval").is_some(), "{id}");
    }
    assert!(builtin("other").is_none());
}

#[test]
fn a_manifest_that_is_not_valid_says_where() {
    let with_rule = |change: &dyn Fn(&str) -> String| change(MINIMAL);
    let cases: Vec<(String, &str)> = vec![
        (
            "schema = 2\nid = \"t\"\nharness = \"x\"\nversions = \">=1\"".to_owned(),
            "schema 2",
        ),
        (
            with_rule(&|m| m.replace(">=1.0.0, <2.0.0", "nope")),
            "versions",
        ),
        (
            with_rule(&|m| m.replace("any = ['esc to interrupt']", "any = ['(']")),
            "not valid",
        ),
        (
            with_rule(&|m| m.replace("state = \"working\"", "state = \"sleeping\"")),
            "state `sleeping`",
        ),
        (
            with_rule(&|m| m.replace("state = \"working\"\n", "")),
            "`state` or `opens`",
        ),
        (
            with_rule(&|m| m.replace("confidence = 0.8", "confidence = 3")),
            "confidence",
        ),
        (
            with_rule(&|m| m.replace("any = ['esc to interrupt']\n", "")),
            "needs",
        ),
        (
            format!(
                "{MINIMAL}\n[[rules]]\nid = \"working\"\nstate = \"idle\"\nany = ['x']\nconfidence = 0.5\n"
            ),
            "used twice",
        ),
        (
            with_rule(&|m| {
                m.replace(
                    "[[rules]]",
                    "[defaults]\nnormalize = [\"shout\"]\n\n[[rules]]",
                )
            }),
            "normalize step",
        ),
        (
            with_rule(&|m| {
                m.replace(
                    "[[rules]]",
                    "[defaults]\nregion = { rows = \"a..b\" }\n\n[[rules]]",
                )
            }),
            "number",
        ),
        (
            with_rule(&|m| m.replace("confidence", "mood = 1\nconfidence")),
            "mood",
        ),
    ];
    for (text, expected) in cases {
        let message = why(&text);
        assert!(
            message.contains(expected),
            "{message} should say {expected}"
        );
    }
}

#[test]
fn a_dialog_needs_a_rule_that_opens_and_a_label_group() {
    let dialog = |extra: &str| format!("{MINIMAL}\n[[rules]]\nid = \"ask\"\n{extra}");
    let no_label = dialog(
        "opens = \"approval\"\nall = ['x']\nconfidence = 0.9\n[rules.dialog]\noptions_regex = '(?P<n>\\d)'\n",
    );
    assert!(why(&no_label).contains("label"));
    let wrong = dialog(
        "state = \"idle\"\nall = ['x']\nconfidence = 0.9\n[rules.dialog]\noptions_regex = '(?P<label>.+)'\n",
    );
    assert!(why(&wrong).contains("belongs to a rule that opens"));
    let kind = dialog(
        "opens = \"approval\"\nall = ['x']\nconfidence = 0.9\n[rules.interaction]\nkind = \"question\"\n",
    );
    assert!(why(&kind).contains("not the kind"));
}
