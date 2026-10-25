//! The sheet for a question of ssh.
//!
//! `nh askpass` calls `askpass.ask` on the core, and the core gives the question to the host
//! ([`nh_core::Host::ask_secret`]). On macOS the host shows an `NSAlert` with the text of ssh. A
//! password or a passphrase has a secure field that hides what the person types. A host key has
//! no field: the person chooses "Connect", and the answer is `yes`.
//!
//! The answer goes to the caller of [`ask`] and nowhere else. The app keeps it in no setting, no
//! file and no log. The alert is app-modal and it runs on the main thread.
//!
//! The ssh server can write the text of a question (keyboard-interactive authentication). Thus
//! the alert says that the text comes from the connection, and that neohar never needs the login
//! password of this Mac. The host asks at most [`MAX_QUESTIONS`] questions of one machine in
//! [`QUESTIONS_WINDOW`], so a server that asks again and again does not hold the app.

// Only macOS has the sheet; the rules stay testable on every system.
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use std::{
    collections::{HashMap, VecDeque},
    time::{Duration, Instant},
};

#[cfg(target_os = "macos")]
use nh_core::host::{SecretKind, SecretQuestion};

/// The most questions of ssh for one machine in [`QUESTIONS_WINDOW`]: a new host key, a
/// passphrase, a password with two mistakes and a code.
pub const MAX_QUESTIONS: usize = 5;

/// The window of time of [`MAX_QUESTIONS`].
pub const QUESTIONS_WINDOW: Duration = Duration::from_secs(120);

/// The most characters of the text of ssh that the alert shows.
const PROMPT_CHARS: usize = 500;

/// The width of the field of a secret, in points.
#[cfg(target_os = "macos")]
const FIELD_WIDTH: f64 = 280.0;

/// The height of the field of a secret, in points.
#[cfg(target_os = "macos")]
const FIELD_HEIGHT: f64 = 24.0;

/// The questions of ssh of each machine in the last [`QUESTIONS_WINDOW`].
#[derive(Debug, Default)]
pub struct Questions(HashMap<String, VecDeque<Instant>>);

impl Questions {
    /// Takes a question of `machine`, if one is free in the window.
    pub fn take(&mut self, machine: &str, now: Instant) -> bool {
        let asked = self.0.entry(machine.to_owned()).or_default();
        while asked
            .front()
            .is_some_and(|at| now.saturating_duration_since(*at) >= QUESTIONS_WINDOW)
        {
            asked.pop_front();
        }
        if asked.len() >= MAX_QUESTIONS {
            return false;
        }
        asked.push_back(now);
        true
    }
}

/// The text of ssh as the alert shows it: no control characters but new lines, and at most
/// [`PROMPT_CHARS`] characters.
fn shown_prompt(prompt: &str) -> String {
    let kept: String = prompt
        .chars()
        .filter(|c| *c == '\n' || !c.is_control())
        .collect();
    let clean = kept.trim();
    match clean.char_indices().nth(PROMPT_CHARS) {
        Some((end, _)) => format!("{}…", &clean[..end]),
        None => clean.to_owned(),
    }
}

/// What the alert says: a title, and the text of ssh with the name of who wrote it.
#[cfg(target_os = "macos")]
fn texts(question: &SecretQuestion) -> (String, String) {
    let machine = question
        .machine
        .as_deref()
        .map_or_else(|| "a remote machine".to_owned(), str::to_owned);
    let prompt = shown_prompt(&question.prompt);
    match question.kind {
        SecretKind::HostKey => (format!("Connect to {machine}?"), prompt),
        SecretKind::Secret => (
            format!("{machine} needs a secret"),
            format!(
                "The ssh connection to {machine} asks:\n\n{prompt}\n\nThe server can write this \
                 text. neohar never needs the login password of this Mac. Do not type it here."
            ),
        ),
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "macos")]
    fn question(kind: SecretKind, machine: Option<&str>, prompt: &str) -> SecretQuestion {
        SecretQuestion {
            prompt: prompt.to_owned(),
            machine: machine.map(str::to_owned),
            kind,
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn the_sheet_names_the_machine_and_says_who_wrote_the_text() {
        let (title, text) = texts(&question(
            SecretKind::Secret,
            Some("studio"),
            "me@studio's password: ",
        ));
        assert_eq!(title, "studio needs a secret");
        assert!(
            text.starts_with("The ssh connection to studio asks:\n\nme@studio's password:\n\n"),
            "{text}"
        );
        assert!(text.contains("The server can write this text."), "{text}");
        assert!(
            text.contains("never needs the login password of this Mac"),
            "{text}"
        );
        let (title, text) = texts(&question(
            SecretKind::HostKey,
            None,
            "Are you sure you want to continue connecting (yes/no)? ",
        ));
        assert_eq!(title, "Connect to a remote machine?");
        assert_eq!(
            text,
            "Are you sure you want to continue connecting (yes/no)?"
        );
    }

    #[test]
    fn the_text_of_a_server_is_short_and_has_no_control_characters() {
        assert_eq!(
            shown_prompt("Enter your macOS password\u{1b}[8m\r\n: \u{7}"),
            "Enter your macOS password[8m\n:"
        );
        let long = shown_prompt(&"a".repeat(2_000));
        assert_eq!(long.chars().count(), PROMPT_CHARS + 1);
        assert!(long.ends_with('…'));
    }

    #[test]
    fn a_machine_gets_at_most_five_questions_in_two_minutes() {
        let mut questions = Questions::default();
        let start = Instant::now();
        let asked = (0..7).filter(|_| questions.take("studio", start)).count();
        assert_eq!(asked, MAX_QUESTIONS);
        // Another machine has its own questions.
        assert!(questions.take("build", start));
        // After the window, a question is free again.
        assert!(questions.take("studio", start + QUESTIONS_WINDOW));
    }
}
