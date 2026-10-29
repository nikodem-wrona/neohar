//! Skills: what the composer of the chat offers after `/`.
//!
//! A skill is a folder with a `SKILL.md`. A custom command of Claude Code is a Markdown file in a
//! `commands` folder. The front matter of the file gives the name and the description. [`list`]
//! reads the places of one harness for the folder of a pane. A file that is not there or not right
//! gives no skill: a fault in one skill must never hide the others.

use std::{
    collections::HashSet,
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
};

use nh_proto::api::{
    calls::agent::{Skill, SkillSource},
    common::Harness,
};
use serde_json::Value;

/// The most skills in one answer.
pub const MAX_SKILLS: usize = 200;
/// The longest description, in characters.
const DESCRIPTION_CHARS: usize = 500;
/// The part of a file that is read for its front matter, in bytes.
const HEAD_BYTES: u64 = 16 * 1024;

/// The places that [`list`] reads.
#[derive(Clone, Debug)]
pub struct Roots {
    /// `CLAUDE_CONFIG_DIR` or `CODEX_HOME`.
    pub config: PathBuf,
    /// The home directory, for `~/.agents/skills`.
    pub home: Option<PathBuf>,
    /// The folder of the pane.
    pub cwd: Option<PathBuf>,
}

/// A skill before it has its text for the prompt.
#[derive(Debug)]
struct Found {
    name: String,
    description: Option<String>,
    source: SkillSource,
}

/// The skills and the commands that `harness` can start in the folder of `roots`. A name found
/// twice keeps its first place: the project, then the user, then the plugins.
pub fn list(harness: Harness, roots: &Roots, query: Option<&str>) -> Vec<Skill> {
    let mut found = Vec::new();
    let projects = project_dirs(roots.cwd.as_deref(), roots.home.as_deref());
    match harness {
        Harness::Claude => {
            for dir in &projects {
                let dir = dir.join(".claude");
                skills_in(&dir.join("skills"), SkillSource::Project, None, &mut found);
                commands_in(
                    &dir.join("commands"),
                    SkillSource::Project,
                    None,
                    &mut found,
                );
            }
            skills_in(
                &roots.config.join("skills"),
                SkillSource::User,
                None,
                &mut found,
            );
            commands_in(
                &roots.config.join("commands"),
                SkillSource::User,
                None,
                &mut found,
            );
            for (plugin, path) in enabled_plugins(&roots.config) {
                let prefix = Some(plugin.as_str());
                skills_in(
                    &path.join("skills"),
                    SkillSource::Plugin,
                    prefix,
                    &mut found,
                );
                commands_in(
                    &path.join("commands"),
                    SkillSource::Plugin,
                    prefix,
                    &mut found,
                );
            }
        }
        Harness::Codex => {
            for dir in &projects {
                let dir = dir.join(".agents/skills");
                skills_in(&dir, SkillSource::Project, None, &mut found);
            }
            let dir = roots.config.join("skills");
            skills_in(&dir, SkillSource::User, None, &mut found);
            skills_in(&dir.join(".system"), SkillSource::User, None, &mut found);
            if let Some(home) = &roots.home {
                skills_in(
                    &home.join(".agents/skills"),
                    SkillSource::User,
                    None,
                    &mut found,
                );
            }
        }
    }
    let sigil = match harness {
        Harness::Claude => '/',
        Harness::Codex => '$',
    };
    let query = query.map(str::to_lowercase).filter(|q| !q.is_empty());
    let mut seen = HashSet::new();
    let mut kept: Vec<(bool, String, Found)> = found
        .into_iter()
        .filter(|skill| seen.insert(skill.name.clone()))
        .filter_map(|skill| {
            let lower = skill.name.to_lowercase();
            match &query {
                Some(q) if !lower.contains(q.as_str()) => None,
                Some(q) => Some((!lower.starts_with(q.as_str()), lower, skill)),
                None => Some((false, lower, skill)),
            }
        })
        .collect();
    kept.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));
    kept.into_iter()
        .take(MAX_SKILLS)
        .map(|(_, _, skill)| Skill {
            insert: format!("{sigil}{} ", skill.name),
            name: skill.name,
            description: skill.description,
            source: skill.source,
        })
        .collect()
}

/// The folder of the pane and its parents up to the root of its repository. Without a
/// repository, the folder only. The home directory is never a project: its `.claude` is the
/// configuration of the user.
fn project_dirs(cwd: Option<&Path>, home: Option<&Path>) -> Vec<PathBuf> {
    let Some(cwd) = cwd else {
        return Vec::new();
    };
    let mut dirs = Vec::new();
    let mut at = Some(cwd);
    while let Some(dir) = at {
        if Some(dir) != home {
            dirs.push(dir.to_path_buf());
        }
        if dir.join(".git").exists() {
            return dirs;
        }
        at = dir.parent();
    }
    if Some(cwd) == home {
        Vec::new()
    } else {
        vec![cwd.to_path_buf()]
    }
}

/// The folders under `dir` that have a `SKILL.md`.
fn skills_in(dir: &Path, source: SkillSource, plugin: Option<&str>, out: &mut Vec<Found>) {
    for path in sorted_entries(dir) {
        let file = path.join("SKILL.md");
        if !file.is_file() {
            continue;
        }
        let front = front_matter(&head(&file));
        if !front.user_invocable {
            continue;
        }
        let Some(name) = front.name.or_else(|| file_name(&path)) else {
            continue;
        };
        out.push(Found {
            name: qualified(plugin, &name),
            description: front.description,
            source,
        });
    }
}

/// The Markdown files in `dir`.
fn commands_in(dir: &Path, source: SkillSource, plugin: Option<&str>, out: &mut Vec<Found>) {
    for path in sorted_entries(dir) {
        if path.extension().and_then(|e| e.to_str()) != Some("md") || !path.is_file() {
            continue;
        }
        let Some(name) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let front = front_matter(&head(&path));
        out.push(Found {
            name: qualified(plugin, name),
            description: front.description,
            source,
        });
    }
}

/// The enabled plugins of Claude Code and the folder of each: `enabledPlugins` of
/// `settings.json`, and the `installPath` that `plugins/installed_plugins.json` gives.
fn enabled_plugins(config: &Path) -> Vec<(String, PathBuf)> {
    let read = |path: PathBuf| -> Option<Value> {
        serde_json::from_str(&fs::read_to_string(path).ok()?).ok()
    };
    let (Some(settings), Some(installed)) = (
        read(config.join("settings.json")),
        read(config.join("plugins/installed_plugins.json")),
    ) else {
        return Vec::new();
    };
    let Some(enabled) = settings.get("enabledPlugins").and_then(Value::as_object) else {
        return Vec::new();
    };
    enabled
        .iter()
        .filter(|(_, on)| on.as_bool() == Some(true))
        .filter_map(|(key, _)| {
            let path = installed
                .get("plugins")?
                .get(key)?
                .as_array()?
                .first()?
                .get("installPath")?
                .as_str()?;
            let name = key.split('@').next().filter(|n| !n.is_empty())?;
            Some((name.to_owned(), PathBuf::from(path)))
        })
        .collect()
}

fn qualified(plugin: Option<&str>, name: &str) -> String {
    match plugin {
        Some(plugin) => format!("{plugin}:{name}"),
        None => name.to_owned(),
    }
}

fn file_name(path: &Path) -> Option<String> {
    path.file_name().and_then(|n| n.to_str()).map(str::to_owned)
}

/// The entries of a folder in the order of their names. A folder that is not there has none.
fn sorted_entries(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    paths.sort();
    paths
}

/// The start of a file, where the front matter is.
fn head(path: &Path) -> String {
    let mut text = String::new();
    if let Ok(file) = File::open(path) {
        let mut bytes = Vec::new();
        if file.take(HEAD_BYTES).read_to_end(&mut bytes).is_ok() {
            text = String::from_utf8_lossy(&bytes).into_owned();
        }
    }
    text
}

/// The keys of the front matter that the list uses.
#[derive(Debug, PartialEq)]
pub(crate) struct Front {
    pub name: Option<String>,
    pub description: Option<String>,
    /// `user-invocable: false` hides a skill from the `/` menu of Claude Code.
    pub user_invocable: bool,
}

/// Reads the front matter between the two `---` lines at the start of a file. It knows the
/// subset of YAML that skills use: `key: value`, quoted values, and a value that continues on
/// indented lines (also after `>` or `|`).
pub(crate) fn front_matter(text: &str) -> Front {
    let mut front = Front {
        name: None,
        description: None,
        user_invocable: true,
    };
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut lines = text.lines();
    if lines.next().map(str::trim_end) != Some("---") {
        return front;
    }
    let mut pending: Option<(String, String)> = None;
    for line in lines {
        if line.trim_end() == "---" {
            break;
        }
        let indented = line.starts_with([' ', '\t']);
        if indented || line.trim().is_empty() {
            if let Some((_, value)) = &mut pending {
                let part = line.trim();
                if !part.is_empty() {
                    if !value.is_empty() {
                        value.push(' ');
                    }
                    value.push_str(part);
                }
            }
            continue;
        }
        if let Some((key, value)) = pending.take() {
            set(&mut front, &key, &value);
        }
        if let Some((key, value)) = line.split_once(':') {
            let value = value.trim();
            let value = if matches!(value, ">" | "|" | ">-" | "|-") {
                ""
            } else {
                value
            };
            pending = Some((key.trim().to_owned(), value.to_owned()));
        }
    }
    if let Some((key, value)) = pending {
        set(&mut front, &key, &value);
    }
    front
}

fn set(front: &mut Front, key: &str, value: &str) {
    let value = unquote(value.trim());
    match key {
        "name" if !value.is_empty() => front.name = Some(value.to_owned()),
        "description" if !value.is_empty() => {
            front.description = Some(value.chars().take(DESCRIPTION_CHARS).collect());
        }
        "user-invocable" => front.user_invocable = value != "false",
        _ => {}
    }
}

fn unquote(value: &str) -> &str {
    for quote in ['"', '\''] {
        if let Some(inner) = value
            .strip_prefix(quote)
            .and_then(|v| v.strip_suffix(quote))
        {
            return inner;
        }
    }
    value
}

#[cfg(test)]
mod tests;
