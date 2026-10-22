//! The sandbox profile of a recording, tried with a shell.
//!
//! The profile is what keeps a harness away from the real `~/.claude`, `~/.claude.json` and
//! `~/.codex`, so these tests do not take the profile text on trust. They run `/bin/sh` under it
//! and look at what the operating system allows. Every probe that must fail has a twin that must
//! work, so that a test cannot pass because the probe is broken.
//!
//! The sandbox is a macOS feature. On other systems the file has no tests.

#![cfg(target_os = "macos")]

use std::{
    fs,
    net::UdpSocket,
    path::{Path, PathBuf},
    process::{Command, Output},
};

use nh_golden::sandbox::{self, profile, real_harness_paths, wrap};

/// A fake home directory with files in the places that a harness uses, and a scratch tree.
struct Fixture {
    dir: PathBuf,
    home: PathBuf,
    scratch: PathBuf,
    profile: PathBuf,
}

impl Fixture {
    fn new(name: &str) -> Self {
        let dir = fs::canonicalize(std::env::temp_dir())
            .expect("the temp dir")
            .join(format!("nhg-iso-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let (home, scratch) = (dir.join("home"), dir.join("scratch"));
        for sub in [".claude", ".codex"] {
            fs::create_dir_all(home.join(sub)).expect("dirs");
        }
        fs::create_dir_all(&scratch).expect("scratch");
        fs::write(home.join(".claude/settings.json"), "claude settings").expect("file");
        fs::write(home.join(".claude.json"), "claude global").expect("file");
        fs::write(home.join(".codex/config.toml"), "codex config").expect("file");
        let profile_path = dir.join("profile.sb");
        fs::write(
            &profile_path,
            profile(&scratch, &[], &real_harness_paths(&home)),
        )
        .expect("profile");
        Self {
            dir,
            home,
            scratch,
            profile: profile_path,
        }
    }

    /// Runs a shell line under the profile.
    fn sh(&self, line: &str) -> Output {
        let argv = wrap(
            &self.profile,
            &["/bin/sh".to_owned(), "-c".to_owned(), line.to_owned()],
        );
        Command::new(&argv[0])
            .args(&argv[1..])
            .output()
            .expect("sandbox-exec starts")
    }

    fn ok(&self, line: &str) -> bool {
        self.sh(line).status.success()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn quoted(path: &Path) -> String {
    format!("'{}'", path.display())
}

#[test]
fn a_harness_can_write_in_the_scratch_tree_and_nowhere_else() {
    let f = Fixture::new("write");
    let inside = f.scratch.join("inside.txt");
    let outside = f.dir.join("outside.txt");
    assert!(
        f.ok(&format!("echo x > {}", quoted(&inside))),
        "the control: writing inside works"
    );
    assert_eq!(fs::read_to_string(&inside).expect("written").trim(), "x");
    assert!(
        !f.ok(&format!("echo x > {}", quoted(&outside))),
        "writing outside must fail"
    );
    assert!(!outside.exists());
    assert!(f.ok("echo x > /dev/null"), "the devices stay open");
    // A harness that makes its own directories in the home of the user is stopped too.
    assert!(!f.ok(&format!("mkdir {}", quoted(&f.home.join(".new")))));
    assert!(!f.home.join(".new").exists());
}

#[test]
fn the_real_harness_directories_are_closed_for_reading_and_writing() {
    let f = Fixture::new("real");
    let settings = f.home.join(".claude/settings.json");
    let global = f.home.join(".claude.json");
    let codex = f.home.join(".codex/config.toml");
    // The control: the same commands work on a file in the scratch tree.
    let control = f.scratch.join("control.txt");
    fs::write(&control, "control").expect("file");
    assert!(f.ok(&format!("cat {} > /dev/null", quoted(&control))));
    assert!(f.ok(&format!("echo more >> {}", quoted(&control))));
    for file in [&settings, &global, &codex] {
        assert!(
            !f.ok(&format!("cat {} > /dev/null", quoted(file))),
            "reading {}",
            file.display()
        );
        assert!(
            !f.ok(&format!("echo hacked >> {}", quoted(file))),
            "writing {}",
            file.display()
        );
        assert!(
            !f.ok(&format!("rm {}", quoted(file))),
            "removing {}",
            file.display()
        );
    }
    assert_eq!(
        fs::read_to_string(&settings).expect("read"),
        "claude settings"
    );
    assert_eq!(fs::read_to_string(&global).expect("read"), "claude global");
    assert_eq!(fs::read_to_string(&codex).expect("read"), "codex config");
}

/// The helper of the network test. The test executable runs itself under the sandbox with
/// `NHG_PROBE=udp:<address>`, and this function connects a UDP socket to the address. A UDP
/// `connect` sends no packet, and the sandbox still decides on it. The exit code says what
/// happened: 0 connected, 3 refused by the sandbox, 4 failed for another reason.
#[test]
fn probe_helper() {
    let Ok(target) = std::env::var("NHG_PROBE") else {
        return;
    };
    let result = UdpSocket::bind("0.0.0.0:0").and_then(|s| s.connect(target.as_str()));
    let code = match result {
        Ok(()) => 0,
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => 3,
        Err(_) => 4,
    };
    std::process::exit(code);
}

/// Runs the helper, outside the sandbox or under the profile of `f`, and returns its exit code.
fn probe(f: Option<&Fixture>, target: &str) -> Option<i32> {
    let exe = std::env::current_exe().expect("this executable");
    let mut argv = vec![
        exe.display().to_string(),
        "--exact".to_owned(),
        "probe_helper".to_owned(),
        "--nocapture".to_owned(),
    ];
    if let Some(f) = f {
        argv = wrap(&f.profile, &argv);
    }
    Command::new(&argv[0])
        .args(&argv[1..])
        .env("NHG_PROBE", target)
        .output()
        .expect("the helper starts")
        .status
        .code()
}

#[test]
fn the_network_is_closed_except_for_the_loopback() {
    let f = Fixture::new("net");
    // 192.0.2.1 is in the range that is reserved for documentation (RFC 5737). Nothing is sent.
    let outside = "192.0.2.1:9";
    assert_eq!(
        probe(Some(&f), "127.0.0.1:9"),
        Some(0),
        "the loopback works"
    );
    assert_eq!(
        probe(Some(&f), outside),
        Some(3),
        "an outside address must be refused"
    );
    match probe(None, outside) {
        Some(0) => {}
        other => {
            // This machine has no route (no network at all). The refusal above is then a fact
            // about the sandbox that the control cannot back, so say so.
            eprintln!("note: without the sandbox the helper ended with {other:?}: no route here");
        }
    }
}

#[test]
fn unix_sockets_work_for_the_api_socket_and_the_holder() {
    let f = Fixture::new("sock");
    let path = f.scratch.join("a.sock");
    let listener = std::os::unix::net::UnixListener::bind(&path).expect("a socket");
    let handle = std::thread::spawn(move || {
        use std::io::{BufRead, BufReader, Write};
        let (mut stream, _) = listener.accept().expect("a connection");
        let mut line = String::new();
        BufReader::new(&stream)
            .read_line(&mut line)
            .expect("a line");
        stream.write_all(b"ok\n").expect("a reply");
        line
    });
    let out = f.sh(&format!("echo hi | /usr/bin/nc -U -w 2 {}", quoted(&path)));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "ok");
    assert_eq!(handle.join().expect("the thread").trim(), "hi");
}

#[test]
fn the_tool_that_applies_profiles_is_there() {
    assert!(sandbox::available());
}
