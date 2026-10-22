use super::*;

#[test]
fn dots_are_resolved_without_the_file_system() {
    assert_eq!(normalize(Path::new("/a/b/../c/./d")), Path::new("/a/c/d"));
    assert_eq!(normalize(Path::new("/a/../../b")), Path::new("/b"));
    assert_eq!(normalize(Path::new("/")), Path::new("/"));
}

#[test]
fn a_word_resolves_against_the_directory_and_the_home() {
    let cwd = Path::new("/work/app");
    let home = Path::new("/home/me");
    let go = |word: &str, dynamic: bool| resolve(word, dynamic, cwd, Some(home));
    assert_eq!(
        go("src/a.rs", false),
        Some(PathBuf::from("/work/app/src/a.rs"))
    );
    assert_eq!(
        go("../other/x", false),
        Some(PathBuf::from("/work/other/x"))
    );
    assert_eq!(go("/etc/hosts", false), Some(PathBuf::from("/etc/hosts")));
    assert_eq!(go("~", false), Some(PathBuf::from("/home/me")));
    assert_eq!(
        go("~/.zshrc", false),
        Some(PathBuf::from("/home/me/.zshrc"))
    );
    assert_eq!(go("~root/x", false), None);
    assert_eq!(go("$HOME/x", true), None);
    assert_eq!(go("", false), None);
    assert_eq!(resolve("~/x", false, cwd, None), None);
}

#[test]
fn the_null_device_is_harmless() {
    assert!(is_harmless_target(Path::new("/dev/null")));
    assert!(is_harmless_target(Path::new("/dev/stderr")));
    assert!(!is_harmless_target(Path::new("/dev/sda")));
    assert!(!is_harmless_target(Path::new("/tmp/x")));
}

#[test]
fn inside_means_below_the_workspace() {
    let root = Path::new("/work/app");
    assert!(is_inside(Path::new("/work/app"), root));
    assert!(is_inside(Path::new("/work/app/src/a.rs"), root));
    assert!(!is_inside(Path::new("/work/application/x"), root));
    assert!(!is_inside(Path::new("/work"), root));
}

#[test]
fn credential_files_are_found_by_name() {
    for path in [
        "/w/.env",
        "/w/.env.local",
        "/w/.env.production",
        "/w/config/credentials.json",
        "/w/.npmrc",
        "/home/me/.ssh/id_ed25519",
        "/home/me/.ssh/config",
        "/home/me/.aws/credentials",
        "/w/server.pem",
        "/w/tls.key",
        "/w/store.p12",
        "/w/service-account-prod.json",
        "/home/me/.netrc",
        "/w/secrets.yaml",
        "/w/.ENV",
    ] {
        assert!(is_credential(Path::new(path)), "{path}");
    }
}

#[test]
fn ordinary_files_are_not_credentials() {
    for path in [
        "/w/.env.example",
        "/w/.env.sample",
        "/w/src/env.rs",
        "/w/README.md",
        "/w/package.json",
        "/w/keyboard.ts",
        "/w/monkey.rs",
        "/w/environment/config.toml",
    ] {
        assert!(!is_credential(Path::new(path)), "{path}");
    }
}
