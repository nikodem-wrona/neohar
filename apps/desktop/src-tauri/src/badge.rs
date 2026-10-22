//! The number on the icon of the app: the count of `attention`.

use tauri::AppHandle;

/// Sets the badge. Call it on the main thread. Zero clears it.
#[cfg(target_os = "macos")]
pub fn set(_app: &AppHandle, count: u32) {
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSApplication;
    use objc2_foundation::NSString;

    let Some(main) = MainThreadMarker::new() else {
        tracing::warn!("the badge must be set on the main thread");
        return;
    };
    let label = (count > 0).then(|| NSString::from_str(&count.to_string()));
    NSApplication::sharedApplication(main)
        .dockTile()
        .setBadgeLabel(label.as_deref());
}

/// Sets the badge on a window of the app, where the platform has a count.
#[cfg(not(target_os = "macos"))]
pub fn set(app: &AppHandle, count: u32) {
    use tauri::Manager;

    let value = (count > 0).then_some(i64::from(count));
    if let Some(window) = app.webview_windows().into_values().next()
        && let Err(error) = window.set_badge_count(value)
    {
        tracing::debug!(%error, "this platform has no badge");
    }
}
