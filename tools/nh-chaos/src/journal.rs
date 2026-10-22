//! The reference for the screens: the journal of a holder, read while the host is dead, and an
//! engine that never restarted.
//!
//! A holder serves one client, and a second client with a higher fencing token takes the lease
//! from the host. So the checker cannot read the journal of a pane while the host
//! runs. It reads the journal after it killed the host. It connects with a token between the token
//! of the dead host and the token of the next one, attaches from the offset where it stopped
//! last time, and detaches.
//!
//! The reference engine reads every byte of the journal from offset 0, with the resizes at the
//! offsets where the holder recorded them, and no restart. The screen of a restored pane at a
//! journal offset must equal the screen of the reference at the same offset.

use std::{collections::VecDeque, path::Path, time::Duration};

use anyhow::{Context, Result, bail};
use nh_core::holder;
use nh_proto::holder::{HolderEvent, HolderRequest, Marker, StreamId};
use nh_store::PaneRecord;
use nh_term::{AlacrittyEngine, MIN_HISTORY, Replaying, ScreenDump, VtEngine, xterm_palette};
use tokio::time::timeout;

/// How long the holder may take to send the journal.
const READ_TIMEOUT: Duration = Duration::from_secs(20);

/// One step of a journal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Item {
    /// Output at `offset`.
    Output { offset: u64, bytes: Vec<u8> },
    /// The holder recorded a resize at `offset`.
    Resize { offset: u64, cols: u16, rows: u16 },
}

impl Item {
    fn offset(&self) -> u64 {
        match self {
            Self::Output { offset, .. } | Self::Resize { offset, .. } => *offset,
        }
    }
}

/// The unix time in milliseconds, as a fencing token.
pub fn clock_token() -> u64 {
    u64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis()),
    )
    .unwrap_or(0)
}

/// Reads the journal of the holder of `pane` from offset `from` to its end at the time of the
/// call. The host must be dead. Returns the items and the end offset.
///
/// Fails when the ring no longer holds `from`: a gap would make the reference wrong.
pub async fn read_from(pane: &PaneRecord, from: u64) -> Result<(Vec<Item>, u64)> {
    let mut link = holder::connect(
        Path::new(&pane.socket),
        pane.secret,
        clock_token(),
        "nh-chaos",
    )
    .await
    .with_context(|| format!("cannot connect to the holder of pane {}", pane.id))?;
    link.tx.send(HolderRequest::Status)?;
    let status = loop {
        match next(&mut link.events).await? {
            HolderEvent::StatusReply(status) => break status,
            _ => continue,
        }
    };
    let end = status.journal.end;
    if from < status.journal.start {
        bail!(
            "the ring of pane {} starts at {} and the checker needs {from}: bytes are lost",
            pane.id,
            status.journal.start
        );
    }
    link.tx.send(HolderRequest::Attach { from_offset: from })?;
    let mut items = Vec::new();
    let mut at = from;
    while at < end {
        match next(&mut link.events).await? {
            HolderEvent::Output {
                offset,
                stream: StreamId::Pty,
                bytes,
            } => {
                if offset != at {
                    bail!(
                        "a gap in the journal of pane {}: {at} then {offset}",
                        pane.id
                    );
                }
                at = offset + bytes.len() as u64;
                items.push(Item::Output { offset, bytes });
            }
            HolderEvent::Marker {
                offset,
                marker: Marker::Resize { cols, rows },
            } => items.push(Item::Resize { offset, cols, rows }),
            HolderEvent::Truncated { requested, start } => {
                bail!("the journal of pane {} lost {requested}..{start}", pane.id)
            }
            _ => {}
        }
    }
    link.tx.send(HolderRequest::Detach)?;
    // The next host must have a later token than ours.
    tokio::time::sleep(Duration::from_millis(5)).await;
    Ok((items, end))
}

async fn next(events: &mut tokio::sync::mpsc::Receiver<HolderEvent>) -> Result<HolderEvent> {
    timeout(READ_TIMEOUT, events.recv())
        .await
        .context("the holder did not send the journal in time")?
        .context("the holder closed the connection")
}

/// An engine that reads the journal of one pane from offset 0 and never restarts.
pub struct Reference {
    engine: AlacrittyEngine,
    /// The offset after the last byte that the engine has read.
    fed: u64,
    /// The items that the engine has not read yet.
    pending: VecDeque<Item>,
}

impl Reference {
    /// A reference for a pane that started with this size.
    pub fn new(cols: u16, rows: u16) -> Self {
        Self {
            engine: AlacrittyEngine::new(cols, rows, MIN_HISTORY, xterm_palette()),
            fed: 0,
            pending: VecDeque::new(),
        }
    }

    /// Adds items that the journal grew by.
    pub fn extend(&mut self, items: impl IntoIterator<Item = Item>) {
        self.pending.extend(items);
    }

    /// Reads the journal up to `offset` and returns the screen. A chunk that crosses `offset` is
    /// split there.
    ///
    /// A resize that the holder recorded before `offset` is applied. A resize at `offset` is
    /// ambiguous: on a pane that makes no output, a resize after the capture gets the same offset
    /// as one before it. `size` is the size that the captured pane had. The reference applies the
    /// resizes at `offset` only until it has that size, and leaves the rest for a later capture.
    pub fn dump_at(&mut self, offset: u64, size: (u16, u16)) -> Result<ScreenDump> {
        if offset < self.fed {
            bail!(
                "the reference is at {} and the capture is at {offset}",
                self.fed
            );
        }
        while let Some(item) = self.pending.pop_front() {
            if item.offset() > offset {
                self.pending.push_front(item);
                break;
            }
            match item {
                Item::Resize { offset: at, .. } if at == offset && self.engine.size() == size => {
                    self.pending.push_front(item);
                    break;
                }
                Item::Resize { cols, rows, .. } => self.engine.resize(cols, rows),
                Item::Output { offset: at, bytes } => {
                    if at != self.fed {
                        bail!(
                            "a gap in the journal: the reference is at {} and the chunk at {at}",
                            self.fed
                        );
                    }
                    let take =
                        usize::try_from(offset - at).map_or(bytes.len(), |n| n.min(bytes.len()));
                    self.engine
                        .advance(&bytes[..take], &mut Replaying::default());
                    self.fed += take as u64;
                    if take < bytes.len() {
                        self.pending.push_front(Item::Output {
                            offset: self.fed,
                            bytes: bytes[take..].to_vec(),
                        });
                        break;
                    }
                }
            }
        }
        if self.fed != offset {
            bail!(
                "the journal ends at {} and the capture is at {offset}",
                self.fed
            );
        }
        Ok(ScreenDump::of(&self.engine, &xterm_palette()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn out(offset: u64, text: &str) -> Item {
        Item::Output {
            offset,
            bytes: text.as_bytes().to_vec(),
        }
    }

    #[test]
    fn the_reference_reads_to_the_offset_and_splits_a_chunk_there() {
        let mut reference = Reference::new(20, 4);
        reference.extend([out(0, "abc"), out(3, "def"), out(6, "ghi")]);
        assert_eq!(
            reference.dump_at(4, (20, 4)).expect("a dump").text()[0],
            "abcd"
        );
        assert_eq!(
            reference.dump_at(4, (20, 4)).expect("again").text()[0],
            "abcd"
        );
        assert_eq!(
            reference.dump_at(9, (20, 4)).expect("a dump").text()[0],
            "abcdefghi"
        );
    }

    #[test]
    fn a_resize_at_or_before_the_offset_is_applied_and_one_after_it_is_not() {
        let mut reference = Reference::new(20, 4);
        reference.extend([
            out(0, "ab"),
            Item::Resize {
                offset: 2,
                cols: 10,
                rows: 3,
            },
            out(2, "cd"),
            Item::Resize {
                offset: 4,
                cols: 8,
                rows: 2,
            },
        ]);
        let dump = reference.dump_at(3, (10, 3)).expect("a dump");
        assert_eq!((dump.cols, dump.rows), (10, 3));
        let dump = reference.dump_at(4, (20, 4)).expect("a dump");
        assert_eq!((dump.cols, dump.rows), (8, 2));
    }

    #[test]
    fn a_resize_at_the_capture_offset_is_applied_only_up_to_the_size_of_the_capture() {
        // An idle pane: the capture at 4 came first, then the pane was resized twice. All the
        // resizes have offset 4.
        let mut reference = Reference::new(20, 4);
        reference.extend([
            out(0, "abcd"),
            Item::Resize {
                offset: 4,
                cols: 10,
                rows: 3,
            },
            Item::Resize {
                offset: 4,
                cols: 8,
                rows: 2,
            },
        ]);
        let size = |dump: ScreenDump| (dump.cols, dump.rows);
        assert_eq!(
            size(reference.dump_at(4, (20, 4)).expect("before both")),
            (20, 4)
        );
        assert_eq!(
            size(reference.dump_at(4, (10, 3)).expect("after one")),
            (10, 3)
        );
        assert_eq!(
            size(reference.dump_at(4, (8, 2)).expect("after both")),
            (8, 2)
        );
    }

    #[test]
    fn a_gap_or_a_capture_beyond_the_journal_is_an_error() {
        let mut reference = Reference::new(20, 4);
        reference.extend([out(0, "ab"), out(5, "cd")]);
        assert!(
            reference.dump_at(7, (20, 4)).is_err(),
            "a gap between 2 and 5"
        );
        let mut reference = Reference::new(20, 4);
        reference.extend([out(0, "ab")]);
        assert!(
            reference.dump_at(9, (20, 4)).is_err(),
            "the journal ends at 2"
        );
    }

    #[test]
    fn a_capture_before_the_position_of_the_reference_is_an_error() {
        let mut reference = Reference::new(20, 4);
        reference.extend([out(0, "abcd")]);
        reference.dump_at(4, (20, 4)).expect("a dump");
        assert!(reference.dump_at(2, (20, 4)).is_err());
    }
}
