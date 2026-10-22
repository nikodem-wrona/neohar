//! The risk rules of a command.
//!
//! The rules read the command line the way a shell does, in [`lex`](super::lex), and judge each
//! command of the line. The risk of the line is the highest risk of its commands. A command that no
//! rule knows is `unknown`. A rule never says `low` for something it cannot see through: a variable
//! as the program, a script that the shell fetches, or a here-document that runs a substitution.

use std::path::{Path, PathBuf};

use super::{
    Context, Risk,
    lex::{self, Op, Token, Word},
    network, paths, tables,
};

/// How deep substitutions and `sh -c` scripts nest before the rules give up.
const MAX_DEPTH: usize = 4;
/// A command longer than this is not analyzed.
pub(super) const MAX_SOURCE: usize = 256 * 1024;
const MAX_REASONS: usize = 8;

/// What the rules found.
#[derive(Debug, Default)]
pub(super) struct Verdict {
    pub risk: Option<Risk>,
    /// Each reason with the risk that it gave.
    entries: Vec<(Risk, String)>,
}

/// The most reasons that are kept before the filter. The card shows [`MAX_REASONS`] of them.
const MAX_ENTRIES: usize = 64;

impl Verdict {
    /// Raises the risk and gives the reason.
    pub fn add(&mut self, risk: Risk, reason: impl Into<String>) {
        self.at_least(risk);
        let reason = reason.into();
        if self.entries.len() < MAX_ENTRIES && !self.entries.iter().any(|(_, r)| *r == reason) {
            self.entries.push((risk, reason));
        }
    }

    /// Raises the risk with no reason of its own.
    pub fn at_least(&mut self, risk: Risk) {
        if self.risk.is_none_or(|current| risk > current) {
            self.risk = Some(risk);
        }
    }

    pub fn merge(&mut self, other: Verdict) {
        if let Some(risk) = other.risk {
            self.at_least(risk);
        }
        for (risk, reason) in other.entries {
            if self.entries.len() < MAX_ENTRIES && !self.entries.iter().any(|(_, r)| *r == reason) {
                self.entries.push((risk, reason));
            }
        }
    }

    /// The reasons for the risk of the whole line: those that gave this risk. A reason that gave
    /// a lower risk (the `ls` of `ls; rm -rf x`) says nothing about why the line is as it is.
    pub fn reasons(&self) -> Vec<String> {
        let risk = self.risk.unwrap_or(Risk::Unknown);
        let mut reasons: Vec<String> = self
            .entries
            .iter()
            .filter(|(entry, _)| *entry == risk)
            .map(|(_, reason)| reason.clone())
            .take(MAX_REASONS)
            .collect();
        if reasons.is_empty() {
            reasons.push("neohar cannot tell more about what this does.".to_owned());
        }
        reasons
    }
}

/// One command of a line, with its redirections.
struct Segment<'a> {
    words: Vec<&'a Word>,
    redirects: Vec<Redirect<'a>>,
    /// A pipe feeds this command.
    piped: bool,
}

struct Redirect<'a> {
    op: &'a str,
    target: Option<&'a Word>,
}

/// Ends the command that is being read. `piped` says whether a pipe feeds the next one.
fn close<'a>(current: &mut Segment<'a>, out: &mut Vec<Segment<'a>>, piped: bool) {
    if current.words.is_empty() && current.redirects.is_empty() {
        current.piped = piped;
        return;
    }
    out.push(std::mem::replace(
        current,
        Segment {
            words: Vec::new(),
            redirects: Vec::new(),
            piped,
        },
    ));
}

fn segments(tokens: &[Token]) -> Vec<Segment<'_>> {
    let mut out = Vec::new();
    let mut current = Segment {
        words: Vec::new(),
        redirects: Vec::new(),
        piped: false,
    };
    let mut pending_redirect = false;
    let mut next_piped = false;
    for token in tokens {
        match token {
            Token::Word(word) => {
                if pending_redirect {
                    if let Some(last) = current.redirects.last_mut() {
                        last.target = Some(word);
                    }
                    pending_redirect = false;
                } else {
                    current.words.push(word);
                }
            }
            Token::Redirect { op, .. } => {
                current.redirects.push(Redirect {
                    op: op.as_str(),
                    target: None,
                });
                pending_redirect = true;
            }
            Token::Op(op, _) => {
                pending_redirect = false;
                next_piped = *op == Op::Pipe;
                close(&mut current, &mut out, next_piped);
            }
            Token::Comment(_) => {}
        }
    }
    close(&mut current, &mut out, next_piped);
    out
}

/// `NAME=value` before a program.
fn is_assignment(word: &Word) -> bool {
    if word.quoted && !word.text.contains('=') {
        return false;
    }
    let Some((name, _)) = word.text.split_once('=') else {
        return false;
    };
    name.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

pub(super) fn basename(program: &str) -> &str {
    program.rsplit('/').next().unwrap_or(program)
}

/// The words that are not flags. Everything after `--` is a word.
fn operands<'a>(args: &[&'a Word]) -> Vec<&'a Word> {
    let mut out = Vec::new();
    let mut ended = false;
    for arg in args {
        if ended || !arg.text.starts_with('-') || arg.text == "-" {
            out.push(*arg);
        } else if arg.text == "--" {
            ended = true;
        }
    }
    out
}

/// Whether a flag is present, as a letter in a short cluster (`-rf`) or as a long name.
fn has_flag(args: &[&Word], short: &[char], long: &[&str]) -> bool {
    for arg in args {
        let text = arg.text.as_str();
        if text == "--" {
            break;
        }
        if let Some(name) = text.strip_prefix("--") {
            let name = name.split('=').next().unwrap_or(name);
            if long.contains(&name) {
                return true;
            }
        } else if let Some(cluster) = text.strip_prefix('-')
            && !cluster.is_empty()
            && cluster.chars().all(|c| c.is_ascii_alphabetic())
            && cluster.chars().any(|c| short.contains(&c))
        {
            return true;
        }
    }
    false
}

fn texts<'a>(words: &[&'a Word]) -> Vec<&'a str> {
    words.iter().map(|w| w.text.as_str()).collect()
}

pub(super) struct Analyzer<'c, 'p> {
    ctx: &'c Context<'p>,
    cwd: PathBuf,
    workspace: PathBuf,
    depth: usize,
    /// A pipe feeds the command that is being judged.
    piped: bool,
}

pub(super) fn assess(source: &str, ctx: &Context<'_>) -> Verdict {
    let mut analyzer = Analyzer {
        ctx,
        cwd: paths::normalize(ctx.cwd),
        workspace: paths::normalize(ctx.workspace),
        depth: 0,
        piped: false,
    };
    analyzer.text(source)
}

impl Analyzer<'_, '_> {
    fn text(&mut self, source: &str) -> Verdict {
        let mut verdict = Verdict::default();
        if source.len() > MAX_SOURCE {
            verdict.add(
                Risk::High,
                "The command is longer than 256 KB, and neohar did not read it.",
            );
            return verdict;
        }
        let lexed = lex::lex(source);
        if lexed.open {
            verdict.add(
                Risk::Unknown,
                "The command has a quote or a bracket that is not closed.",
            );
        }
        if !lexed.nested.is_empty() {
            if self.depth >= MAX_DEPTH {
                verdict.add(
                    Risk::Unknown,
                    "The command nests commands too deep, and neohar did not read all of them.",
                );
            } else {
                self.depth += 1;
                for nested in &lexed.nested {
                    let inner = self.text(nested);
                    verdict.merge(inner);
                }
                self.depth -= 1;
            }
        }
        let mut before: Option<String> = None;
        let segments = segments(&lexed.tokens);
        for segment in &segments {
            let (inner, program) =
                self.segment(segment, before.as_deref().filter(|_| segment.piped));
            verdict.merge(inner);
            before = program;
        }
        sql(source, &mut verdict);
        if verdict.risk.is_none() {
            verdict.add(Risk::Unknown, "The command is empty.");
        }
        verdict
    }

    /// The verdict for one command and the name of its program.
    fn segment(
        &mut self,
        segment: &Segment<'_>,
        piped_from: Option<&str>,
    ) -> (Verdict, Option<String>) {
        let mut verdict = Verdict::default();
        let mut words: &[&Word] = &segment.words;
        while let Some(first) = words.first() {
            if is_assignment(first)
                || (!first.quoted && tables::RESERVED.contains(&first.text.as_str()))
            {
                words = &words[1..];
            } else {
                break;
            }
        }
        self.redirects(segment, &mut verdict);
        let Some(first) = words.first() else {
            if verdict.risk.is_none() {
                verdict.add(Risk::Low, "The command only sets shell variables.");
            }
            return (verdict, None);
        };
        let name = basename(&first.text).to_owned();
        if first.dynamic {
            verdict.add(
                Risk::Unknown,
                "The program is not known before the command runs.",
            );
        }
        if let Some(from) = piped_from
            && tables::INTERPRETERS.contains(&name.as_str())
            && tables::DOWNLOADERS.contains(&from)
        {
            verdict.add(
                Risk::High,
                "The command runs a script that another command downloads.",
            );
        }
        if let Some(from) = piped_from
            && tables::INTERPRETERS.contains(&name.as_str())
            && tables::DECODERS.contains(&from)
        {
            verdict.add(
                Risk::High,
                "The command runs text that another command decodes.",
            );
        }
        self.piped = segment.piped;
        self.program(&name, &words[1..], &mut verdict);
        (verdict, Some(name))
    }

    fn redirects(&mut self, segment: &Segment<'_>, verdict: &mut Verdict) {
        for redirect in &segment.redirects {
            let writes = redirect.op.contains('>');
            if !writes {
                continue;
            }
            // `2>&1` and `>&2` copy a file descriptor and open no file.
            if redirect.op.ends_with('&') {
                continue;
            }
            let Some(target) = redirect.target else {
                continue;
            };
            if target.text == "-" {
                continue;
            }
            if let Some(path) =
                paths::resolve(&target.text, target.dynamic, &self.cwd, self.ctx.home)
            {
                self.check_write(&path, verdict);
            } else {
                verdict.at_least(Risk::Unknown);
            }
        }
    }

    /// A program writes to `path`.
    fn check_write(&self, path: &Path, verdict: &mut Verdict) {
        if paths::is_harmless_target(path) {
            return;
        }
        verdict.at_least(Risk::Unknown);
        if !paths::is_inside(path, &self.workspace) {
            verdict.add(
                Risk::High,
                format!(
                    "The command writes outside the workspace, to {}.",
                    path.display()
                ),
            );
        } else {
            verdict.add(Risk::Unknown, "The command changes files in the workspace.");
        }
        if paths::is_credential(path) {
            verdict.add(
                Risk::High,
                format!(
                    "The command changes a file that can hold secrets, {}.",
                    path.display()
                ),
            );
        }
    }

    fn write_targets(&mut self, targets: &[&Word], verdict: &mut Verdict) {
        for target in targets {
            if let Some(path) =
                paths::resolve(&target.text, target.dynamic, &self.cwd, self.ctx.home)
            {
                self.check_write(&path, verdict);
            } else {
                verdict.at_least(Risk::Unknown);
            }
        }
    }

    fn nested_text(&mut self, text: &str, verdict: &mut Verdict) {
        if self.depth >= MAX_DEPTH {
            verdict.add(
                Risk::Unknown,
                "The command nests commands too deep, and neohar did not read all of them.",
            );
            return;
        }
        self.depth += 1;
        let inner = self.text(text);
        self.depth -= 1;
        verdict.merge(inner);
    }

    /// Whether a command line has a downloader as one of its programs.
    fn downloads(&self, text: &str) -> bool {
        let lexed = lex::lex(text);
        segments(&lexed.tokens).iter().any(|segment| {
            segment
                .words
                .iter()
                .find(|w| !is_assignment(w))
                .is_some_and(|w| tables::DOWNLOADERS.contains(&basename(&w.text)))
        })
    }

    fn program(&mut self, name: &str, args: &[&Word], verdict: &mut Verdict) {
        if matches!(name, "sudo" | "doas" | "su" | "pkexec" | "runuser") {
            verdict.add(
                Risk::High,
                "The command runs with the rights of another user.",
            );
            return;
        }
        if self.wrapper(name, args, verdict) || self.interpreter(name, args, verdict) {
            return;
        }
        if !tables::READ_ONLY.contains(&name) && is_migration(name, args) {
            verdict.add(
                Risk::Medium,
                "The command changes the schema of a database.",
            );
        }
        match name {
            "cd" | "pushd" => {
                verdict.add(Risk::Low, "The command changes the directory only.");
                if let Some(target) = operands(args).first()
                    && let Some(path) =
                        paths::resolve(&target.text, target.dynamic, &self.cwd, self.ctx.home)
                {
                    self.cwd = path;
                }
            }
            "rm" => {
                if has_flag(args, &['r', 'R'], &["recursive"]) {
                    verdict.add(
                        Risk::High,
                        "The command removes files and folders, with everything in them.",
                    );
                } else {
                    verdict.add(Risk::Unknown, "The command removes files.");
                }
                self.write_targets(&operands(args), verdict);
            }
            "chmod" | "chown" | "chgrp" => {
                if has_flag(args, &['R'], &["recursive"]) {
                    verdict.add(Risk::High, "The command changes the rights of many files.");
                }
                let operands = operands(args);
                // The first operand is the mode or the owner.
                self.write_targets(operands.get(1..).unwrap_or(&[]), verdict);
                verdict.at_least(Risk::Unknown);
            }
            "find" => self.find(args, verdict),
            "git" => self.git(args, verdict),
            "sed" if has_flag(args, &['i'], &["in-place"]) => {
                verdict.add(Risk::Unknown, "The command changes files in place.");
                let operands = operands(args);
                self.write_targets(operands.get(1..).unwrap_or(&[]), verdict);
            }
            "dd" => {
                for arg in args {
                    if let Some(target) = arg.text.strip_prefix("of=") {
                        let word = Word {
                            text: target.to_owned(),
                            ..(*arg).clone()
                        };
                        self.write_targets(&[&word], verdict);
                    }
                }
                verdict.add(Risk::Unknown, "The command copies raw data.");
            }
            "tee" => {
                let files = operands(args);
                if files.is_empty() {
                    verdict.add(Risk::Low, "The command passes its input on.");
                } else {
                    self.write_targets(&files, verdict);
                }
            }
            "cp" | "ln" | "install" => {
                // The last operand is the target, or the folder of `-t`.
                let operands = operands(args);
                if let Some(last) = operands.last() {
                    self.write_targets(&[last], verdict);
                }
                verdict.at_least(Risk::Unknown);
            }
            "mv" | "touch" | "mkdir" | "rmdir" | "unlink" | "truncate" | "shred" => {
                if name == "shred" {
                    verdict.add(
                        Risk::High,
                        "The command overwrites files so that nobody can read them again.",
                    );
                }
                self.write_targets(&operands(args), verdict);
                verdict.at_least(Risk::Unknown);
            }
            "kubectl" | "helm" | "terraform" | "tofu" | "docker" | "podman" | "gh"
            | "docker-compose" => {
                self.infra(name, args, verdict);
            }
            "npm" | "pnpm" | "yarn" | "bun" | "npx" | "pnpx" | "bunx" | "deno" | "corepack" => {
                self.javascript(name, args, verdict);
            }
            "cargo" => self.cargo(args, verdict),
            "make" | "gmake" => {
                let project = self.ctx.project;
                self.targets(args, &project.make_targets, "make", verdict);
            }
            "just" => {
                let project = self.ctx.project;
                self.targets(args, &project.just_recipes, "just", verdict);
            }
            "go" => self.go(args, verdict),
            "mvn" | "mvnw" | "gradle" | "gradlew" => {
                match operands(args).first().map(|w| w.text.as_str()) {
                    Some("deploy" | "publish" | "release") => {
                        verdict.add(Risk::High, "The command publishes an artifact.")
                    }
                    Some(
                        "test" | "verify" | "compile" | "package" | "check" | "build" | "assemble"
                        | "lint" | "clean" | "dependencies" | "tasks",
                    ) => {
                        verdict.add(Risk::Low, "The command builds or tests the project.");
                    }
                    _ => verdict.add(
                        Risk::Unknown,
                        format!("neohar has no rule for this {name} command."),
                    ),
                }
            }
            "python" | "python3" | "py" => self.python(args, verdict),
            _ if network::is_network_program(name) => self.network(name, args, verdict),
            _ if tables::INSTALLERS.contains(&name) => self.installer(name, args, verdict),
            _ if tables::HIGH_PROGRAMS.contains(&name) || name.starts_with("mkfs.") => {
                verdict.add(
                    Risk::High,
                    format!("The command {name} can destroy data or stop the machine."),
                );
            }
            _ if tables::READ_ONLY.contains(&name) => {
                if name == "sort" && has_flag(args, &['o'], &["output"]) {
                    verdict.add(Risk::Unknown, "The command writes a file.");
                } else {
                    verdict.add(Risk::Low, "The command only reads.");
                }
            }
            _ if tables::RUNNERS.contains(&name) => self.runner(name, args, verdict),
            _ => verdict.add(Risk::Unknown, "neohar has no rule for this command."),
        }
    }

    /// Programs that run another program: `env`, `time`, `xargs` and the like.
    fn wrapper(&mut self, name: &str, args: &[&Word], verdict: &mut Verdict) -> bool {
        let inner: Vec<&Word> = match name {
            "env" => {
                let mut rest = args;
                while let Some(first) = rest.first() {
                    if first.text == "-u" || first.text == "-C" {
                        rest = rest.get(2..).unwrap_or(&[]);
                    } else if first.text.starts_with('-') || is_assignment(first) {
                        rest = &rest[1..];
                    } else {
                        break;
                    }
                }
                if rest.is_empty() {
                    verdict.add(
                        Risk::Unknown,
                        "The command prints the environment, which can hold secrets.",
                    );
                    return true;
                }
                rest.to_vec()
            }
            "command" => {
                if has_flag(args, &['v', 'V'], &[]) {
                    verdict.add(Risk::Low, "The command looks up a program.");
                    return true;
                }
                skip_flags(args, &[]).to_vec()
            }
            "builtin" | "exec" | "nohup" | "time" | "setsid" | "caffeinate" | "unbuffer" => {
                skip_flags(args, &[]).to_vec()
            }
            "nice" => skip_flags(args, &["-n", "--adjustment"]).to_vec(),
            "ionice" => skip_flags(args, &["-c", "-n", "-p", "-P", "-u"]).to_vec(),
            "stdbuf" => skip_flags(args, &["-i", "-o", "-e"]).to_vec(),
            "xargs" => skip_flags(args, &["-n", "-I", "-P", "-L", "-d", "-s", "-E", "-a"]).to_vec(),
            "watch" => skip_flags(args, &["-n", "--interval"]).to_vec(),
            "timeout" => {
                // The first operand is the time limit.
                let rest = skip_flags(args, &["-s", "-k", "--signal", "--kill-after"]);
                rest.get(1..).unwrap_or(&[]).to_vec()
            }
            _ => return false,
        };
        let Some(first) = inner.first() else {
            verdict.add(
                Risk::Unknown,
                format!("The command {name} has no command to run."),
            );
            return true;
        };
        if first.dynamic {
            verdict.add(
                Risk::Unknown,
                "The program is not known before the command runs.",
            );
        }
        let inner_name = basename(&first.text).to_owned();
        self.program(&inner_name, &inner[1..], verdict);
        true
    }

    /// Shells and interpreters: the code they run may be in the arguments.
    fn interpreter(&mut self, name: &str, args: &[&Word], verdict: &mut Verdict) -> bool {
        match name {
            "sh" | "bash" | "zsh" | "dash" | "ksh" | "fish" | "ash" => {
                let mut script = None;
                for (at, arg) in args.iter().enumerate() {
                    let text = arg.text.as_str();
                    if text.starts_with('-') && !text.starts_with("--") && text[1..].contains('c') {
                        script = args.get(at + 1);
                        break;
                    }
                }
                match script {
                    Some(script) => {
                        self.nested_text(&script.text, verdict);
                        if script.subs.iter().any(|sub| self.downloads(sub)) {
                            verdict.add(Risk::High, "The command runs a script that it downloads.");
                        }
                    }
                    // A shell that reads its input from a pipe: the pipe has its own rules.
                    None if self.piped && args.is_empty() => verdict.at_least(Risk::Unknown),
                    None => verdict.add(
                        Risk::Unknown,
                        "The command runs a script that neohar did not read.",
                    ),
                }
                self.downloaded_argument(args, verdict);
                true
            }
            "eval" => {
                let joined = args
                    .iter()
                    .map(|a| a.text.as_str())
                    .collect::<Vec<_>>()
                    .join(" ");
                self.nested_text(&joined, verdict);
                if args
                    .iter()
                    .any(|arg| arg.subs.iter().any(|sub| self.downloads(sub)))
                {
                    verdict.add(Risk::High, "The command runs a script that it downloads.");
                }
                verdict.at_least(Risk::Unknown);
                true
            }
            "source" | "." => {
                verdict.add(
                    Risk::Unknown,
                    "The command runs a script that neohar did not read.",
                );
                self.downloaded_argument(args, verdict);
                true
            }
            "node" | "deno-run" | "perl" | "ruby" | "php" | "lua" | "osascript" | "pwsh"
            | "powershell" => {
                verdict.add(
                    Risk::Unknown,
                    "The command runs code that neohar did not read.",
                );
                self.downloaded_argument(args, verdict);
                true
            }
            _ => false,
        }
    }

    /// `bash <(curl …)`: an argument that is a download.
    fn downloaded_argument(&self, args: &[&Word], verdict: &mut Verdict) {
        if args
            .iter()
            .any(|arg| arg.subs.iter().any(|sub| self.downloads(sub)))
        {
            verdict.add(Risk::High, "The command runs a script that it downloads.");
        }
    }

    fn find(&mut self, args: &[&Word], verdict: &mut Verdict) {
        let texts = texts(args);
        if texts.contains(&"-delete") {
            verdict.add(Risk::High, "The command deletes the files that it finds.");
            return;
        }
        if let Some(at) = texts
            .iter()
            .position(|t| matches!(*t, "-exec" | "-execdir" | "-ok" | "-okdir"))
        {
            let end = texts[at..]
                .iter()
                .position(|t| matches!(*t, ";" | "+"))
                .map_or(args.len(), |n| at + n);
            let inner = &args[at + 1..end];
            if let Some(first) = inner.first() {
                let inner_name = basename(&first.text).to_owned();
                self.program(&inner_name, &inner[1..], verdict);
            } else {
                verdict.add(
                    Risk::Unknown,
                    "The command runs another command on each file.",
                );
            }
            return;
        }
        if texts
            .iter()
            .any(|t| matches!(*t, "-fprint" | "-fprint0" | "-fprintf" | "-fls"))
        {
            verdict.add(
                Risk::Unknown,
                "The command writes a list of files to a file.",
            );
            return;
        }
        verdict.add(Risk::Low, "The command only reads.");
    }

    fn git(&mut self, args: &[&Word], verdict: &mut Verdict) {
        let (sub, rest) = git_split(args);
        let Some(sub) = sub else {
            verdict.add(Risk::Low, "The command prints help of git.");
            return;
        };
        let has = |short: &[char], long: &[&str]| has_flag(&rest, short, long);
        if matches!(
            sub,
            "clone" | "fetch" | "pull" | "push" | "ls-remote" | "remote" | "submodule"
        ) {
            self.git_hosts(&rest, verdict);
        }
        match sub {
            "push" => {
                let force = has(
                    &['f'],
                    &[
                        "force",
                        "force-with-lease",
                        "force-if-includes",
                        "mirror",
                        "delete",
                        "prune",
                    ],
                ) || rest
                    .iter()
                    .any(|a| a.text.starts_with('+') || a.text.starts_with(':'))
                    || has(&['d'], &[]);
                if force {
                    verdict.add(Risk::High, "The command pushes with force or deletes on the remote, and can destroy commits there.");
                } else {
                    verdict.add(Risk::Medium, "The command pushes commits to a remote.");
                }
            }
            "reset" => {
                if has(&[], &["hard"]) {
                    verdict.add(
                        Risk::High,
                        "The command resets the work tree and drops changes.",
                    );
                } else {
                    verdict.add(Risk::Unknown, "The command moves the branch pointer.");
                }
            }
            "clean" => {
                if has(&['n'], &["dry-run"]) {
                    verdict.add(
                        Risk::Low,
                        "The command only lists the files that it would remove.",
                    );
                } else if has(&['f'], &["force"]) {
                    verdict.add(
                        Risk::High,
                        "The command deletes files that git does not track.",
                    );
                } else {
                    verdict.add(Risk::Unknown, "The command removes untracked files.");
                }
            }
            "commit" | "merge" | "rebase" | "cherry-pick" | "revert" | "am" | "pull" | "apply" => {
                verdict.add(
                    Risk::Medium,
                    format!("The command changes the history of the repository with git {sub}."),
                );
            }
            "tag" => {
                if rest.is_empty() || has(&['l'], &["list"]) {
                    verdict.add(Risk::Low, "The command lists tags.");
                } else {
                    verdict.add(Risk::Medium, "The command makes or removes a tag.");
                }
            }
            "stash" => match rest.first().map(|w| w.text.as_str()) {
                Some("list" | "show") => verdict.add(Risk::Low, "The command shows the stash."),
                Some("drop" | "clear") => {
                    verdict.add(Risk::Medium, "The command drops stashed changes.")
                }
                _ => verdict.add(Risk::Unknown, "The command changes the work tree."),
            },
            "checkout" | "restore" | "switch" => {
                let discards = has(&['f'], &["force", "discard-changes"])
                    || rest.iter().any(|a| a.text == "." || a.text == "--");
                if discards {
                    verdict.add(
                        Risk::Medium,
                        "The command can drop changes in the work tree.",
                    );
                } else {
                    verdict.add(Risk::Unknown, "The command changes the work tree.");
                }
            }
            "branch" => {
                let ops = operands(&rest);
                if has(&['D'], &[]) {
                    verdict.add(
                        Risk::Medium,
                        "The command deletes a branch with commits that may not be merged.",
                    );
                } else if ops.is_empty()
                    || has(
                        &['a', 'r', 'v', 'l'],
                        &["list", "show-current", "contains", "merged", "no-merged"],
                    )
                {
                    verdict.add(Risk::Low, "The command lists branches.");
                } else {
                    verdict.add(Risk::Unknown, "The command changes branches.");
                }
            }
            "remote" => {
                let first = operands(&rest).first().map(|w| w.text.clone());
                match first.as_deref() {
                    None | Some("show" | "get-url") => {
                        verdict.add(Risk::Low, "The command lists remotes.")
                    }
                    _ => verdict.add(Risk::Unknown, "The command changes the remotes."),
                }
            }
            "config" => {
                if has(&[], &["get", "list", "get-all", "get-regexp"]) || has(&['l'], &[]) {
                    verdict.add(Risk::Low, "The command reads the git configuration.");
                } else {
                    verdict.add(
                        Risk::Unknown,
                        "The command may change the git configuration.",
                    );
                }
            }
            "reflog" => {
                if rest
                    .first()
                    .is_some_and(|w| matches!(w.text.as_str(), "expire" | "delete"))
                {
                    verdict.add(
                        Risk::High,
                        "The command drops the reflog, the last way to get lost commits back.",
                    );
                } else {
                    verdict.add(Risk::Low, "The command only reads.");
                }
            }
            "gc" => {
                if rest.iter().any(|a| a.text.starts_with("--prune")) {
                    verdict.add(
                        Risk::High,
                        "The command prunes objects that nothing points to anymore.",
                    );
                } else {
                    verdict.add(Risk::Unknown, "The command cleans up the repository.");
                }
            }
            "worktree" => {
                if rest.first().is_some_and(|w| w.text == "list") {
                    verdict.add(Risk::Low, "The command lists worktrees.");
                } else {
                    verdict.add(Risk::Unknown, "The command changes worktrees.");
                }
            }
            s if tables::GIT_READ_ONLY.contains(&s) => {
                verdict.add(Risk::Low, "The command only reads the repository.");
            }
            _ => verdict.add(Risk::Unknown, "neohar has no rule for this git command."),
        }
    }

    /// A git command that names a remote by its address.
    fn git_hosts(&self, rest: &[&Word], verdict: &mut Verdict) {
        let found = network::hosts("git", rest);
        for host in found.names {
            if !network::is_allowed(&host, self.ctx.network_allow) {
                verdict.add(
                    Risk::High,
                    format!("The command connects to {host}, which is not on the allowlist."),
                );
            }
        }
    }

    fn infra(&mut self, name: &str, args: &[&Word], verdict: &mut Verdict) {
        let operands = operands(args);
        let first = operands.first().map_or("", |w| w.text.as_str());
        let second = operands.get(1).map_or("", |w| w.text.as_str());
        match (name, first, second) {
            ("kubectl", "delete", _) => {
                verdict.add(Risk::High, "The command deletes resources of a cluster.")
            }
            (
                "kubectl",
                "get" | "describe" | "logs" | "top" | "version" | "api-resources" | "explain"
                | "diff" | "events",
                _,
            )
            | ("kubectl", "config", "view" | "current-context" | "get-contexts") => {
                verdict.add(Risk::Low, "The command only reads from a cluster.");
            }
            (
                "kubectl",
                "apply" | "create" | "patch" | "replace" | "scale" | "rollout" | "exec" | "edit"
                | "drain" | "cordon" | "label" | "annotate" | "set",
                _,
            ) => {
                verdict.add(Risk::Medium, "The command changes a cluster.");
            }
            ("helm", "uninstall" | "delete", _) => {
                verdict.add(Risk::High, "The command removes a release from a cluster.")
            }
            (
                "helm",
                "list" | "ls" | "status" | "get" | "template" | "lint" | "show" | "search"
                | "version" | "history",
                _,
            ) => {
                verdict.add(Risk::Low, "The command only reads.");
            }
            ("helm", "install" | "upgrade" | "rollback", _) => {
                verdict.add(Risk::Medium, "The command changes a cluster.")
            }
            ("terraform" | "tofu", "destroy", _) => {
                verdict.add(Risk::High, "The command destroys infrastructure.")
            }
            ("terraform" | "tofu", "apply", _) => {
                verdict.add(Risk::High, "The command changes infrastructure.")
            }
            (
                "terraform" | "tofu",
                "plan" | "validate" | "show" | "output" | "version" | "providers" | "graph",
                _,
            ) => {
                verdict.add(Risk::Low, "The command only reads.");
            }
            (
                "docker" | "podman",
                "system" | "volume" | "container" | "image" | "network" | "builder",
                "prune",
            ) => {
                verdict.add(Risk::High, "The command removes unused data of containers.");
            }
            ("docker" | "podman", "volume", "rm") => {
                verdict.add(Risk::High, "The command removes a volume and its data.")
            }
            ("docker" | "podman", "rm" | "rmi", _) if has_flag(args, &['f'], &["force"]) => {
                verdict.add(
                    Risk::High,
                    "The command removes containers or images with force.",
                );
            }
            ("docker" | "podman", "push", _) => {
                verdict.add(Risk::High, "The command publishes an image.")
            }
            (
                "docker" | "podman",
                "ps" | "images" | "logs" | "inspect" | "version" | "info" | "stats" | "top"
                | "port" | "diff" | "history" | "events",
                _,
            ) => {
                verdict.add(
                    Risk::Low,
                    "The command only reads from the container engine.",
                );
            }
            ("docker", "compose", "down") | ("docker-compose", "down", _)
                if has_flag(args, &['v'], &["volumes"]) =>
            {
                verdict.add(Risk::High, "The command removes volumes and their data.");
            }
            ("docker", "compose", "ps" | "logs" | "config" | "ls")
            | ("docker-compose", "ps" | "logs" | "config", _) => {
                verdict.add(
                    Risk::Low,
                    "The command only reads from the container engine.",
                );
            }
            (
                "gh",
                "pr" | "issue" | "run" | "repo" | "release" | "workflow" | "gist",
                "view" | "list" | "status" | "diff" | "checks" | "download",
            ) => {
                verdict.add(Risk::Low, "The command only reads from GitHub.");
            }
            ("gh", "auth", "status") => verdict.add(Risk::Low, "The command only reads."),
            ("gh", "repo", "delete") => {
                verdict.add(Risk::High, "The command deletes a repository.")
            }
            ("gh", "pr" | "issue" | "release" | "workflow" | "run" | "repo" | "gist", _) => {
                verdict.add(Risk::Medium, "The command changes something on GitHub.");
            }
            ("gh", "api", _) => {
                let method = args
                    .windows(2)
                    .find(|pair| matches!(pair[0].text.as_str(), "-X" | "--method"))
                    .map(|pair| pair[1].text.to_ascii_uppercase());
                match method.as_deref() {
                    None | Some("GET") => {
                        verdict.add(Risk::Unknown, "The command reads from the GitHub API.")
                    }
                    Some("DELETE") => {
                        verdict.add(Risk::High, "The command deletes through the GitHub API.")
                    }
                    Some(_) => verdict.add(
                        Risk::Medium,
                        "The command changes something through the GitHub API.",
                    ),
                }
            }
            _ => verdict.add(
                Risk::Unknown,
                format!("neohar has no rule for this {name} command."),
            ),
        }
    }

    fn installer(&mut self, name: &str, args: &[&Word], verdict: &mut Verdict) {
        let operands: Vec<&str> = operands(args)
            .iter()
            .take(3)
            .map(|w| w.text.as_str())
            .collect();
        if operands
            .iter()
            .any(|w| matches!(*w, "publish" | "upload" | "push"))
        {
            verdict.add(Risk::High, "The command publishes a package.");
        } else if operands.iter().any(|w| tables::INSTALL_WORDS.contains(w)) {
            verdict.add(Risk::Medium, "The command installs or changes packages.");
        } else if operands.iter().any(|w| tables::LIST_WORDS.contains(w))
            || has_flag(args, &['V'], &["version"])
        {
            verdict.add(Risk::Low, "The command only lists packages.");
        } else if operands.is_empty() && matches!(name, "bundle" | "poetry" | "pipenv") {
            verdict.add(Risk::Medium, "The command installs or changes packages.");
        } else {
            verdict.add(
                Risk::Unknown,
                format!("neohar has no rule for this {name} command."),
            );
        }
    }

    fn javascript(&mut self, name: &str, args: &[&Word], verdict: &mut Verdict) {
        let operands = operands(args);
        let words = texts(&operands);
        let sub = words.first().copied();
        if matches!(name, "npx" | "pnpx" | "bunx")
            || (name == "pnpm" && sub == Some("dlx"))
            || (name == "yarn" && sub == Some("dlx"))
            || (name == "npm" && sub == Some("exec"))
        {
            let skip = usize::from(sub == Some("dlx") || sub == Some("exec"));
            let tool = words.get(skip).map_or("", |w| basename(w));
            if tables::RUNNERS.contains(&tool) && !has_flag(args, &[], &["fix", "write"]) {
                verdict.add(
                    Risk::Low,
                    format!("The command runs {tool}, a test, lint or build tool."),
                );
            } else {
                verdict.add(
                    Risk::Medium,
                    "The command runs a package that it may fetch from the registry.",
                );
            }
            return;
        }
        if name == "deno" {
            match sub {
                Some("test" | "lint" | "check" | "bench" | "doc" | "info" | "types") => {
                    verdict.add(Risk::Low, "The command runs a check of deno.");
                }
                Some("fmt") if has_flag(args, &[], &["check"]) => {
                    verdict.add(Risk::Low, "The command checks the format.")
                }
                Some("install" | "add" | "remove" | "upgrade") => {
                    verdict.add(Risk::Medium, "The command installs or changes packages.")
                }
                _ => verdict.add(Risk::Unknown, "neohar has no rule for this deno command."),
            }
            return;
        }
        match sub {
            None => {
                if name == "yarn" {
                    verdict.add(Risk::Medium, "The command installs packages.");
                } else {
                    verdict.add(
                        Risk::Unknown,
                        format!("neohar has no rule for this {name} command."),
                    );
                }
            }
            Some(
                "install" | "i" | "add" | "ci" | "update" | "up" | "upgrade" | "uninstall"
                | "remove" | "rm" | "un" | "link" | "unlink" | "dedupe" | "prune" | "rebuild"
                | "import" | "install-test" | "it",
            ) => {
                verdict.add(Risk::Medium, "The command installs or changes packages.");
            }
            Some("publish") => verdict.add(Risk::High, "The command publishes a package."),
            Some("run" | "run-script") => match words.get(1) {
                Some(script) => self.script(script, verdict),
                None => verdict.add(Risk::Low, "The command lists the scripts of the project."),
            },
            Some("test" | "t") => {
                if name == "bun" {
                    verdict.add(Risk::Low, "The command runs the tests with bun.");
                } else {
                    self.script("test", verdict);
                }
            }
            Some("start" | "stop" | "restart") if name == "npm" => {
                verdict.add(
                    Risk::Unknown,
                    "The command runs the start script of the project.",
                );
            }
            Some(
                "list" | "ls" | "outdated" | "why" | "audit" | "view" | "info" | "config" | "root"
                | "bin" | "prefix" | "whoami" | "ping" | "pack" | "help" | "version" | "-v",
            ) => {
                verdict.add(Risk::Low, "The command only reads.");
            }
            Some(other)
                if matches!(name, "pnpm" | "yarn" | "bun")
                    && !tables::JS_BUILTINS.contains(&other) =>
            {
                self.script(other, verdict);
            }
            Some(_) => verdict.add(
                Risk::Unknown,
                format!("neohar has no rule for this {name} command."),
            ),
        }
    }

    /// A script of `package.json` is low risk when the project names it and it is a test, a lint or
    /// a build.
    fn script(&mut self, script: &str, verdict: &mut Verdict) {
        let known = self.ctx.project.package_scripts.contains(script);
        if known && tables::is_runner_name(script) {
            verdict.add(
                Risk::Low,
                format!(
                    "The command runs the script {script} of package.json, a test, lint or build."
                ),
            );
        } else if known {
            verdict.add(
                Risk::Unknown,
                format!("The command runs the script {script} of package.json."),
            );
        } else {
            verdict.add(
                Risk::Unknown,
                format!("The script {script} is not in package.json."),
            );
        }
    }

    /// `make` and `just`.
    fn targets(
        &mut self,
        args: &[&Word],
        known: &std::collections::BTreeSet<String>,
        tool: &str,
        verdict: &mut Verdict,
    ) {
        if has_flag(
            args,
            &['n'],
            &["dry-run", "just-print", "list", "summary", "show"],
        ) {
            verdict.add(
                Risk::Low,
                format!("The command only prints what {tool} would do."),
            );
            return;
        }
        let wanted: Vec<&str> = operands(args)
            .iter()
            .map(|w| w.text.as_str())
            .filter(|w| !w.contains('='))
            .collect();
        if wanted.is_empty() {
            verdict.add(
                Risk::Unknown,
                format!("The command runs the default target of {tool}."),
            );
        } else if wanted
            .iter()
            .all(|w| known.contains(*w) && tables::is_runner_name(w))
        {
            verdict.add(
                Risk::Low,
                format!("The command runs {tool} targets for a test, lint or build."),
            );
        } else {
            verdict.add(
                Risk::Unknown,
                format!("The command runs {tool} targets that are not a test, lint or build."),
            );
        }
    }

    fn cargo(&mut self, args: &[&Word], verdict: &mut Verdict) {
        let (sub, rest) = cargo_split(args);
        match sub {
            Some(
                "build" | "check" | "test" | "clippy" | "bench" | "doc" | "tree" | "metadata"
                | "fetch" | "verify-project" | "locate-project" | "search" | "help" | "version"
                | "nextest" | "llvm-cov" | "deny" | "audit" | "machete" | "udeps",
            ) => {
                verdict.add(Risk::Low, "The command builds or tests the project.");
            }
            Some("run" | "r") => {
                let named = rest
                    .windows(2)
                    .find(|pair| matches!(pair[0].text.as_str(), "--bin" | "--example"))
                    .map(|pair| pair[1].text.clone());
                match named {
                    Some(target) if !self.ctx.project.cargo_targets.contains(&target) => {
                        verdict.add(
                            Risk::Unknown,
                            format!("The target {target} is not in Cargo.toml."),
                        );
                    }
                    _ => verdict.add(Risk::Low, "The command runs a program of the project."),
                }
            }
            Some("fmt") => {
                if has_flag(&rest, &[], &["check"]) {
                    verdict.add(Risk::Low, "The command checks the format.");
                } else {
                    verdict.add(Risk::Unknown, "The command formats the source files.");
                }
            }
            Some("install" | "add" | "remove" | "update" | "rm" | "upgrade") => {
                verdict.add(Risk::Medium, "The command installs or changes packages.");
            }
            Some("publish" | "yank" | "login" | "owner") => verdict.add(
                Risk::High,
                "The command publishes or changes a crate on the registry.",
            ),
            _ => verdict.add(Risk::Unknown, "neohar has no rule for this cargo command."),
        }
    }

    fn go(&mut self, args: &[&Word], verdict: &mut Verdict) {
        match operands(args).first().map(|w| w.text.as_str()) {
            Some("test" | "vet" | "build" | "list" | "version" | "env" | "doc" | "help") => {
                verdict.add(Risk::Low, "The command builds or tests the project.");
            }
            Some("get" | "install" | "mod" | "work") => {
                verdict.add(Risk::Medium, "The command installs or changes packages.")
            }
            _ => verdict.add(Risk::Unknown, "neohar has no rule for this go command."),
        }
    }

    fn python(&mut self, args: &[&Word], verdict: &mut Verdict) {
        let texts = texts(args);
        if let Some(at) = texts.iter().position(|t| *t == "-m")
            && let Some(module) = texts.get(at + 1)
        {
            match *module {
                "pytest" | "unittest" | "mypy" | "pyright" | "flake8" | "pylint" | "tox"
                | "nox" | "doctest" => {
                    verdict.add(Risk::Low, "The command runs a test or check.");
                }
                "pip" | "pipx" | "uv" => {
                    let rest = &args[at + 2..];
                    self.installer(module, rest, verdict);
                }
                "venv" | "ensurepip" => {
                    verdict.add(Risk::Medium, "The command installs or changes packages.")
                }
                _ => verdict.add(
                    Risk::Unknown,
                    "The command runs a module that neohar has no rule for.",
                ),
            }
            return;
        }
        verdict.add(
            Risk::Unknown,
            "The command runs code that neohar did not read.",
        );
        self.downloaded_argument(args, verdict);
    }

    fn runner(&mut self, name: &str, args: &[&Word], verdict: &mut Verdict) {
        if has_flag(
            args,
            &[],
            &[
                "fix",
                "write",
                "autocorrect",
                "fix-only",
                "unsafe-fixes",
                "in-place",
            ],
        ) || (name == "ruff" && operands(args).first().is_some_and(|w| w.text == "format"))
            || (name == "prettier" && !has_flag(args, &[], &["check", "list-different"]))
            || (matches!(
                name,
                "black" | "isort" | "gofmt" | "rustfmt" | "clang-format"
            ) && !has_flag(args, &[], &["check", "diff", "dry-run"]))
        {
            verdict.add(
                Risk::Unknown,
                format!("The command {name} may change files."),
            );
        } else {
            verdict.add(
                Risk::Low,
                format!("The command runs {name}, a test, lint or build tool."),
            );
        }
    }

    fn network(&mut self, name: &str, args: &[&Word], verdict: &mut Verdict) {
        let found = network::hosts(name, args);
        // `rsync a/ b/` and `scp a b` with no remote operand copy files on this machine.
        if matches!(name, "scp" | "rsync")
            && found.names.is_empty()
            && !args.iter().any(|arg| arg.dynamic)
        {
            if let Some(last) = operands(args).last() {
                self.write_targets(&[last], verdict);
            }
            verdict.at_least(Risk::Unknown);
            return;
        }
        let mut blocked = false;
        if found.unknown {
            blocked = true;
            verdict.add(
                Risk::High,
                "The command connects to a host that its text does not name.",
            );
        }
        let mut seen: Vec<&str> = Vec::new();
        for host in &found.names {
            if seen.contains(&host.as_str()) {
                continue;
            }
            seen.push(host);
            if !network::is_allowed(host, self.ctx.network_allow) {
                blocked = true;
                verdict.add(
                    Risk::High,
                    format!("The command connects to {host}, which is not on the allowlist."),
                );
            }
        }
        if !blocked {
            if found.names.is_empty() {
                verdict.add(Risk::Low, "The command only prints help or a version.");
            } else {
                verdict.add(
                    Risk::Unknown,
                    "The command connects to a host on the allowlist.",
                );
            }
        }
        if matches!(name, "scp" | "rsync") {
            let operands = operands(args);
            if let Some(last) = operands.last()
                && !found.names.is_empty()
                && !last.text.contains(':')
            {
                self.write_targets(&[last], verdict);
            }
        }
        if matches!(name, "curl" | "wget")
            && has_flag(args, &['o', 'O'], &["output", "output-document"])
        {
            let operands = args
                .windows(2)
                .filter(|pair| {
                    matches!(
                        pair[0].text.as_str(),
                        "-o" | "-O" | "--output" | "--output-document"
                    )
                })
                .map(|pair| pair[1]);
            let targets: Vec<&Word> = operands.collect();
            self.write_targets(&targets, verdict);
        }
    }
}

/// A command that migrates a database: `migrate`, `db:migrate`, `migration run`, `alembic upgrade`.
fn is_migration(name: &str, args: &[&Word]) -> bool {
    if matches!(name, "alembic" | "goose" | "dbmate") {
        return operands(args).first().is_some_and(|w| {
            matches!(
                w.text.as_str(),
                "upgrade" | "downgrade" | "up" | "down" | "migrate" | "redo"
            )
        });
    }
    if name == "flyway" {
        return operands(args)
            .first()
            .is_some_and(|w| matches!(w.text.as_str(), "migrate" | "undo" | "repair"));
    }
    operands(args).iter().take(3).any(|w| {
        let word = w.text.as_str();
        matches!(word, "migrate" | "migration" | "db:migrate" | "db:rollback")
            || word.starts_with("migrate:")
            || word.starts_with("migration:")
            || word.starts_with("db:migrate")
    })
}

/// Splits the arguments of `git` into the subcommand and what follows it. The options of git
/// itself come first.
fn git_split<'a>(args: &[&'a Word]) -> (Option<&'a str>, Vec<&'a Word>) {
    let mut at = 0;
    while let Some(arg) = args.get(at) {
        let text = arg.text.as_str();
        if matches!(
            text,
            "-C" | "-c" | "--git-dir" | "--work-tree" | "--namespace" | "--exec-path"
        ) {
            at += 2;
        } else if text.starts_with('-') {
            at += 1;
        } else {
            return (Some(text), args[at + 1..].to_vec());
        }
    }
    (None, Vec::new())
}

/// Splits the arguments of `cargo`: a toolchain `+nightly` and options come before the subcommand.
fn cargo_split<'a>(args: &[&'a Word]) -> (Option<&'a str>, Vec<&'a Word>) {
    let mut at = 0;
    while let Some(arg) = args.get(at) {
        let text = arg.text.as_str();
        if text.starts_with('+') || text.starts_with('-') {
            at += if matches!(text, "-C" | "--config" | "--manifest-path") {
                2
            } else {
                1
            };
        } else {
            return (Some(text), args[at + 1..].to_vec());
        }
    }
    (None, Vec::new())
}

/// Takes the flags off the front of the arguments of a wrapper. A flag in `with_value` takes the
/// next word as its value.
fn skip_flags<'a, 'b>(args: &'b [&'a Word], with_value: &[&str]) -> &'b [&'a Word] {
    let mut at = 0;
    while let Some(arg) = args.get(at) {
        let text = arg.text.as_str();
        if text == "--" {
            at += 1;
            break;
        }
        if with_value.contains(&text) {
            at += 2;
        } else if text.starts_with('-') && text.len() > 1 {
            at += 1;
        } else {
            break;
        }
    }
    args.get(at..).unwrap_or(&[])
}

/// SQL that drops data, in any word of the command.
fn sql(source: &str, verdict: &mut Verdict) {
    let flat: String = source
        .to_ascii_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    for needle in [
        "drop table",
        "drop database",
        "drop schema",
        "truncate table",
    ] {
        if flat.contains(needle) {
            verdict.add(
                Risk::High,
                "The command has SQL that drops or empties a table or a database.",
            );
            return;
        }
    }
    if let Some(at) = flat.find("delete from ")
        && !flat[at..].contains(" where ")
    {
        verdict.add(
            Risk::High,
            "The command has SQL that deletes every row of a table.",
        );
    }
}
