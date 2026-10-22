//! The global hotkey: from any application it shows neohar and the card that waits
//! longest. The chord is the setting `ui.global_hotkey`; it changes while the app runs.

use serde_json::{Value, json};
use tauri::{AppHandle, Wry, plugin::TauriPlugin};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};
use tokio::sync::mpsc;

use crate::{backend::Backend, machines::Feed, reach};

/// The plugin, with the handler for a press of the chord.
pub fn plugin() -> TauriPlugin<Wry> {
    tauri_plugin_global_shortcut::Builder::new()
        .with_handler(|app, _shortcut, event| {
            if event.state() == ShortcutState::Pressed {
                reach::reveal_next(app);
            }
        })
        .build()
}

/// The chord of the setting as the plugin writes it: `ctrl+alt+cmd+n` is `Control+Alt+Super+N`.
/// `None` for a text that is no chord, and for an empty text.
pub fn parse(chord: &str) -> Option<String> {
    let mut parts: Vec<&str> = chord.split('+').map(str::trim).collect();
    let key = parts.pop().filter(|key| !key.is_empty())?;
    let mut out: Vec<String> = Vec::new();
    for part in parts {
        out.push(
            match part.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => "Control",
                "alt" | "option" => "Alt",
                "shift" => "Shift",
                "cmd" | "command" | "super" | "mod" => "Super",
                _ => return None,
            }
            .to_owned(),
        );
    }
    if out.is_empty() {
        // A global key with no modifier would take the key from every application.
        return None;
    }
    let mut chars = key.chars();
    let letter_or_digit =
        chars.next().is_some_and(|c| c.is_ascii_alphanumeric()) && chars.next().is_none();
    if !letter_or_digit {
        return None;
    }
    out.push(key.to_ascii_uppercase());
    Some(out.join("+"))
}

/// Registers the chord and drops the one before it.
fn apply(app: &AppHandle, chord: &str) {
    let shortcuts = app.global_shortcut();
    if let Err(error) = shortcuts.unregister_all() {
        tracing::warn!(%error, "the global hotkey did not clear");
    }
    if chord.is_empty() {
        return;
    }
    let Some(text) = parse(chord) else {
        tracing::warn!(chord, "the setting ui.global_hotkey is no chord");
        return;
    };
    let registered = text
        .parse::<Shortcut>()
        .map_err(|error| error.to_string())
        .and_then(|shortcut| {
            shortcuts
                .register(shortcut)
                .map_err(|error| error.to_string())
        });
    if let Err(error) = registered {
        tracing::warn!(chord, %error, "the global hotkey did not register");
    }
}

fn chord_of(values: &Value) -> Option<String> {
    values["sections"]["ui"]["entries"]["global_hotkey"]["value"]
        .as_str()
        .map(str::to_owned)
}

/// Registers the chord from the settings now, and again each time the settings change.
pub fn follow(app: &AppHandle, backend: &Backend) -> Result<(), nh_core::RpcError> {
    let (tx, mut rx) = mpsc::unbounded_channel();
    tauri::async_runtime::block_on(backend.subscribe("settings", None, Box::new(Feed(tx))))?;
    let (app, core) = (app.clone(), backend.clone());
    tauri::async_runtime::spawn(async move {
        let mut applied: Option<String> = None;
        while rx.recv().await.is_some() {
            let Ok(values) = core.rpc("config.get", json!({ "section": "ui" })).await else {
                continue;
            };
            let Some(chord) = chord_of(&values) else {
                continue;
            };
            if applied.as_deref() == Some(chord.as_str()) {
                continue;
            }
            let shown = app.clone();
            let wanted = chord.clone();
            let _ = app.run_on_main_thread(move || apply(&shown, &wanted));
            applied = Some(chord);
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chord_becomes_the_text_of_the_plugin() {
        assert_eq!(
            parse("ctrl+alt+cmd+n").as_deref(),
            Some("Control+Alt+Super+N")
        );
        assert_eq!(parse("shift+ctrl+7").as_deref(), Some("Shift+Control+7"));
        assert_eq!(parse(" Alt + Shift + k ").as_deref(), Some("Alt+Shift+K"));
    }

    #[test]
    fn a_text_that_is_no_chord_gives_none() {
        for text in [
            "",
            "n",
            "ctrl+",
            "ctrl+enter",
            "ctrl+hyper+n",
            "ctrl+alt+nn",
            "+",
        ] {
            assert_eq!(parse(text), None, "{text:?}");
        }
    }
}
