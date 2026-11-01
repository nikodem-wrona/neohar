//! The texts of an execution: the footer of a prompt, the verdict of a report, the time texts and
//! `report.md`. Pure functions, so the tests can read each text.

use nh_proto::api::{model::Verdict, time::Timestamp};

/// The reminder for a turn that ended without a report.
pub(crate) const REMINDER: &str = "Write your full report in Markdown as your last message. \
     Make its last line \"Verdict: pass\", \"Verdict: warn\" or \"Verdict: fail\".";

/// The first part of the prompt of a summary, when the Report node has none of its own.
pub(crate) const SUMMARY_PROMPT: &str = "Merge the reports below into one summary. Sort the \
     findings by severity, the worst first. Remove the findings that two reports have twice. Link \
     each finding to its source report with a relative Markdown link, for example \
     [code.md](code.md).";

/// An input of a node, as the footer lists it.
pub(crate) struct Input {
    pub title: String,
    /// `done`, or `skipped`.
    pub state: &'static str,
    pub verdict: Option<Verdict>,
    pub report: Option<String>,
}

/// What the footer says about the node.
pub(crate) struct Footer<'a> {
    pub workflow: &'a str,
    pub node: &'a str,
    pub execution: u32,
    pub inputs: &'a [Input],
    pub can_edit: bool,
    /// The node writes a summary, not a report with a verdict.
    pub summary: bool,
}

/// The word of a verdict.
pub(crate) fn verdict_word(verdict: Verdict) -> &'static str {
    match verdict {
        Verdict::Pass => "pass",
        Verdict::Warn => "warn",
        Verdict::Fail => "fail",
        Verdict::Unknown => "unknown",
    }
}

/// The footer that the core adds to each prompt. The person cannot change it.
pub(crate) fn footer(footer: &Footer<'_>) -> String {
    let mut out = format!(
        "---\nneohar workflow \"{}\", node \"{}\", execution {}.\n",
        footer.workflow, footer.node, footer.execution
    );
    if !footer.inputs.is_empty() {
        out.push_str("Inputs. Read them first:\n");
        for input in footer.inputs {
            let state = match (input.state, input.verdict) {
                ("done", Some(verdict)) => format!("done, verdict {}", verdict_word(verdict)),
                (state, _) => state.to_owned(),
            };
            match &input.report {
                Some(path) => out.push_str(&format!("- {} ({state}): {path}\n", input.title)),
                None => out.push_str(&format!("- {} ({state}, no report)\n", input.title)),
            }
        }
    }
    out.push_str("Rules:\n- Do not ask questions. Write your assumptions in the report.\n");
    if !footer.can_edit {
        out.push_str("- Do not edit files.\n");
    }
    if footer.summary {
        out.push_str("- End your turn with the full summary in Markdown as your last message.\n");
    } else {
        out.push_str(
            "- End your turn with the full report in Markdown as your last message.\n\
             - Make the last line \"Verdict: pass\", \"Verdict: warn\" or \"Verdict: fail\".\n",
        );
    }
    out
}

/// The verdict line of a report: one of the last five lines that are not empty, for example
/// `Verdict: warn` or `**Verdict:** pass`. `None` when the report has no verdict line.
pub(crate) fn verdict(report: &str) -> Option<Verdict> {
    for line in report
        .lines()
        .rev()
        .filter(|line| !line.trim().is_empty())
        .take(5)
    {
        let clean: String = line
            .chars()
            .filter(|c| !matches!(c, '*' | '_' | '`' | '>' | '#'))
            .collect();
        let clean = clean.trim().trim_start_matches('-').trim().to_lowercase();
        let Some(rest) = clean.strip_prefix("verdict") else {
            continue;
        };
        let word = rest
            .trim_start_matches([':', ' '])
            .split(|c: char| !c.is_ascii_alphabetic())
            .next()
            .unwrap_or_default();
        return Some(match word {
            "pass" | "passed" => Verdict::Pass,
            "warn" | "warning" => Verdict::Warn,
            "fail" | "failed" => Verdict::Fail,
            _ => Verdict::Unknown,
        });
    }
    None
}

/// The worse of two verdicts: fail, then warn, then pass. Unknown counts only when nothing else
/// is there.
pub(crate) fn worse(a: Option<Verdict>, b: Option<Verdict>) -> Option<Verdict> {
    let rank = |v: Option<Verdict>| match v {
        None => 0,
        Some(Verdict::Unknown) => 1,
        Some(Verdict::Pass) => 2,
        Some(Verdict::Warn) => 3,
        Some(Verdict::Fail) => 4,
    };
    if rank(b) > rank(a) { b } else { a }
}

/// A length of time as a person reads it: "under 1 min", "6 min", "1 h 5 min".
pub(crate) fn duration(ms: i64) -> String {
    let minutes = ms.max(0) / 60_000;
    match minutes {
        0 => "under 1 min".to_owned(),
        1..60 => format!("{minutes} min"),
        _ if minutes % 60 == 0 => format!("{} h", minutes / 60),
        _ => format!("{} h {} min", minutes / 60, minutes % 60),
    }
}

/// A moment in UTC as a person reads it: "2026-10-30 14:12 UTC".
pub(crate) fn moment(ms: i64) -> String {
    let text = Timestamp(ms).to_rfc3339();
    match (text.get(..10), text.get(11..16)) {
        (Some(day), Some(time)) => format!("{day} {time} UTC"),
        _ => text,
    }
}

/// The part of the name of an execution folder that comes from the time: "20261030-141205".
pub(crate) fn stamp(ms: i64) -> String {
    let text = Timestamp(ms).to_rfc3339();
    let digits: String = text.chars().take(19).filter(char::is_ascii_digit).collect();
    match (digits.get(..8), digits.get(8..14)) {
        (Some(day), Some(time)) => format!("{day}-{time}"),
        _ => digits,
    }
}

/// The first paragraph of a report that is not a heading, cut to 600 characters.
pub(crate) fn first_paragraph(report: &str) -> String {
    let mut lines = Vec::new();
    for line in report.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if lines.is_empty() {
                continue;
            }
            break;
        }
        if lines.is_empty() && (trimmed.starts_with('#') || trimmed.starts_with("---")) {
            continue;
        }
        lines.push(trimmed);
    }
    let text = lines.join(" ");
    if text.chars().count() > 600 {
        let cut: String = text.chars().take(600).collect();
        format!("{}…", cut.trim_end())
    } else {
        text
    }
}

/// One input of a Report node, as `report.md` lists it.
pub(crate) struct ReportRow {
    pub title: String,
    /// "Codex · gpt-5.5 · high", or empty for a Report node.
    pub agent: String,
    pub state: &'static str,
    pub verdict: Option<Verdict>,
    pub time: Option<String>,
    /// The link to the report: its name when it is in the folder of `report.md`, else its path.
    pub link: Option<String>,
    pub text: Option<String>,
}

/// The facts at the top of `report.md`.
pub(crate) struct ReportHead<'a> {
    pub label: &'a str,
    pub folder: &'a str,
    pub branch: Option<&'a str>,
    pub head: Option<&'a str>,
    pub base: Option<&'a str>,
    pub started_at: i64,
    pub now: i64,
    pub tree_changed: bool,
}

/// `report.md` of a Report node.
pub(crate) fn report(head: &ReportHead<'_>, summary: Option<&str>, rows: &[ReportRow]) -> String {
    let place = std::path::Path::new(head.folder).file_name().map_or_else(
        || head.folder.to_owned(),
        |n| n.to_string_lossy().into_owned(),
    );
    let mut title = format!("# {} · {place}", head.label);
    if let Some(branch) = head.branch {
        title.push_str(&format!(" · {branch}"));
    }
    if let Some(sha) = head.head {
        title.push_str(&format!(" @ {sha}"));
    }
    let mut facts = vec![
        moment(head.started_at),
        duration(head.now - head.started_at),
    ];
    if let Some(base) = head.base {
        facts.push(format!("base `{base}`"));
    }
    facts.push(if head.tree_changed {
        "the tree changed during the execution".to_owned()
    } else {
        "the tree did not change during the execution".to_owned()
    });
    let mut out = format!("{title}\n\n{}\n", facts.join(" · "));
    if let Some(summary) = summary.map(str::trim).filter(|s| !s.is_empty()) {
        out.push_str(&format!("\n## Summary\n\n{summary}\n"));
    }
    out.push_str("\n## Results\n\n| Node | Agent | Verdict | Time | Report |\n| --- | --- | --- | --- | --- |\n");
    for row in rows {
        let verdict = match (row.state, row.verdict) {
            ("done", Some(verdict)) => verdict_word(verdict).to_owned(),
            ("done", None) => "—".to_owned(),
            (state, _) => state.to_owned(),
        };
        let link = row.link.as_ref().map_or_else(
            || "—".to_owned(),
            |link| format!("[{}]({link})", file_name(link)),
        );
        out.push_str(&format!(
            "| {} | {} | {verdict} | {} | {link} |\n",
            cell(&row.title),
            cell(if row.agent.is_empty() {
                "—"
            } else {
                &row.agent
            }),
            row.time.as_deref().unwrap_or("—"),
        ));
    }
    for row in rows {
        out.push_str(&format!("\n## {}\n\n", row.title));
        match (&row.text, &row.link) {
            (Some(text), Some(link)) => {
                let first = first_paragraph(text);
                if !first.is_empty() {
                    out.push_str(&format!("{first}\n\n"));
                }
                out.push_str(&format!("[Full report]({link})\n"));
            }
            _ => out.push_str("No report.\n"),
        }
    }
    out
}

fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn cell(text: &str) -> String {
    text.replace('|', "\\|")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_verdict_is_on_one_of_the_last_lines_in_many_forms() {
        assert_eq!(verdict("All good.\n\nVerdict: pass\n"), Some(Verdict::Pass));
        assert_eq!(verdict("x\n**Verdict:** warn\n\n"), Some(Verdict::Warn));
        assert_eq!(
            verdict("x\n- Verdict: FAIL (2 issues)"),
            Some(Verdict::Fail)
        );
        assert_eq!(verdict("x\n`Verdict: maybe`"), Some(Verdict::Unknown));
        assert_eq!(verdict("No verdict here."), None);
        assert_eq!(verdict("Verdict: pass\n1\n2\n3\n4\n5\n"), None);
    }

    #[test]
    fn the_worse_verdict_wins() {
        assert_eq!(
            worse(Some(Verdict::Pass), Some(Verdict::Fail)),
            Some(Verdict::Fail)
        );
        assert_eq!(
            worse(Some(Verdict::Warn), Some(Verdict::Pass)),
            Some(Verdict::Warn)
        );
        assert_eq!(worse(None, Some(Verdict::Unknown)), Some(Verdict::Unknown));
        assert_eq!(
            worse(Some(Verdict::Unknown), Some(Verdict::Pass)),
            Some(Verdict::Pass)
        );
    }

    #[test]
    fn the_times_read_as_a_person_says_them() {
        assert_eq!(duration(30_000), "under 1 min");
        assert_eq!(duration(6 * 60_000 + 5_000), "6 min");
        assert_eq!(duration(60 * 60_000), "1 h");
        assert_eq!(duration(65 * 60_000), "1 h 5 min");
        assert_eq!(moment(1_793_369_520_000), "2026-10-30 14:12 UTC");
        assert_eq!(stamp(1_793_369_525_000), "20261030-141205");
    }

    #[test]
    fn the_footer_lists_the_inputs_and_the_rules() {
        let inputs = [
            Input {
                title: "Code review".to_owned(),
                state: "done",
                verdict: Some(Verdict::Warn),
                report: Some("/x/code.md".to_owned()),
            },
            Input {
                title: "Security review".to_owned(),
                state: "skipped",
                verdict: None,
                report: None,
            },
        ];
        let text = footer(&Footer {
            workflow: "Review",
            node: "Business-logic review",
            execution: 3,
            inputs: &inputs,
            can_edit: false,
            summary: false,
        });
        assert_eq!(
            text,
            "---\nneohar workflow \"Review\", node \"Business-logic review\", execution 3.\n\
             Inputs. Read them first:\n\
             - Code review (done, verdict warn): /x/code.md\n\
             - Security review (skipped, no report)\n\
             Rules:\n\
             - Do not ask questions. Write your assumptions in the report.\n\
             - Do not edit files.\n\
             - End your turn with the full report in Markdown as your last message.\n\
             - Make the last line \"Verdict: pass\", \"Verdict: warn\" or \"Verdict: fail\".\n"
        );
        let edit = footer(&Footer {
            workflow: "Fix",
            node: "Fix",
            execution: 1,
            inputs: &[],
            can_edit: true,
            summary: true,
        });
        assert!(!edit.contains("Do not edit files"));
        assert!(!edit.contains("Inputs."));
        assert!(edit.contains("full summary"));
    }

    #[test]
    fn the_report_links_each_input_and_shows_its_first_paragraph() {
        let rows = [
            ReportRow {
                title: "Code review".to_owned(),
                agent: "Codex · high".to_owned(),
                state: "done",
                verdict: Some(Verdict::Warn),
                time: Some("6 min".to_owned()),
                link: Some("code.md".to_owned()),
                text: Some("# Code review\n\nTwo bugs in the parser.\nOne test is missing.\n\n## Findings\n...".to_owned()),
            },
            ReportRow {
                title: "Security review".to_owned(),
                agent: "Claude Code".to_owned(),
                state: "skipped",
                verdict: None,
                time: None,
                link: None,
                text: None,
            },
        ];
        let text = report(
            &ReportHead {
                label: "Review 3",
                folder: "/w/neohar",
                branch: Some("feat/refunds"),
                head: Some("1a2b3c4"),
                base: Some("origin/main"),
                started_at: 1_793_369_520_000,
                now: 1_793_369_520_000 + 14 * 60_000,
                tree_changed: false,
            },
            Some("Fix the parser first."),
            &rows,
        );
        assert!(text.starts_with("# Review 3 · neohar · feat/refunds @ 1a2b3c4\n\n2026-10-30 14:12 UTC · 14 min · base `origin/main` · the tree did not change during the execution\n"));
        assert!(text.contains("## Summary\n\nFix the parser first.\n"));
        assert!(
            text.contains("| Code review | Codex · high | warn | 6 min | [code.md](code.md) |")
        );
        assert!(text.contains("| Security review | Claude Code | skipped | — | — |"));
        assert!(text.contains("## Code review\n\nTwo bugs in the parser. One test is missing.\n\n[Full report](code.md)\n"));
        assert!(text.contains("## Security review\n\nNo report.\n"));
    }
}
