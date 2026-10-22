//! The text of a result for a person. A result for a program is JSON (`--json`, or stdout that is
//! no terminal), and it is the result of the method as it came.

use std::io;

use serde_json::Value;

use super::{output::Output, verbs::Verb};

/// The widest a cell of a table gets, in characters.
const CELL_MAX: usize = 48;

/// Prints the result of a verb.
pub fn result(output: &mut Output<'_>, verb: &Verb, value: &Value) -> io::Result<()> {
    if output.json {
        return output.value(value);
    }
    match verb.method {
        "task.changes" => changes(output, value),
        "task.diff" => diff(output, value),
        _ => human(output, value),
    }
}

/// The letter of the status of a changed file.
fn status_letter(status: &Value) -> &'static str {
    match status.as_str() {
        Some("added") => "A",
        Some("deleted") => "D",
        Some("renamed") => "R",
        Some("untracked") => "U",
        _ => "M",
    }
}

/// `task.changes`: a row for each file, then the totals. `*` marks a change that is not in a
/// commit.
fn changes(output: &mut Output<'_>, value: &Value) -> io::Result<()> {
    let totals = &value["changes"];
    let files = value["files"].as_array().map_or(&[][..], Vec::as_slice);
    if files.is_empty() {
        writeln!(output.out, "no changes")?;
    } else {
        let body: Vec<Vec<String>> = files
            .iter()
            .map(|file| {
                let path = file["path"].as_str().unwrap_or_default();
                let path = match file["old_path"].as_str() {
                    Some(old) => format!("{path} <- {old}"),
                    None => path.to_owned(),
                };
                let lines = |key: &str| {
                    if file["binary"] == true {
                        "-".to_owned()
                    } else {
                        file[key].to_string()
                    }
                };
                vec![
                    status_letter(&file["status"]).to_owned(),
                    path,
                    lines("add"),
                    lines("del"),
                    if file["uncommitted"] == true { "*" } else { "" }.to_owned(),
                ]
            })
            .collect();
        output.table(&["status", "path", "+", "-", ""], &body)?;
    }
    let mut line = format!(
        "{} files, {} commits",
        totals["files"].as_u64().unwrap_or(0),
        totals["commits"].as_u64().unwrap_or(0)
    );
    if let Some(base) = totals["merge_base"].as_str() {
        line.push_str(&format!(", merge base {base}"));
    }
    if totals["truncated"] == true {
        line.push_str(&format!(", the first {} shown", files.len()));
    }
    writeln!(output.out, "{line}")?;
    if let Some(error) = totals["error"].as_str() {
        writeln!(output.out, "the last count failed: {error}")?;
    }
    Ok(())
}

/// `task.diff`: the lines of the diff with `+`, `-` or a space in front, under the header of each
/// hunk.
fn diff(output: &mut Output<'_>, value: &Value) -> io::Result<()> {
    let hunks = value["hunks"].as_array().map_or(&[][..], Vec::as_slice);
    if hunks.is_empty() {
        return writeln!(output.out, "no lines to show");
    }
    for hunk in hunks {
        writeln!(output.out, "{}", hunk["header"].as_str().unwrap_or("@@"))?;
        for line in hunk["lines"].as_array().map_or(&[][..], Vec::as_slice) {
            let mark = match line["kind"].as_str() {
                Some("add") => '+',
                Some("del") => '-',
                _ => ' ',
            };
            let text = line["text"]["text"].as_str().unwrap_or_default();
            writeln!(output.out, "{mark}{text}")?;
        }
    }
    if value["truncated"] == true {
        writeln!(output.out, "(the diff is cut)")?;
    }
    Ok(())
}

/// A value as a cell: text as it is, other values as compact JSON, and a cut at [`CELL_MAX`].
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
    fn the_changes_of_a_task_are_a_table_with_the_totals() {
        let value = json!({
            "changes": {"files": 2, "commits": 1, "merge_base": "abc1234", "truncated": false},
            "files": [
                {"path": "src/a.rs", "status": "renamed", "old_path": "a.rs", "add": 3,
                 "del": 1, "uncommitted": true},
                {"path": "logo.png", "status": "untracked", "add": 0, "del": 0, "binary": true},
            ]
        });
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let mut output = Output {
            out: &mut out,
            err: &mut err,
            json: false,
            pretty: true,
        };
        changes(&mut output, &value).expect("written");
        let text = String::from_utf8(out).expect("text");
        let lines: Vec<Vec<&str>> = text
            .lines()
            .map(|line| line.split_whitespace().collect())
            .collect();
        assert_eq!(lines[1], ["R", "src/a.rs", "<-", "a.rs", "3", "1", "*"]);
        assert_eq!(lines[2], ["U", "logo.png", "-", "-"]);
        assert!(
            text.contains("2 files, 1 commits, merge base abc1234"),
            "{text}"
        );
    }

    #[test]
    fn a_diff_prints_its_lines_with_their_marks() {
        let value = json!({"hunks": [{"header": "@@ -1 +1 @@", "lines": [
            {"kind": "del", "text": {"text": "old", "spans": []}},
            {"kind": "add", "text": {"text": "new", "spans": []}},
            {"kind": "ctx", "text": {"text": "same", "spans": []}},
        ]}], "truncated": true});
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let mut output = Output {
            out: &mut out,
            err: &mut err,
            json: false,
            pretty: true,
        };
        diff(&mut output, &value).expect("written");
        assert_eq!(
            String::from_utf8(out).expect("text"),
            "@@ -1 +1 @@\n-old\n+new\n same\n(the diff is cut)\n"
        );
    }

    #[test]
    fn a_long_cell_is_cut() {
        let long = "x".repeat(100);
        assert!(cell(&json!(long)).chars().count() <= CELL_MAX);
    }
}
