//! What a project says about its own commands: the scripts of `package.json`, the targets of a
//! `Makefile` and a `justfile`, and the targets of `Cargo.toml`. The risk rules call a
//! test, lint or build runner that the project names a low risk.

use std::{collections::BTreeSet, fs, io::Read, path::Path};

/// The largest file that is read. A file that is larger is not a manifest anyone wrote by hand.
const FILE_CAP: u64 = 1024 * 1024;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Project {
    /// The keys of `scripts` in `package.json`.
    pub package_scripts: BTreeSet<String>,
    pub make_targets: BTreeSet<String>,
    pub just_recipes: BTreeSet<String>,
    /// The package name and the names of the `bin`, `example`, `test` and `bench` targets.
    pub cargo_targets: BTreeSet<String>,
}

fn read(path: &Path) -> Option<String> {
    let file = fs::File::open(path).ok()?;
    let mut text = String::new();
    file.take(FILE_CAP).read_to_string(&mut text).ok()?;
    Some(text)
}

impl Project {
    /// Reads the manifests in the root of a project. A file that is missing or broken adds nothing.
    pub fn scan(root: &Path) -> Self {
        let first = |names: &[&str]| names.iter().find_map(|name| read(&root.join(name)));
        Self::from_texts(
            read(&root.join("package.json")).as_deref(),
            first(&["Makefile", "makefile", "GNUmakefile"]).as_deref(),
            first(&["justfile", "Justfile", ".justfile"]).as_deref(),
            read(&root.join("Cargo.toml")).as_deref(),
        )
    }

    /// The same from the text of the files.
    pub fn from_texts(
        package_json: Option<&str>,
        makefile: Option<&str>,
        justfile: Option<&str>,
        cargo_toml: Option<&str>,
    ) -> Self {
        Self {
            package_scripts: package_json.map(package_scripts).unwrap_or_default(),
            make_targets: makefile.map(make_targets).unwrap_or_default(),
            just_recipes: justfile.map(just_recipes).unwrap_or_default(),
            cargo_targets: cargo_toml.map(cargo_targets).unwrap_or_default(),
        }
    }
}

fn package_scripts(text: &str) -> BTreeSet<String> {
    let Ok(serde_json::Value::Object(root)) = serde_json::from_str(text) else {
        return BTreeSet::new();
    };
    match root.get("scripts") {
        Some(serde_json::Value::Object(scripts)) => scripts.keys().cloned().collect(),
        _ => BTreeSet::new(),
    }
}

fn plain_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('.')
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.' | b'/' | b'@'))
}

fn make_targets(text: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for line in text.lines() {
        if line.starts_with(['\t', ' ', '#']) {
            continue;
        }
        let Some(colon) = line.find(':') else {
            continue;
        };
        // `NAME := value` and `NAME ::= value` are variables.
        if line[colon..].starts_with(":=") || line[colon..].starts_with("::=") {
            continue;
        }
        if line[..colon].contains('=') {
            continue;
        }
        out.extend(
            line[..colon]
                .split_whitespace()
                .filter(|n| plain_name(n))
                .map(str::to_owned),
        );
    }
    out
}

fn just_recipes(text: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for line in text.lines() {
        if line.starts_with([' ', '\t', '#', '[']) || line.trim().is_empty() {
            continue;
        }
        let Some(colon) = line.find(':') else {
            continue;
        };
        if line[colon..].starts_with(":=") || line[..colon].contains('=') {
            continue;
        }
        let head = line[..colon].trim_start_matches('@');
        let Some(name) = head.split_whitespace().next() else {
            continue;
        };
        let valid = name.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
            && name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'));
        if valid && !matches!(name, "set" | "import" | "mod" | "alias" | "export") {
            out.insert(name.to_owned());
        }
    }
    out
}

fn cargo_targets(text: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let Ok(table) = text.parse::<toml::Table>() else {
        return out;
    };
    if let Some(name) = table
        .get("package")
        .and_then(|package| package.get("name"))
        .and_then(toml::Value::as_str)
    {
        out.insert(name.to_owned());
    }
    for kind in ["bin", "example", "test", "bench"] {
        if let Some(toml::Value::Array(items)) = table.get(kind) {
            out.extend(
                items
                    .iter()
                    .filter_map(|item| item.get("name").and_then(toml::Value::as_str))
                    .map(str::to_owned),
            );
        }
    }
    out
}

#[cfg(test)]
mod tests;
