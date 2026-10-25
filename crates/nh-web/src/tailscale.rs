//! Tailscale for the listener of the phone: where the CLI is, what
//! `tailscale status --json` and `tailscale serve status --json` say, and the mapping of
//! `tailscale serve` that `phone.publish` adds and `phone.unpublish` removes.
//!
//! neohar runs `tailscale status`, `tailscale serve status`, the mapping of one HTTPS port to the
//! listener, and the removal of that mapping. It never runs `tailscale funnel`, never changes a
//! mapping that serves something else, and never changes the tailnet.

use std::{
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use nh_proto::api::model::{TailscaleState, TailscaleStatus};
use serde_json::Value;

/// The CLI in the app of Tailscale, when no `tailscale` is on the `PATH`.
const APP_CLI: &str = "/Applications/Tailscale.app/Contents/MacOS/Tailscale";

/// How long a read of the state waits for the CLI.
const READ_WAIT: Duration = Duration::from_secs(5);

/// How long a change of the mapping waits for the CLI.
const CHANGE_WAIT: Duration = Duration::from_secs(20);

/// The longest text of an error of the CLI that an answer carries.
const ERROR_MAX: usize = 400;

/// Where the listener finds the CLI of Tailscale.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TailscaleCli {
    /// `tailscale` on the `PATH` of the login environment, else the CLI in
    /// `/Applications/Tailscale.app`.
    Search,
    /// This program and no other. The tests give a fake, so that they never reach the real
    /// Tailscale. A path that is not a program means that Tailscale is not installed.
    At(PathBuf),
}

impl TailscaleCli {
    /// [`TailscaleCli::Search`], or in a debug build the program of `NEOHAR_TAILSCALE`: the shell
    /// tests start `nh daemon` with a fake.
    pub fn from_env() -> Self {
        if cfg!(debug_assertions)
            && let Some(path) = std::env::var_os("NEOHAR_TAILSCALE")
        {
            return Self::At(PathBuf::from(path));
        }
        Self::Search
    }

    /// The program to run, or `None` when Tailscale is not installed.
    fn find(&self, env: &[(String, String)]) -> Option<PathBuf> {
        match self {
            Self::At(path) => is_program(path).then(|| path.clone()),
            Self::Search => {
                let path = env
                    .iter()
                    .find(|(name, _)| name == "PATH")
                    .map_or("", |(_, value)| value.as_str());
                path.split(':')
                    .filter(|dir| !dir.is_empty())
                    .map(|dir| Path::new(dir).join("tailscale"))
                    .find(|candidate| is_program(candidate))
                    .or_else(|| {
                        let app = PathBuf::from(APP_CLI);
                        is_program(&app).then_some(app)
                    })
            }
        }
    }
}

fn is_program(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt as _;
    path.metadata()
        .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}

/// What Tailscale said about the listener, for one pair of ports.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Facts {
    pub status: TailscaleStatus,
    /// The port of the listener that the facts were read for.
    pub port: u16,
    /// The HTTPS port serves something other than the listener. Then `phone.publish` does not
    /// touch it.
    pub taken: bool,
    /// The CLI that gave the facts.
    program: Option<PathBuf>,
}

impl Facts {
    fn without(state: TailscaleState, https_port: u16, port: u16) -> Self {
        Self {
            status: TailscaleStatus {
                state,
                name: None,
                https: false,
                https_port,
                published: false,
                funnel: false,
            },
            port,
            taken: false,
            program: None,
        }
    }

    /// Whether these facts are about these ports.
    pub fn for_ports(&self, https_port: u16, port: u16) -> bool {
        self.status.https_port == https_port && self.port == port
    }

    /// The public origin that Tailscale gives the listener: `https://<name>:<https_port>`. `None`
    /// when Tailscale does not run, gives no name, or gives no HTTPS certificate. Also `None` when
    /// a Funnel is on the HTTPS port: the port is then on the internet, so every request but the
    /// files of the app fails until the person turns the Funnel off.
    pub fn origin(&self) -> Option<String> {
        let status = &self.status;
        if status.state != TailscaleState::Running || !status.https || status.funnel {
            return None;
        }
        let name = status.name.as_deref()?;
        Some(if status.https_port == 443 {
            format!("https://{name}")
        } else {
            format!("https://{name}:{}", status.https_port)
        })
    }

    /// What the person must do before a phone can reach the listener, or `None` when nothing.
    pub fn hint(&self) -> Option<String> {
        let status = &self.status;
        let port = status.https_port;
        let hint = match status.state {
            TailscaleState::Missing => {
                "Install Tailscale, or set phone.url to the HTTPS address of this Mac.".to_owned()
            }
            TailscaleState::Unreachable | TailscaleState::Starting | TailscaleState::Stopped => {
                "Open Tailscale and connect it to the tailnet.".to_owned()
            }
            TailscaleState::NeedsLogin => "Open Tailscale and log in.".to_owned(),
            TailscaleState::Running if status.name.is_none() => {
                "Tailscale gives this Mac no name. Turn on MagicDNS in the admin console of the \
                 tailnet."
                    .to_owned()
            }
            TailscaleState::Running if !status.https => {
                "Turn on HTTPS certificates in the admin console of the tailnet.".to_owned()
            }
            TailscaleState::Running if status.funnel => format!(
                "Turn off the Funnel on port {port}: run `tailscale funnel --https={port} off`. \
                 The phone listener must stay in the tailnet."
            ),
            TailscaleState::Running if self.taken => format!(
                "Port {port} of the tailnet serves something else. Change phone.https_port, or \
                 remove that mapping with `tailscale serve`."
            ),
            TailscaleState::Running if !status.published => {
                "Click \"Publish with Tailscale\" in the section Phone of the settings, or run \
                 `nh phone publish`."
                    .to_owned()
            }
            TailscaleState::Running => return None,
        };
        Some(hint)
    }

    /// Why `phone.publish` cannot add the mapping, or `None` when it can.
    pub fn refusal(&self) -> Option<String> {
        if self.status.published {
            return None;
        }
        self.hint().filter(|_| {
            // The only hint that a publish cures is the missing mapping.
            self.status.state != TailscaleState::Running
                || self.status.name.is_none()
                || !self.status.https
                || self.status.funnel
                || self.taken
        })
    }
}

/// Reads the state of Tailscale for a listener on `127.0.0.1:<port>` and the HTTPS port
/// `https_port`. `env` is the login environment: the CLI runs with it.
pub(crate) async fn read(
    cli: &TailscaleCli,
    env: &[(String, String)],
    https_port: u16,
    port: u16,
) -> Facts {
    let Some(program) = cli.find(env) else {
        return Facts::without(TailscaleState::Missing, https_port, port);
    };
    // The CLI prints the JSON also when the daemon is stopped. When it cannot reach the daemon,
    // it prints an error and no JSON.
    let parsed = run(&program, &["status", "--json"], env, READ_WAIT)
        .await
        .ok()
        .and_then(|output| parse_status(&output));
    let mut facts = Facts::without(TailscaleState::Unreachable, https_port, port);
    facts.program = Some(program.clone());
    let Some(status) = parsed else {
        return facts;
    };
    facts.status.state = status.state;
    facts.status.name = status.name;
    facts.status.https = status.https;
    if facts.status.state != TailscaleState::Running {
        return facts;
    }
    if let Some(name) = facts.status.name.clone()
        && let Ok(output) = run(&program, &["serve", "status", "--json"], env, READ_WAIT).await
        && let Some(serve) = parse_serve(&output, &name, https_port, port)
    {
        facts.status.published = serve.published;
        facts.status.funnel = serve.funnel;
        facts.taken = serve.taken;
    }
    facts
}

/// Adds the mapping: `tailscale serve --bg --yes --https=<https_port> http://127.0.0.1:<port>`.
pub(crate) async fn publish(facts: &Facts, env: &[(String, String)]) -> Result<(), String> {
    let program = facts
        .program
        .as_deref()
        .ok_or_else(|| "Tailscale is not installed.".to_owned())?;
    let https = format!("--https={}", facts.status.https_port);
    let target = format!("http://127.0.0.1:{}", facts.port);
    run(
        program,
        &["serve", "--bg", "--yes", &https, &target],
        env,
        CHANGE_WAIT,
    )
    .await
    .map(drop)
}

/// Removes the mapping: `tailscale serve --yes --https=<https_port> off`.
pub(crate) async fn unpublish(facts: &Facts, env: &[(String, String)]) -> Result<(), String> {
    let program = facts
        .program
        .as_deref()
        .ok_or_else(|| "Tailscale is not installed.".to_owned())?;
    let https = format!("--https={}", facts.status.https_port);
    run(
        program,
        &["serve", "--yes", &https, "off"],
        env,
        CHANGE_WAIT,
    )
    .await
    .map(drop)
}

/// Runs the CLI and gives what it printed. An exit with an error gives the text of its error, or
/// its output when that is JSON, because `tailscale status --json` of a stopped daemon exits with
/// an error and still prints the state.
async fn run(
    program: &Path,
    args: &[&str],
    env: &[(String, String)],
    wait: Duration,
) -> Result<String, String> {
    let mut command = tokio::process::Command::new(program);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if !env.is_empty() {
        command.env_clear().envs(env.iter().map(|(k, v)| (k, v)));
    }
    let output = match tokio::time::timeout(wait, command.output()).await {
        Ok(Ok(output)) => output,
        Ok(Err(error)) => return Err(format!("tailscale did not start: {error}")),
        Err(_) => return Err(format!("tailscale gave no answer in {} s", wait.as_secs())),
    };
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    if output.status.success() || stdout.trim_start().starts_with('{') {
        return Ok(stdout);
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let mut text = stderr.trim().to_owned();
    if text.is_empty() {
        text = format!("tailscale ended with {}", output.status);
    }
    if text.len() > ERROR_MAX {
        let mut end = ERROR_MAX;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
        text.push('…');
    }
    Err(text)
}

/// The part of `tailscale status --json` that the listener uses.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Status {
    state: TailscaleState,
    name: Option<String>,
    https: bool,
}

/// `BackendState`, `Self.DNSName` and `CertDomains` of `tailscale status --json`.
fn parse_status(text: &str) -> Option<Status> {
    let json: Value = serde_json::from_str(text).ok()?;
    let state = match json.get("BackendState")?.as_str()? {
        "Running" => TailscaleState::Running,
        "NeedsLogin" | "NeedsMachineAuth" => TailscaleState::NeedsLogin,
        "Stopped" => TailscaleState::Stopped,
        "Starting" | "NoState" => TailscaleState::Starting,
        _ => TailscaleState::Unreachable,
    };
    let name = json
        .pointer("/Self/DNSName")
        .and_then(Value::as_str)
        .map(|name| name.trim_end_matches('.').to_ascii_lowercase())
        .filter(|name| !name.is_empty());
    let https = name.as_deref().is_some_and(|name| {
        json.get("CertDomains")
            .and_then(Value::as_array)
            .is_some_and(|domains| {
                domains
                    .iter()
                    .filter_map(Value::as_str)
                    .any(|domain| domain.eq_ignore_ascii_case(name))
            })
    });
    Some(Status { state, name, https })
}

/// What the configuration of `tailscale serve` says about one HTTPS port.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Serve {
    /// `/` of the port proxies to the listener, with HTTPS.
    published: bool,
    /// The port serves something else.
    taken: bool,
    /// A Funnel is on the port.
    funnel: bool,
}

/// Reads `tailscale serve status --json`: `TCP` has the ports, `Web` has the handlers of each
/// `<name>:<port>`, and `AllowFunnel` has the ports with a Funnel. `Foreground` has the
/// configuration of each `tailscale serve` that runs in a terminal.
fn parse_serve(text: &str, name: &str, https_port: u16, port: u16) -> Option<Serve> {
    let json: Value = serde_json::from_str(text).ok()?;
    let host = format!("{name}:{https_port}");
    let tcp_key = https_port.to_string();
    let mut serve = Serve::default();
    let mut configs = vec![&json];
    if let Some(foreground) = json.get("Foreground").and_then(Value::as_object) {
        configs.extend(foreground.values());
    }
    for (index, config) in configs.into_iter().enumerate() {
        let background = index == 0;
        if allows_funnel(config, &host) {
            serve.funnel = true;
        }
        let Some(tcp) = config.pointer(&format!("/TCP/{tcp_key}")) else {
            continue;
        };
        let ours = background
            && tcp.get("HTTPS").and_then(Value::as_bool) == Some(true)
            && config
                .get("Web")
                .and_then(|web| web.get(&host))
                .and_then(|site| site.pointer("/Handlers/~1/Proxy"))
                .and_then(Value::as_str)
                .is_some_and(|proxy| proxies_to(proxy, port));
        if ours {
            serve.published = true;
        } else {
            serve.taken = true;
        }
    }
    if serve.published {
        serve.taken = false;
    }
    Some(serve)
}

fn allows_funnel(config: &Value, host: &str) -> bool {
    config
        .get("AllowFunnel")
        .and_then(|funnel| funnel.get(host))
        .and_then(Value::as_bool)
        == Some(true)
}

/// Whether the target of a proxy is the listener: `http://127.0.0.1:<port>`, also with
/// `localhost`, without the scheme, or with `/` at the end.
fn proxies_to(proxy: &str, port: u16) -> bool {
    let rest = proxy.strip_prefix("http://").unwrap_or(proxy);
    let rest = rest.trim_end_matches('/');
    let Some((host, given)) = rest.rsplit_once(':') else {
        return false;
    };
    matches!(host, "127.0.0.1" | "localhost") && given.parse::<u16>() == Ok(port)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `tailscale status --json` of Tailscale 1.104.1 on a Mac, with the fields that the parser
    /// reads, and the names of the tailnet and of the machine replaced.
    const STATUS: &str = include_str!("../tests/fixtures/status-running.json");

    /// The same, from a tailnet without HTTPS certificates: `CertDomains` is `null`.
    const STATUS_NO_HTTPS: &str = include_str!("../tests/fixtures/status-no-https.json");

    /// `tailscale serve status --json` without a mapping, as Tailscale 1.104.1 prints it.
    const SERVE_EMPTY: &str = include_str!("../tests/fixtures/serve-empty.json");

    /// A mapping of port 8443 to the listener, with the form of the configuration of
    /// `tailscale serve` **[verify the JSON of 1.104]**.
    const SERVE_PUBLISHED: &str = include_str!("../tests/fixtures/serve-published.json");

    #[test]
    fn the_status_gives_the_state_the_name_and_the_certificates() {
        let status = parse_status(STATUS).expect("a status");
        assert_eq!(status.state, TailscaleState::Running);
        assert_eq!(status.name.as_deref(), Some("studio.example.ts.net"));
        assert!(status.https);
        let status = parse_status(STATUS_NO_HTTPS).expect("a status");
        assert_eq!(status.state, TailscaleState::Running);
        assert!(!status.https, "CertDomains is null");
    }

    #[test]
    fn each_backend_state_has_a_state() {
        for (backend, state) in [
            ("Running", TailscaleState::Running),
            ("NeedsLogin", TailscaleState::NeedsLogin),
            ("NeedsMachineAuth", TailscaleState::NeedsLogin),
            ("Stopped", TailscaleState::Stopped),
            ("Starting", TailscaleState::Starting),
            ("NoState", TailscaleState::Starting),
            ("Something", TailscaleState::Unreachable),
        ] {
            let text = format!(r#"{{"BackendState": "{backend}", "Self": {{"DNSName": ""}}}}"#);
            let status = parse_status(&text).expect("a status");
            assert_eq!(status.state, state, "{backend}");
            assert_eq!(status.name, None, "an empty name is no name");
        }
        assert_eq!(parse_status("failed to connect to local tailscaled"), None);
        assert_eq!(parse_status("{}"), None);
    }

    #[test]
    fn an_empty_serve_configuration_has_no_mapping() {
        let serve = parse_serve(SERVE_EMPTY, "studio.example.ts.net", 8443, 47_310).expect("JSON");
        assert_eq!(serve, Serve::default());
    }

    #[test]
    fn the_mapping_of_the_port_to_the_listener_is_published() {
        let serve =
            parse_serve(SERVE_PUBLISHED, "studio.example.ts.net", 8443, 47_310).expect("JSON");
        assert_eq!(
            serve,
            Serve {
                published: true,
                taken: false,
                funnel: false,
            }
        );
        // Another port of the listener: the mapping serves something else.
        let serve =
            parse_serve(SERVE_PUBLISHED, "studio.example.ts.net", 8443, 47_311).expect("JSON");
        assert!(!serve.published && serve.taken);
        // Another HTTPS port: no mapping.
        let serve =
            parse_serve(SERVE_PUBLISHED, "studio.example.ts.net", 443, 47_310).expect("JSON");
        assert_eq!(serve, Serve::default());
    }

    #[test]
    fn a_funnel_or_a_tcp_forward_on_the_port_is_seen() {
        let text = r#"{
            "TCP": {"8443": {"TCPForward": "127.0.0.1:22"}},
            "AllowFunnel": {"studio.example.ts.net:8443": true}
        }"#;
        let serve = parse_serve(text, "studio.example.ts.net", 8443, 47_310).expect("JSON");
        assert!(serve.funnel && serve.taken && !serve.published);
        let text = r#"{
            "Foreground": {"session": {
                "TCP": {"8443": {"HTTPS": true}},
                "Web": {"studio.example.ts.net:8443": {"Handlers": {"/": {"Proxy": "http://127.0.0.1:47310"}}}},
                "AllowFunnel": {"studio.example.ts.net:8443": true}
            }}
        }"#;
        let serve = parse_serve(text, "studio.example.ts.net", 8443, 47_310).expect("JSON");
        assert!(
            serve.funnel && serve.taken && !serve.published,
            "a terminal session is not the mapping of neohar"
        );
    }

    #[test]
    fn a_proxy_to_the_listener_has_several_forms() {
        assert!(proxies_to("http://127.0.0.1:47310", 47_310));
        assert!(proxies_to("http://localhost:47310/", 47_310));
        assert!(proxies_to("127.0.0.1:47310", 47_310));
        assert!(!proxies_to("http://127.0.0.1:47311", 47_310));
        assert!(!proxies_to("http://10.0.0.2:47310", 47_310));
        assert!(!proxies_to("https+insecure://localhost:47310", 47_310));
    }

    fn running() -> Facts {
        Facts {
            status: TailscaleStatus {
                state: TailscaleState::Running,
                name: Some("studio.example.ts.net".to_owned()),
                https: true,
                https_port: 8443,
                published: true,
                funnel: false,
            },
            port: 47_310,
            taken: false,
            program: None,
        }
    }

    #[test]
    fn a_published_listener_has_an_origin_and_no_hint() {
        let facts = running();
        assert_eq!(
            facts.origin().as_deref(),
            Some("https://studio.example.ts.net:8443")
        );
        assert_eq!(facts.hint(), None);
        assert_eq!(facts.refusal(), None);
        let mut facts = running();
        facts.status.https_port = 443;
        assert_eq!(
            facts.origin().as_deref(),
            Some("https://studio.example.ts.net")
        );
    }

    #[test]
    fn each_missing_step_has_a_hint_and_only_the_mapping_can_be_published() {
        let mut facts = running();
        facts.status.published = false;
        assert!(facts.hint().is_some_and(|hint| hint.contains("Publish")));
        assert_eq!(facts.refusal(), None, "a publish adds the mapping");
        assert!(facts.origin().is_some(), "the origin is known before");

        let mut facts = running();
        facts.status.https = false;
        facts.status.published = false;
        assert!(
            facts
                .hint()
                .is_some_and(|h| h.contains("HTTPS certificates"))
        );
        assert!(facts.refusal().is_some());
        assert_eq!(facts.origin(), None);

        let mut facts = running();
        facts.status.funnel = true;
        facts.status.published = false;
        assert!(facts.hint().is_some_and(|h| h.contains("Funnel")));
        assert!(facts.refusal().is_some());
        assert_eq!(facts.origin(), None, "a Funnel closes the listener");
        // Also when the mapping is the one of neohar.
        facts.status.published = true;
        assert_eq!(facts.origin(), None);
        assert!(facts.hint().is_some_and(|h| h.contains("Funnel")));

        let mut facts = running();
        facts.taken = true;
        facts.status.published = false;
        assert!(facts.hint().is_some_and(|h| h.contains("something else")));
        assert!(facts.refusal().is_some());

        let facts = Facts::without(TailscaleState::Missing, 8443, 47_310);
        assert!(
            facts
                .hint()
                .is_some_and(|h| h.contains("Install Tailscale"))
        );
        assert!(facts.refusal().is_some());
        assert_eq!(facts.origin(), None);
    }

    #[test]
    fn a_path_that_is_not_a_program_is_no_cli() {
        let cli = TailscaleCli::At(PathBuf::from("/nonexistent/tailscale"));
        assert_eq!(cli.find(&[]), None);
        let cli = TailscaleCli::Search;
        assert_eq!(
            cli.find(&[("PATH".to_owned(), "/nonexistent".to_owned())])
                .filter(|path| path != Path::new(APP_CLI)),
            None
        );
    }
}
