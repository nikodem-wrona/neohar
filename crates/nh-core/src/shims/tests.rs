//! The shim of Codex, run by `sh` with fake `codex` programs on the `PATH`.

use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
};

use super::*;

struct Dir(PathBuf);

impl Dir {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("nh-shims-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("a directory");
        Self(path)
    }

    fn sub(&self, name: &str) -> PathBuf {
        let path = self.0.join(name);
        fs::create_dir_all(&path).expect("a directory");
        path
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// The flags that the shim adds for a Codex that knows `--no-daemon`.
const FLAGS: [&str; 3] = ["--disable", "daemon_auto_start", "--no-daemon"];

/// The name of a fake `codex` and the flags of the shim, as it prints them.
fn flagged(name: &str) -> Vec<&str> {
    std::iter::once(name).chain(FLAGS).collect()
}

/// A fake `codex` that prints its arguments, one for each line, after its name.
fn fake_codex(dir: &Path, name: &str) {
    write_fake(dir, &format!("echo \"{name}\""));
}

/// A fake `codex` older than `--no-daemon`: as clap does, it refuses the unknown argument.
fn fake_old_codex(dir: &Path, name: &str) {
    write_fake(
        dir,
        &format!(
            "for arg in \"$@\"; do [ \"$arg\" = --no-daemon ] && exit 2; done\necho \"{name}\""
        ),
    );
}

fn write_fake(dir: &Path, head: &str) {
    let path = dir.join("codex");
    fs::write(
        &path,
        format!("#!/bin/sh\n{head}\nfor arg in \"$@\"; do printf '%s\\n' \"$arg\"; done\n"),
    )
    .expect("a script");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("a mode");
}

/// Runs `codex` through the `PATH` with the shims first, and gives the lines it printed.
fn run(shims: &Path, others: &[&Path], args: &[&str], env: &[(&str, &str)]) -> (Vec<String>, i32) {
    let mut path = vec![shims.display().to_string()];
    path.extend(others.iter().map(|p| p.display().to_string()));
    path.push("/usr/bin".to_owned());
    path.push("/bin".to_owned());
    let mut command = Command::new("codex");
    command.args(args).env("PATH", path.join(":"));
    for (key, value) in env {
        command.env(key, value);
    }
    let output = command.output().expect("the shim runs");
    let lines = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::to_owned)
        .collect();
    (lines, output.status.code().unwrap_or(-1))
}

#[test]
fn the_shim_is_written_once_and_with_the_mode_of_a_program() {
    let dir = Dir::new("write");
    let shims = dir.0.join("data").join("shims");
    ensure(&shims).expect("written");
    let path = shims.join("codex");
    assert_eq!(fs::read_to_string(&path).expect("the file"), CODEX);
    assert_eq!(
        fs::metadata(&path).expect("a file").permissions().mode() & 0o777,
        0o755
    );
    // A second start changes nothing, also not the time of the file.
    let before = fs::metadata(&path)
        .expect("a file")
        .modified()
        .expect("a time");
    ensure(&shims).expect("written");
    assert_eq!(
        fs::metadata(&path)
            .expect("a file")
            .modified()
            .expect("a time"),
        before
    );
    // A file that someone changed is written again.
    fs::write(&path, "#!/bin/sh\necho hacked\n").expect("a file");
    ensure(&shims).expect("written");
    assert_eq!(fs::read_to_string(&path).expect("the file"), CODEX);
    // The `.zshenv` of a zsh in a pane is in the same directory, out of sight of the `PATH`.
    assert_eq!(
        fs::read_to_string(zdotdir(&shims).join(".zshenv")).expect("the file"),
        ZSHENV
    );
    assert_eq!(
        dir.0.join("data").read_dir().expect("a directory").count(),
        1
    );
}

#[test]
fn the_next_codex_runs_with_the_flag_once_and_the_arguments_are_exact() {
    let dir = Dir::new("flag");
    let (shims, real) = (dir.sub("shims"), dir.sub("real"));
    ensure(&shims).expect("written");
    fake_codex(&real, "real");
    let args = [
        "resume",
        "--last",
        "a b",
        "it's",
        "say \"hi\"",
        "--",
        "-x",
        "",
        "$HOME",
        "*",
        "line\nbreak",
    ];
    let (lines, code) = run(&shims, &[&real], &args, &[]);
    assert_eq!(code, 0);
    let mut want = flagged("real");
    want.extend(args);
    // The last argument has a newline, so it makes two lines.
    let want: Vec<String> = want
        .iter()
        .flat_map(|a| a.split('\n').map(str::to_owned).collect::<Vec<_>>())
        .collect();
    assert_eq!(lines, want);
    // Without arguments the flags still come once.
    let (lines, _) = run(&shims, &[&real], &[], &[]);
    assert_eq!(lines, flagged("real"));
}

#[test]
fn the_switch_and_an_explicit_flag_pass_everything_on_unchanged() {
    let dir = Dir::new("pass");
    let (shims, real) = (dir.sub("shims"), dir.sub("real"));
    ensure(&shims).expect("written");
    fake_codex(&real, "real");
    let (lines, _) = run(
        &shims,
        &[&real],
        &["--model", "x"],
        &[("CODEX_NEOHAR_SHIM", "0")],
    );
    assert_eq!(lines, ["real", "--model", "x"]);
    for explicit in [
        vec!["--enable", "daemon_auto_start"],
        vec!["--disable", "daemon_auto_start", "resume"],
        vec!["--enable=daemon_auto_start"],
        vec!["-c", "features.daemon_auto_start=true"],
        vec!["resume", "--no-daemon"],
    ] {
        let (lines, _) = run(&shims, &[&real], &explicit, &[]);
        let mut want = vec!["real"];
        want.extend(explicit.iter().copied());
        assert_eq!(lines, want, "{explicit:?}");
    }
    // Another value of the switch does not turn it off.
    let (lines, _) = run(&shims, &[&real], &["x"], &[("CODEX_NEOHAR_SHIM", "1")]);
    let mut want = flagged("real");
    want.push("x");
    assert_eq!(lines, want);
}

#[test]
fn an_old_codex_and_the_commands_of_the_shared_daemon_get_only_the_first_flag() {
    let dir = Dir::new("daemon");
    let (shims, real, old) = (dir.sub("shims"), dir.sub("real"), dir.sub("old"));
    ensure(&shims).expect("written");
    fake_codex(&real, "real");
    fake_old_codex(&old, "old");
    // A Codex that does not know `--no-daemon` still starts.
    let (lines, code) = run(&shims, &[&old], &["x"], &[]);
    assert_eq!(code, 0, "{lines:?}");
    assert_eq!(lines, ["old", "--disable", "daemon_auto_start", "x"]);
    // Codex refuses `--no-daemon` with these, because they need the shared daemon.
    for args in [
        vec!["agents"],
        vec!["queue", "--thread", "t"],
        vec!["remote-control"],
        vec!["app-server", "daemon", "start"],
        vec!["--remote", "unix://"],
        vec!["--remote=ws://h:1", "x"],
    ] {
        let (lines, _) = run(&shims, &[&real], &args, &[]);
        let mut want = vec!["real", "--disable", "daemon_auto_start"];
        want.extend(args.iter().copied());
        assert_eq!(lines, want, "{args:?}");
    }
    // A command can come after options.
    let (lines, _) = run(&shims, &[&real], &["--model", "x", "agents"], &[]);
    assert_eq!(
        lines,
        [
            "real",
            "--disable",
            "daemon_auto_start",
            "--model",
            "x",
            "agents"
        ]
    );
    // A prompt that only has the name of a command in it is not the command.
    let (lines, _) = run(&shims, &[&real], &["fix the queue"], &[]);
    let mut want = flagged("real");
    want.push("fix the queue");
    assert_eq!(lines, want);
}

#[test]
fn the_shim_skips_itself_and_empty_entries_and_takes_the_first_codex_after_it() {
    let dir = Dir::new("order");
    let (shims, first, second) = (dir.sub("shims"), dir.sub("first"), dir.sub("second"));
    ensure(&shims).expect("written");
    fake_codex(&first, "first");
    fake_codex(&second, "second");
    // A directory with no `codex` and a directory that does not exist come before the real ones.
    let empty = dir.sub("empty");
    let missing = dir.0.join("missing");
    let (lines, _) = run(&shims, &[&empty, &missing, &first, &second], &["a"], &[]);
    assert_eq!(lines[0], "first");
    // A second shim farther on (neohar in a pane of neohar) is run once, and it looks after its own
    // place, so the chain ends at the real program. The flags are there once: the second shim sees
    // them in the arguments and passes them on.
    let copy = dir.sub("copy");
    ensure(&copy).expect("written");
    let (lines, code) = run(&shims, &[&copy, &second], &["a"], &[]);
    assert_eq!(code, 0, "{lines:?}");
    let mut want = flagged("second");
    want.push("a");
    assert_eq!(lines, want);
}

#[test]
fn without_a_codex_the_shim_says_so_and_exits_127() {
    let dir = Dir::new("none");
    let shims = dir.sub("shims");
    ensure(&shims).expect("written");
    let output = Command::new("codex")
        .env("PATH", format!("{}:/usr/bin:/bin", shims.display()))
        .output()
        .expect("the shim runs");
    assert_eq!(output.status.code(), Some(127));
    assert!(String::from_utf8_lossy(&output.stderr).contains("codex: command not found"));
}

#[test]
fn a_path_to_the_shim_through_a_link_is_still_the_shim() {
    let dir = Dir::new("link");
    let (shims, real, links) = (dir.sub("shims"), dir.sub("real"), dir.sub("links"));
    ensure(&shims).expect("written");
    fake_codex(&real, "real");
    // The directory `links` holds a link to the shim. Running it must not call itself again.
    std::os::unix::fs::symlink(shims.join("codex"), links.join("codex")).expect("a link");
    let (lines, code) = run(&links, &[&real], &["x"], &[]);
    assert_eq!(code, 0, "{lines:?}");
    let mut want = flagged("real");
    want.push("x");
    assert_eq!(lines, want);
}

/// Runs an interactive login zsh as a pane starts it, with `input` on stdin, and gives what it
/// printed. `zdotdir` is the `ZDOTDIR` of neohar, or `None` for a zsh without it. `user` is the
/// `ZDOTDIR` of the user. Without zsh, `None`.
fn zsh(
    shims: &Path,
    home: &Path,
    zdotdir: Option<&Path>,
    user: Option<&Path>,
    input: &str,
) -> Option<Vec<String>> {
    use std::io::Write as _;
    if !Path::new("/bin/zsh").exists() {
        return None;
    }
    let mut command = Command::new("/bin/zsh");
    command
        .args(["-l", "-i"])
        .env_clear()
        .env("HOME", home)
        .env("TERM", "dumb")
        .env("PATH", format!("{}:/usr/bin:/bin", shims.display()))
        .env("NEOHAR_SHIMS", shims)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    if let Some(zdotdir) = zdotdir {
        command.env("ZDOTDIR", zdotdir);
    }
    if let Some(user) = user {
        command.env("NEOHAR_ZDOTDIR", user);
    }
    let mut child = command.spawn().expect("zsh starts");
    child
        .stdin
        .take()
        .expect("a stdin")
        .write_all(input.as_bytes())
        .expect("the input");
    let output = child.wait_with_output().expect("zsh ends");
    Some(
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(str::to_owned)
            .collect(),
    )
}

#[test]
fn a_login_zsh_of_a_pane_reads_the_files_of_the_user_and_runs_the_shim_first() {
    let dir = Dir::new("zsh");
    let (shims, front, home) = (dir.sub("shims"), dir.sub("front"), dir.sub("home"));
    ensure(&shims).expect("written");
    fake_codex(&front, "front");
    // The files of the user put another `codex` first, as `brew shellenv` does.
    fs::write(home.join(".zshenv"), "echo zshenv\n").expect("a file");
    fs::write(home.join(".zprofile"), "echo zprofile\n").expect("a file");
    fs::write(
        home.join(".zshrc"),
        format!(
            "export PATH=\"{}:$PATH\"\necho \"zshrc ${{ZDOTDIR-unset}}\"\n",
            front.display()
        ),
    )
    .expect("a file");
    let input = "codex\nexit\n";
    // Without the `.zshenv` of neohar, the shim comes too late.
    let Some(lines) = zsh(&shims, &home, None, None, input) else {
        eprintln!("no /bin/zsh: skipped");
        return;
    };
    assert_eq!(lines, ["zshenv", "zprofile", "zshrc unset", "front"]);
    // With it, the files of the user run as before, with the `ZDOTDIR` of the user, and `codex` is
    // the shim, which runs the `codex` of the user with the flags.
    let lines = zsh(&shims, &home, Some(&zdotdir(&shims)), None, input).expect("zsh");
    let mut want = vec!["zshenv", "zprofile", "zshrc unset"];
    want.extend(flagged("front"));
    assert_eq!(lines, want);
}

#[test]
fn the_zdotdir_of_the_user_comes_back_and_a_precmd_hook_of_the_user_does_not_win() {
    let dir = Dir::new("zdotdir");
    let (shims, front, home, user) = (
        dir.sub("shims"),
        dir.sub("front"),
        dir.sub("home"),
        dir.sub("user"),
    );
    ensure(&shims).expect("written");
    fake_codex(&front, "front");
    // The `.zshrc` of the user is in its own `ZDOTDIR`, and a precmd hook puts `front` first
    // before each prompt, as mise and direnv change the PATH.
    fs::write(
        user.join(".zshrc"),
        format!(
            "_user_hook() {{ path=(\"{0}\" ${{path:#{0}}}); }}\n\
             autoload -Uz add-zsh-hook\nadd-zsh-hook precmd _user_hook\n",
            front.display()
        ),
    )
    .expect("a file");
    // Two prompts: the first one and a later one.
    let input = "echo \"$ZDOTDIR\"; codex\ncodex\nexit\n";
    let Some(lines) = zsh(&shims, &home, Some(&zdotdir(&shims)), Some(&user), input) else {
        eprintln!("no /bin/zsh: skipped");
        return;
    };
    let front = flagged("front");
    let mut want = vec![user.display().to_string()];
    want.extend(front.iter().map(|s| (*s).to_owned()));
    want.extend(front.iter().map(|s| (*s).to_owned()));
    assert_eq!(lines, want);
}
