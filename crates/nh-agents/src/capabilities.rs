//! The capability resolver.
//!
//! `run.capabilities` is the answer to one question: what may neohar do with this run? It comes
//! from four things and only from them:
//!
//! - the ranges of the manifest that name the version and the mode,
//! - the rows of the golden coverage (`golden-coverage.toml`) that name the same,
//! - the transports that the run uses (the hooks give `gate` and the native answers),
//! - the binding of the run: a run that is not bound gets no gate, no answer and no key.
//!   A guess about identity never authorizes.
//!
//! A version outside the validated ranges gets `observe`, and `answer_keystroke` while the screen
//! manifest still validates on the live screen. The UI offers only what the resolver lists.

use nh_proto::api::{
    common::Harness,
    model::{Capabilities, InteractionKind},
};
use semver::{Version, VersionReq};
use serde::Deserialize;

use crate::{
    harness::name,
    manifest::{Manifest, schema::CapabilityRange},
};

/// The text of the file `golden-coverage.toml`, compiled in.
const BUILTIN: &str = include_str!("../golden-coverage.toml");

/// The mode of a run. A `headless` mode does not exist yet.
pub const TUI: &str = "tui";

#[derive(Debug, Default, Deserialize)]
struct File {
    #[serde(default)]
    covered: Vec<Covered>,
}

/// A row of the coverage: what the recordings of one range of versions show.
#[derive(Clone, Debug, Deserialize)]
pub struct Covered {
    pub harness: String,
    #[serde(default = "tui")]
    pub mode: String,
    pub versions: String,
    #[serde(default)]
    pub recorded: Vec<String>,
    #[serde(default)]
    pub observe: bool,
    #[serde(default)]
    pub gate: bool,
    #[serde(default)]
    pub answer_native: Vec<String>,
    #[serde(default)]
    pub answer_keystroke: bool,
    #[serde(default)]
    pub resume: bool,
    #[serde(default)]
    pub survive_disconnect: bool,
}

fn tui() -> String {
    TUI.to_owned()
}

/// The rows of the golden coverage.
#[derive(Clone, Debug, Default)]
pub struct Coverage {
    rows: Vec<Covered>,
}

impl Coverage {
    /// The coverage that is compiled in.
    pub fn builtin() -> Self {
        Self::parse(BUILTIN).unwrap_or_default()
    }

    pub fn parse(text: &str) -> Result<Self, toml::de::Error> {
        let file: File = toml::from_str(text)?;
        Ok(Self { rows: file.covered })
    }

    pub fn rows(&self) -> &[Covered] {
        &self.rows
    }

    fn find(&self, harness: Harness, mode: &str, version: &Version) -> Option<&Covered> {
        self.rows.iter().find(|row| {
            row.harness == name(harness)
                && row.mode == mode
                && VersionReq::parse(&row.versions).is_ok_and(|req| req.matches(version))
        })
    }
}

/// The transports that a run uses now. Only the hooks matter to the capabilities.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Transports {
    pub hooks: bool,
}

/// What the resolver needs to know about a run.
#[derive(Clone, Copy, Debug)]
pub struct Context<'a> {
    pub manifest: &'a Manifest,
    pub harness: Harness,
    /// The version that the probe found. `None` when it did not run, or it printed nothing.
    pub version: Option<&'a Version>,
    pub mode: &'a str,
    pub transports: Transports,
    /// Neohar launched the run, a signal carried the token of the pane, or the user linked it.
    pub bound: bool,
    /// The rules of the screen manifest match the live screen.
    pub screen_validates: bool,
}

/// Why a version has no validated capabilities.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Unvalidated {
    /// No version is known.
    NoVersion,
    /// The version is below `min_version`.
    BelowMinimum,
    /// No range of the manifest names this version and mode.
    OutsideRanges,
    /// The manifest names the version, but no golden recording covers it.
    NotCovered,
}

impl Unvalidated {
    /// A short text for the doctor.
    pub fn text(&self) -> &'static str {
        match self {
            Self::NoVersion => "the version is not known",
            Self::BelowMinimum => "the version is below the minimum of the manifest",
            Self::OutsideRanges => "no range of the manifest names this version",
            Self::NotCovered => "no golden recording covers this version",
        }
    }
}

/// The result.
#[derive(Clone, Debug, PartialEq)]
pub struct Resolved {
    pub capabilities: Capabilities,
    /// `None` when the version is validated.
    pub unvalidated: Option<Unvalidated>,
}

fn kinds(names: &[String]) -> Vec<InteractionKind> {
    let mut out = Vec::new();
    for text in names {
        let kind = match text.as_str() {
            "approval" => InteractionKind::Approval,
            "question" => InteractionKind::Question,
            "plan_review" => InteractionKind::PlanReview,
            _ => continue,
        };
        if !out.contains(&kind) {
            out.push(kind);
        }
    }
    out
}

fn range_for<'a>(
    manifest: &'a Manifest,
    mode: &str,
    version: &Version,
) -> Option<&'a CapabilityRange> {
    manifest.capabilities.iter().find(|range| {
        let range_mode = if range.mode.is_empty() {
            TUI
        } else {
            &range.mode
        };
        range_mode == mode
            && VersionReq::parse(&range.versions).is_ok_and(|req| req.matches(version))
    })
}

/// Resolves the capabilities of a run.
pub fn resolve(context: &Context<'_>, coverage: &Coverage) -> Resolved {
    let (granted, unvalidated) = granted(context, coverage);
    let mut capabilities = granted;
    if !context.bound {
        // A suggested or unlinked run is never answered, never gated and never typed to.
        capabilities.gate = false;
        capabilities.answer_native.clear();
        capabilities.answer_keystroke = false;
        capabilities.steer = false;
    }
    Resolved {
        capabilities,
        unvalidated,
    }
}

fn granted(context: &Context<'_>, coverage: &Coverage) -> (Capabilities, Option<Unvalidated>) {
    let observe_only = |reason| {
        (
            Capabilities {
                observe: true,
                gate: false,
                answer_native: Vec::new(),
                answer_keystroke: context.screen_validates,
                reconcile: false,
                resume: false,
                steer: false,
                survive_disconnect: false,
            },
            Some(reason),
        )
    };
    let Some(version) = context.version else {
        return observe_only(Unvalidated::NoVersion);
    };
    if let Some(minimum) = context
        .manifest
        .min_version
        .as_deref()
        .and_then(|text| crate::manifest::validate::parse_version(text).ok())
        && *version < minimum
    {
        return observe_only(Unvalidated::BelowMinimum);
    }
    let Some(range) = range_for(context.manifest, context.mode, version) else {
        return observe_only(Unvalidated::OutsideRanges);
    };
    let Some(covered) = coverage.find(context.harness, context.mode, version) else {
        return observe_only(Unvalidated::NotCovered);
    };
    let both = |a: bool, b: bool| a && b;
    let hooks = context.transports.hooks;
    let native: Vec<InteractionKind> = kinds(&range.answer_native)
        .into_iter()
        .filter(|kind| kinds(&covered.answer_native).contains(kind))
        .collect();
    (
        Capabilities {
            observe: both(range.observe, covered.observe),
            gate: both(range.gate, covered.gate) && hooks,
            answer_native: if hooks { native } else { Vec::new() },
            answer_keystroke: both(range.answer_keystroke, covered.answer_keystroke),
            reconcile: false,
            resume: both(range.resume, covered.resume),
            steer: false,
            survive_disconnect: both(range.survive_disconnect, covered.survive_disconnect),
        },
        None,
    )
}

#[cfg(test)]
mod tests;
