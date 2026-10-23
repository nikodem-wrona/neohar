//! The text of a result for a person. A result for a program is JSON (`--json`, or stdout that is
//! no terminal), and it is the result of the method as it came.

use std::io;

use serde_json::Value;

use super::output::Output;

/// The widest a cell of a table gets, in characters.
const CELL_MAX: usize = 48;

/// Prints the result of a verb.
pub fn result(output: &mut Output<'_>, value: &Value) -> io::Result<()> {
    if output.json {
        return output.value(value);
    }
    human(output, value)
}

fn cell(value: &Value) -> String {
    let text = match value {
        Value::Null => "-".to_owned(),
        Value::String(text) => text.clone(),
        other => other.to_string(),
    };
    if text.chars().count() > CELL_MAX {
        let cut: String = text.chars().take(CELL_MAX - 1).collect();
        format!("{cut}…")
    } else {
        text
    }
}

fn is_scalar(value: &Value) -> bool {
    !matches!(value, Value::Object(_) | Value::Array(_))
}

fn human(output: &mut Output<'_>, value: &Value) -> io::Result<()> {
    let Value::Object(map) = value else {
        return output.value(value);
    };
    if map.is_empty() {
        return writeln!(output.out, "ok");
    }
    // The text of a pane is the point of its command.
    if let Some(Value::String(text)) = map.get("text") {
        write!(output.out, "{text}")?;
        if !text.ends_with('\n') {
            writeln!(output.out)?;
        }
        return Ok(());
    }
    // One list of objects is a table.
    let lists: Vec<(&String, &Vec<Value>)> = map
        .iter()
        .filter_map(|(key, value)| value.as_array().map(|list| (key, list)))
        .filter(|(_, list)| list.iter().all(Value::is_object))
        .collect();
    if let [(_, rows)] = lists[..] {
        if rows.is_empty() {
            return writeln!(output.out, "nothing");
        }
        let columns: Vec<String> = rows[0]
            .as_object()
            .map(|first| {
                first
                    .iter()
                    .filter(|(_, v)| is_scalar(v))
                    .map(|(k, _)| k.clone())
                    .take(8)
                    .collect()
            })
            .unwrap_or_default();
        let headers: Vec<&str> = columns.iter().map(String::as_str).collect();
        let body: Vec<Vec<String>> = rows
            .iter()
            .map(|row| columns.iter().map(|c| cell(&row[c])).collect())
            .collect();
        return output.table(&headers, &body);
    }
    let pairs: Vec<(&str, String)> = map.iter().map(|(k, v)| (k.as_str(), cell(v))).collect();
    output.pairs(&pairs)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn text_of(value: &Value) -> String {
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let mut output = Output {
            out: &mut out,
            err: &mut err,
            json: false,
            pretty: true,
        };
        human(&mut output, value).expect("written");
        String::from_utf8(out).expect("text")
    }

    #[test]
    fn an_empty_result_says_ok() {
        assert_eq!(text_of(&json!({})), "ok\n");
    }

    #[test]
    fn a_list_of_objects_is_a_table() {
        let text = text_of(&json!({"workspaces": [
            {"handle": "w1", "name": "Work", "tabs": 2, "extra": {"a": 1}},
            {"handle": "w2", "name": "Play", "tabs": 1, "extra": {"a": 2}},
        ]}));
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(
            lines[0].split_whitespace().collect::<Vec<_>>(),
            ["handle", "name", "tabs"]
        );
        assert!(lines[1].starts_with("w1"));
        assert_eq!(lines.len(), 3);
        assert_eq!(text_of(&json!({"agents": []})), "nothing\n");
    }

    #[test]
    fn the_text_of_a_pane_prints_as_it_is() {
        assert_eq!(
            text_of(&json!({"text": "one\ntwo", "from_line": 0})),
            "one\ntwo\n"
        );
    }

    #[test]
    fn other_results_are_pairs() {
        let text = text_of(&json!({"job": "01ABC", "nested": {"a": 1}}));
        assert!(text.contains("job     01ABC"), "{text}");
        assert!(text.contains("nested  {\"a\":1}"), "{text}");
    }

    #[test]
    fn a_long_cell_is_cut() {
        let long = "x".repeat(100);
        assert!(cell(&json!(long)).chars().count() <= CELL_MAX);
    }
}
