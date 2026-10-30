//! The checks of a manifest that the types cannot make: the id, every regular
//! expression, every range of versions. A manifest that fails a check is left out, and the one
//! below it stays in force.

use regex::Regex;
use semver::VersionReq;

use super::{
    choices::{CODEX_CATALOG, valid_effort},
    schema::{Auth, Launch, Manifest, SCHEMA},
};
use crate::auth::METHODS;

/// The longest id. The id has this many characters after the first one at most.
const ID_REST: usize = 31;

/// Whether `id` is `[a-z][a-z0-9_-]{0,31}`.
pub fn valid_id(id: &str) -> bool {
    let mut chars = id.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.clone().count() <= ID_REST
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '_' | '-'))
}

/// The states that a `[states]` value can name besides `open:<kind>`.
const STATES: &[&str] = &["working", "idle", "error", "session_ended"];

/// The kinds of interaction that `open:<kind>` can name.
const KINDS: &[&str] = &["approval", "question", "plan_review", "notice"];

/// Parses a version that a manifest writes: `2.1.0`, or `2.1` which means `2.1.0`.
pub fn parse_version(text: &str) -> Result<semver::Version, semver::Error> {
    let text = text.trim();
    match text.matches('.').count() {
        0 => semver::Version::parse(&format!("{text}.0.0")),
        1 => semver::Version::parse(&format!("{text}.0")),
        _ => semver::Version::parse(text),
    }
}

/// What is wrong with a manifest. An empty list means that it is right.
pub fn check(manifest: &Manifest) -> Vec<String> {
    let mut problems = Vec::new();
    if manifest.schema != SCHEMA {
        problems.push(format!(
            "schema {} is not a version that neohar reads (it reads {SCHEMA})",
            manifest.schema
        ));
    }
    if !valid_id(&manifest.id) {
        problems.push(format!(
            "the id {:?} is not [a-z][a-z0-9_-] with at most 32 characters",
            manifest.id
        ));
    }
    let mut regex = |what: String, text: &str| {
        if let Err(error) = Regex::new(text) {
            problems.push(format!("{what} is not a regular expression: {error}"));
        }
    };
    for (index, rule) in manifest.detect.process.iter().enumerate() {
        let at = format!("detect.process[{index}]");
        if let Some(text) = &rule.script_regex {
            regex(format!("{at}.script_regex"), text);
        }
        if let Some(text) = &rule.exe_path_regex {
            regex(format!("{at}.exe_path_regex"), text);
        }
        if let Some(text) = &rule.argv_regex {
            regex(format!("{at}.argv_regex"), text);
        }
    }
    if let Some(rule) = &manifest.version {
        regex("version.regex".to_owned(), &rule.regex);
        match Regex::new(&rule.regex) {
            Ok(compiled) if !compiled.capture_names().flatten().any(|name| name == "v") => {
                problems.push("version.regex has no group named v".to_owned());
            }
            _ => {}
        }
        if rule.command.is_empty() {
            problems.push("version.command is empty".to_owned());
        }
    }
    if let Some(text) = &manifest.min_version
        && parse_version(text).is_err()
    {
        problems.push(format!("min_version {text:?} is not a version"));
    }
    for (index, range) in manifest.capabilities.iter().enumerate() {
        if let Err(error) = VersionReq::parse(&range.versions) {
            problems.push(format!(
                "capabilities[{index}].versions {:?} is not a range of versions: {error}",
                range.versions
            ));
        }
    }
    for (event, value) in &manifest.states {
        let known = STATES.contains(&value.as_str())
            || value
                .strip_prefix("open:")
                .is_some_and(|kind| KINDS.contains(&kind));
        if !known {
            problems.push(format!("states.{event} = {value:?} is not a state"));
        }
    }
    if let Some(launch) = &manifest.launch {
        launch_choices(launch, &mut problems);
    }
    if let Some(auth) = &manifest.auth {
        auth_rules(auth, &mut problems);
    }
    problems
}

/// The rules of `[auth]`.
fn auth_rules(auth: &Auth, problems: &mut Vec<String>) {
    if auth.status.is_empty() {
        problems.push("auth.status is empty".to_owned());
    }
    let pointer = |what: &str, text: &Option<String>, problems: &mut Vec<String>| {
        if let Some(text) = text
            && !text.is_empty()
            && !text.starts_with('/')
        {
            problems.push(format!("{what}.json {text:?} is not a JSON pointer"));
        }
    };
    let compiles = |what: &str, text: &Option<String>, problems: &mut Vec<String>| {
        if let Some(text) = text
            && let Err(error) = Regex::new(text)
        {
            problems.push(format!("{what}.regex is not a regular expression: {error}"));
        }
    };
    for (what, rule) in [
        ("auth.logged_in", &auth.logged_in),
        ("auth.logged_out", &auth.logged_out),
    ] {
        let Some(rule) = rule else { continue };
        if rule.exit.is_none() && rule.regex.is_none() && rule.json.is_none() {
            problems.push(format!("{what} has no exit, regex or json"));
        }
        if rule.equals.is_some() && rule.json.is_none() {
            problems.push(format!("{what}.equals needs json"));
        }
        pointer(what, &rule.json, problems);
        compiles(what, &rule.regex, problems);
    }
    if let Some(method) = &auth.method {
        if method.json.is_none() && method.regex.is_none() {
            problems.push("auth.method has no json or regex".to_owned());
        }
        pointer("auth.method", &method.json, problems);
        compiles("auth.method", &method.regex, problems);
        for (text, name) in &method.values {
            if !METHODS.contains(&name.as_str()) {
                problems.push(format!(
                    "auth.method.values.{text:?} = {name:?} is not {}",
                    METHODS.join(", ")
                ));
            }
        }
    }
}

/// The model and the effort parts of `[launch]`.
fn launch_choices(launch: &Launch, problems: &mut Vec<String>) {
    let placeholder = |name: &str, arg: &Option<Vec<String>>, problems: &mut Vec<String>| {
        let marker = format!("{{{name}}}");
        if let Some(arg) = arg
            && !arg.iter().any(|part| part.contains(&marker))
        {
            problems.push(format!("launch.{name}_arg has no {marker}"));
        }
    };
    placeholder("model", &launch.model_arg, problems);
    placeholder("effort", &launch.effort_arg, problems);
    placeholder("add_dir", &launch.add_dir_arg, problems);
    if let Some(catalog) = &launch.model_catalog
        && catalog != CODEX_CATALOG
    {
        problems.push(format!(
            "launch.model_catalog {catalog:?} is not a catalog that neohar reads ({CODEX_CATALOG})"
        ));
    }
    for (index, entry) in launch.models.iter().enumerate() {
        if entry.id.is_empty() {
            problems.push(format!("launch.models[{index}].id is empty"));
        }
    }
    for level in &launch.efforts {
        if !valid_effort(level) {
            problems.push(format!(
                "launch.efforts has {level:?}, which is not letters, digits, - and _"
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_id_is_lowercase_and_short() {
        assert!(valid_id("claude"));
        assert!(valid_id("cc"));
        assert!(valid_id("my-wrapper_2"));
        assert!(valid_id(&format!("a{}", "b".repeat(31))));
        assert!(!valid_id(&format!("a{}", "b".repeat(32))));
        assert!(!valid_id(""));
        assert!(!valid_id("Claude"));
        assert!(!valid_id("1claude"));
        assert!(!valid_id("my wrapper"));
    }

    #[test]
    fn the_model_and_effort_parts_of_a_launch_are_checked() {
        let manifest = |launch: Launch| Manifest {
            schema: SCHEMA,
            id: "claude".to_owned(),
            launch: Some(launch),
            ..Manifest::default()
        };
        let right = Launch {
            argv: vec!["claude".to_owned()],
            model_arg: Some(vec!["--model".to_owned(), "{model}".to_owned()]),
            effort_arg: Some(vec!["--effort".to_owned(), "{effort}".to_owned()]),
            efforts: vec!["low".to_owned(), "xhigh".to_owned()],
            model_catalog: Some("codex".to_owned()),
            ..Launch::default()
        };
        assert!(check(&manifest(right.clone())).is_empty());
        let wrong = Launch {
            effort_arg: Some(vec!["--effort".to_owned(), "high".to_owned()]),
            efforts: vec!["very high".to_owned()],
            model_catalog: Some("openai".to_owned()),
            models: vec![super::super::schema::ModelEntry::default()],
            ..right
        };
        let problems = check(&manifest(wrong));
        assert_eq!(problems.len(), 4, "{problems:?}");
        assert!(problems[0].contains("effort_arg has no {effort}"));
    }

    #[test]
    fn the_rules_of_a_login_probe_are_checked() {
        use super::super::schema::{AuthRule, MethodRule};
        let manifest = |auth: Auth| Manifest {
            schema: SCHEMA,
            id: "claude".to_owned(),
            auth: Some(auth),
            ..Manifest::default()
        };
        let right = Auth {
            status: vec!["claude".to_owned(), "auth".to_owned(), "status".to_owned()],
            logged_in: Some(AuthRule {
                json: Some("/loggedIn".to_owned()),
                equals: Some(serde_json::Value::Bool(true)),
                ..AuthRule::default()
            }),
            logged_out: Some(AuthRule {
                exit: Some(1),
                ..AuthRule::default()
            }),
            method: Some(MethodRule {
                json: Some("/authMethod".to_owned()),
                values: [("claude.ai".to_owned(), "subscription".to_owned())].into(),
                ..MethodRule::default()
            }),
            login_hint: None,
        };
        assert!(check(&manifest(right.clone())).is_empty());
        let wrong = Auth {
            status: Vec::new(),
            logged_in: Some(AuthRule {
                equals: Some(serde_json::Value::Bool(true)),
                regex: Some("(".to_owned()),
                ..AuthRule::default()
            }),
            logged_out: Some(AuthRule::default()),
            method: Some(MethodRule {
                json: Some("authMethod".to_owned()),
                values: [("x".to_owned(), "password".to_owned())].into(),
                ..MethodRule::default()
            }),
            login_hint: None,
        };
        let problems = check(&manifest(wrong));
        assert_eq!(problems.len(), 6, "{problems:?}");
        assert_eq!(problems[0], "auth.status is empty");
    }

    #[test]
    fn a_version_may_have_fewer_parts() {
        assert_eq!(
            parse_version("2.1.292").expect("a version").to_string(),
            "2.1.292"
        );
        assert_eq!(
            parse_version("2.1").expect("a version").to_string(),
            "2.1.0"
        );
        assert_eq!(parse_version("2").expect("a version").to_string(), "2.0.0");
        assert!(parse_version("two").is_err());
    }
}
