//! The holder socket: binding it, and the buffers of one client connection.

use std::{
    fs::{self, DirBuilder},
    io::{self, Read, Write},
    os::unix::{
        fs::{DirBuilderExt, MetadataExt, PermissionsExt},
        net::{UnixListener, UnixStream},
    },
    path::Path,
    time::Instant,
};

use nh_proto::{
    framing::{self, FramingError, MAX_MSG_BYTES},
    holder::{HolderEvent, HolderRequest, StreamId},
    paths::{FileKind, RootMeta, check_runtime_root},
};

use crate::Error;

/// The most unsent bytes a connection holds before the holder stops adding output to it.
pub const OUT_HIGH_WATER: usize = 256 * 1024;

/// Creates the socket directory (mode 0700, owned by `uid`) and binds the socket (mode 0600).
/// Returns the listener and the inode of the socket file, to find out later if the file was
/// replaced.
///
/// The directory is private, so nobody else can reach the socket before its mode is set. The
/// process umask must be `0o077`, or the directory gets the wrong mode.
pub fn bind(socket_path: &str, uid: u32) -> Result<(UnixListener, u64), Error> {
    let path = Path::new(socket_path);
    let dir = path
        .parent()
        .ok_or_else(|| Error::new("socket path", "it has no directory"))?;
    DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)
        .map_err(|e| Error::new("socket directory", e))?;
    let meta = fs::symlink_metadata(dir).map_err(|e| Error::new("socket directory", e))?;
    let kind = match meta.file_type() {
        t if t.is_symlink() => FileKind::Symlink,
        t if t.is_dir() => FileKind::Directory,
        _ => FileKind::Other,
    };
    check_runtime_root(
        RootMeta {
            kind,
            uid: meta.uid(),
            mode: meta.mode(),
        },
        uid,
    )
    .map_err(|e| Error::new(&format!("socket directory {}", dir.display()), e))?;
    let listener = UnixListener::bind(path).map_err(|e| Error::new("bind socket", e))?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|e| Error::new("socket mode", e))?;
    listener
        .set_nonblocking(true)
        .map_err(|e| Error::new("socket", e))?;
    let inode = fs::symlink_metadata(path)
        .map_err(|e| Error::new("socket", e))?
        .ino();
    Ok((listener, inode))
}

/// What a client may do after a successful `Hello`.
pub struct Session {
    /// Set by `Attach`. Without it the client gets replies only.
    pub send: Option<Send>,
}

/// Where the output stream of a client has got to.
pub struct Send {
    /// The next journal byte to send.
    pub offset: u64,
    /// The stream of the bytes that come next.
    pub stream: StreamId,
    /// The next marker to send, by sequence number.
    pub marker_seq: u64,
    /// Whether this client has been told that the child exited.
    pub exit_sent: bool,
}

/// One client connection: its buffers, its deadline and its session.
pub struct Conn {
    pub stream: UnixStream,
    inbuf: Vec<u8>,
    out: Vec<u8>,
    out_pos: usize,
    /// The time by which a client that has not said `Hello` must do so.
    pub hello_deadline: Option<Instant>,
    pub session: Option<Session>,
    /// Close the connection when the output is sent.
    pub closing: bool,
    /// Whether the poller watches the socket for writes.
    pub write_interest: bool,
}

impl Conn {
    pub fn new(stream: UnixStream, hello_deadline: Instant) -> Self {
        Self {
            stream,
            inbuf: Vec::new(),
            out: Vec::new(),
            out_pos: 0,
            hello_deadline: Some(hello_deadline),
            session: None,
            closing: false,
            write_interest: false,
        }
    }

    /// Reads what the socket has. Returns `false` when the peer closed it.
    pub fn fill(&mut self) -> io::Result<bool> {
        let mut chunk = [0u8; 16 * 1024];
        loop {
            match self.stream.read(&mut chunk) {
                Ok(0) => return Ok(false),
                Ok(n) => self.inbuf.extend_from_slice(&chunk[..n]),
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => return Ok(true),
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
            if self.inbuf.len() > MAX_MSG_BYTES + 4 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "message over the limit",
                ));
            }
        }
    }

    /// Takes the next complete request out of the buffer, if there is one.
    pub fn next_request(&mut self) -> Result<Option<HolderRequest>, FramingError> {
        let Some(header) = self.inbuf.first_chunk::<4>() else {
            return Ok(None);
        };
        let len = u32::from_le_bytes(*header) as usize;
        if len > MAX_MSG_BYTES {
            return Err(FramingError::TooLarge { len: len as u64 });
        }
        if self.inbuf.len() < 4 + len {
            return Ok(None);
        }
        let request = framing::read_msg(&mut &self.inbuf[..4 + len])?;
        self.inbuf.drain(..4 + len);
        Ok(Some(request))
    }

    /// Adds an event to the output buffer.
    pub fn queue(&mut self, event: &HolderEvent) {
        framing::write_msg(&mut self.out, event).expect("a holder event is below the message cap");
    }

    /// The number of bytes that are queued and not yet sent.
    pub fn queued_bytes(&self) -> usize {
        self.out.len() - self.out_pos
    }

    /// Sends what the socket accepts and keeps the rest.
    pub fn flush(&mut self) -> io::Result<()> {
        while self.out_pos < self.out.len() {
            match self.stream.write(&self.out[self.out_pos..]) {
                Ok(n) => self.out_pos += n,
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => return Ok(()),
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
        }
        self.out.clear();
        self.out_pos = 0;
        Ok(())
    }

    /// Whether the connection should close now.
    pub fn finished(&self) -> bool {
        self.closing && self.queued_bytes() == 0
    }
}
