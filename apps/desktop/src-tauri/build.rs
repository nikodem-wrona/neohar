//! Declares the commands of the app, so that a capability can allow each one by name.
//!
//! Without this manifest every webview may call every command. With it, the webview can call only
//! the commands that a capability file lists.

fn main() {
    tauri_build::try_build(
        tauri_build::Attributes::new().app_manifest(tauri_build::AppManifest::new().commands(&[
            "rpc",
            "subscribe",
            "stream",
        ])),
    )
    .expect("the Tauri build step runs");
}
