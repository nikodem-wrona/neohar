//! A check of the sandbox of the webview, for a build with the feature `selftest`.
//!
//! The check runs a script in the first window. It asks for what the page must not get, and it
//! reads the answers. The page sends the result with the one command that it may call (`rpc`,
//! with the method `host.selftest_report`), and the host prints it and ends the process with the
//! code 0 when every rule holds and 1 when one does not. `mise run test:host` runs it.
//!
//! The rules:
//!
//! - the three commands of the transport work;
//! - a command of a plugin, a command of Tauri itself and a command that does not exist are
//!   refused. The drag and the zoom of the window from the command line are the only
//!   commands of Tauri that the page may call;
//! - an inline script does not run (CSP), and the page cannot fetch a page from the network.

use std::{
    sync::{Mutex, PoisonError},
    time::{Duration, Instant},
};

use serde_json::Value;
use tauri::WebviewWindow;

/// The report of the page, when it came.
static REPORT: Mutex<Option<Value>> = Mutex::new(None);

/// The method that the page calls to report. The command `rpc` takes it before the core sees it.
pub const REPORT_METHOD: &str = "host.selftest_report";

/// Keeps the report of the page.
pub fn report(results: Value) {
    *REPORT.lock().unwrap_or_else(PoisonError::into_inner) = Some(results);
}

/// How long the page has to answer.
const LIMIT: Duration = Duration::from_secs(20);

const SCRIPT: &str = r#"
(async () => {
  const results = {};
  const attempt = async (name, run) => {
    try { results[name] = { ok: true, value: JSON.stringify(await run()).slice(0, 200) }; }
    catch (error) { results[name] = { ok: false, error: String(error && error.message || error) }; }
  };
  const invoke = window.__TAURI_INTERNALS__.invoke;
  let violations = 0;
  document.addEventListener('securitypolicyviolation', () => { violations += 1; });
  await attempt('rpc', () => invoke('rpc', { method: 'session.info', params: {} }));
  await attempt('plugin_dialog', () => invoke('plugin:dialog|message', { message: 'x' }));
  await attempt('plugin_opener', () => invoke('plugin:opener|open_url', { url: 'https://example.com' }));
  await attempt('plugin_clipboard', () => invoke('plugin:clipboard-manager|write_text', { text: 'x' }));
  await attempt('tauri_window', () => invoke('plugin:window|close', { label: 'nothing' }));
  await attempt('fourth_command', () => invoke('not_a_command', {}));
  const inline = document.createElement('script');
  inline.textContent = 'window.__inline_ran = true';
  document.head.append(inline);
  results.inline_script_ran = window.__inline_ran === true;
  await attempt('fetch_network', async () => { await fetch('https://example.com/'); return 'fetched'; });
  await new Promise((resolve) => setTimeout(resolve, 200));
  results.csp_violations = violations;
  await invoke('rpc', { method: 'host.selftest_report', params: results });
})();
"#;

/// What went wrong, as lines of text. An empty list means that every rule holds.
fn check(results: &Value) -> Vec<String> {
    let ok = |name: &str| results[name]["ok"].as_bool();
    let mut problems = Vec::new();
    if ok("rpc") != Some(true) {
        problems.push(format!("the command rpc does not work: {}", results["rpc"]));
    }
    for name in [
        "plugin_dialog",
        "plugin_opener",
        "plugin_clipboard",
        "tauri_window",
        "fourth_command",
        "fetch_network",
    ] {
        if ok(name) != Some(false) {
            problems.push(format!("{name} was not refused: {}", results[name]));
        }
    }
    if results["inline_script_ran"] != Value::Bool(false) {
        problems.push("an inline script ran".to_owned());
    }
    if results["csp_violations"].as_u64().unwrap_or(0) == 0 {
        problems.push("the page reported no violation of the CSP".to_owned());
    }
    problems
}

/// Starts the check in a window, when `NEOHAR_SELFTEST` is set. The window evaluates the script
/// when its page has loaded.
pub fn start(webview: WebviewWindow) {
    if std::env::var_os("NEOHAR_SELFTEST").is_none() {
        return;
    }
    // The page may load slowly. The script runs when it is there.
    let runner = webview;
    std::thread::spawn(move || {
        let started = Instant::now();
        std::thread::sleep(Duration::from_secs(2));
        if let Err(error) = runner.eval(SCRIPT) {
            eprintln!("selftest: cannot run the script: {error}");
            std::process::exit(1);
        }
        while started.elapsed() < LIMIT {
            std::thread::sleep(Duration::from_millis(100));
            let Some(results) = REPORT.lock().unwrap_or_else(PoisonError::into_inner).take() else {
                continue;
            };
            println!("{results}");
            let problems = check(&results);
            for problem in &problems {
                eprintln!("selftest: {problem}");
            }
            std::process::exit(i32::from(!problems.is_empty()));
        }
        eprintln!("selftest: the page did not answer in {LIMIT:?}");
        std::process::exit(1);
    });
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn good() -> Value {
        json!({
            "rpc": { "ok": true },
            "plugin_dialog": { "ok": false },
            "plugin_opener": { "ok": false },
            "plugin_clipboard": { "ok": false },
            "tauri_window": { "ok": false },
            "fourth_command": { "ok": false },
            "fetch_network": { "ok": false },
            "inline_script_ran": false,
            "csp_violations": 2,
        })
    }

    #[test]
    fn a_sandbox_that_holds_has_no_problem() {
        assert!(check(&good()).is_empty());
    }

    #[test]
    fn each_broken_rule_is_a_problem() {
        let mut results = good();
        results["plugin_dialog"]["ok"] = json!(true);
        results["inline_script_ran"] = json!(true);
        results["rpc"]["ok"] = json!(false);
        let problems = check(&results);
        assert_eq!(problems.len(), 3, "{problems:?}");
        assert!(problems.iter().any(|p| p.contains("plugin_dialog")));
        assert!(problems.iter().any(|p| p.contains("inline script")));
        assert!(problems.iter().any(|p| p.contains("rpc")));
        // A page that did not answer at all is a problem for every rule.
        assert!(check(&Value::Null).len() >= 8);
    }
}
