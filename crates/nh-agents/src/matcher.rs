//! The matcher: which harness runs in a pane.
//!
//! It checks every process of a [`Tree`] against the `[[detect.process]]` rules of every manifest.
//! A process matches a rule when every key of the rule matches. A manifest matches when any
//! process matches any of its rules. When several manifests match, the higher `priority` wins,
//! then a rule that names the program over a rule of `env` alone, and then the process
//! that is nearer to the leader. Equal priorities are a tie, and the matcher lists the manifests
//! that tied so that the doctor can warn. A match by `env` alone does not tie with a match by the
//! program.

use std::collections::BTreeMap;

use nh_proto::api::common::Harness;
use regex::Regex;

use crate::{
    harness::harness_of,
    manifest::{
        Loaded,
        schema::{ProcessRule, Yolo},
    },
    process::{Process, Tree},
};

/// A harness found in a tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Detection {
    /// The id of the manifest that matched. It can be the id of a wrapper (`cc`).
    pub manifest: String,
    pub harness: Harness,
    pub priority: i32,
    /// The `remote_loop` of the manifest: the hooks of this harness may not run in the pane.
    pub remote_loop: Option<String>,
    /// The process that matched, with the time it started.
    pub pid: i32,
    pub start: u64,
    /// The place of the process in the tree. A smaller number is nearer to the leader.
    pub place: usize,
}

impl Detection {
    /// The key that names the run for as long as the process lives.
    pub fn key(&self) -> (i32, u64) {
        (self.pid, self.start)
    }
}

/// What a search found.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Found {
    pub best: Option<Detection>,
    /// The ids of the other manifests that matched with the same priority as the best one.
    pub tied: Vec<String>,
}

/// The manifests, with their expressions compiled.
#[derive(Debug, Default)]
pub struct Matcher {
    manifests: Vec<Compiled>,
}

#[derive(Debug)]
struct Compiled {
    id: String,
    harness: Harness,
    priority: i32,
    remote_loop: Option<String>,
    rules: Vec<Rule>,
}

#[derive(Debug, Default)]
struct Rule {
    exe_basename: Vec<String>,
    interpreter: Vec<String>,
    script_regex: Option<Regex>,
    exe_path_regex: Option<Regex>,
    argv_regex: Option<Regex>,
    argv0_basename: Vec<String>,
    env: BTreeMap<String, String>,
}

impl Matcher {
    /// Compiles the manifests in force. A manifest of no known harness is left out. The loader has
    /// checked every expression, so one that does not compile is left out as well.
    pub fn new(loaded: &Loaded) -> Self {
        let mut manifests = Vec::new();
        for (id, entry) in &loaded.manifests {
            let Some(harness) = harness_of(loaded, id) else {
                continue;
            };
            let manifest = &entry.manifest;
            let rules = manifest
                .detect
                .process
                .iter()
                .filter_map(Rule::compile)
                .collect::<Vec<_>>();
            if rules.is_empty() {
                continue;
            }
            manifests.push(Compiled {
                id: id.clone(),
                harness,
                priority: manifest.priority(),
                remote_loop: manifest.detect.remote_loop.clone(),
                rules,
            });
        }
        Self { manifests }
    }

    /// Whether a rule asks for the environment of a process. A walk reads it only then.
    pub fn wants_env(&self) -> bool {
        self.manifests
            .iter()
            .any(|manifest| manifest.rules.iter().any(|rule| !rule.env.is_empty()))
    }

    /// Searches the tree.
    pub fn find(&self, tree: &Tree) -> Found {
        let mut hits = Vec::new();
        for manifest in &self.manifests {
            // A variable passes to every descendant of a harness, also to the other harness that
            // it started. So a rule that names the program wins at any place, and a rule of `env`
            // alone counts only when no rule of the manifest names a process of the tree.
            let first = |by_program: bool| {
                tree.processes.iter().enumerate().find(|(_, process)| {
                    manifest
                        .rules
                        .iter()
                        .any(|rule| rule.names_program() == by_program && rule.matches(process))
                })
            };
            let hit = first(true)
                .map(|hit| (true, hit))
                .or_else(|| first(false).map(|hit| (false, hit)));
            if let Some((by_program, (place, process))) = hit {
                let detection = Detection {
                    manifest: manifest.id.clone(),
                    harness: manifest.harness,
                    priority: manifest.priority,
                    remote_loop: manifest.remote_loop.clone(),
                    pid: process.pid,
                    start: process.start,
                    place,
                };
                hits.push((by_program, detection));
            }
        }
        // The higher priority first, then the match by the program, then the nearer process, then
        // the id, so that the answer does not depend on the order of the manifests.
        hits.sort_by(|(a_program, a), (b_program, b)| {
            b.priority
                .cmp(&a.priority)
                .then(b_program.cmp(a_program))
                .then(a.place.cmp(&b.place))
                .then(a.manifest.cmp(&b.manifest))
        });
        let mut hits = hits.into_iter();
        let Some((by_program, best)) = hits.next() else {
            return Found::default();
        };
        let tied = hits
            .filter(|(program, other)| other.priority == best.priority && *program == by_program)
            .map(|(_, other)| other.manifest)
            .collect();
        Found {
            best: Some(best),
            tied,
        }
    }
}

impl Rule {
    fn compile(rule: &ProcessRule) -> Option<Self> {
        let regex = |text: &Option<String>| -> Result<Option<Regex>, regex::Error> {
            text.as_deref().map(Regex::new).transpose()
        };
        Some(Self {
            exe_basename: rule.exe_basename.clone(),
            interpreter: rule.interpreter.clone(),
            script_regex: regex(&rule.script_regex).ok()?,
            exe_path_regex: regex(&rule.exe_path_regex).ok()?,
            argv_regex: regex(&rule.argv_regex).ok()?,
            argv0_basename: rule.argv0_basename.clone(),
            env: rule.env.clone(),
        })
    }

    /// Whether the rule has any key. A rule without keys matches nothing: it would match every
    /// process.
    fn has_keys(&self) -> bool {
        self.names_program() || !self.env.is_empty()
    }

    /// Whether the rule names the program: it has a key that is not `env`. A rule of `env` alone
    /// matches every descendant of a harness, whatever program it runs.
    fn names_program(&self) -> bool {
        !self.exe_basename.is_empty()
            || !self.interpreter.is_empty()
            || self.script_regex.is_some()
            || self.exe_path_regex.is_some()
            || self.argv_regex.is_some()
            || !self.argv0_basename.is_empty()
    }

    fn matches(&self, process: &Process) -> bool {
        if !self.has_keys() {
            return false;
        }
        // `exe_basename` takes the name of the file and the name in `argv[0]`. An install often
        // links `claude` to a file with the number of the version, and the file has another name.
        if !self.exe_basename.is_empty() {
            let by_exe = process
                .exe_name()
                .is_some_and(|name| self.exe_basename.iter().any(|want| want == name));
            let by_argv0 = process
                .argv0_name()
                .is_some_and(|name| self.exe_basename.iter().any(|want| want == name));
            if !by_exe && !by_argv0 {
                return false;
            }
        }
        if !self.interpreter.is_empty()
            && !process
                .exe_name()
                .is_some_and(|name| self.interpreter.iter().any(|want| want == name))
        {
            return false;
        }
        if let Some(regex) = &self.script_regex
            && !script(process).is_some_and(|script| regex.is_match(script))
        {
            return false;
        }
        if let Some(regex) = &self.exe_path_regex
            && !process
                .exe
                .as_ref()
                .is_some_and(|path| regex.is_match(&path.to_string_lossy()))
        {
            return false;
        }
        if let Some(regex) = &self.argv_regex
            && !regex.is_match(&process.argv.join(" "))
        {
            return false;
        }
        if !self.argv0_basename.is_empty()
            && !process
                .argv0_name()
                .is_some_and(|name| self.argv0_basename.iter().any(|want| want == name))
        {
            return false;
        }
        if !self.env.is_empty() {
            let Some(env) = &process.env else {
                return false;
            };
            if !self.env.iter().all(|(k, v)| env.get(k) == Some(v)) {
                return false;
            }
        }
        true
    }
}

/// Whether the command line asks for a run with no approval dialog (`[yolo]` of the manifest). A flag
/// does it alone, and so does a run of arguments that come one after the other. `--flag=value` counts
/// as the two arguments `--flag value`.
pub fn is_yolo(yolo: &Yolo, argv: &[String]) -> bool {
    let args: Vec<&str> = argv
        .iter()
        .skip(1)
        .flat_map(|arg| match arg.split_once('=') {
            Some((flag, value)) if flag.starts_with('-') => vec![flag, value],
            _ => vec![arg.as_str()],
        })
        .collect();
    yolo.flags.iter().any(|flag| args.contains(&flag.as_str()))
        || yolo.sequences.iter().any(|sequence| {
            !sequence.is_empty()
                && args.windows(sequence.len()).any(|window| {
                    window
                        .iter()
                        .zip(sequence)
                        .all(|(arg, want)| *arg == want.as_str())
                })
        })
}

/// The script of an interpreter: the first argument after `argv[0]` that is not a flag. A shebang
/// script and `node script.js` both give the script path there.
fn script(process: &Process) -> Option<&str> {
    process
        .argv
        .iter()
        .skip(1)
        .map(String::as_str)
        .find(|arg| !arg.starts_with('-'))
}

#[cfg(test)]
mod tests;
