//! The language of the screen manifests: the text of a manifest, and the rules that
//! the detector runs. A rule is compiled once: its regexes are built (Rust `regex`, no
//! backtracking) and its regions are read.

use std::collections::BTreeMap;
use std::ops::RangeInclusive;

use nh_proto::api::model::InteractionKind;
use regex::Regex;
use semver::{Version, VersionReq};
use serde::Deserialize;

/// The names of the steps of `normalize`. `strip_sgr_except_fg` and `nfc` are accepted and do
/// nothing: the engine delivers cells, not escape sequences, and it delivers composed text.
const NORMALIZE_STEPS: &[&str] = &["strip_sgr_except_fg", "collapse_spaces", "nfc"];

/// Why a manifest cannot be used.
#[derive(Debug, thiserror::Error)]
pub enum ScreenError {
    #[error("the screen manifest is not valid TOML: {0}")]
    Toml(String),
    #[error("screen manifest {id}: {why}")]
    Invalid { id: String, why: String },
}

// --- the text of a manifest ------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawManifest {
    schema: u32,
    id: String,
    harness: String,
    versions: String,
    #[serde(default)]
    defaults: RawDefaults,
    #[serde(default)]
    rules: Vec<RawRule>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawDefaults {
    region: Option<RawRegion>,
    #[serde(default)]
    normalize: Vec<String>,
}

/// `region = { rows = "-30..-1" }`, or the name of a screen: `"alt_screen_only"`,
/// `"primary_only"`.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RawRegion {
    Rows { rows: String },
    Screen(String),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRule {
    id: String,
    state: Option<String>,
    opens: Option<String>,
    #[serde(default)]
    any: Vec<String>,
    #[serde(default)]
    all: Vec<String>,
    #[serde(default)]
    not: Vec<String>,
    confidence: f32,
    #[serde(default)]
    hold_ms: u64,
    region: Option<RawRegion>,
    style: Option<RawStyle>,
    cursor_in_region: Option<bool>,
    window_title_regex: Option<String>,
    interaction: Option<RawInteraction>,
    dialog: Option<RawDialog>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawStyle {
    fg: Option<String>,
    bg: Option<String>,
    bold: Option<bool>,
    dim: Option<bool>,
    italic: Option<bool>,
    inverse: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawInteraction {
    kind: String,
    title_from: Option<RawTitle>,
    command_from: Option<RawCommand>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTitle {
    regex: String,
    #[serde(default = "one")]
    group: usize,
}

fn one() -> usize {
    1
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCommand {
    region: String,
    #[serde(default)]
    trim: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawDialog {
    options_regex: String,
    pointer: Option<RawPointer>,
    #[serde(default)]
    accelerators: Option<String>,
    #[serde(default)]
    map: BTreeMap<String, u32>,
    /// Patterns on the labels of the options: `allow`, `allow_always`, `deny`. The numbers of the
    /// options change between versions of a harness, and the labels say more.
    #[serde(default)]
    labels: BTreeMap<String, String>,
    multi_toggle: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPointer {
    glyph: String,
}

// --- the compiled form -------------------------------------------------------------------------

/// A range of rows. A negative number counts from the bottom: `-1` is the last row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RowRange {
    from: i32,
    to: i32,
}

impl RowRange {
    pub const ALL: RowRange = RowRange { from: 0, to: -1 };

    fn parse(text: &str) -> Result<Self, String> {
        let text = text.trim();
        if text == "all" {
            return Ok(Self::ALL);
        }
        let (from, to) = text
            .split_once("..")
            .ok_or_else(|| format!("`{text}` is not a range of rows such as `-30..-1`"))?;
        let number = |part: &str| {
            part.trim()
                .parse::<i32>()
                .map_err(|_| format!("`{part}` in `{text}` is not a number"))
        };
        Ok(Self {
            from: number(from)?,
            to: number(to)?,
        })
    }

    /// The rows of a screen with `total` rows, or `None` when the range is empty.
    pub fn resolve(self, total: u16) -> Option<RangeInclusive<u16>> {
        let total = i32::from(total);
        let at = |n: i32| if n < 0 { total + n } else { n };
        let (from, to) = (at(self.from).max(0), at(self.to).min(total - 1));
        (from <= to && total > 0).then_some(RangeInclusive::new(from as u16, to as u16))
    }
}

/// Which screen a rule reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScreenKind {
    Any,
    AltOnly,
    PrimaryOnly,
}

/// The state that a screen rule sets. A screen rule never sets another one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScreenState {
    Working,
    Idle,
}

#[derive(Clone, Debug)]
pub struct Region {
    pub rows: Option<RowRange>,
    pub screen: ScreenKind,
}

/// A colour or an attribute that a cell must have.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StyleMatcher {
    pub fg: Option<(u8, u8, u8)>,
    pub bg: Option<(u8, u8, u8)>,
    pub bold: Option<bool>,
    pub dim: Option<bool>,
    pub italic: Option<bool>,
    pub inverse: Option<bool>,
}

#[derive(Clone, Debug)]
pub struct TitleFrom {
    pub regex: Regex,
    pub group: usize,
}

/// Where the command of an approval is on the screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandFrom {
    /// The lines above the question, between the rules that frame them.
    BoxAboveQuestion,
    /// The line that starts with `$ ` between the question and the options, and the lines that
    /// continue it up to an empty line.
    ShellBelowQuestion,
}

#[derive(Clone, Debug)]
pub struct InteractionRule {
    pub kind: InteractionKind,
    pub title_from: Option<TitleFrom>,
    pub command_from: Option<(CommandFrom, bool)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Accelerators {
    None,
    Digits,
}

/// The geometry that the keystroke verifier needs.
#[derive(Clone, Debug)]
pub struct DialogRule {
    pub options: Regex,
    pub pointer: Option<String>,
    pub accelerators: Accelerators,
    pub map: BTreeMap<String, u32>,
    /// The patterns on the labels, for `allow`, `allow_always` and `deny`.
    pub labels: BTreeMap<String, Regex>,
    /// The key that toggles a choice of a list with many.
    pub multi_toggle: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Rule {
    pub id: String,
    pub state: Option<ScreenState>,
    pub opens: Option<InteractionKind>,
    pub any: Vec<Regex>,
    pub all: Vec<Regex>,
    pub not: Vec<Regex>,
    pub confidence: f32,
    pub hold_ms: u64,
    pub region: Region,
    pub style: Option<StyleMatcher>,
    pub cursor_in_region: bool,
    pub title: Option<Regex>,
    pub interaction: Option<InteractionRule>,
    pub dialog: Option<DialogRule>,
}

/// The steps that clean the text of a row before a rule reads it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Normalize {
    pub collapse_spaces: bool,
}

impl Normalize {
    pub fn apply(self, text: &str) -> String {
        if !self.collapse_spaces {
            return text.trim_end().to_owned();
        }
        let mut out = String::with_capacity(text.len());
        let mut last_space = false;
        for c in text.trim_end().chars() {
            if c == ' ' {
                if !last_space {
                    out.push(' ');
                }
                last_space = true;
            } else {
                out.push(c);
                last_space = false;
            }
        }
        out
    }
}

/// A screen manifest, ready to run.
#[derive(Clone, Debug)]
pub struct ScreenManifest {
    pub id: String,
    pub harness: String,
    pub versions: VersionReq,
    pub default_rows: Option<RowRange>,
    pub normalize: Normalize,
    pub rules: Vec<Rule>,
}

impl ScreenManifest {
    /// Reads and compiles a manifest. Every error names the manifest and the rule.
    pub fn parse(text: &str) -> Result<Self, ScreenError> {
        let raw: RawManifest =
            toml::from_str(text).map_err(|e| ScreenError::Toml(e.to_string()))?;
        compile(raw)
    }

    /// Whether the manifest was validated for the version of the harness.
    pub fn applies_to(&self, version: &Version) -> bool {
        self.versions.matches(version)
    }

    /// The rule with the id, if any.
    pub fn rule(&self, id: &str) -> Option<&Rule> {
        self.rules.iter().find(|r| r.id == id)
    }
}

fn compile(raw: RawManifest) -> Result<ScreenManifest, ScreenError> {
    let id = raw.id.clone();
    let invalid = |why: String| ScreenError::Invalid {
        id: id.clone(),
        why,
    };
    if raw.schema != 1 {
        return Err(invalid(format!("schema {} is not known", raw.schema)));
    }
    let versions = VersionReq::parse(&raw.versions)
        .map_err(|e| invalid(format!("versions `{}`: {e}", raw.versions)))?;
    let mut normalize = Normalize::default();
    for step in &raw.defaults.normalize {
        if !NORMALIZE_STEPS.contains(&step.as_str()) {
            return Err(invalid(format!("normalize step `{step}` is not known")));
        }
        if step == "collapse_spaces" {
            normalize.collapse_spaces = true;
        }
    }
    let (default_rows, _) = match &raw.defaults.region {
        Some(region) => region_of(region).map_err(invalid)?,
        None => (None, ScreenKind::Any),
    };
    let mut rules = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for rule in raw.rules {
        if !seen.insert(rule.id.clone()) {
            return Err(invalid(format!("the rule id `{}` is used twice", rule.id)));
        }
        let rule_id = rule.id.clone();
        rules.push(compile_rule(rule).map_err(|why| invalid(format!("rule `{rule_id}`: {why}")))?);
    }
    Ok(ScreenManifest {
        id: raw.id,
        harness: raw.harness,
        versions,
        default_rows,
        normalize,
        rules,
    })
}

fn region_of(region: &RawRegion) -> Result<(Option<RowRange>, ScreenKind), String> {
    match region {
        RawRegion::Rows { rows } => Ok((Some(RowRange::parse(rows)?), ScreenKind::Any)),
        RawRegion::Screen(name) => match name.as_str() {
            "alt_screen_only" => Ok((None, ScreenKind::AltOnly)),
            "primary_only" => Ok((None, ScreenKind::PrimaryOnly)),
            other => Err(format!(
                "region `{other}` is not `alt_screen_only` or `primary_only`"
            )),
        },
    }
}

fn regex(text: &str) -> Result<Regex, String> {
    Regex::new(text).map_err(|e| format!("the pattern `{text}` is not valid: {e}"))
}

fn regexes(list: &[String]) -> Result<Vec<Regex>, String> {
    list.iter().map(|text| regex(text)).collect()
}

fn kind_of(name: &str) -> Result<InteractionKind, String> {
    match name {
        "approval" => Ok(InteractionKind::Approval),
        "question" => Ok(InteractionKind::Question),
        "plan_review" => Ok(InteractionKind::PlanReview),
        other => Err(format!(
            "`{other}` is not `approval`, `question` or `plan_review`"
        )),
    }
}

fn hex(text: &str) -> Result<(u8, u8, u8), String> {
    let digits = text.strip_prefix('#').unwrap_or(text);
    let part = |at: usize| {
        digits
            .get(at..at + 2)
            .and_then(|p| u8::from_str_radix(p, 16).ok())
    };
    match (digits.len(), part(0), part(2), part(4)) {
        (6, Some(r), Some(g), Some(b)) => Ok((r, g, b)),
        _ => Err(format!("`{text}` is not a colour such as `#d97757`")),
    }
}

fn compile_rule(raw: RawRule) -> Result<Rule, String> {
    let state = match raw.state.as_deref() {
        None => None,
        Some("working") => Some(ScreenState::Working),
        Some("idle") => Some(ScreenState::Idle),
        Some(other) => return Err(format!("state `{other}` is not `working` or `idle`")),
    };
    let opens = raw.opens.as_deref().map(kind_of).transpose()?;
    if state.is_some() == opens.is_some() {
        return Err("a rule has a `state` or `opens`, and not both".to_owned());
    }
    if !(0.0..=1.0).contains(&raw.confidence) {
        return Err("confidence is between 0 and 1".to_owned());
    }
    if raw.any.is_empty()
        && raw.all.is_empty()
        && raw.style.is_none()
        && raw.window_title_regex.is_none()
        && raw.cursor_in_region.is_none()
    {
        return Err(
            "a rule needs `any`, `all`, `style`, `window_title_regex` or `cursor_in_region`"
                .to_owned(),
        );
    }
    let (rows, screen) = match &raw.region {
        Some(region) => region_of(region)?,
        None => (None, ScreenKind::Any),
    };
    let style = raw
        .style
        .map(|s| {
            Ok::<_, String>(StyleMatcher {
                fg: s.fg.as_deref().map(hex).transpose()?,
                bg: s.bg.as_deref().map(hex).transpose()?,
                bold: s.bold,
                dim: s.dim,
                italic: s.italic,
                inverse: s.inverse,
            })
        })
        .transpose()?;
    let interaction = raw
        .interaction
        .map(|i| {
            let kind = kind_of(&i.kind)?;
            if Some(kind) != opens {
                return Err("`interaction.kind` is not the kind that the rule opens".to_owned());
            }
            let title_from = i
                .title_from
                .map(|t| {
                    let re = regex(&t.regex)?;
                    if t.group >= re.captures_len() {
                        return Err(format!("title_from group {} does not exist", t.group));
                    }
                    Ok(TitleFrom {
                        regex: re,
                        group: t.group,
                    })
                })
                .transpose()?;
            let command_from = i
                .command_from
                .map(|c| match c.region.as_str() {
                    "box_above_question" => Ok((CommandFrom::BoxAboveQuestion, c.trim)),
                    "shell_below_question" => Ok((CommandFrom::ShellBelowQuestion, c.trim)),
                    other => Err(format!("command_from region `{other}` is not known")),
                })
                .transpose()?;
            Ok::<_, String>(InteractionRule {
                kind,
                title_from,
                command_from,
            })
        })
        .transpose()?;
    let dialog = raw
        .dialog
        .map(|d| {
            let options = regex(&d.options_regex)?;
            let names: Vec<&str> = options.capture_names().flatten().collect();
            if !names.contains(&"label") {
                return Err("`options_regex` needs a group named `label`".to_owned());
            }
            let accelerators = match d.accelerators.as_deref() {
                None | Some("none") => Accelerators::None,
                Some("digits") => Accelerators::Digits,
                Some(other) => {
                    return Err(format!("accelerators `{other}` is not `none` or `digits`"));
                }
            };
            if let Some(toggle) = d.multi_toggle.as_deref()
                && toggle != "space"
            {
                return Err(format!("multi_toggle `{toggle}` is not `space`"));
            }
            let labels = d
                .labels
                .iter()
                .map(|(name, pattern)| Ok((name.clone(), regex(pattern)?)))
                .collect::<Result<BTreeMap<_, _>, String>>()?;
            Ok(DialogRule {
                options,
                pointer: d.pointer.map(|p| p.glyph),
                accelerators,
                map: d.map,
                labels,
                multi_toggle: d.multi_toggle,
            })
        })
        .transpose()?;
    if dialog.is_some() && opens.is_none() {
        return Err("`dialog` belongs to a rule that opens an interaction".to_owned());
    }
    Ok(Rule {
        id: raw.id,
        state,
        opens,
        any: regexes(&raw.any)?,
        all: regexes(&raw.all)?,
        not: regexes(&raw.not)?,
        confidence: raw.confidence,
        hold_ms: raw.hold_ms,
        region: Region { rows, screen },
        style,
        cursor_in_region: raw.cursor_in_region.unwrap_or(false),
        title: raw.window_title_regex.as_deref().map(regex).transpose()?,
        interaction,
        dialog,
    })
}
