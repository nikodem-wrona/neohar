//! What the tests of the recorder share: the golden directory, one recording of a scenario of
//! the repository, and the files of a recording in a temporary directory.

#![allow(dead_code)]

use std::path::{Path, PathBuf};

use nh_golden::{
    chain,
    driver::{Recording, Request, record_chain},
    recipe::{Harness, find_binary},
};
use nh_record::holder::locate_nh;

pub fn enabled() -> bool {
    if std::env::var_os("NEOHAR_HARNESS").is_some_and(|v| v == "1") {
        return true;
    }
    eprintln!("skipped: set NEOHAR_HARNESS=1 to record the installed Claude Code and Codex");
    false
}

pub fn golden_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/harness-golden")
}

pub fn record_once(harness: Harness, name: &str) -> Recording {
    let scenario_path = golden_root()
        .join("scenarios")
        .join(harness.name())
        .join(format!("{name}.toml"));
    let chain = chain::load_chain(&scenario_path, None)
        .unwrap_or_else(|e| panic!("load {}: {e}", scenario_path.display()));
    let binary = find_binary(harness, std::env::var_os("PATH").as_deref())
        .unwrap_or_else(|| panic!("{} is not installed (not on PATH)", harness.name()));
    let binary = std::fs::canonicalize(&binary).unwrap_or(binary);
    let nh = std::fs::canonicalize(locate_nh(None).expect("the nh binary")).expect("nh resolves");
    let root = PathBuf::from(format!("/tmp/nhg-{}", harness.name()));
    let requests: Vec<Request<'_>> = chain
        .iter()
        .map(|loaded| Request {
            harness,
            scenario: &loaded.scenario,
            scenario_text: &loaded.text,
            scenario_name: &loaded.name,
            nh: &nh,
            binary: &binary,
            root: &root,
        })
        .collect();
    let refs: Vec<&Request<'_>> = requests.iter().collect();
    record_chain(&refs).unwrap_or_else(|e| panic!("recording {}: {e}", harness.name()))
}

/// The paths of the files under `dir`.
pub fn files_under(dir: &Path, into: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir)
        .expect("a directory")
        .filter_map(Result::ok)
    {
        let path = entry.path();
        if path.is_dir() {
            files_under(&path, into);
        } else {
            into.push(path);
        }
    }
}

pub fn save(recording: &Recording, name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("nhg-test-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a directory");
    recording.write(&dir).expect("the recording is written");
    // The files hold no path of the user.
    let home = std::env::var("HOME").expect("HOME");
    let mut files = Vec::new();
    files_under(&dir, &mut files);
    for file in files {
        let text = std::fs::read_to_string(&file).expect("a text file");
        assert!(!text.contains(&home), "{} holds {home}", file.display());
    }
    dir
}
