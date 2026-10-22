//! Puts the files of the phone app into the binary.
//!
//! `NEOHAR_WEB_DIST` names the folder that `bun run build:web` writes (`target/web-dist`). Without
//! the variable, the binary has one page that says that the phone app is not built, so that
//! `cargo build` and `cargo test` work without Bun.

use std::{
    env,
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
};

fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, PathBuf)>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.filter_map(Result::ok).collect();
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            walk(root, &path, out);
        } else if let Ok(relative) = path.strip_prefix(root) {
            out.push((relative.to_string_lossy().replace('\\', "/"), path));
        }
    }
}

fn main() {
    println!("cargo:rerun-if-env-changed=NEOHAR_WEB_DIST");
    let mut files = Vec::new();
    if let Some(dist) = env::var_os("NEOHAR_WEB_DIST") {
        let dist = PathBuf::from(dist);
        println!("cargo:rerun-if-changed={}", dist.display());
        walk(&dist, &dist, &mut files);
    }
    let mut code = String::from("/// The files of the phone app: the path and the bytes.\n");
    code.push_str("pub(crate) static FILES: &[(&str, &[u8])] = &[\n");
    for (path, file) in &files {
        let _ = writeln!(
            code,
            "    ({path:?}, include_bytes!({:?})),",
            file.display().to_string()
        );
    }
    code.push_str("];\n");
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("cargo sets OUT_DIR")).join("files.rs");
    fs::write(out, code).expect("OUT_DIR is writable");
}
