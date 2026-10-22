//! Sequences that `alacritty_terminal` does not handle, or handles in a way the core cannot use.
//!
//! The interceptor reads the same bytes as the engine, and reports where in the chunk each one
//! of these sequences ends:
//!
//! - DA3 and XTVERSION. The engine ignores them, but the holder answers them.
//! - OSC 9 and OSC 777 notifications. The engine has no event for them.
//!
//! An `ESC` starts a new sequence in every parser state, so this small machine is exact.

use nh_proto::answers::QueryKind;

const MAX_CSI: usize = 8;
const MAX_OSC: usize = 4096;

/// What the interceptor found.
#[derive(Debug, PartialEq, Eq)]
pub enum What {
    Query(QueryKind),
    Notify { title: String, body: String },
}

/// A sequence in a chunk: the bytes `start..end` of the chunk. `start` is 0 when the sequence
/// began in an earlier chunk.
#[derive(Debug, PartialEq, Eq)]
pub struct Item {
    pub start: usize,
    pub end: usize,
    pub what: What,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum State {
    #[default]
    Ground,
    Esc,
    Csi,
    Osc,
}

/// The state of the interceptor between chunks.
#[derive(Default)]
pub struct Interceptor {
    state: State,
    body: Vec<u8>,
    overflow: bool,
    /// Where in the current chunk the open sequence began.
    start: usize,
}

impl Interceptor {
    /// Reads `bytes` and adds each sequence that ends in them to `found`.
    pub fn scan(&mut self, bytes: &[u8], found: &mut Vec<Item>) {
        self.start = 0;
        for (i, &b) in bytes.iter().enumerate() {
            match self.state {
                State::Ground => {
                    if b == 0x1B {
                        self.begin(i);
                    }
                }
                State::Esc => match b {
                    b'[' => self.open(State::Csi),
                    b']' => self.open(State::Osc),
                    b'c' => self.state = State::Ground,
                    0x1B => self.begin(i),
                    _ => self.state = State::Ground,
                },
                State::Csi => match b {
                    0x20..=0x3F => self.keep(b, MAX_CSI),
                    0x40..=0x7E => {
                        if let Some(what) = self.csi(b) {
                            found.push(Item {
                                start: self.start,
                                end: i + 1,
                                what,
                            });
                        }
                        self.state = State::Ground;
                    }
                    0x1B => self.begin(i),
                    0x18 | 0x1A => self.state = State::Ground,
                    _ => {}
                },
                State::Osc => match b {
                    0x07 | 0x1B => {
                        if let Some(what) = self.osc() {
                            found.push(Item {
                                start: self.start,
                                end: i + 1,
                                what,
                            });
                        }
                        if b == 0x1B {
                            self.begin(i);
                        } else {
                            self.state = State::Ground;
                        }
                    }
                    0x18 | 0x1A => self.state = State::Ground,
                    0x20.. => self.keep(b, MAX_OSC),
                    _ => {}
                },
            }
        }
    }

    fn begin(&mut self, at: usize) {
        self.state = State::Esc;
        self.start = at;
    }

    fn open(&mut self, state: State) {
        self.state = state;
        self.body.clear();
        self.overflow = false;
    }

    fn keep(&mut self, b: u8, limit: usize) {
        if self.body.len() < limit {
            self.body.push(b);
        } else {
            self.overflow = true;
        }
    }

    fn csi(&self, fin: u8) -> Option<What> {
        if self.overflow {
            return None;
        }
        match (self.body.as_slice(), fin) {
            (b">" | b">0", b'q') => Some(What::Query(QueryKind::XtVersion)),
            (b"=" | b"=0", b'c') => Some(What::Query(QueryKind::Da3)),
            _ => None,
        }
    }

    fn osc(&self) -> Option<What> {
        if self.overflow {
            return None;
        }
        let text = String::from_utf8_lossy(&self.body);
        if let Some(message) = text.strip_prefix("9;") {
            // ConEmu uses OSC 9 for other things: `9;4` is progress and `9;9` is the directory.
            let subcommand = message.split_once(';').map_or(message, |(head, _)| head);
            let is_subcommand = !subcommand.is_empty()
                && subcommand.len() <= 2
                && subcommand.bytes().all(|b| b.is_ascii_digit());
            return (!is_subcommand).then(|| What::Notify {
                title: String::new(),
                body: message.to_owned(),
            });
        }
        let rest = text.strip_prefix("777;notify;")?;
        let (title, body) = rest.split_once(';').unwrap_or((rest, ""));
        Some(What::Notify {
            title: title.to_owned(),
            body: body.to_owned(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scan(chunks: &[&[u8]]) -> Vec<(usize, usize, usize, What)> {
        let mut interceptor = Interceptor::default();
        let mut out = Vec::new();
        for (n, chunk) in chunks.iter().enumerate() {
            let mut found = Vec::new();
            interceptor.scan(chunk, &mut found);
            out.extend(found.into_iter().map(|i| (n, i.start, i.end, i.what)));
        }
        out
    }

    #[test]
    fn da3_and_xtversion_are_found_with_their_position() {
        let found = scan(&[b"ab\x1b[=c\x1b[>0q"]);
        assert_eq!(
            found,
            [
                (0, 2, 6, What::Query(QueryKind::Da3)),
                (0, 6, 11, What::Query(QueryKind::XtVersion)),
            ]
        );
        assert!(scan(&[b"\x1b[>1q\x1b[=5c\x1b[c"]).is_empty());
    }

    #[test]
    fn a_sequence_split_between_chunks_starts_at_zero_in_the_last_chunk() {
        let found = scan(&[b"x\x1b[", b">", b"q"]);
        assert_eq!(found, [(2, 0, 1, What::Query(QueryKind::XtVersion))]);
    }

    #[test]
    fn notifications_from_osc_9_and_777() {
        let found = scan(&[b"\x1b]9;build done\x07\x1b]777;notify;Codex;needs approval\x1b\\"]);
        assert_eq!(
            found,
            [
                (
                    0,
                    0,
                    15,
                    What::Notify {
                        title: String::new(),
                        body: "build done".into()
                    }
                ),
                (
                    0,
                    15,
                    49,
                    What::Notify {
                        title: "Codex".into(),
                        body: "needs approval".into()
                    }
                ),
            ]
        );
        // ConEmu progress and directory reports are not notifications.
        assert!(scan(&[b"\x1b]9;4;1;50\x07\x1b]9;9;/tmp\x07\x1b]777;other\x07"]).is_empty());
    }

    #[test]
    fn an_esc_starts_a_new_sequence_whatever_the_state() {
        // The CSI is cut by an ESC, and the OSC by an ESC that is not part of a terminator.
        let found = scan(&[b"\x1b[3\x1b[>q\x1b]9;a\x1b[=c"]);
        assert_eq!(found.len(), 3, "{found:?}");
        assert_eq!(found[0].3, What::Query(QueryKind::XtVersion));
        assert_eq!(
            found[1].3,
            What::Notify {
                title: String::new(),
                body: "a".into()
            }
        );
        assert_eq!(found[2].3, What::Query(QueryKind::Da3));
    }

    #[test]
    fn can_and_sub_abort_and_long_sequences_are_ignored() {
        assert!(scan(&[b"\x1b[>\x18q\x1b]9;a\x1aq\x07"]).is_empty());
        let mut long = b"\x1b]9;".to_vec();
        long.extend(std::iter::repeat_n(b'x', 5000));
        long.push(0x07);
        assert!(scan(&[&long]).is_empty());
    }
}
