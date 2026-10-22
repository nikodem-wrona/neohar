//! The manifest of a harness: the types, the merge of files, and the check.

pub mod choices;
pub mod command;
pub mod loader;
pub mod merge;
pub mod schema;
pub mod validate;

use std::{collections::BTreeMap, path::PathBuf};

pub use schema::Manifest;

/// Where a manifest came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    /// Compiled into neohar.
    Builtin,
    /// A file of the user. A built-in with the same id lies below it.
    User(PathBuf),
}

impl Source {
    /// The text of the event `harness.manifest_loaded`: `builtin` or the path of the file.
    pub fn text(&self) -> String {
        match self {
            Self::Builtin => "builtin".to_owned(),
            Self::User(path) => path.display().to_string(),
        }
    }
}

/// A manifest that is in force, with where it came from and the merged table that it was made from.
/// A file that extends it starts from that table.
#[derive(Clone, Debug)]
pub struct Entry {
    pub manifest: Manifest,
    pub source: Source,
    pub(crate) raw: toml::Table,
}

/// A note about a file that was left out, or a key that was ignored. The core shows each as a
/// warning of the doctor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Warning {
    /// The file, when the note is about one.
    pub file: Option<PathBuf>,
    pub message: String,
}

impl std::fmt::Display for Warning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.file {
            Some(file) => write!(f, "{}: {}", file.display(), self.message),
            None => f.write_str(&self.message),
        }
    }
}

/// The manifests in force, by id, and what the load left out.
#[derive(Clone, Debug, Default)]
pub struct Loaded {
    pub manifests: BTreeMap<String, Entry>,
    pub warnings: Vec<Warning>,
}

impl Loaded {
    pub fn get(&self, id: &str) -> Option<&Manifest> {
        self.manifests.get(id).map(|entry| &entry.manifest)
    }
}

impl Entry {
    /// A short name for the content of the manifest: 16 hexadecimal digits that change when the
    /// manifest changes (FNV-1a over its JSON). The event `harness.manifest_loaded` carries it.
    pub fn fingerprint(&self) -> String {
        let json = serde_json::to_string(&self.manifest).unwrap_or_default();
        let hash = json.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
        });
        format!("{hash:016x}")
    }
}
