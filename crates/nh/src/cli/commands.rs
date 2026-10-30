//! The commands.

use std::io::Read;

use serde_json::Value;

use super::{
    client::Client,
    output::{CliError, Output},
};

/// `nh api call <method> [params]`: prints the result as JSON.
pub fn api_call(
    client: &mut Client,
    output: &mut Output<'_>,
    method: &str,
    params: Value,
) -> Result<(), CliError> {
    let result = client.call(method, params)?;
    output.value(&result)?;
    Ok(())
}

/// The params of `nh api call`: absent means `{}`, `-` reads stdin, anything else is JSON.
pub fn parse_params(params: Option<&str>, stdin: &mut dyn Read) -> Result<Value, CliError> {
    let text = match params {
        None => return Ok(Value::Object(serde_json::Map::new())),
        Some("-") => {
            let mut text = String::new();
            stdin
                .read_to_string(&mut text)
                .map_err(|e| CliError::Usage(format!("cannot read the params from stdin: {e}")))?;
            text
        }
        Some(text) => text.to_owned(),
    };
    let value: Value = serde_json::from_str(&text)
        .map_err(|e| CliError::Usage(format!("the params are not JSON: {e}")))?;
    if value.is_object() || value.is_array() || value.is_null() {
        Ok(value)
    } else {
        Err(CliError::Usage(
            "the params must be a JSON object".to_owned(),
        ))
    }
}

/// `nh api schema`: the contract of the API. It needs no core.
pub fn api_schema(output: &mut Output<'_>) -> Result<(), CliError> {
    let document = nh_proto::api::schema::document();
    let text = serde_json::to_string_pretty(&document)
        .map_err(|e| CliError::Usage(format!("the schema does not serialize: {e}")))?;
    writeln!(output.out, "{text}")?;
    Ok(())
}

/// `nh session info`.
pub fn session_info(client: &mut Client, output: &mut Output<'_>) -> Result<(), CliError> {
    let info = client.call("session.info", Value::Object(serde_json::Map::new()))?;
    if output.json {
        output.value(&info)?;
        return Ok(());
    }
    let text = |value: &Value, path: &[&str]| -> String {
        let mut node = value;
        for key in path {
            match node.get(key) {
                Some(next) => node = next,
                None => return "-".to_owned(),
            }
        }
        match node {
            Value::String(s) => s.clone(),
            Value::Null => "-".to_owned(),
            other => other.to_string(),
        }
    };
    output.pairs(&[
        ("session", text(&info, &["name"])),
        ("version", text(&info, &["version"])),
        ("api", text(&info, &["api_version"])),
        ("pid", text(&info, &["pid"])),
        ("started", text(&info, &["started_at"])),
        ("session uuid", text(&info, &["session_uuid"])),
        ("machine uuid", text(&info, &["machine_uuid"])),
        ("log epoch", text(&info, &["log_epoch"])),
        ("frame protocol", text(&info, &["protocols", "frame"])),
        ("holder protocol", text(&info, &["protocols", "holder"])),
    ])?;
    Ok(())
}

/// The lines of a report of the doctor for a person.
fn doctor_lines(report: &Value) -> (Vec<String>, usize, usize) {
    let mut lines = Vec::new();
    let (mut warnings, mut failures) = (0, 0);
    for check in report["checks"].as_array().into_iter().flatten() {
        let status = check["status"].as_str().unwrap_or("info");
        let mark = match status {
            "ok" => "ok  ",
            "info" => "info",
            "warn" => {
                warnings += 1;
                "warn"
            }
            _ => {
                failures += 1;
                "FAIL"
            }
        };
        let title = check["title"].as_str().unwrap_or("");
        let detail = check["detail"].as_str().unwrap_or("");
        lines.push(format!("{mark}  {title}: {detail}"));
        if let Some(hint) = check["hint"].as_str() {
            lines.push(format!("      {hint}"));
        }
    }
    (lines, warnings, failures)
}

/// `nh doctor`. With a core the checks run there. Without one they are the checks of the install.
pub fn doctor(
    client: Option<&mut Client>,
    offline: impl FnOnce() -> Value,
    output: &mut Output<'_>,
) -> Result<(), CliError> {
    let report = match client {
        Some(client) => client.call("doctor.run", Value::Object(serde_json::Map::new()))?,
        None => offline(),
    };
    let (lines, warnings, failures) = doctor_lines(&report);
    if output.json {
        output.value(&report)?;
    } else {
        for line in &lines {
            writeln!(output.out, "{line}")?;
        }
        writeln!(output.out, "{warnings} warnings, {failures} failures")?;
    }
    if failures > 0 {
        return Err(CliError::Failed(format!("{failures} checks failed")));
    }
    Ok(())
}

/// `nh debug bundle`.
pub fn debug_bundle(
    client: &mut Client,
    output: &mut Output<'_>,
    out: &std::path::Path,
    include_scrollback: bool,
) -> Result<(), CliError> {
    let result = client.call(
        "debug.bundle",
        serde_json::json!({
            "out": out.display().to_string(),
            "include_scrollback": include_scrollback,
        }),
    )?;
    if output.json {
        output.value(&result)?;
    } else {
        writeln!(
            output.out,
            "Wrote {} ({} files, {} bytes). Read it before you send it: the redaction can miss a secret.",
            result["path"].as_str().unwrap_or("?"),
            result["files"],
            result["bytes"]
        )?;
    }
    Ok(())
}

/// `nh completions <shell>`.
pub fn completions(
    shell: clap_complete::Shell,
    command: &clap::Command,
    output: &mut Output<'_>,
) -> Result<(), CliError> {
    let mut command = command.clone();
    clap_complete::generate(shell, &mut command, "nh", output.out);
    Ok(())
}

/// `nh config edit`: opens the file in the editor of the person, and then asks the core to read it.
pub fn config_edit(
    client: &mut Client,
    output: &mut Output<'_>,
    env: &super::Environment,
) -> Result<(), CliError> {
    let dir = env
        .paths
        .config_dir()
        .map_err(|e| CliError::Usage(e.to_string()))?;
    std::fs::create_dir_all(&dir)?;
    let file = dir.join("config.toml");
    if !file.exists() {
        std::fs::write(&file, "# The configuration of neohar.\n")?;
    }
    let editor = std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| "vi".to_owned());
    // The editor may be a command with arguments, so the shell reads the line.
    let status = std::process::Command::new("/bin/sh")
        .arg("-c")
        .arg(format!("{editor} \"$0\""))
        .arg(&file)
        .status()?;
    if !status.success() {
        return Err(CliError::Failed(format!("the editor ended with {status}")));
    }
    let reloaded = client.call("config.reload", Value::Object(serde_json::Map::new()))?;
    output.value(&reloaded)?;
    Ok(())
}

/// `nh events tail`: events as JSON lines. `--since` replays from a cursor, and `--follow` goes on
/// at the end of the log. With neither, the command prints the cursor of the end of the log.
pub fn events_tail(
    client: &mut Client,
    output: &mut Output<'_>,
    matches: &clap::ArgMatches,
) -> Result<(), CliError> {
    let mut params = serde_json::json!({});
    let since: Option<Value> = match matches.get_one::<String>("since") {
        Some(text) => Some(
            serde_json::from_str(text)
                .map_err(|e| CliError::Usage(format!("--since is not JSON: {e}")))?,
        ),
        None => None,
    };
    if let Some(cursor) = &since {
        params["cursor"] = cursor.clone();
    }
    let mut filter = serde_json::Map::new();
    if let Some(kinds) = matches.get_many::<String>("kind") {
        filter.insert(
            "kinds".to_owned(),
            serde_json::json!(kinds.collect::<Vec<_>>()),
        );
    }
    if let Some(subject) = matches.get_one::<String>("subject") {
        filter.insert("subject".to_owned(), Value::String(subject.clone()));
    }
    if !filter.is_empty() {
        params["filter"] = Value::Object(filter);
    }
    let follow = matches.get_flag("follow");
    let subscribed = client.call("events.subscribe", params)?;
    let end = subscribed["cursor"]["seq"].as_u64().unwrap_or(0);
    let id = subscribed["subscription"].as_str().unwrap_or("").to_owned();
    let start = since
        .as_ref()
        .and_then(|cursor| cursor["seq"].as_u64())
        .unwrap_or(end);
    if !follow && start >= end {
        // Nothing to read. The cursor is what the person wants.
        output.value(&serde_json::json!({"cursor": subscribed["cursor"]}))?;
        return Ok(());
    }
    if follow {
        client.wait_without_limit()?;
    }
    loop {
        let Some(message) = client.read_message()? else {
            return Ok(());
        };
        match message["method"].as_str() {
            Some("events.event") if message["params"]["subscription"] == id.as_str() => {
                let event = &message["params"]["event"];
                output.value(event)?;
                let seq = event["seq"].as_u64().unwrap_or(0);
                if !follow && seq >= end {
                    output.value(&serde_json::json!({"cursor": subscribed["cursor"]}))?;
                    return Ok(());
                }
            }
            Some("events.truncated") => {
                output.value(&message["params"])?;
                return Err(CliError::Failed(
                    "the cursor is too old: take a new one".to_owned(),
                ));
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests;

/// How often `nh workflow execute --wait` looks at the execution.
const WORKFLOW_POLL: std::time::Duration = std::time::Duration::from_secs(1);

/// `nh workflow execute --wait`: waits until the execution does not run, then prints its state and
/// the path of `report.md`. A paused or cancelled execution is a failure.
pub fn workflow_wait(
    client: &mut Client,
    output: &mut Output<'_>,
    started: &Value,
) -> Result<(), CliError> {
    let execution = started["execution"].clone();
    loop {
        let shown = client.call(
            "workflow.execution",
            serde_json::json!({ "execution": execution }),
        )?;
        let state = shown["execution"]["state"].as_str().unwrap_or("running");
        if state != "running" {
            let result = serde_json::json!({
                "execution": execution,
                "label": shown["execution"]["label"],
                "state": state,
                "report": shown["execution"]["report"],
            });
            super::render::result(output, &result)?;
            return match state {
                "done" => Ok(()),
                _ => Err(CliError::Failed(format!(
                    "the execution is {state}: {}",
                    shown["execution"]["label"].as_str().unwrap_or_default()
                ))),
            };
        }
        std::thread::sleep(WORKFLOW_POLL);
    }
}
