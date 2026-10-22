//! One open link: a reader, a writer and the pings.
//!
//! Both ends use it. The writer sends the parts that the [`Scheduler`] gives, the lowest class
//! first. The reader decodes the parts, answers `Ping` with `Pong`, ends the link on `Bye`, and
//! passes every other message on. The pinger sends `Ping` every 5 s and calls the link down when
//! no `Pong` came for 15 s.
//!
//! The writer never waits for a peer that has stopped reading after the link is down: each write
//! races the signal of the down link, and the writes after `Bye` have [`DRAIN_WITHIN`]. Then the
//! tasks end, the stream closes, and the peer gets the end of the stream when it reads again.

use std::{
    pin::Pin,
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    task::{Context, Poll},
    time::{Duration, Instant},
};

use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    sync::{Notify, mpsc, watch},
};

use crate::{
    codec::{Decoder, PART_SIZE, Scheduler, encode_within},
    msg::LinkMsg,
};

/// How long the writer may take, after `Bye`, to send what waits and to end the stream. A peer
/// that does not read in this time does not keep the link.
pub(crate) const DRAIN_WITHIN: Duration = Duration::from_secs(1);

/// The size limits of the messages of one side (`codec::MAX_MESSAGE`, `codec::MAX_FROM_CORE`).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Sizes {
    /// The largest message that this side reads. A larger one ends the link.
    pub recv: usize,
    /// The largest message that the other side reads. This side does not send a larger one.
    pub send: usize,
}

/// The times of a link. Tests make them short.
#[derive(Clone, Copy, Debug)]
pub struct LinkTiming {
    /// The time between two pings of one side.
    pub ping_every: Duration,
    /// A link with no `Pong` for this long is down.
    pub down_after: Duration,
    /// How long the core waits for `Hello`, and the client for `Welcome`.
    pub hello_within: Duration,
}

impl Default for LinkTiming {
    fn default() -> Self {
        Self {
            ping_every: Duration::from_secs(5),
            down_after: Duration::from_secs(15),
            hello_within: Duration::from_secs(10),
        }
    }
}

/// What the reader passes on.
#[derive(Debug)]
pub(crate) enum Incoming {
    Msg(LinkMsg),
    /// A whole message that this version cannot read. The core answers a first message of this
    /// kind with `Refused {proto}`.
    Unreadable,
}

/// The shared state of one link.
pub(crate) struct Wire {
    scheduler: Mutex<Scheduler>,
    wake: Notify,
    /// `Some(reason)` once the link is down.
    down: watch::Sender<Option<String>>,
    /// A `Bye` is queued: the writer sends what waits, then ends the stream.
    closing: AtomicBool,
    started: Instant,
    /// The time of the last `Pong`, in milliseconds since `started`.
    last_pong_ms: AtomicU64,
    /// The last ping that waits for its `Pong`: its nonce and when it went.
    ping: Mutex<Option<(u64, Instant)>>,
    /// The last round-trip time in microseconds, plus one. Zero is "not measured yet".
    rtt_us: AtomicU64,
    nonce: AtomicU64,
    timing: LinkTiming,
    sizes: Sizes,
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Wire {
    /// Queues a message. Returns `false` when the link is down or ends, or when the message is
    /// larger than the other side reads.
    pub fn send(&self, msg: &LinkMsg) -> bool {
        if self.is_down() || self.closing.load(Ordering::SeqCst) {
            return false;
        }
        match encode_within(msg, self.sizes.send) {
            Ok(out) => {
                lock(&self.scheduler).push(out);
                self.wake.notify_one();
                true
            }
            Err(error) => {
                tracing::warn!(%error, kind = msg.kind(), "a message of the link does not encode");
                false
            }
        }
    }

    pub fn is_down(&self) -> bool {
        self.down.borrow().is_some()
    }

    /// Calls the link down. The first reason stays.
    pub fn set_down(&self, reason: impl Into<String>) {
        let reason = reason.into();
        self.down.send_if_modified(|down| {
            if down.is_none() {
                *down = Some(reason);
                true
            } else {
                false
            }
        });
        self.wake.notify_one();
    }

    /// Ends the link cleanly: `Bye`, then the end of the stream.
    pub fn close(&self, reason: &str) {
        if self.send(&LinkMsg::Bye {
            reason: reason.to_owned(),
        }) {
            self.closing.store(true, Ordering::SeqCst);
            self.wake.notify_one();
        }
    }

    /// Waits until the link is down and returns the reason.
    pub async fn wait_down(&self) -> String {
        let mut down = self.down.subscribe();
        match down.wait_for(Option::is_some).await {
            Ok(reason) => reason.clone().unwrap_or_default(),
            Err(_) => "the link ended".to_owned(),
        }
    }

    /// The last round-trip time of a ping, if one came back.
    pub fn rtt(&self) -> Option<Duration> {
        match self.rtt_us.load(Ordering::Relaxed) {
            0 => None,
            us => Some(Duration::from_micros(us - 1)),
        }
    }

    fn now_ms(&self) -> u64 {
        u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    fn pong(&self, nonce: u64) {
        self.last_pong_ms.store(self.now_ms(), Ordering::Relaxed);
        let mut ping = lock(&self.ping);
        if let Some((sent, at)) = *ping
            && sent == nonce
        {
            let us = u64::try_from(at.elapsed().as_micros()).unwrap_or(u64::MAX - 1);
            self.rtt_us.store(us + 1, Ordering::Relaxed);
            *ping = None;
        }
    }
}

/// Starts the tasks of a link on the current runtime. The receiver gets every message except
/// `Ping`, `Pong` and `Bye`, and it ends when the link is down.
pub(crate) fn start<R, W>(
    read: R,
    write: W,
    timing: LinkTiming,
    sizes: Sizes,
) -> (Arc<Wire>, mpsc::UnboundedReceiver<Incoming>)
where
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
{
    let (down, _) = watch::channel(None);
    let wire = Arc::new(Wire {
        scheduler: Mutex::default(),
        wake: Notify::new(),
        down,
        closing: AtomicBool::new(false),
        started: Instant::now(),
        last_pong_ms: AtomicU64::new(0),
        ping: Mutex::new(None),
        rtt_us: AtomicU64::new(0),
        nonce: AtomicU64::new(1),
        timing,
        sizes,
    });
    let (tx, rx) = mpsc::unbounded_channel();
    tokio::spawn(reader(Arc::clone(&wire), read, tx));
    match link_delay() {
        Some(delay) => tokio::spawn(writer(Arc::clone(&wire), delay_line(write, delay))),
        None => tokio::spawn(writer(Arc::clone(&wire), write)),
    };
    tokio::spawn(pinger(Arc::clone(&wire)));
    (wire, rx)
}

/// The delay that a development build adds to each message it writes. Both ends of a
/// link read the variable, so `NEOHAR_LINK_DELAY_MS=25` is a round trip of 50 ms. A debug build
/// and a build with the feature `dev-ws` read it. The product ignores it.
pub(crate) fn link_delay() -> Option<Duration> {
    if !cfg!(any(debug_assertions, feature = "dev-ws")) {
        return None;
    }
    let ms: u64 = std::env::var("NEOHAR_LINK_DELAY_MS").ok()?.parse().ok()?;
    (ms > 0).then(|| Duration::from_millis(ms))
}

/// What the delay line holds.
enum Chunk {
    /// Bytes, and the moment they may go out.
    Data(tokio::time::Instant, Vec<u8>),
    /// The end of the stream, after the bytes before it.
    Shutdown(tokio::time::Instant),
}

/// A writer that gives its bytes to the real one `delay` later, in the order they came. It is a
/// delay line, so a message does not wait for the one before it: a link with a delay still
/// carries as much as one without.
pub(crate) struct DelayWriter {
    tx: mpsc::UnboundedSender<Chunk>,
    delay: Duration,
}

/// Puts a delay line before `write`. The task that pumps it ends with the stream.
pub(crate) fn delay_line<W>(mut write: W, delay: Duration) -> DelayWriter
where
    W: AsyncWrite + Unpin + Send + 'static,
{
    let (tx, mut rx) = mpsc::unbounded_channel::<Chunk>();
    tokio::spawn(async move {
        while let Some(chunk) = rx.recv().await {
            match chunk {
                Chunk::Data(due, bytes) => {
                    tokio::time::sleep_until(due).await;
                    if write.write_all(&bytes).await.is_err() || write.flush().await.is_err() {
                        return;
                    }
                }
                Chunk::Shutdown(due) => {
                    tokio::time::sleep_until(due).await;
                    let _ = write.shutdown().await;
                    return;
                }
            }
        }
    });
    DelayWriter { tx, delay }
}

impl AsyncWrite for DelayWriter {
    fn poll_write(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let due = tokio::time::Instant::now() + self.delay;
        match self.tx.send(Chunk::Data(due, buf.to_vec())) {
            Ok(()) => Poll::Ready(Ok(buf.len())),
            Err(_) => Poll::Ready(Err(std::io::ErrorKind::BrokenPipe.into())),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        let due = tokio::time::Instant::now() + self.delay;
        let _ = self.tx.send(Chunk::Shutdown(due));
        Poll::Ready(Ok(()))
    }
}

async fn reader<R: AsyncRead + Unpin>(
    wire: Arc<Wire>,
    mut read: R,
    tx: mpsc::UnboundedSender<Incoming>,
) {
    let mut decoder = Decoder::with_limit(wire.sizes.recv);
    let mut buf = vec![0u8; 64 * 1024];
    let mut down = wire.down.subscribe();
    'link: loop {
        let read = tokio::select! {
            read = read.read(&mut buf) => read,
            _ = down.wait_for(Option::is_some) => break,
        };
        let n = match read {
            Ok(0) => {
                wire.set_down("the other side closed the link");
                break;
            }
            Ok(n) => n,
            Err(error) => {
                wire.set_down(format!("the link failed: {error}"));
                break;
            }
        };
        let skipped = decoder.skipped;
        let messages = match decoder.feed(&buf[..n]) {
            Ok(messages) => messages,
            Err(error) => {
                tracing::warn!(%error, "the link sent bytes that are not link/1");
                wire.set_down(format!("bad bytes: {error}"));
                break;
            }
        };
        for _ in skipped..decoder.skipped {
            let _ = tx.send(Incoming::Unreadable);
        }
        for msg in messages {
            match msg {
                LinkMsg::Ping { nonce, sent_at_ms } => {
                    wire.send(&LinkMsg::Pong { nonce, sent_at_ms });
                }
                LinkMsg::Pong { nonce, .. } => wire.pong(nonce),
                LinkMsg::Bye { reason } => {
                    wire.set_down(format!("the other side ended the link: {reason}"));
                    break 'link;
                }
                other => {
                    let _ = tx.send(Incoming::Msg(other));
                }
            }
        }
    }
}

async fn writer<W: AsyncWrite + Unpin>(wire: Arc<Wire>, mut write: W) {
    let mut down = wire.down.subscribe();
    let mut batch = Vec::with_capacity(2 * PART_SIZE);
    // Set when `Bye` is queued: the end of the time for the rest.
    let mut drain_until: Option<tokio::time::Instant> = None;
    loop {
        batch.clear();
        let drained = {
            let mut scheduler = lock(&wire.scheduler);
            // A few small parts go in one write, but never more than one part's worth, so that
            // class 0 does not wait behind a long batch.
            while batch.len() < PART_SIZE && scheduler.next_part(&mut batch) {}
            scheduler.is_empty()
        };
        let closing = wire.closing.load(Ordering::SeqCst);
        if wire.is_down() && !closing {
            break;
        }
        if closing && drain_until.is_none() {
            drain_until = Some(tokio::time::Instant::now() + DRAIN_WITHIN);
        }
        if batch.is_empty() {
            if closing || wire.is_down() {
                break;
            }
            wire.wake.notified().await;
            continue;
        }
        let io = async {
            write.write_all(&batch).await?;
            if drained {
                write.flush().await?;
            }
            Ok::<(), std::io::Error>(())
        };
        // A peer that stopped reading must not hold the writer: before `Bye` the write ends when
        // the link is down, and after `Bye` when the time of the drain is over.
        let written = match drain_until {
            Some(until) => match tokio::time::timeout_at(until, io).await {
                Ok(written) => written,
                Err(_) => {
                    wire.set_down("the other side did not read the end of the link in time");
                    break;
                }
            },
            None => tokio::select! {
                written = io => written,
                _ = down.wait_for(Option::is_some) => break,
            },
        };
        if let Err(error) = written {
            wire.set_down(format!("the link failed: {error}"));
            break;
        }
    }
    // The end of the stream can wait for the peer too, for example on a pipe.
    let _ = tokio::time::timeout(DRAIN_WITHIN, write.shutdown()).await;
    wire.set_down("this side ended the link");
}

async fn pinger(wire: Arc<Wire>) {
    let mut down = wire.down.subscribe();
    let mut tick = tokio::time::interval(wire.timing.ping_every);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    // The first tick of an interval is at once.
    tick.tick().await;
    let down_after = u64::try_from(wire.timing.down_after.as_millis()).unwrap_or(u64::MAX);
    loop {
        tokio::select! {
            _ = tick.tick() => {}
            _ = down.wait_for(Option::is_some) => return,
        }
        let now = wire.now_ms();
        if now.saturating_sub(wire.last_pong_ms.load(Ordering::Relaxed)) > down_after {
            wire.set_down("no Pong came for the time of the limit");
            return;
        }
        let nonce = wire.nonce.fetch_add(1, Ordering::Relaxed);
        *lock(&wire.ping) = Some((nonce, Instant::now()));
        wire.send(&LinkMsg::Ping {
            nonce,
            sent_at_ms: now,
        });
    }
}

#[cfg(test)]
mod tests {
    use std::{
        io,
        pin::Pin,
        task::{Context, Poll},
    };

    use super::*;
    use crate::codec::MAX_MESSAGE;

    #[tokio::test(start_paused = true)]
    async fn a_delay_line_holds_each_write_for_the_delay_and_keeps_the_order() {
        let (near, mut far) = tokio::io::duplex(1 << 16);
        let mut line = delay_line(near, Duration::from_millis(25));
        let start = tokio::time::Instant::now();
        // Three writes in a row, then one 10 ms later.
        line.write_all(b"one ").await.unwrap();
        line.write_all(b"two ").await.unwrap();
        line.write_all(b"three ").await.unwrap();
        tokio::time::sleep(Duration::from_millis(10)).await;
        line.write_all(b"four").await.unwrap();
        line.shutdown().await.unwrap();

        // Nothing is there before the delay is over.
        let mut buf = [0u8; 64];
        let early = tokio::time::timeout(Duration::from_millis(10), far.read(&mut buf)).await;
        assert!(early.is_err(), "nothing before 25 ms");
        let mut got = Vec::new();
        let mut stamps = Vec::new();
        loop {
            let n = far.read(&mut buf).await.unwrap();
            if n == 0 {
                break;
            }
            stamps.push(start.elapsed());
            got.extend_from_slice(&buf[..n]);
        }
        assert_eq!(String::from_utf8(got).unwrap(), "one two three four");
        // The first three arrive together at 25 ms, and the last at 35 ms: the line does not
        // add the delays up.
        assert!(stamps[0] >= Duration::from_millis(25));
        assert!(*stamps.last().unwrap() >= Duration::from_millis(35));
        assert!(
            *stamps.last().unwrap() < Duration::from_millis(60),
            "{stamps:?}"
        );
    }

    /// The write half of a peer that never reads: each write waits forever. It says when it is
    /// dropped, that is, when the link lets go of the stream.
    struct Stalled(Arc<AtomicBool>);

    impl AsyncWrite for Stalled {
        fn poll_write(
            self: Pin<&mut Self>,
            _: &mut Context<'_>,
            _: &[u8],
        ) -> Poll<io::Result<usize>> {
            Poll::Pending
        }

        fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Pending
        }

        fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Pending
        }
    }

    impl Drop for Stalled {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    const SIZES: Sizes = Sizes {
        recv: MAX_MESSAGE,
        send: MAX_MESSAGE,
    };

    /// A link whose peer sends nothing and reads nothing.
    fn stalled(down_after: Duration) -> (Arc<Wire>, Arc<AtomicBool>, tokio::io::DuplexStream) {
        let dropped = Arc::new(AtomicBool::new(false));
        let (read, peer) = tokio::io::duplex(1024);
        let timing = LinkTiming {
            ping_every: Duration::from_millis(20),
            down_after,
            hello_within: Duration::from_secs(5),
        };
        let (wire, _rx) = start(read, Stalled(Arc::clone(&dropped)), timing, SIZES);
        (wire, dropped, peer)
    }

    async fn until_dropped(dropped: &AtomicBool, within: Duration) {
        let deadline = Instant::now() + within;
        while !dropped.load(Ordering::SeqCst) {
            assert!(
                Instant::now() < deadline,
                "the writer still holds the stream of a peer that does not read"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    #[tokio::test]
    async fn a_writer_blocked_on_a_peer_that_does_not_read_ends_when_the_link_is_down() {
        let (wire, dropped, _peer) = stalled(Duration::from_millis(100));
        assert!(wire.send(&LinkMsg::Refused {
            reason: "a message that waits".to_owned()
        }));
        // No `Pong` comes, so the pinger calls the link down while the write waits.
        let reason = tokio::time::timeout(Duration::from_secs(5), wire.wait_down())
            .await
            .expect("down in time");
        assert!(reason.contains("Pong"), "{reason}");
        until_dropped(&dropped, Duration::from_secs(5)).await;
    }

    #[tokio::test]
    async fn the_drain_after_bye_ends_in_its_time() {
        // The pinger would wait a minute: only the time of the drain ends the link.
        let (wire, dropped, _peer) = stalled(Duration::from_secs(60));
        wire.close("the end");
        until_dropped(&dropped, DRAIN_WITHIN * 5).await;
        assert!(wire.is_down());
    }

    #[tokio::test]
    async fn a_message_over_the_limit_of_the_other_side_is_not_sent() {
        let (read, _peer) = tokio::io::duplex(1024);
        let (write, _peer_read) = tokio::io::duplex(1 << 16);
        let sizes = Sizes {
            recv: MAX_MESSAGE,
            send: 1024,
        };
        let (wire, _rx) = start(read, write, LinkTiming::default(), sizes);
        assert!(!wire.send(&LinkMsg::Frame {
            stream: 1,
            bytes: vec![7; 4096],
        }));
        assert!(!wire.is_down(), "the link stays");
        assert!(wire.send(&LinkMsg::Frame {
            stream: 1,
            bytes: vec![7; 16],
        }));
    }
}
