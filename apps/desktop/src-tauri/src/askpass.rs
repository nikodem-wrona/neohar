//! The sheet for a question of ssh.
//!
//! `nh askpass` calls `askpass.ask` on the core, and the core gives the question to the host
//! ([`nh_core::Host::ask_secret`]). On macOS the host shows an `NSAlert` with the text of ssh. A
//! password or a passphrase has a secure field that hides what the person types. A host key has
//! no field: the person chooses "Connect", and the answer is `yes`.
//!
//! The answer goes to the caller of [`ask`] and nowhere else. The app keeps it in no setting, no
//! file and no log. The alert is app-modal and it runs on the main thread.

#[cfg(target_os = "macos")]
use nh_core::host::{SecretKind, SecretQuestion};

/// The width of the field of a secret, in points.
#[cfg(target_os = "macos")]
const FIELD_WIDTH: f64 = 280.0;

/// The height of the field of a secret, in points.
#[cfg(target_os = "macos")]
const FIELD_HEIGHT: f64 = 24.0;

/// What the alert says: a title, and the text of ssh.
#[cfg(target_os = "macos")]
fn texts(question: &SecretQuestion) -> (String, String) {
    let machine = question
        .machine
        .as_deref()
        .map_or_else(|| "a remote machine".to_owned(), str::to_owned);
    let title = match question.kind {
        SecretKind::HostKey => format!("Connect to {machine}?"),
        SecretKind::Secret => format!("{machine} needs a secret"),
    };
    (title, question.prompt.trim().to_owned())
}

/// Shows the alert and waits for the person. Call it on the main thread. Returns the answer, or
/// `None` when the person cancelled.
#[cfg(target_os = "macos")]
pub fn ask(question: &SecretQuestion) -> Option<String> {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSAlert, NSAlertFirstButtonReturn, NSSecureTextField, NSView};
    use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};

    let Some(main) = MainThreadMarker::new() else {
        tracing::warn!("the question of ssh must be shown on the main thread");
        return None;
    };
    let (title, text) = texts(question);
    let alert = NSAlert::new(main);
    alert.setMessageText(&NSString::from_str(&title));
    alert.setInformativeText(&NSString::from_str(&text));
    alert.addButtonWithTitle(&NSString::from_str("Connect"));
    alert.addButtonWithTitle(&NSString::from_str("Cancel"));
    let field = (question.kind == SecretKind::Secret).then(|| {
        let frame = NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(FIELD_WIDTH, FIELD_HEIGHT),
        );
        NSSecureTextField::initWithFrame(main.alloc(), frame)
    });
    if let Some(field) = &field {
        let view: &NSView = field;
        alert.setAccessoryView(Some(view));
        alert.layout();
        alert.window().setInitialFirstResponder(Some(view));
    }
    if alert.runModal() != NSAlertFirstButtonReturn {
        return None;
    }
    Some(match &field {
        Some(field) => field.stringValue().to_string(),
        None => "yes".to_owned(),
    })
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    fn question(kind: SecretKind, machine: Option<&str>) -> SecretQuestion {
        SecretQuestion {
            prompt: "me@studio's password: ".to_owned(),
            machine: machine.map(str::to_owned),
            kind,
        }
    }

    #[test]
    fn the_sheet_names_the_machine_and_keeps_the_text_of_ssh() {
        let (title, text) = texts(&question(SecretKind::Secret, Some("studio")));
        assert_eq!(title, "studio needs a secret");
        assert_eq!(text, "me@studio's password:");
        let (title, _) = texts(&question(SecretKind::HostKey, None));
        assert_eq!(title, "Connect to a remote machine?");
    }
}
