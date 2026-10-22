use serde_json::json;

use super::*;

fn parse(params: Option<&str>, stdin: &str) -> Result<Value, CliError> {
    parse_params(params, &mut stdin.as_bytes())
}

#[test]
fn no_params_are_an_empty_object() {
    assert_eq!(parse(None, "").expect("params"), json!({}));
}

#[test]
fn params_come_from_the_argument_or_from_stdin() {
    assert_eq!(
        parse(Some(r#"{"pane": "w1:p2", "lines": 5}"#), "").expect("params"),
        json!({"pane": "w1:p2", "lines": 5})
    );
    assert_eq!(
        parse(Some("-"), "{\"a\": [1, 2]}\n").expect("params"),
        json!({"a": [1, 2]})
    );
}

#[test]
fn params_that_are_not_json_or_not_structured_are_a_usage_error() {
    for bad in ["{not json", "", "42", "\"text\"", "true"] {
        let error = parse(Some(bad), "").expect_err(bad);
        assert!(matches!(error, CliError::Usage(_)), "{bad}: {error:?}");
        assert_eq!(error.exit_code(), 2);
    }
    assert!(matches!(parse(Some("-"), "nope"), Err(CliError::Usage(_))));
    // `null` and an array go to the core, which decides.
    assert!(parse(Some("null"), "").is_ok());
    assert!(parse(Some("[1]"), "").is_ok());
}
