//! Helpers for the frozen wire fixtures.
//!
//! A fixture is the `postcard` encoding of one value, stored in `tests/fixtures/<dir>/<prefix>_<name>.bin`.
//! Fixtures are frozen: a test never overwrites one. To add a case, run the tests once with
//! `NEOHAR_BLESS_FIXTURES=1`. That writes the missing files. To change an existing case, you
//! change the wire format: add a new protocol version and keep the old fixtures.

use std::{
    collections::BTreeSet,
    fmt::Debug,
    fs,
    path::{Path, PathBuf},
};

use serde::{Serialize, de::DeserializeOwned};

fn dir(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

/// Checks every case against its frozen file, and checks that no `<prefix>_*` file lacks a case.
pub fn check_fixtures<T>(fixture_dir: &str, prefix: &str, cases: &[(&str, T)])
where
    T: Serialize + DeserializeOwned + PartialEq + Debug,
{
    let bless = std::env::var_os("NEOHAR_BLESS_FIXTURES").is_some_and(|v| v == "1");
    let root = dir(fixture_dir);
    let mut names = BTreeSet::new();
    for (name, value) in cases {
        assert!(names.insert(*name), "the case name {name} is used twice");
        let path = root.join(format!("{prefix}_{name}.bin"));
        let encoded = postcard::to_stdvec(value).expect("the value serializes");
        if bless && !path.exists() {
            fs::write(&path, &encoded).expect("the fixture is written");
        }
        let frozen = fs::read(&path).unwrap_or_else(|e| {
            panic!(
                "missing fixture {}: {e}. Run the tests with NEOHAR_BLESS_FIXTURES=1 to write it.",
                path.display()
            )
        });
        assert_eq!(
            frozen, encoded,
            "the wire format of {fixture_dir}/{prefix}_{name} changed"
        );
        let decoded: T = postcard::from_bytes(&frozen).expect("the frozen bytes decode");
        assert_eq!(
            &decoded, value,
            "{fixture_dir}/{prefix}_{name} decodes to another value"
        );
    }
    let on_disk: BTreeSet<String> = fs::read_dir(&root)
        .expect("the fixture directory exists")
        .map(|e| {
            e.expect("a directory entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .filter_map(|f| f.strip_suffix(".bin").map(str::to_owned))
        .filter_map(|f| f.strip_prefix(&format!("{prefix}_")).map(str::to_owned))
        .collect();
    let expected: BTreeSet<String> = names.iter().map(|n| (*n).to_owned()).collect();
    assert_eq!(
        on_disk, expected,
        "fixture files and cases differ in {fixture_dir}"
    );
}
