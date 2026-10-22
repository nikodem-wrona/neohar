//! Scenario files that run one after the other (`after` in a scenario).
//!
//! A scenario can name another scenario of the same directory with `after = "<name>"`. The recorder
//! runs that one first, in the same scratch tree, and records only the scenario that you asked
//! for. A resume scenario uses it: the first scenario makes a session, and the second one starts
//! `claude --continue` or `codex resume --last`.

use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use crate::{GoldenError, scenario::Scenario};

/// The most scenarios that run before the one you asked for.
const MAX_CHAIN: usize = 4;

/// A scenario file, read.
#[derive(Clone, Debug)]
pub struct Loaded {
    pub name: String,
    pub text: String,
    pub scenario: Scenario,
}

fn read(path: &Path, name: String) -> Result<Loaded, GoldenError> {
    let text =
        fs::read_to_string(path).map_err(GoldenError::io(format!("read {}", path.display())))?;
    let scenario = Scenario::parse(&text)?;
    Ok(Loaded {
        name,
        text,
        scenario,
    })
}

/// Reads the scenario at `path` and the scenarios that it names with `after`. The result starts
/// with the scenario that runs first, and the last is the one at `path`. `name` is the name of
/// that last scenario (the file name without `.toml` if it is `None`).
pub fn load_chain(path: &Path, name: Option<&str>) -> Result<Vec<Loaded>, GoldenError> {
    let stem = |p: &Path| {
        p.file_stem().map_or_else(
            || "scenario".to_owned(),
            |s| s.to_string_lossy().into_owned(),
        )
    };
    let mut chain = vec![read(path, name.map_or_else(|| stem(path), str::to_owned))?];
    let dir: PathBuf = path.parent().map(Path::to_owned).unwrap_or_default();
    let mut seen: BTreeSet<String> = chain.iter().map(|l| l.name.clone()).collect();
    while let Some(after) = chain[0].scenario.after.clone() {
        if chain.len() > MAX_CHAIN {
            return Err(GoldenError::Setup(format!(
                "more than {MAX_CHAIN} scenarios run before {}",
                chain.last().map_or("?", |l| l.name.as_str())
            )));
        }
        if !seen.insert(after.clone()) {
            return Err(GoldenError::Setup(format!(
                "the scenarios name each other in a loop at {after}"
            )));
        }
        let previous = read(&dir.join(format!("{after}.toml")), after)?;
        chain.insert(0, previous);
    }
    Ok(chain)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("nhg-chain-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("a directory");
        dir
    }

    #[test]
    fn a_chain_starts_with_the_scenario_that_runs_first() {
        let d = dir("a");
        fs::write(d.join("one.toml"), "").expect("file");
        fs::write(d.join("two.toml"), "after = \"one\"\n").expect("file");
        fs::write(d.join("three.toml"), "after = \"two\"\n").expect("file");
        let chain = load_chain(&d.join("three.toml"), None).expect("a chain");
        let names: Vec<&str> = chain.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, ["one", "two", "three"]);
        let alone = load_chain(&d.join("one.toml"), Some("renamed")).expect("a chain");
        assert_eq!(alone.len(), 1);
        assert_eq!(alone[0].name, "renamed");
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn a_loop_and_a_missing_file_are_errors() {
        let d = dir("b");
        fs::write(d.join("a.toml"), "after = \"b\"\n").expect("file");
        fs::write(d.join("b.toml"), "after = \"a\"\n").expect("file");
        let error = load_chain(&d.join("a.toml"), None).expect_err("a loop");
        assert!(error.to_string().contains("loop"), "{error}");
        fs::write(d.join("c.toml"), "after = \"missing\"\n").expect("file");
        assert!(load_chain(&d.join("c.toml"), None).is_err());
        let _ = fs::remove_dir_all(d);
    }
}
