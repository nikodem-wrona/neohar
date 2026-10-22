//! What the engine does to the world outside the screen.

/// The receiver of the side effects of output. The pane task implements it.
pub trait EffectSink {
    /// Bytes for the child: the reply to a query.
    fn pty_write(&mut self, bytes: &[u8]);
    fn bell(&mut self);
    fn title_changed(&mut self);
    /// OSC 52 write. `clipboard` is `b'c'` for the clipboard and `b'p'` for the selection.
    fn clipboard_store(&mut self, clipboard: u8, text: &str);
    /// OSC 52 read. The core never answers it.
    fn clipboard_load_requested(&mut self);
    /// OSC 9 and OSC 777 (`notify`).
    fn notify(&mut self, title: &str, body: &str);
}

/// A sink for replay: no notification, bell, clipboard write or reply happens
/// while the engine reads old output. It counts what it drops, for the log.
#[derive(Default)]
pub struct Replaying {
    pub dropped: usize,
}

impl EffectSink for Replaying {
    fn pty_write(&mut self, _: &[u8]) {
        self.dropped += 1;
    }
    fn bell(&mut self) {
        self.dropped += 1;
    }
    fn title_changed(&mut self) {
        self.dropped += 1;
    }
    fn clipboard_store(&mut self, _: u8, _: &str) {
        self.dropped += 1;
    }
    fn clipboard_load_requested(&mut self) {
        self.dropped += 1;
    }
    fn notify(&mut self, _: &str, _: &str) {
        self.dropped += 1;
    }
}

/// One effect, as [`Recorder`] keeps it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    PtyWrite(Vec<u8>),
    Bell,
    TitleChanged,
    ClipboardStore { clipboard: u8, text: String },
    ClipboardLoadRequested,
    Notify { title: String, body: String },
}

/// A sink that keeps every effect. Tests use it.
#[derive(Default)]
pub struct Recorder(pub Vec<Effect>);

impl EffectSink for Recorder {
    fn pty_write(&mut self, bytes: &[u8]) {
        self.0.push(Effect::PtyWrite(bytes.to_vec()));
    }
    fn bell(&mut self) {
        self.0.push(Effect::Bell);
    }
    fn title_changed(&mut self) {
        self.0.push(Effect::TitleChanged);
    }
    fn clipboard_store(&mut self, clipboard: u8, text: &str) {
        self.0.push(Effect::ClipboardStore {
            clipboard,
            text: text.to_owned(),
        });
    }
    fn clipboard_load_requested(&mut self) {
        self.0.push(Effect::ClipboardLoadRequested);
    }
    fn notify(&mut self, title: &str, body: &str) {
        self.0.push(Effect::Notify {
            title: title.to_owned(),
            body: body.to_owned(),
        });
    }
}
