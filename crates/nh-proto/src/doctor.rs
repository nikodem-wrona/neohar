//! The checks of the doctor, as pure functions of the facts that the core gathered.
//!
//! They live here, and not in the core, because the command line runs them without a core: the
//! checks of the install need the paths and the files, not the state of a running core.
//!
//! Each function reads a fact and gives a [`DoctorCheck`]. The tests build the facts by hand, so
//! a broken symlink or a socket with a wrong mode needs no real system.

use std::path::{Path, PathBuf};

use crate::{
    api::{
        calls::system::{CheckStatus, DoctorCheck, DoctorReport},
        common::Harness,
        model::{
            HealthValue, MachineState, MachineUpdate, MachineValue, PhoneStatusValue,
            StorageHealth, TailscaleState, TailscaleStatus, UnvalidatedHarness,
        },
    },
    paths::{Env, FileKind, RootMeta, check_runtime_root},
};

/// What the symlink `~/.local/bin/nh` is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Link {
    /// This run does not manage the symlink: a test or a development run.
    NotManaged,
    Missing,
    /// It is not a symbolic link, but a file or a directory.
    NotALink,
    /// It points to `target`, and the target exists or not.
    Points {
        target: PathBuf,
        exists: bool,
    },
}

/// A file with the facts that the checks read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node {
    pub path: PathBuf,
    /// The permission bits, or `None` when the file is missing.
    pub mode: Option<u32>,
}

/// What an integration looks like on disk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Integration {
    pub harness: String,
    pub installed: bool,
    /// Codex: every hook is trusted.
    pub trusted: Option<bool>,
}

/// How a harness name resolves in the login shell of the person.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shadow {
    pub harness: String,
    /// `alias` or `function`, or `None` when nothing shadows the shim.
    pub kind: Option<String>,
}

/// What the doctor reads about sleep, for a paired phone.
#[derive(Clone, Debug, Default)]
pub struct PowerFacts {
    /// What `pmset -g` prints: the power settings in use.
    pub settings: String,
    /// What `pmset -g batt` prints: the power source and the battery.
    pub battery: String,
    /// The setting `phone.keep_awake`.
    pub keep_awake: bool,
    /// A paired phone has a push subscription.
    pub subscribed: bool,
}

/// Everything the checks read.
#[derive(Clone, Debug)]
pub struct Facts {
    pub app_version: String,
    pub nh_path: PathBuf,
    pub link: Link,
    pub uid: u32,
    pub runtime_root: PathBuf,
    /// `None` when the root does not exist.
    pub runtime_meta: Option<RootMeta>,
    pub api_socket: Node,
    /// The panes whose holder socket is missing, and how many panes have one.
    pub holders_missing: Vec<String>,
    pub holders_live: usize,
    /// `None` for the doctor of a core that does not run.
    pub health: Option<HealthValue>,
    pub integrations: Vec<Integration>,
    pub shadows: Vec<Shadow>,
    /// The links to the machines of `config.toml`, without `local`.
    pub machines: Vec<MachineValue>,
    /// The listener of the phone, when `phone.enabled` is on.
    pub phone: Option<PhoneStatusValue>,
    /// The sleep settings and the power source, with the listener of the phone.
    pub power: Option<PowerFacts>,
    pub data_dir: PathBuf,
    pub free_bytes: Option<u64>,
}

fn check(
    id: &str,
    title: &str,
    status: CheckStatus,
    detail: impl Into<String>,
    hint: Option<&str>,
) -> DoctorCheck {
    DoctorCheck {
        id: id.to_owned(),
        title: title.to_owned(),
        status,
        detail: detail.into(),
        hint: hint.map(str::to_owned),
    }
}

const MIB: u64 = 1 << 20;

pub fn install(facts: &Facts) -> Vec<DoctorCheck> {
    let mut out = vec![check(
        "versions",
        "Versions",
        CheckStatus::Info,
        format!(
            "neohar {}, nh at {}",
            facts.app_version,
            facts.nh_path.display()
        ),
        None,
    )];
    out.push(match &facts.link {
        Link::NotManaged => check(
            "nh_symlink",
            "The nh symlink",
            CheckStatus::Info,
            "this run does not manage ~/.local/bin/nh",
            None,
        ),
        Link::Missing => check(
            "nh_symlink",
            "The nh symlink",
            CheckStatus::Warn,
            "~/.local/bin/nh does not exist, so the hooks of the harnesses cannot find nh",
            Some("Start neohar.app once: it makes the symlink."),
        ),
        Link::NotALink => check(
            "nh_symlink",
            "The nh symlink",
            CheckStatus::Fail,
            "~/.local/bin/nh is a file, not a symbolic link",
            Some("Move it away and start neohar.app: it makes the symlink."),
        ),
        Link::Points {
            target,
            exists: false,
        } => check(
            "nh_symlink",
            "The nh symlink",
            CheckStatus::Fail,
            format!(
                "~/.local/bin/nh points to {}, which does not exist",
                target.display()
            ),
            Some("Start neohar.app once: it repairs the symlink."),
        ),
        Link::Points {
            target,
            exists: true,
        } if target != &facts.nh_path => check(
            "nh_symlink",
            "The nh symlink",
            CheckStatus::Warn,
            format!(
                "~/.local/bin/nh points to {}, and this core runs {}",
                target.display(),
                facts.nh_path.display()
            ),
            Some("Another copy of neohar made the symlink. Start the app that you want."),
        ),
        Link::Points { target, .. } => check(
            "nh_symlink",
            "The nh symlink",
            CheckStatus::Ok,
            format!("~/.local/bin/nh points to {}", target.display()),
            None,
        ),
    });
    out
}

pub fn runtime(facts: &Facts) -> Vec<DoctorCheck> {
    let mut out = Vec::new();
    out.push(match facts.runtime_meta {
        None => check(
            "runtime_dir",
            "The runtime directory",
            CheckStatus::Fail,
            format!("{} does not exist", facts.runtime_root.display()),
            Some("Start neohar: it makes the directory."),
        ),
        Some(meta) => match check_runtime_root(meta, facts.uid) {
            Ok(()) => check(
                "runtime_dir",
                "The runtime directory",
                CheckStatus::Ok,
                format!(
                    "{} is a directory of yours with mode 0700",
                    facts.runtime_root.display()
                ),
                None,
            ),
            Err(problem) => check(
                "runtime_dir",
                "The runtime directory",
                CheckStatus::Fail,
                format!("{}: {problem}", facts.runtime_root.display()),
                Some("Another user may have made it first. Remove it and start neohar again."),
            ),
        },
    });
    out.push(match facts.api_socket.mode {
        None => check(
            "api_socket",
            "The API socket",
            CheckStatus::Fail,
            format!("{} does not exist", facts.api_socket.path.display()),
            Some("The core is not running. Start neohar."),
        ),
        Some(mode) if mode & 0o077 != 0 => check(
            "api_socket",
            "The API socket",
            CheckStatus::Fail,
            format!(
                "{} has mode {:04o}: other users can reach it",
                facts.api_socket.path.display(),
                mode & 0o7777
            ),
            Some("Stop neohar and remove the socket."),
        ),
        Some(_) => check(
            "api_socket",
            "The API socket",
            CheckStatus::Ok,
            format!("{} is private to you", facts.api_socket.path.display()),
            None,
        ),
    });
    if facts.health.is_some() {
        out.push(if facts.holders_missing.is_empty() {
            check(
                "holder_sockets",
                "The sockets of the holders",
                CheckStatus::Ok,
                format!("{} live panes, each with its socket", facts.holders_live),
                None,
            )
        } else {
            check(
                "holder_sockets",
                "The sockets of the holders",
                CheckStatus::Warn,
                format!(
                    "the socket of the holder is missing for {}",
                    facts.holders_missing.join(", ")
                ),
                Some("Restart the pane. Its program is not reachable."),
            )
        });
    }
    out
}

fn gib(bytes: u64) -> String {
    format!("{:.1} GiB", bytes as f64 / (1u64 << 30) as f64)
}

pub fn storage(facts: &Facts) -> Vec<DoctorCheck> {
    let mut out = Vec::new();
    if let Some(health) = &facts.health {
        out.push(match &health.storage {
            StorageHealth::Ok => check(
                "storage",
                "The database",
                CheckStatus::Ok,
                "it writes",
                None,
            ),
            StorageHealth::Degraded { reason } => check(
                "storage",
                "The database",
                CheckStatus::Fail,
                format!("it is read-only or full: {reason}. Changes are not saved."),
                Some("Free disk space. The core tries to write again once a second."),
            ),
        });
    }
    out.push(match facts.free_bytes {
        None => check(
            "disk_space",
            "Disk space",
            CheckStatus::Info,
            format!("unknown for {}", facts.data_dir.display()),
            None,
        ),
        Some(bytes) if bytes < 100 * MIB => check(
            "disk_space",
            "Disk space",
            CheckStatus::Fail,
            format!("only {} free where neohar keeps its data", gib(bytes)),
            Some("Free disk space."),
        ),
        Some(bytes) if bytes < 1024 * MIB => check(
            "disk_space",
            "Disk space",
            CheckStatus::Warn,
            format!("{} free where neohar keeps its data", gib(bytes)),
            Some("Free disk space."),
        ),
        Some(bytes) => check(
            "disk_space",
            "Disk space",
            CheckStatus::Ok,
            format!("{} free", gib(bytes)),
            None,
        ),
    });
    out
}

fn harness_name(harness: &str) -> &str {
    match harness {
        "claude" => "Claude Code",
        "codex" => "Codex",
        other => other,
    }
}

pub fn harnesses(facts: &Facts) -> Vec<DoctorCheck> {
    let mut out = Vec::new();
    for integration in &facts.integrations {
        let name = harness_name(&integration.harness);
        let id = format!("integration_{}", integration.harness);
        let title = format!("The hooks of {name}");
        out.push(match (integration.installed, integration.trusted) {
            (false, _) => check(
                &id,
                &title,
                CheckStatus::Warn,
                format!("not installed: {name} is read from its screen, and the state is a guess"),
                Some(&format!(
                    "Run `nh integration install {}`.",
                    integration.harness
                )),
            ),
            (true, Some(false)) => check(
                &id,
                &title,
                CheckStatus::Warn,
                format!("installed, and {name} has not trusted every hook"),
                Some("Start the harness and confirm the hooks when it asks."),
            ),
            (true, _) => check(&id, &title, CheckStatus::Ok, "installed", None),
        });
    }
    if let Some(health) = &facts.health {
        if health.unvalidated_harnesses.is_empty() {
            out.push(check(
                "harness_versions",
                "Versions of the harnesses",
                CheckStatus::Ok,
                "every version that runs is one that neohar knows",
                None,
            ));
        }
        for UnvalidatedHarness { harness, version } in &health.unvalidated_harnesses {
            let name = harness_name(nh_agents_name(*harness));
            out.push(check(
                &format!("harness_version_{}", nh_agents_name(*harness)),
                &format!("Version of {name}"),
                CheckStatus::Warn,
                format!(
                    "{name} {version} is outside the range that neohar knows, so it only observes"
                ),
                Some("Update neohar, or run the version that neohar knows."),
            ));
        }
    }
    for shadow in &facts.shadows {
        let name = harness_name(&shadow.harness);
        out.push(match &shadow.kind {
            None => check(
                &format!("shim_{}", shadow.harness),
                &format!("The command {}", shadow.harness),
                CheckStatus::Ok,
                "the shell runs the program, not an alias or a function",
                None,
            ),
            Some(kind) => check(
                &format!("shim_{}", shadow.harness),
                &format!("The command {}", shadow.harness),
                CheckStatus::Warn,
                format!(
                    "your shell has an {kind} called {}, and it hides the shim of neohar for {name}",
                    shadow.harness
                ),
                Some("Remove the alias or function, or call the program with a full path."),
            ),
        });
    }
    out
}

fn nh_agents_name(harness: Harness) -> &'static str {
    match harness {
        Harness::Claude => "claude",
        Harness::Codex => "codex",
    }
}

pub fn core(facts: &Facts) -> Vec<DoctorCheck> {
    let Some(health) = &facts.health else {
        return Vec::new();
    };
    let mut out = Vec::new();
    out.push(match &health.renderer {
        Some(renderer) => check(
            "renderer",
            "The renderer",
            CheckStatus::Info,
            format!("{renderer:?}").to_lowercase(),
            None,
        ),
        None => check(
            "renderer",
            "The renderer",
            CheckStatus::Info,
            "no window has told which renderer it uses",
            None,
        ),
    });
    let mut protocols = health.holder_protocols.clone();
    protocols.sort_unstable();
    protocols.dedup();
    out.push(if protocols.len() > 1 {
        check(
            "holder_protocols",
            "The protocols of the holders",
            CheckStatus::Warn,
            format!("holders speak the protocols {protocols:?}"),
            Some("Restart the panes that run an old holder when you can."),
        )
    } else {
        check(
            "holder_protocols",
            "The protocols of the holders",
            CheckStatus::Ok,
            if protocols.is_empty() {
                "no holder runs".to_owned()
            } else {
                format!("every holder speaks the protocol {}", protocols[0])
            },
            None,
        )
    });
    // The lines that other parts of the core wrote: notifications, drift, ties of manifests.
    for warning in &health.warnings {
        out.push(check(
            &warning.id,
            "A warning of the core",
            CheckStatus::Warn,
            warning.text.clone(),
            warning.hint.as_deref(),
        ));
    }
    out
}

/// Whether a line of `type -a name` says that an alias or a function hides the program. The shell
/// writes `claude is an alias for …`, `claude is a shell function` or `claude is a function`.
pub fn shadow_kind(output: &str, name: &str) -> Option<String> {
    for line in output.lines() {
        let line = line.trim();
        let Some(rest) = line
            .strip_prefix(name)
            .and_then(|rest| rest.strip_prefix(" is "))
        else {
            continue;
        };
        if rest.starts_with("an alias") || rest.starts_with("aliased") {
            return Some("alias".to_owned());
        }
        if rest.starts_with("a shell function") || rest.starts_with("a function") {
            return Some("function".to_owned());
        }
    }
    None
}

/// All the checks, in the order of the report.
pub fn evaluate(facts: &Facts) -> Vec<DoctorCheck> {
    let mut out = install(facts);
    out.extend(runtime(facts));
    out.extend(storage(facts));
    out.extend(harnesses(facts));
    out.extend(core(facts));
    out.extend(machines(facts));
    out.extend(phone(facts));
    out.extend(tailscale(facts));
    out.extend(power(facts));
    out
}

/// The listener of the phone: up, or the reason that it is not. No line when
/// `phone.enabled` is off.
pub fn phone(facts: &Facts) -> Option<DoctorCheck> {
    let status = facts.phone.as_ref()?;
    let title = "The phone listener";
    Some(match (status.listening, status.port) {
        (true, Some(port)) => {
            let origin = status
                .origin
                .as_deref()
                .map_or_else(String::new, |origin| format!(" for {origin}"));
            match &status.hint {
                None => check(
                    "phone",
                    title,
                    CheckStatus::Ok,
                    format!("listens on 127.0.0.1:{port}{origin}"),
                    None,
                ),
                Some(hint) => check(
                    "phone",
                    title,
                    CheckStatus::Warn,
                    format!("listens on 127.0.0.1:{port}{origin}"),
                    Some(hint),
                ),
            }
        }
        _ => check(
            "phone",
            title,
            CheckStatus::Fail,
            "does not listen",
            status.hint.as_deref(),
        ),
    })
}

/// The lines of Tailscale for the listener of the phone: Tailscale runs, the
/// tailnet gives HTTPS certificates, the mapping of `tailscale serve` names the port, and no Funnel
/// is on the port. No line when the listener does not use Tailscale. The lines after the first
/// need a Tailscale that runs.
pub fn tailscale(facts: &Facts) -> Vec<DoctorCheck> {
    let Some(status) = facts.phone.as_ref() else {
        return Vec::new();
    };
    let Some(tailscale) = status.tailscale.as_ref() else {
        return Vec::new();
    };
    let not_running = |detail: &str, hint: &str| {
        vec![check(
            "tailscale",
            "Tailscale",
            CheckStatus::Fail,
            detail,
            Some(hint),
        )]
    };
    match tailscale.state {
        TailscaleState::Running => {}
        TailscaleState::Missing => {
            return not_running(
                "is not installed",
                "Install Tailscale, or set phone.url to the HTTPS address of this Mac.",
            );
        }
        TailscaleState::Unreachable => {
            return not_running(
                "does not answer",
                "Open Tailscale and connect it to the tailnet.",
            );
        }
        TailscaleState::Starting | TailscaleState::Stopped => {
            return not_running(
                "is not connected",
                "Open Tailscale and connect it to the tailnet.",
            );
        }
        TailscaleState::NeedsLogin => {
            return not_running("needs a login", "Open Tailscale and log in.");
        }
    }
    let name = tailscale.name.as_deref().unwrap_or("this Mac");
    let port = tailscale.https_port;
    let mut out = vec![check(
        "tailscale",
        "Tailscale",
        CheckStatus::Ok,
        format!("runs as {name}"),
        None,
    )];
    out.push(if tailscale.https {
        check(
            "tailscale_https",
            "HTTPS certificates of the tailnet",
            CheckStatus::Ok,
            format!("on for {name}"),
            None,
        )
    } else {
        check(
            "tailscale_https",
            "HTTPS certificates of the tailnet",
            CheckStatus::Fail,
            "off",
            Some("Turn on HTTPS certificates in the admin console of the tailnet."),
        )
    });
    out.push(serve_line(tailscale, status.port));
    out.push(if tailscale.funnel {
        check(
            "tailscale_funnel",
            "No Funnel on the port of the phone",
            CheckStatus::Fail,
            format!("a Funnel puts port {port} on the internet"),
            Some(&format!(
                "Run `tailscale funnel --https={port} off`. The phone listener must stay in the tailnet."
            )),
        )
    } else {
        check(
            "tailscale_funnel",
            "No Funnel on the port of the phone",
            CheckStatus::Ok,
            format!("no Funnel on port {port}"),
            None,
        )
    });
    out
}

fn serve_line(tailscale: &TailscaleStatus, listener: Option<u16>) -> DoctorCheck {
    let port = tailscale.https_port;
    let target = listener.map_or_else(
        || "the listener".to_owned(),
        |listener| format!("127.0.0.1:{listener}"),
    );
    if tailscale.published {
        check(
            "tailscale_serve",
            "The mapping of tailscale serve",
            CheckStatus::Ok,
            format!("port {port} maps to {target}"),
            None,
        )
    } else {
        check(
            "tailscale_serve",
            "The mapping of tailscale serve",
            CheckStatus::Warn,
            format!("port {port} does not map to {target}"),
            Some(
                "Click \"Publish with Tailscale\" in the section Phone of the settings, or run `nh phone publish`.",
            ),
        )
    }
}

/// The minutes of a setting in the first section of `pmset -g`, for example `sleep` or
/// `displaysleep`. 0 means never. `None` when the output does not name it.
pub fn pmset_minutes(settings: &str, name: &str) -> Option<u32> {
    settings.lines().find_map(|line| {
        let mut words = line.split_whitespace();
        (words.next() == Some(name))
            .then(|| words.next()?.parse().ok())
            .flatten()
    })
}

/// What `pmset -g batt` says about the power.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PowerSource {
    /// The source in use, for example `AC Power` or `Battery Power`.
    pub source: Option<String>,
    /// The machine has an internal battery.
    pub has_battery: bool,
    /// The charge of the battery, in percent.
    pub charge: Option<u8>,
}

impl PowerSource {
    /// The Mac runs on its battery now.
    pub fn on_battery(&self) -> bool {
        self.source.as_deref() == Some("Battery Power")
    }
}

/// Reads `pmset -g batt`: `Now drawing from 'AC Power'`, then a line for each battery.
pub fn power_source(battery: &str) -> PowerSource {
    let source = battery.lines().find_map(|line| {
        let rest = line.trim().strip_prefix("Now drawing from '")?;
        Some(rest.split('\'').next()?.to_owned())
    });
    let line = battery
        .lines()
        .find(|line| line.contains("InternalBattery"));
    let charge = line.and_then(|line| {
        line.split_whitespace()
            .find_map(|word| word.strip_suffix("%;")?.parse().ok())
    });
    PowerSource {
        source,
        has_battery: line.is_some(),
        charge,
    }
}

fn minutes(value: u32) -> String {
    if value == 1 {
        "1 minute".to_owned()
    } else {
        format!("{value} minutes")
    }
}

/// The lines of sleep for a paired phone: when the Mac sleeps, what neohar does
/// against it, the power source, and the lid of a MacBook. No line without the listener of the
/// phone, or when `pmset` said nothing.
pub fn power(facts: &Facts) -> Vec<DoctorCheck> {
    let Some(power) = facts.power.as_ref() else {
        return Vec::new();
    };
    let Some(idle) = pmset_minutes(&power.settings, "sleep") else {
        return Vec::new();
    };
    let display = match pmset_minutes(&power.settings, "displaysleep") {
        Some(0) => " The display does not sleep.".to_owned(),
        Some(value) => format!(" The display sleeps after {}.", minutes(value)),
        None => String::new(),
    };
    let mut out = vec![if idle == 0 {
        check(
            "sleep",
            "Sleep",
            CheckStatus::Ok,
            format!("The Mac does not sleep when it is idle.{display}"),
            None,
        )
    } else if !power.keep_awake {
        check(
            "sleep",
            "Sleep",
            CheckStatus::Warn,
            format!(
                "The Mac sleeps after {} without use, and phone.keep_awake is off. The agents \
                 stop while the Mac sleeps.{display}",
                minutes(idle)
            ),
            Some("Set phone.keep_awake to true."),
        )
    } else if power.subscribed {
        check(
            "sleep",
            "Sleep",
            CheckStatus::Ok,
            format!(
                "The Mac sleeps after {} without use. neohar keeps it awake while an agent \
                 works, while a gate holds a hook, and for 2 hours after a push.{display}",
                minutes(idle)
            ),
            None,
        )
    } else {
        check(
            "sleep",
            "Sleep",
            CheckStatus::Info,
            format!(
                "The Mac sleeps after {} without use. neohar keeps it awake only for a phone \
                 with Notifications on.{display}",
                minutes(idle)
            ),
            None,
        )
    }];
    let source = power_source(&power.battery);
    if let Some(name) = &source.source {
        let detail = match (source.on_battery(), source.charge) {
            (true, Some(charge)) => format!("battery, {charge}%"),
            (true, None) => "battery".to_owned(),
            (false, _) if name == "AC Power" => "AC power".to_owned(),
            (false, _) => name.clone(),
        };
        out.push(check(
            "power_source",
            "Power source",
            CheckStatus::Info,
            detail,
            None,
        ));
    }
    if source.has_battery {
        let detail = "A MacBook sleeps when its lid closes on battery. While it sleeps, the agents \
                      stop and no push goes.";
        out.push(if source.on_battery() {
            check(
                "lid",
                "Closed lid",
                CheckStatus::Warn,
                detail,
                Some("Keep the lid open, or connect the power."),
            )
        } else {
            check("lid", "Closed lid", CheckStatus::Info, detail, None)
        });
    }
    out
}

/// One line for each machine of `config.toml`: the state of its link, the version of its bridge
/// and the round-trip time. The doctor of the remote core has its own checks.
pub fn machines(facts: &Facts) -> Vec<DoctorCheck> {
    facts
        .machines
        .iter()
        .map(|machine| {
            let id = format!("machine:{}", machine.label);
            let title = format!("The link to {}", machine.label);
            let version = machine
                .version
                .as_deref()
                .map_or(String::new(), |v| format!(", bridge {v}"));
            match machine.state {
                MachineState::Connected => {
                    let time = machine
                        .latency_ms
                        .map_or(String::new(), |ms| format!(", {ms:.1} ms"));
                    let note = match machine.update {
                        Some(MachineUpdate::UpdateAvailable) => Some(
                            "The neohar on that machine is older than this app. Update it with `nh machine install`.",
                        ),
                        Some(MachineUpdate::UpdateApp) => {
                            Some("The neohar on that machine is newer than this app. Update this app.")
                        }
                        _ => None,
                    };
                    let status = if note.is_some() {
                        CheckStatus::Warn
                    } else {
                        CheckStatus::Ok
                    };
                    check(&id, &title, status, format!("connected{version}{time}"), note)
                }
                MachineState::Connecting => {
                    check(&id, &title, CheckStatus::Info, "connecting", None)
                }
                _ => match &machine.reason {
                    None => check(
                        &id,
                        &title,
                        CheckStatus::Info,
                        "not connected",
                        Some(&format!("Run `nh machine connect {}`.", machine.label)),
                    ),
                    Some(reason) => check(
                        &id,
                        &title,
                        CheckStatus::Warn,
                        format!("disconnected: {reason}"),
                        Some(&format!("Run `nh machine connect {}`.", machine.label)),
                    ),
                },
            }
        })
        .collect()
}

fn link_of(paths: &Env) -> Link {
    if paths.has_overrides() {
        return Link::NotManaged;
    }
    let Ok(path) = paths.nh_symlink() else {
        return Link::Missing;
    };
    match std::fs::symlink_metadata(&path) {
        Err(_) => Link::Missing,
        Ok(meta) if meta.file_type().is_symlink() => Link::Points {
            target: std::fs::read_link(&path).unwrap_or_default(),
            exists: std::fs::metadata(&path).is_ok(),
        },
        Ok(_) => Link::NotALink,
    }
}

fn runtime_meta(root: &Path) -> Option<RootMeta> {
    use std::os::unix::fs::MetadataExt;
    let meta = std::fs::symlink_metadata(root).ok()?;
    let kind = match meta.file_type() {
        t if t.is_symlink() => FileKind::Symlink,
        t if t.is_dir() => FileKind::Directory,
        _ => FileKind::Other,
    };
    Some(RootMeta {
        kind,
        uid: meta.uid(),
        mode: meta.mode(),
    })
}

/// The facts that need no core.
pub fn static_facts(paths: &Env, session: &str, nh_path: &Path) -> Facts {
    let runtime_root = paths.runtime_root().unwrap_or_default();
    let api_socket = paths.api_socket(session).unwrap_or_default();
    let data_dir = paths.data_dir(session).unwrap_or_default();
    Facts {
        app_version: env!("CARGO_PKG_VERSION").to_owned(),
        nh_path: nh_path.to_owned(),
        link: link_of(paths),
        uid: paths.uid,
        runtime_meta: runtime_meta(&runtime_root),
        runtime_root,
        api_socket: node(&api_socket),
        holders_missing: Vec::new(),
        holders_live: 0,
        health: None,
        integrations: Vec::new(),
        shadows: Vec::new(),
        machines: Vec::new(),
        phone: None,
        power: None,
        free_bytes: free_bytes(&data_dir),
        data_dir,
    }
}

/// The free space on the disk of a directory, from `df`. `None` when the directory or the
/// program is not there.
pub fn free_bytes(dir: &Path) -> Option<u64> {
    // A directory that does not exist yet is on the disk of its parent.
    let mut at = dir;
    while !at.exists() {
        at = at.parent()?;
    }
    let output = std::process::Command::new("df")
        .arg("-Pk")
        .arg(at)
        .output()
        .ok()?;
    let text = String::from_utf8(output.stdout).ok()?;
    let line = text.lines().nth(1)?;
    let available: u64 = line.split_whitespace().nth(3)?.parse().ok()?;
    Some(available.saturating_mul(1024))
}

/// The report of an install whose core does not run: the checks that need no core.
pub fn offline(paths: &Env, session: &str, nh_path: &Path) -> DoctorReport {
    DoctorReport {
        checks: evaluate(&static_facts(paths, session, nh_path)),
    }
}

/// The path that a file has on disk, as the doctor reads it.
pub fn node(path: &Path) -> Node {
    use std::os::unix::fs::PermissionsExt;
    Node {
        path: path.to_owned(),
        mode: std::fs::symlink_metadata(path)
            .ok()
            .map(|meta| meta.permissions().mode()),
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;
    use crate::{
        api::model::HealthValue,
        paths::{Env, Platform},
    };

    fn facts() -> Facts {
        Facts {
            app_version: "0.1.0".to_owned(),
            nh_path: PathBuf::from("/Applications/neohar.app/Contents/MacOS/nh"),
            link: Link::Points {
                target: PathBuf::from("/Applications/neohar.app/Contents/MacOS/nh"),
                exists: true,
            },
            uid: 501,
            runtime_root: PathBuf::from("/tmp/neohar-501"),
            runtime_meta: Some(RootMeta {
                kind: FileKind::Directory,
                uid: 501,
                mode: 0o040_700,
            }),
            api_socket: Node {
                path: PathBuf::from("/tmp/neohar-501/default/api.sock"),
                mode: Some(0o140_600),
            },
            holders_missing: Vec::new(),
            holders_live: 2,
            health: Some(HealthValue {
                storage: StorageHealth::Ok,
                renderer: None,
                holder_protocols: vec![2],
                unvalidated_harnesses: Vec::new(),
                warnings: Vec::new(),
            }),
            integrations: vec![Integration {
                harness: "claude".to_owned(),
                installed: true,
                trusted: None,
            }],
            shadows: vec![Shadow {
                harness: "claude".to_owned(),
                kind: None,
            }],
            machines: Vec::new(),
            phone: None,
            power: None,
            data_dir: PathBuf::from("/data"),
            free_bytes: Some(50 << 30),
        }
    }

    fn machine(label: &str, state: MachineState) -> MachineValue {
        MachineValue {
            label: label.to_owned(),
            state,
            latency_ms: None,
            version: None,
            reason: None,
            forwards: Vec::new(),
            harnesses: Vec::new(),
            update: None,
        }
    }

    #[test]
    fn each_machine_has_a_line_with_its_state_version_and_round_trip_time() {
        let mut f = facts();
        let mut up = machine("studio", MachineState::Connected);
        up.version = Some("0.4.0".to_owned());
        up.latency_ms = Some(14.25);
        let mut old = machine("old", MachineState::Connected);
        old.update = Some(MachineUpdate::UpdateAvailable);
        let mut down = machine("down", MachineState::Disconnected);
        down.reason = Some("Host key verification failed.".to_owned());
        f.machines = vec![
            up,
            old,
            down,
            machine("idle", MachineState::Disconnected),
            machine("wait", MachineState::Connecting),
        ];
        let checks = evaluate(&f);
        let line = |id: &str| checks.iter().find(|c| c.id == id).expect(id);
        assert_eq!(line("machine:studio").status, CheckStatus::Ok);
        assert_eq!(
            line("machine:studio").detail,
            "connected, bridge 0.4.0, 14.2 ms"
        );
        assert_eq!(line("machine:old").status, CheckStatus::Warn);
        assert!(line("machine:old").hint.is_some());
        assert_eq!(line("machine:down").status, CheckStatus::Warn);
        assert!(
            line("machine:down")
                .detail
                .contains("Host key verification failed.")
        );
        assert_eq!(line("machine:idle").status, CheckStatus::Info);
        assert_eq!(line("machine:wait").status, CheckStatus::Info);
        assert_eq!(
            line("machine:down").hint.as_deref(),
            Some("Run `nh machine connect down`.")
        );
    }

    fn status_of(checks: &[DoctorCheck], id: &str) -> CheckStatus {
        checks
            .iter()
            .find(|c| c.id == id)
            .unwrap_or_else(|| panic!("no check {id}"))
            .status
    }

    #[test]
    fn a_healthy_install_has_no_warning() {
        let checks = evaluate(&facts());
        let bad: Vec<_> = checks
            .iter()
            .filter(|c| matches!(c.status, CheckStatus::Warn | CheckStatus::Fail))
            .map(|c| c.id.as_str())
            .collect();
        assert!(bad.is_empty(), "{bad:?}");
    }

    #[test]
    fn the_symlink_is_missing_broken_or_somebody_elses() {
        let mut f = facts();
        f.link = Link::Missing;
        assert_eq!(status_of(&evaluate(&f), "nh_symlink"), CheckStatus::Warn);
        f.link = Link::Points {
            target: PathBuf::from("/gone/nh"),
            exists: false,
        };
        assert_eq!(status_of(&evaluate(&f), "nh_symlink"), CheckStatus::Fail);
        f.link = Link::Points {
            target: PathBuf::from("/other/nh"),
            exists: true,
        };
        assert_eq!(status_of(&evaluate(&f), "nh_symlink"), CheckStatus::Warn);
        f.link = Link::NotALink;
        assert_eq!(status_of(&evaluate(&f), "nh_symlink"), CheckStatus::Fail);
        f.link = Link::NotManaged;
        assert_eq!(status_of(&evaluate(&f), "nh_symlink"), CheckStatus::Info);
    }

    #[test]
    fn a_runtime_directory_of_another_user_or_with_a_wide_mode_fails() {
        let mut f = facts();
        f.runtime_meta = Some(RootMeta {
            kind: FileKind::Directory,
            uid: 501,
            mode: 0o040_755,
        });
        let checks = evaluate(&f);
        assert_eq!(status_of(&checks, "runtime_dir"), CheckStatus::Fail);
        let detail = &checks
            .iter()
            .find(|c| c.id == "runtime_dir")
            .unwrap()
            .detail;
        assert!(detail.contains("0755"), "{detail}");
        f.runtime_meta = Some(RootMeta {
            kind: FileKind::Symlink,
            uid: 501,
            mode: 0o120_777,
        });
        assert_eq!(status_of(&evaluate(&f), "runtime_dir"), CheckStatus::Fail);
        f.runtime_meta = None;
        assert_eq!(status_of(&evaluate(&f), "runtime_dir"), CheckStatus::Fail);
    }

    #[test]
    fn a_socket_that_others_can_reach_fails() {
        let mut f = facts();
        f.api_socket.mode = Some(0o140_666);
        assert_eq!(status_of(&evaluate(&f), "api_socket"), CheckStatus::Fail);
        f.api_socket.mode = None;
        assert_eq!(status_of(&evaluate(&f), "api_socket"), CheckStatus::Fail);
    }

    #[test]
    fn a_holder_without_a_socket_is_a_warning() {
        let mut f = facts();
        f.holders_missing = vec!["01M4JVZ55PZMR7HXB3HR118VVQ".to_owned()];
        assert_eq!(
            status_of(&evaluate(&f), "holder_sockets"),
            CheckStatus::Warn
        );
    }

    #[test]
    fn the_phone_listener_has_a_line_only_when_it_is_on() {
        let mut f = facts();
        assert!(evaluate(&f).iter().all(|c| c.id != "phone"));
        f.phone = Some(PhoneStatusValue {
            listening: true,
            port: Some(47_310),
            origin: Some("https://mac.example.ts.net:8443".to_owned()),
            hint: None,
            summary: String::new(),
            tailscale: None,
            vapid_key: None,
        });
        assert_eq!(status_of(&evaluate(&f), "phone"), CheckStatus::Ok);
        assert!(
            evaluate(&f).iter().all(|c| !c.id.starts_with("tailscale")),
            "no Tailscale: no line of it"
        );
        f.phone = Some(PhoneStatusValue {
            listening: false,
            port: None,
            origin: None,
            hint: Some("Port 47310 is busy.".to_owned()),
            summary: String::new(),
            tailscale: None,
            vapid_key: None,
        });
        let checks = evaluate(&f);
        assert_eq!(status_of(&checks, "phone"), CheckStatus::Fail);
        let line = checks.iter().find(|c| c.id == "phone").expect("a line");
        assert_eq!(line.hint.as_deref(), Some("Port 47310 is busy."));
    }

    fn with_tailscale(tailscale: TailscaleStatus) -> Facts {
        let mut f = facts();
        f.phone = Some(PhoneStatusValue {
            listening: true,
            port: Some(47_310),
            origin: Some("https://mac.example.ts.net:8443".to_owned()),
            hint: None,
            summary: String::new(),
            tailscale: Some(tailscale),
            vapid_key: None,
        });
        f
    }

    fn running() -> TailscaleStatus {
        TailscaleStatus {
            state: TailscaleState::Running,
            name: Some("mac.example.ts.net".to_owned()),
            https: true,
            https_port: 8443,
            published: true,
            funnel: false,
        }
    }

    #[test]
    fn tailscale_that_runs_and_maps_the_port_has_four_good_lines() {
        let checks = evaluate(&with_tailscale(running()));
        for id in [
            "tailscale",
            "tailscale_https",
            "tailscale_serve",
            "tailscale_funnel",
        ] {
            assert_eq!(status_of(&checks, id), CheckStatus::Ok, "{id}");
        }
        let serve = checks.iter().find(|c| c.id == "tailscale_serve").unwrap();
        assert_eq!(serve.detail, "port 8443 maps to 127.0.0.1:47310");
    }

    #[test]
    fn tailscale_without_certificates_a_mapping_or_with_a_funnel_has_bad_lines() {
        let checks = evaluate(&with_tailscale(TailscaleStatus {
            https: false,
            published: false,
            funnel: true,
            ..running()
        }));
        assert_eq!(status_of(&checks, "tailscale"), CheckStatus::Ok);
        assert_eq!(status_of(&checks, "tailscale_https"), CheckStatus::Fail);
        assert_eq!(status_of(&checks, "tailscale_serve"), CheckStatus::Warn);
        assert_eq!(status_of(&checks, "tailscale_funnel"), CheckStatus::Fail);
        let https = checks.iter().find(|c| c.id == "tailscale_https").unwrap();
        assert!(
            https
                .hint
                .as_deref()
                .is_some_and(|hint| hint.contains("HTTPS certificates"))
        );
    }

    #[test]
    fn tailscale_that_does_not_run_has_one_failed_line() {
        for state in [
            TailscaleState::Missing,
            TailscaleState::Unreachable,
            TailscaleState::Starting,
            TailscaleState::Stopped,
            TailscaleState::NeedsLogin,
        ] {
            let checks = evaluate(&with_tailscale(TailscaleStatus {
                state,
                name: None,
                ..running()
            }));
            assert_eq!(
                status_of(&checks, "tailscale"),
                CheckStatus::Fail,
                "{state:?}"
            );
            assert!(
                checks
                    .iter()
                    .all(|c| c.id == "tailscale" || !c.id.starts_with("tailscale")),
                "{state:?}"
            );
        }
    }

    #[test]
    fn a_degraded_database_and_a_full_disk_fail() {
        let mut f = facts();
        f.health.as_mut().unwrap().storage = StorageHealth::Degraded {
            reason: "disk full".to_owned(),
        };
        f.free_bytes = Some(10 << 20);
        let checks = evaluate(&f);
        assert_eq!(status_of(&checks, "storage"), CheckStatus::Fail);
        assert_eq!(status_of(&checks, "disk_space"), CheckStatus::Fail);
        f.free_bytes = Some(500 << 20);
        assert_eq!(status_of(&evaluate(&f), "disk_space"), CheckStatus::Warn);
    }

    #[test]
    fn integrations_versions_and_shadows_show_what_is_wrong() {
        let mut f = facts();
        f.integrations = vec![
            Integration {
                harness: "claude".to_owned(),
                installed: false,
                trusted: None,
            },
            Integration {
                harness: "codex".to_owned(),
                installed: true,
                trusted: Some(false),
            },
        ];
        f.health.as_mut().unwrap().unvalidated_harnesses = vec![UnvalidatedHarness {
            harness: Harness::Claude,
            version: "2.2.0".to_owned(),
        }];
        f.shadows[0].kind = Some("alias".to_owned());
        let checks = evaluate(&f);
        assert_eq!(status_of(&checks, "integration_claude"), CheckStatus::Warn);
        assert_eq!(status_of(&checks, "integration_codex"), CheckStatus::Warn);
        assert_eq!(
            status_of(&checks, "harness_version_claude"),
            CheckStatus::Warn
        );
        assert_eq!(status_of(&checks, "shim_claude"), CheckStatus::Warn);
    }

    #[test]
    fn the_warnings_of_the_core_become_checks() {
        let mut f = facts();
        f.health.as_mut().unwrap().warnings = vec![crate::api::model::DoctorWarning {
            id: "drift:claude:2.2.0".to_owned(),
            text: "Claude Code 2.2.0 behaves differently than neohar expects.".to_owned(),
            hint: Some("Run `nh integration doctor claude`.".to_owned()),
        }];
        let checks = evaluate(&f);
        let found = checks
            .iter()
            .find(|c| c.id == "drift:claude:2.2.0")
            .unwrap();
        assert_eq!(found.status, CheckStatus::Warn);
        assert!(
            found
                .hint
                .as_deref()
                .unwrap()
                .contains("integration doctor")
        );
    }

    #[test]
    fn the_shell_says_how_an_alias_or_a_function_hides_a_program() {
        let zsh = "claude is an alias for claude --verbose\nclaude is /opt/bin/claude\n";
        let bash = "claude is aliased to `claude --verbose'\n";
        let function = "claude is a shell function from /home/ada/.zshrc\n";
        let plain = "claude is /home/ada/.local/bin/claude\n";
        assert_eq!(shadow_kind(zsh, "claude").as_deref(), Some("alias"));
        assert_eq!(shadow_kind(bash, "claude").as_deref(), Some("alias"));
        assert_eq!(shadow_kind(function, "claude").as_deref(), Some("function"));
        assert_eq!(shadow_kind(plain, "claude"), None);
        assert_eq!(shadow_kind(zsh, "codex"), None);
    }

    #[test]
    fn an_install_without_a_core_is_checked_from_its_directories() {
        let dir = std::env::temp_dir().join(format!("nh-doctor-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("run")).expect("a directory");
        // A runtime directory that others can read.
        std::fs::set_permissions(dir.join("run"), std::fs::Permissions::from_mode(0o755))
            .expect("a mode");
        let paths = Env {
            platform: Platform::current(),
            uid: rustix::process::getuid().as_raw(),
            home: Some(dir.join("home")),
            config_override: Some(dir.join("config")),
            data_override: Some(dir.join("data")),
            runtime_override: Some(dir.join("run")),
            xdg_config_home: None,
            xdg_data_home: None,
            xdg_runtime_dir: None,
        };
        let report = offline(&paths, "default", Path::new("/bin/nh"));
        let status = |id: &str| status_of(&report.checks, id);
        assert_eq!(
            status("nh_symlink"),
            CheckStatus::Info,
            "overrides: not managed"
        );
        assert_eq!(status("runtime_dir"), CheckStatus::Fail, "mode 0755");
        assert_eq!(status("api_socket"), CheckStatus::Fail, "nothing listens");
        assert_eq!(status("disk_space"), CheckStatus::Ok);
        // Without a core there is nothing to say about holders or storage.
        assert!(report.checks.iter().all(|c| c.id != "holder_sockets"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `pmset -g` of a MacBook with the settings in use, as macOS 26 prints them.
    const PMSET: &str = "System-wide power settings:
Currently in use:
 standby              1
 Sleep On Power Button 1
 hibernatefile        /var/vm/sleepimage
 powernap             1
 disksleep            10
 sleep                1 (sleep prevented by powerd, caffeinate)
 hibernatemode        3
 displaysleep         0
 tcpkeepalive         1
";

    const ON_AC: &str = "Now drawing from 'AC Power'
 -InternalBattery-0 (id=1234567)\t100%; charged; 0:00 remaining present: true
";

    const ON_BATTERY: &str = "Now drawing from 'Battery Power'
 -InternalBattery-0 (id=1234567)\t76%; discharging; 5:12 remaining present: true
";

    fn with_power(power: PowerFacts) -> Vec<DoctorCheck> {
        let mut facts = facts();
        facts.power = Some(power);
        evaluate(&facts)
    }

    fn line<'a>(checks: &'a [DoctorCheck], id: &str) -> &'a DoctorCheck {
        checks
            .iter()
            .find(|c| c.id == id)
            .unwrap_or_else(|| panic!("no line {id}: {checks:?}"))
    }

    #[test]
    fn pmset_gives_the_minutes_of_a_setting_and_the_source_of_the_power() {
        assert_eq!(pmset_minutes(PMSET, "sleep"), Some(1));
        assert_eq!(pmset_minutes(PMSET, "displaysleep"), Some(0));
        assert_eq!(pmset_minutes(PMSET, "disksleep"), Some(10));
        assert_eq!(pmset_minutes("", "sleep"), None);
        assert_eq!(
            power_source(ON_BATTERY),
            PowerSource {
                source: Some("Battery Power".to_owned()),
                has_battery: true,
                charge: Some(76),
            }
        );
        assert!(!power_source(ON_AC).on_battery());
        // A Mac without a battery names only the source.
        let desktop = power_source("Now drawing from 'AC Power'\n");
        assert!(!desktop.has_battery);
        assert_eq!(desktop.source.as_deref(), Some("AC Power"));
    }

    #[test]
    fn the_sleep_line_says_what_keeps_the_mac_awake() {
        let power = |keep_awake, subscribed| PowerFacts {
            settings: PMSET.to_owned(),
            battery: ON_AC.to_owned(),
            keep_awake,
            subscribed,
        };
        let checks = with_power(power(true, true));
        let sleep = line(&checks, "sleep");
        assert_eq!(sleep.status, CheckStatus::Ok);
        assert!(sleep.detail.contains("after 1 minute"), "{}", sleep.detail);
        assert!(
            sleep.detail.contains("2 hours after a push"),
            "{}",
            sleep.detail
        );
        assert!(
            sleep.detail.contains("display does not sleep"),
            "{}",
            sleep.detail
        );
        assert_eq!(line(&checks, "power_source").detail, "AC power");
        assert_eq!(line(&checks, "lid").status, CheckStatus::Info);
        // No phone with Notifications: neohar does not hold the Mac awake.
        assert_eq!(
            line(&with_power(power(true, false)), "sleep").status,
            CheckStatus::Info
        );
        // The setting is off: agents stop while the Mac sleeps.
        let off = with_power(power(false, true));
        assert_eq!(line(&off, "sleep").status, CheckStatus::Warn);
        assert!(line(&off, "sleep").hint.is_some());
    }

    #[test]
    fn a_macbook_on_battery_gets_a_warning_for_the_closed_lid() {
        let checks = with_power(PowerFacts {
            settings: PMSET.replace("sleep                1", "sleep                0"),
            battery: ON_BATTERY.to_owned(),
            keep_awake: true,
            subscribed: true,
        });
        assert_eq!(line(&checks, "sleep").status, CheckStatus::Ok);
        assert!(
            line(&checks, "sleep")
                .detail
                .starts_with("The Mac does not sleep")
        );
        assert_eq!(line(&checks, "power_source").detail, "battery, 76%");
        assert_eq!(line(&checks, "lid").status, CheckStatus::Warn);
    }

    #[test]
    fn no_power_line_without_the_phone_or_without_pmset() {
        assert!(evaluate(&facts()).iter().all(|c| c.id != "sleep"));
        let checks = with_power(PowerFacts::default());
        assert!(checks.iter().all(|c| c.id != "sleep" && c.id != "lid"));
    }
}
