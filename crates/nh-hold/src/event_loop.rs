//! The holder's state and its event loop.
//!
//! One thread polls the child's output, the listener, a self-pipe for signals and the client
//! connections. Output flows by pull: a client's send offset advances only when its socket has
//! room, so a slow client never blocks the child.

#![allow(unsafe_code)]

use std::{
    collections::{HashMap, VecDeque},
    fs::{self, File},
    io::{self, Write},
    os::{
        fd::{AsRawFd, OwnedFd},
        unix::{
            fs::{FileTypeExt, MetadataExt},
            net::{UnixListener, UnixStream},
        },
    },
    path::Path,
    sync::atomic::{AtomicI32, Ordering},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use nh_proto::{
    answers::{self, QueryKind},
    holder::{
        ExitStatus, HoldConfig, HoldMode, HoldModeKind, HolderEvent, HolderRequest, HolderStatus,
        InputResult, MAX_OUTPUT_CHUNK, Marker, QueuedQuery, RefuseReason, SignalKind, SignalTarget,
        StreamId,
    },
    version::HOLDER_PROTO,
};
use polling::{Event, Events, PollMode, Poller};
use rustix::{
    fs::{Mode, OFlags, fcntl_setfl},
    io::{Errno, read, write},
    process::{Pid, Signal, WaitOptions, kill_process_group, waitpid},
    termios::tcsetwinsize,
};

use crate::{
    Error, auth,
    checkpoint::Checkpoints,
    feed::{Feed, Query},
    hlog,
    journal::Journal,
    log::Log,
    pty::{self, Pty},
    server::{self, Conn, OUT_HIGH_WATER, Send, Session},
    spawn::{self, ChildIo, errno_location},
};

/// Poll keys: the child's output (the PTY master, or stdout), the listener, the signal pipe, the
/// child's stderr and the child's stdin (the last two in pipe mode only).
const K_OUT: usize = 0;
const K_LISTENER: usize = 1;
const K_SIGNAL: usize = 2;
const K_ERR: usize = 3;
const K_IN: usize = 4;
const FIRST_CONN_KEY: usize = 5;

/// How long a new connection can wait before it says `Hello`.
const HELLO_TIMEOUT: Duration = Duration::from_secs(2);
/// The holder remembers this many input ids to drop duplicates.
const SEEN_INPUTS: usize = 4096;
/// A read from the child. A wakeup reads at most `READS_PER_WAKEUP` of them.
const READ_SIZE: usize = 16 * 1024;
const READS_PER_WAKEUP: usize = 16;
/// How often the holder looks at the foreground process group of the PTY. The kernel has no
/// notification for a change, so this is a poll, and each poll wakes the holder. 20 idle holders
/// at 4 Hz used 0.7% of a core. The adapters walk the process tree every second anyway,
/// so a change is known within a second.
const FOREGROUND_EVERY: Duration = Duration::from_millis(1000);
/// How often the holder checks that its socket file is still there.
const REBIND_EVERY: Duration = Duration::from_secs(60);
/// The holder queues at most this many queries.
const MAX_QUERIES: usize = 64;

/// The write end of the signal self-pipe, for the signal handler.
static SIGNAL_FD: AtomicI32 = AtomicI32::new(-1);

/// A signal handler that writes the signal number to the self-pipe. It is async-signal-safe.
extern "C" fn on_signal(signal: libc::c_int) {
    // SAFETY: `errno_location` is valid for this thread. `write` is async-signal-safe. The
    // handler restores `errno` so that it does not change the error of the interrupted code.
    unsafe {
        let saved = *errno_location();
        let byte = signal as u8;
        libc::write(
            SIGNAL_FD.load(Ordering::Relaxed),
            (&raw const byte).cast(),
            1,
        );
        *errno_location() = saved;
    }
}

/// A descriptor that the child writes to: the PTY master, or a pipe from stdout or stderr.
struct Source {
    fd: OwnedFd,
    key: usize,
    stream: StreamId,
    /// Whether the poller still watches it. The PTY master stays in the list after its end.
    open: bool,
}

/// Bytes for the child. A reply to a query has no id.
struct PendingInput {
    id: Option<u64>,
    bytes: Vec<u8>,
    written: usize,
}

/// The state of one holder.
pub struct Holder {
    config: HoldConfig,
    log: Log,
    poller: Poller,
    listener: UnixListener,
    socket_inode: u64,
    uid: u32,
    pipe: bool,
    sources: Vec<Source>,
    /// The child's stdin, in pipe mode.
    stdin: Option<OwnedFd>,
    write_interest: bool,
    signals: OwnedFd,
    input: VecDeque<PendingInput>,
    /// Input ids that were queued or written, with the journal offset of the ack once written. An
    /// id that a failed write dropped stays `None`: it is never acknowledged.
    seen: HashMap<u64, Option<u64>>,
    seen_order: VecDeque<u64>,
    feed: Feed,
    checkpoints: Checkpoints,
    child: Pid,
    exit: Option<ExitStatus>,
    /// The child ended and its output up to the end is in the journal.
    exit_ready: bool,
    queries: Vec<QueuedQuery>,
    next_query: u64,
    conns: HashMap<usize, Conn>,
    next_key: usize,
    active: Option<usize>,
    top_fence: Option<u64>,
    /// Events for the active client that are not output.
    outbox: Vec<HolderEvent>,
    size: (u16, u16),
    foreground: Option<i32>,
    next_foreground_check: Instant,
    next_rebind: Instant,
    started_at_ms: i64,
    terminating: bool,
    done: bool,
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

fn nonblocking(fd: &OwnedFd) -> Result<(), Error> {
    fcntl_setfl(fd, OFlags::NONBLOCK).map_err(|e| Error::new("descriptor", e))
}

impl Holder {
    /// Installs the signal handlers and starts the child under a PTY or on pipes.
    pub fn start(
        config: HoldConfig,
        (listener, socket_inode): (UnixListener, u64),
        mut log: Log,
        uid: u32,
        child_umask: Mode,
    ) -> Result<Self, Error> {
        let (signals, signal_write) =
            spawn::cloexec_pipe().map_err(|e| Error::new("signal pipe", e))?;
        nonblocking(&signals)?;
        nonblocking(&signal_write)?;
        // The write end stays open for the life of the process, for the handler.
        SIGNAL_FD.store(
            std::os::fd::IntoRawFd::into_raw_fd(signal_write),
            Ordering::Relaxed,
        );
        install_signal_handlers();

        let pipe_err = |e| Error::new("pipe", e);
        let (spawned, sources, stdin, size) = match config.mode {
            HoldMode::Pty {
                cols,
                rows,
                px_width,
                px_height,
            } => {
                let Pty { master, slave } =
                    pty::open(pty::winsize(cols, rows, px_width, px_height))
                        .map_err(|e| Error::new("open PTY", e))?;
                let spawned = spawn::spawn(&config, &ChildIo::Pty(&slave), child_umask)?;
                let out = Source {
                    fd: master,
                    key: K_OUT,
                    stream: StreamId::Pty,
                    open: true,
                };
                (spawned, vec![out], None, (cols, rows))
            }
            HoldMode::Pipe => {
                let (stdin_read, stdin_write) = spawn::cloexec_pipe().map_err(pipe_err)?;
                let (out_read, out_write) = spawn::cloexec_pipe().map_err(pipe_err)?;
                let (err_read, err_write) = spawn::cloexec_pipe().map_err(pipe_err)?;
                let io = ChildIo::Pipes {
                    stdin: &stdin_read,
                    stdout: &out_write,
                    stderr: &err_write,
                };
                let spawned = spawn::spawn(&config, &io, child_umask)?;
                for fd in [&stdin_write, &out_read, &err_read] {
                    nonblocking(fd)?;
                }
                let out = Source {
                    fd: out_read,
                    key: K_OUT,
                    stream: StreamId::Stdout,
                    open: true,
                };
                let err = Source {
                    fd: err_read,
                    key: K_ERR,
                    stream: StreamId::Stderr,
                    open: true,
                };
                (spawned, vec![out, err], Some(stdin_write), (0, 0))
            }
        };
        hlog!(
            log,
            "child {} started: {:?}",
            spawned.pid.as_raw_nonzero(),
            config.argv
        );

        let now = Instant::now();
        let pipe = config.mode == HoldMode::Pipe;
        Ok(Self {
            feed: Feed::new(Journal::new(config.ring_bytes), pipe),
            checkpoints: Checkpoints::new(&config, now),
            poller: Poller::new().map_err(|e| Error::new("poller", e))?,
            config,
            log,
            listener,
            socket_inode,
            uid,
            pipe,
            sources,
            stdin,
            write_interest: false,
            signals,
            input: VecDeque::new(),
            seen: HashMap::new(),
            seen_order: VecDeque::new(),
            child: spawned.pid,
            exit_ready: spawned.exec_failed.is_some(),
            exit: spawned.exec_failed,
            queries: Vec::new(),
            next_query: 1,
            conns: HashMap::new(),
            next_key: FIRST_CONN_KEY,
            active: None,
            top_fence: None,
            outbox: Vec::new(),
            size,
            foreground: None,
            next_foreground_check: now,
            next_rebind: now + REBIND_EVERY,
            started_at_ms: now_ms(),
            terminating: false,
            done: false,
        })
    }

    /// Registers the descriptors, reports `READY` and serves until the exit is acknowledged.
    pub fn run(mut self, mut ready: File) -> Result<(), Error> {
        let poll_err = |e| Error::new("poller", e);
        // SAFETY: the descriptors live as long as `self`, which owns the poller. The poller stops
        // watching a descriptor when it closes, and the code deletes it before it closes one.
        unsafe {
            for source in &self.sources {
                let interest = Event::readable(source.key);
                self.poller
                    .add_with_mode(&source.fd, interest, PollMode::Level)
                    .map_err(poll_err)?;
            }
            if let Some(stdin) = &self.stdin {
                self.poller
                    .add_with_mode(stdin, Event::none(K_IN), PollMode::Level)
                    .map_err(poll_err)?;
            }
            let listener = self.listener.as_raw_fd();
            self.poller
                .add_with_mode(listener, Event::readable(K_LISTENER), PollMode::Level)
                .map_err(poll_err)?;
            let signals = self.signals.as_raw_fd();
            self.poller
                .add_with_mode(signals, Event::readable(K_SIGNAL), PollMode::Level)
                .map_err(poll_err)?;
        }
        writeln!(ready, "READY {}", std::process::id()).map_err(|e| Error::new("ready pipe", e))?;
        drop(ready);

        let mut events = Events::new();
        while !self.done {
            events.clear();
            match self.poller.wait(&mut events, self.next_timeout()) {
                Ok(_) => {}
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(Error::new("poll", e)),
            }
            for event in events.iter() {
                match event.key {
                    K_OUT | K_ERR => self.source_event(event.key, event.readable, event.writable),
                    K_IN => self.write_input(),
                    K_LISTENER => self.accept(),
                    K_SIGNAL => self.signal_event(),
                    key => self.conn_event(key, event.readable),
                }
            }
            self.drop_late_connections();
            self.timers();
            self.pump();
        }
        let _ = fs::remove_file(&self.config.socket_path);
        hlog!(self.log, "exit acknowledged, holder done");
        Ok(())
    }

    fn next_timeout(&self) -> Option<Duration> {
        let now = Instant::now();
        let hello = self.conns.values().filter_map(|c| c.hello_deadline);
        let client = self.active.is_some().then(|| {
            let foreground =
                (!self.pipe && self.exit.is_none()).then_some(self.next_foreground_check);
            let checkpoint = self.checkpoints.deadline(&self.feed.journal.info());
            [foreground, checkpoint]
        });
        hello
            .chain(client.into_iter().flatten().flatten())
            .chain([self.next_rebind])
            .min()
            .map(|deadline| deadline.saturating_duration_since(now))
    }

    /// The work that the clock triggers: checkpoint requests, foreground changes, the socket check.
    fn timers(&mut self) {
        let now = Instant::now();
        if self.active.is_some() {
            if let Some((offset, reason)) = self.checkpoints.due(now, &self.feed.journal.info()) {
                self.outbox
                    .push(HolderEvent::CheckpointWanted { offset, reason });
            }
            if now >= self.next_foreground_check {
                self.check_foreground();
                self.next_foreground_check = now + FOREGROUND_EVERY;
            }
        }
        if now >= self.next_rebind {
            self.check_socket();
            self.next_rebind = now + REBIND_EVERY;
        }
    }

    // ---- the child's output and input ----

    fn master(&self) -> Option<&OwnedFd> {
        self.sources.first().filter(|_| !self.pipe).map(|s| &s.fd)
    }

    fn source_event(&mut self, key: usize, readable: bool, writable: bool) {
        if readable {
            self.read_source(key);
        }
        if writable && key == K_OUT && !self.pipe {
            self.write_input();
        }
    }

    /// Reads the child's output into the journal.
    fn read_source(&mut self, key: usize) {
        let Some(index) = self.sources.iter().position(|s| s.key == key && s.open) else {
            return;
        };
        let stream = self.sources[index].stream;
        let mut buf = [0u8; READ_SIZE];
        for _ in 0..READS_PER_WAKEUP {
            match read(&self.sources[index].fd, &mut buf) {
                Ok(n) if n > 0 => {
                    self.checkpoints.output(Instant::now());
                    let queries = self.feed.push(stream, &buf[..n]);
                    self.on_queries(queries);
                }
                Err(Errno::AGAIN | Errno::INTR) => break,
                // End of output: the other side closed (`EIO` on Linux, a zero read on macOS).
                _ => {
                    self.sources[index].open = false;
                    let _ = self.poller.delete(&self.sources[index].fd);
                    break;
                }
            }
        }
        if self.active.is_some() && !self.pipe {
            self.check_foreground();
        }
    }

    /// Answers the queries that need no screen, and queues the others, while no core is attached.
    fn on_queries(&mut self, found: Vec<Query>) {
        if self.active.is_some() {
            return;
        }
        for Query { offset, kind, raw } in found {
            if kind.needs_screen() {
                self.queries.retain(|q| {
                    now_ms() - q.received_at_ms <= i64::from(self.config.query_fresh_ms)
                });
                if self.queries.len() < MAX_QUERIES {
                    let seq = self.next_query;
                    self.next_query += 1;
                    self.queries.push(QueuedQuery {
                        seq,
                        offset,
                        received_at_ms: now_ms(),
                        kind,
                        raw,
                    });
                }
            } else if let Some(reply) = self.answer(kind) {
                self.input.push_back(PendingInput {
                    id: None,
                    bytes: reply,
                    written: 0,
                });
                self.write_input();
            }
        }
    }

    /// The reply to a query that the holder can answer without the screen.
    fn answer(&self, kind: QueryKind) -> Option<Vec<u8>> {
        let tracked = &self.feed.tracked;
        match kind {
            QueryKind::Da1 => Some(answers::DA1.to_vec()),
            QueryKind::Da2 => Some(answers::DA2.to_vec()),
            QueryKind::Da3 => Some(answers::DA3.to_vec()),
            QueryKind::XtVersion => Some(answers::XTVERSION.to_vec()),
            QueryKind::Decrqm {
                private: true,
                mode: 2004,
            } => Some(answers::decrqm(true, 2004, Some(tracked.bracketed_paste))),
            QueryKind::Decrqm {
                private: true,
                mode: 1004,
            } => Some(answers::decrqm(true, 1004, Some(tracked.focus_events))),
            _ => None,
        }
    }

    /// The descriptor that input goes to: the PTY master, or the child's stdin.
    fn writer(&self) -> Option<&OwnedFd> {
        if self.pipe {
            self.stdin.as_ref()
        } else {
            self.master()
        }
    }

    /// Writes queued input to the child. An input is acknowledged after its last byte is written.
    fn write_input(&mut self) {
        while let Some(mut input) = self.input.pop_front() {
            let rest = &input.bytes[input.written..];
            let result = match self.writer() {
                Some(fd) => write(fd, rest),
                None => Err(Errno::BADF),
            };
            match result {
                Ok(n) => {
                    if self.pipe {
                        // In pipe mode the journal has every byte that went to stdin.
                        self.feed.push(StreamId::Stdin, &rest[..n]);
                    }
                    input.written += n;
                }
                Err(Errno::AGAIN | Errno::INTR) => {
                    self.input.push_front(input);
                    break;
                }
                Err(e) => {
                    hlog!(self.log, "write to the child failed: {e}");
                    self.input.clear();
                    break;
                }
            }
            if input.written < input.bytes.len() {
                self.input.push_front(input);
            } else if let Some(id) = input.id {
                let offset = self.feed.journal.mark(Marker::InputAck { input_id: id });
                self.seen.insert(id, Some(offset));
                self.outbox.push(HolderEvent::InputAck {
                    input_id: id,
                    offset,
                    result: InputResult::Written,
                });
            }
        }
        self.watch_input();
    }

    /// Watches the descriptor for writes only while input is queued.
    fn watch_input(&mut self) {
        let want = !self.input.is_empty();
        if want == self.write_interest {
            return;
        }
        let (key, readable) = if self.pipe {
            (K_IN, false)
        } else {
            (K_OUT, true)
        };
        let interest = match (want, readable) {
            (true, true) => Event::all(key),
            (true, false) => Event::writable(key),
            (false, true) => Event::readable(key),
            (false, false) => Event::none(key),
        };
        if let Some(fd) = self.writer()
            && self
                .poller
                .modify_with_mode(fd, interest, PollMode::Level)
                .is_ok()
        {
            self.write_interest = want;
        }
    }

    fn check_foreground(&mut self) {
        if self.exit.is_some() {
            return;
        }
        let pgid = self.master().and_then(pty::foreground_pgid).unwrap_or(0);
        if self.foreground.is_some_and(|last| last != pgid) {
            self.outbox.push(HolderEvent::ForegroundChanged { pgid });
        }
        self.foreground = Some(pgid);
    }

    // ---- signals and the child ----

    fn signal_event(&mut self) {
        let mut buf = [0u8; 64];
        let Ok(n) = read(&self.signals, &mut buf) else {
            return;
        };
        for &signal in &buf[..n] {
            match i32::from(signal) {
                libc::SIGCHLD => self.reap(),
                libc::SIGTERM | libc::SIGHUP => self.terminate(),
                libc::SIGUSR1 => self.check_socket(),
                _ => {}
            }
        }
    }

    /// Collects the child when it has ended, and reads the output that is left.
    fn reap(&mut self) {
        if self.exit.is_some() {
            return;
        }
        if let Ok(Some((_, status))) = waitpid(Some(self.child), WaitOptions::NOHANG) {
            let status = spawn::exit_status(status);
            hlog!(self.log, "child ended: {status:?}");
            self.exit = Some(status);
            self.read_source(K_OUT);
            self.read_source(K_ERR);
            self.exit_ready = true;
            if self.terminating {
                self.done = true;
            }
        }
    }

    /// `SIGTERM` or `SIGHUP` to the holder: hang up the child and leave when it ends.
    fn terminate(&mut self) {
        self.terminating = true;
        if self.exit.is_none() {
            let _ = kill_process_group(self.child, Signal::HUP);
        } else {
            self.done = true;
        }
    }

    fn signal(&mut self, target: SignalTarget, signal: SignalKind) {
        if self.exit.is_some() {
            return;
        }
        let signal = match signal {
            SignalKind::Hup => Signal::HUP,
            SignalKind::Int => Signal::INT,
            SignalKind::Term => Signal::TERM,
            SignalKind::Kill => Signal::KILL,
            SignalKind::Usr1 => Signal::USR1,
        };
        let group = match target {
            SignalTarget::Foreground if !self.pipe => self
                .master()
                .and_then(pty::foreground_pgid)
                .and_then(Pid::from_raw),
            _ => Some(self.child),
        };
        if let Some(group) = group
            && let Err(e) = kill_process_group(group, signal)
        {
            hlog!(self.log, "signal failed: {e}");
        }
    }

    // ---- the socket ----

    /// Binds the socket again if its file is gone or is another file. macOS deletes old
    /// files in `/tmp`.
    fn check_socket(&mut self) {
        let path = Path::new(&self.config.socket_path);
        let meta = fs::symlink_metadata(path);
        if meta
            .as_ref()
            .is_ok_and(|m| m.file_type().is_socket() && m.ino() == self.socket_inode)
        {
            return;
        }
        // A file that is not ours is left alone. A socket of this user is a stale one.
        if let Ok(m) = &meta
            && !(m.file_type().is_socket() && m.uid() == self.uid)
        {
            return hlog!(
                self.log,
                "the socket path holds another file, not binding again"
            );
        }
        let _ = fs::remove_file(path);
        match server::bind(&self.config.socket_path, self.uid) {
            Ok((listener, inode)) => {
                let _ = self.poller.delete(&self.listener);
                // SAFETY: the new listener lives in `self`, which owns the poller.
                let added = unsafe {
                    self.poller.add_with_mode(
                        &listener,
                        Event::readable(K_LISTENER),
                        PollMode::Level,
                    )
                };
                match added {
                    Ok(()) => {
                        self.listener = listener;
                        self.socket_inode = inode;
                        hlog!(self.log, "bound the socket again");
                    }
                    Err(e) => hlog!(self.log, "cannot watch the new socket: {e}"),
                }
            }
            Err(e) => hlog!(self.log, "cannot bind the socket again: {e}"),
        }
    }

    // ---- connections ----

    fn accept(&mut self) {
        loop {
            let stream = match self.listener.accept() {
                Ok((stream, _)) => stream,
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => return,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return hlog!(self.log, "accept failed: {e}"),
            };
            if stream.set_nonblocking(true).is_err() {
                continue;
            }
            if auth::peer_uid(&stream).ok() != Some(self.uid) {
                refuse_now(stream, RefuseReason::WrongUser);
                continue;
            }
            let key = self.next_key;
            self.next_key += 1;
            // SAFETY: the stream is stored in `self.conns` and removed from the poller before it
            // is dropped (`drop_conn`), or the poller sees it close.
            let added = unsafe {
                self.poller
                    .add_with_mode(stream.as_raw_fd(), Event::readable(key), PollMode::Level)
            };
            match added {
                Ok(()) => {
                    self.conns
                        .insert(key, Conn::new(stream, Instant::now() + HELLO_TIMEOUT));
                }
                Err(e) => hlog!(self.log, "cannot watch a connection: {e}"),
            }
        }
    }

    fn conn_event(&mut self, key: usize, readable: bool) {
        let Some(mut conn) = self.conns.remove(&key) else {
            return;
        };
        let mut open = true;
        if readable {
            match conn.fill() {
                Ok(true) => {}
                Ok(false) => open = false,
                Err(e) => {
                    hlog!(self.log, "connection {key}: {e}");
                    open = false;
                }
            }
            while !conn.closing {
                match conn.next_request() {
                    Ok(Some(request)) => {
                        if !self.on_request(&mut conn, key, request) {
                            open = false;
                            break;
                        }
                    }
                    Ok(None) => break,
                    Err(e) => {
                        hlog!(self.log, "connection {key}: {e}");
                        open = false;
                        break;
                    }
                }
            }
        }
        if open || conn.queued_bytes() > 0 && conn.closing {
            self.conns.insert(key, conn);
        } else {
            self.drop_conn(key, conn);
        }
    }

    fn drop_conn(&mut self, key: usize, conn: Conn) {
        let _ = self.poller.delete(&conn.stream);
        if self.active == Some(key) {
            self.active = None;
        }
    }

    fn drop_late_connections(&mut self) {
        let now = Instant::now();
        let late: Vec<usize> = self
            .conns
            .iter()
            .filter(|(_, c)| c.hello_deadline.is_some_and(|d| d <= now))
            .map(|(&k, _)| k)
            .collect();
        for key in late {
            if let Some(conn) = self.conns.remove(&key) {
                hlog!(self.log, "connection {key} sent no Hello");
                self.drop_conn(key, conn);
            }
        }
    }

    /// Handles one request. Returns `false` when the connection must close at once.
    fn on_request(&mut self, conn: &mut Conn, key: usize, request: HolderRequest) -> bool {
        use HolderRequest as R;
        if conn.session.is_none() {
            return match request {
                R::Hello {
                    proto,
                    secret,
                    fencing_token,
                    client,
                } => self.hello(conn, key, (proto, &secret, fencing_token), &client),
                _ => false,
            };
        }
        match request {
            R::Hello { .. } | R::Detach => return false,
            R::Status => conn.queue(&HolderEvent::StatusReply(self.status())),
            R::Attach { from_offset } => self.attach(conn, from_offset),
            R::Input { input_id, bytes } => self.client_input(conn, input_id, bytes),
            R::Resize {
                cols,
                rows,
                px_width,
                px_height,
            } => self.resize(cols, rows, px_width, px_height),
            R::CheckpointTaken { offset } => {
                self.feed.journal.set_checkpoint(offset);
                self.checkpoints.taken(Instant::now());
            }
            R::QueriesAnswered { up_to_seq } => self.queries.retain(|q| q.seq > up_to_seq),
            R::Signal { target, signal } => self.signal(target, signal),
            R::CloseStdin => {
                if let Some(stdin) = self.stdin.take() {
                    let _ = self.poller.delete(&stdin);
                }
            }
            R::ExitAck => self.done |= self.exit_ready,
            R::Ping { nonce } => conn.queue(&HolderEvent::Pong { nonce }),
        }
        true
    }

    fn hello(
        &mut self,
        conn: &mut Conn,
        key: usize,
        (proto, secret, token): (u16, &[u8; 32], u64),
        client: &str,
    ) -> bool {
        let refusal = if proto != HOLDER_PROTO {
            Some(RefuseReason::ProtoUnsupported)
        } else if !auth::secrets_equal(secret, &self.config.secret) {
            Some(RefuseReason::BadSecret)
        } else if self.top_fence.is_some_and(|top| token <= top) {
            Some(RefuseReason::StaleFence)
        } else {
            None
        };
        if let Some(reason) = refusal {
            hlog!(self.log, "refused {client}: {reason:?}");
            conn.queue(&HolderEvent::Refused { reason });
            conn.closing = true;
            return true;
        }
        // The new client takes the lease. The old client is told, then closed.
        if let Some(old) = self.active.take()
            && let Some(old_conn) = self.conns.get_mut(&old)
        {
            old_conn.queue(&HolderEvent::LeaseLost {
                by_fencing_token: token,
            });
            old_conn.session = None;
            old_conn.closing = true;
        }
        conn.session = Some(Session { send: None });
        conn.hello_deadline = None;
        self.active = Some(key);
        self.top_fence = Some(token);
        // The new client has made no checkpoint and has seen no change of the foreground group.
        self.checkpoints.forget_request();
        self.foreground = self.master().and_then(pty::foreground_pgid);
        hlog!(self.log, "{client} attached with fencing token {token}");
        conn.queue(&HolderEvent::HelloOk {
            proto: HOLDER_PROTO,
            pane: self.config.pane,
            mode: if self.pipe {
                HoldModeKind::Pipe
            } else {
                HoldModeKind::Pty
            },
            child_pid: self.child.as_raw_nonzero().get(),
            journal: self.feed.journal.info(),
            fencing_token: token,
        });
        true
    }

    fn attach(&mut self, conn: &mut Conn, from: u64) {
        let journal = &self.feed.journal;
        let (start, end) = (journal.start(), journal.end());
        let offset = if from < start {
            conn.queue(&HolderEvent::Truncated {
                requested: from,
                start,
            });
            start
        } else {
            from.min(end)
        };
        if let Some(session) = &mut conn.session {
            let first = if self.pipe {
                StreamId::Stdout
            } else {
                StreamId::Pty
            };
            session.send = Some(Send {
                offset,
                stream: journal.stream_before(offset).unwrap_or(first),
                marker_seq: journal.marker_seq_from(offset),
                exit_sent: false,
            });
        }
    }

    fn client_input(&mut self, conn: &mut Conn, id: u64, bytes: Vec<u8>) {
        match self.seen.get(&id) {
            Some(&Some(offset)) => {
                conn.queue(&HolderEvent::InputAck {
                    input_id: id,
                    offset,
                    result: InputResult::Duplicate,
                });
                return;
            }
            // The first copy waits in the queue, and its ack comes when it is written. Or a failed
            // write dropped it, and no ack comes. A `Duplicate` now would confirm bytes that the
            // child did not get.
            Some(None) => return,
            None => {}
        }
        self.seen.insert(id, None);
        self.seen_order.push_back(id);
        if self.seen_order.len() > SEEN_INPUTS
            && let Some(oldest) = self.seen_order.pop_front()
        {
            self.seen.remove(&oldest);
        }
        self.input.push_back(PendingInput {
            id: Some(id),
            bytes,
            written: 0,
        });
        self.write_input();
    }

    fn resize(&mut self, cols: u16, rows: u16, px_width: u16, px_height: u16) {
        let Some(master) = self.master() else {
            return hlog!(self.log, "resize ignored: the child has no terminal");
        };
        if let Err(e) = tcsetwinsize(master, pty::winsize(cols, rows, px_width, px_height)) {
            hlog!(self.log, "resize failed: {e}");
        }
        self.size = (cols, rows);
        self.feed.journal.mark(Marker::Resize { cols, rows });
    }

    fn status(&mut self) -> HolderStatus {
        let fresh = i64::from(self.config.query_fresh_ms);
        self.queries
            .retain(|q| now_ms() - q.received_at_ms <= fresh);
        HolderStatus {
            child_pid: self.child.as_raw_nonzero().get(),
            fg_pgid: self.master().and_then(pty::foreground_pgid).unwrap_or(0),
            exited: self.exit,
            cols: self.size.0,
            rows: self.size.1,
            journal: self.feed.journal.info(),
            tracked: self.feed.tracked.clone(),
            queued_queries: self.queries.clone(),
            holder_version: env!("CARGO_PKG_VERSION").to_owned(),
            started_at_ms: self.started_at_ms,
        }
    }

    // ---- output to the client ----

    /// Moves output to the active client and sends what every socket accepts.
    fn pump(&mut self) {
        if let Some(key) = self.active
            && let Some(mut conn) = self.conns.remove(&key)
        {
            self.produce(&mut conn);
            self.conns.insert(key, conn);
        } else {
            self.outbox.clear();
        }
        let keys: Vec<usize> = self.conns.keys().copied().collect();
        for key in keys {
            let Some(mut conn) = self.conns.remove(&key) else {
                continue;
            };
            let flushed = conn.flush();
            if let Err(e) = &flushed {
                hlog!(self.log, "connection {key}: {e}");
            }
            if flushed.is_err() || conn.finished() {
                self.drop_conn(key, conn);
                continue;
            }
            let want = conn.queued_bytes() > 0;
            if want != conn.write_interest {
                let interest = if want {
                    Event::all(key)
                } else {
                    Event::readable(key)
                };
                let _ = self
                    .poller
                    .modify_with_mode(&conn.stream, interest, PollMode::Level);
                conn.write_interest = want;
            }
            self.conns.insert(key, conn);
        }
    }

    fn produce(&mut self, conn: &mut Conn) {
        for event in self.outbox.drain(..) {
            conn.queue(&event);
        }
        let Some(mut send) = conn.session.as_mut().and_then(|s| s.send.take()) else {
            return;
        };
        while conn.queued_bytes() < OUT_HIGH_WATER {
            match self.next_event(&mut send) {
                Some(event) => conn.queue(&event),
                None => break,
            }
        }
        if let Some(session) = &mut conn.session {
            session.send = Some(send);
        }
    }

    /// The next output event for a client: a marker at its offset, else journal bytes up to the
    /// next marker, else the child's exit.
    fn next_event(&self, send: &mut Send) -> Option<HolderEvent> {
        let journal = &self.feed.journal;
        if send.offset < journal.start() {
            let event = HolderEvent::Truncated {
                requested: send.offset,
                start: journal.start(),
            };
            send.offset = journal.start();
            send.marker_seq = journal.marker_seq_from(send.offset);
            send.stream = journal.stream_before(send.offset).unwrap_or(send.stream);
            return Some(event);
        }
        while let Some((seq, entry)) = journal.marker(send.marker_seq) {
            send.marker_seq = seq + 1;
            if entry.offset == send.offset {
                if let Marker::Stream { id } = entry.marker {
                    send.stream = id;
                }
                return Some(HolderEvent::Marker {
                    offset: entry.offset,
                    marker: entry.marker,
                });
            }
            if entry.offset > send.offset {
                send.marker_seq = seq;
                break;
            }
        }
        let limit = journal
            .marker(send.marker_seq)
            .map_or(journal.end(), |(_, e)| e.offset.min(journal.end()));
        if send.offset < limit {
            let len = usize::try_from(limit - send.offset)
                .map_or(MAX_OUTPUT_CHUNK, |n| n.min(MAX_OUTPUT_CHUNK));
            let (a, b) = journal.read(send.offset, len);
            let event = HolderEvent::Output {
                offset: send.offset,
                stream: send.stream,
                bytes: [a, b].concat(),
            };
            send.offset += len as u64;
            return Some(event);
        }
        if self.exit_ready && !send.exit_sent && send.offset == journal.end() {
            send.exit_sent = true;
            return self.exit.map(|status| HolderEvent::ChildExited { status });
        }
        None
    }
}

/// Tells a connection why it is refused and drops it.
fn refuse_now(mut stream: UnixStream, reason: RefuseReason) {
    let _ = nh_proto::framing::write_msg(&mut stream, &HolderEvent::Refused { reason });
}

fn install_signal_handlers() {
    // SAFETY: the structure is fully initialised before use, and the handler is async-signal-safe.
    unsafe {
        let mut action: libc::sigaction = std::mem::zeroed();
        action.sa_sigaction = on_signal as *const () as libc::sighandler_t;
        action.sa_flags = libc::SA_RESTART;
        libc::sigemptyset(&mut action.sa_mask);
        for signal in [libc::SIGCHLD, libc::SIGTERM, libc::SIGHUP, libc::SIGUSR1] {
            libc::sigaction(signal, &action, std::ptr::null_mut());
        }
        // A client that closes its socket must not kill the holder. The child gets the default
        // again in `spawn`.
        libc::signal(libc::SIGPIPE, libc::SIG_IGN);
    }
}
