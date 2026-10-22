//! Files that a client puts in the blob store over a link, and files dropped on a pane of a remote
//! window (the `Bulk` class of the link).

mod core_support;

use std::{
    collections::VecDeque,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use core_support::{Harness, TIMEOUT, next_input_id};
use nh_core::{ErrorCode, RecordingHost};
use nh_proto::input::{LogicalInput, LogicalText};
use nh_remote::{
    ClientHello, ClientOptions, Device, Hello, LinkClient, LinkMsg, LinkServer, LinkTiming,
    codec::{Decoder, Scheduler, encode},
    msg::BlobOutcome,
};
use serde_json::{Value, json};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixStream,
    time::timeout,
};
use ulid::Ulid;

fn timing() -> LinkTiming {
    LinkTiming {
        ping_every: Duration::from_millis(100),
        down_after: Duration::from_secs(5),
        hello_within: Duration::from_secs(5),
    }
}

fn serve(h: &Harness) -> LinkServer {
    nh_remote::serve_with(h.handle.clone(), &h.paths, "test", timing()).expect("link.sock serves")
}

async fn connect(server: &LinkServer) -> LinkClient {
    LinkClient::connect_unix(
        server.path(),
        ClientHello {
            device: Ulid::from_parts(1, 61),
            name: "laptop".to_owned(),
            app_version: "0.0.0".to_owned(),
        },
        Arc::new(RecordingHost::new(PathBuf::from("/bin/nh"), true)),
        ClientOptions {
            timing: timing(),
            ..ClientOptions::default()
        },
    )
    .await
    .expect("the link opens")
}

/// Bytes that do not repeat inside a part, so a part in the wrong place would show.
fn bytes_of(len: usize) -> Vec<u8> {
    (0..len).map(|n| (n * 31 + n / 251) as u8).collect()
}

fn text_input(pane: &str, text: &str) -> Value {
    let event = serde_json::to_value(LogicalInput::Text(LogicalText {
        text: text.to_owned(),
    }))
    .expect("an input");
    json!({ "pane": pane, "input_id": next_input_id(), "event": event })
}

async fn first_pane(h: &Harness, file: &Path) -> String {
    let script = format!("stty -echo; cat >> {}", file.display());
    let made = h
        .call(
            "workspace.create",
            json!({ "command": { "argv": ["/bin/sh", "-c", script] } }),
        )
        .await;
    let pane = made["pane"].as_str().expect("a pane").to_owned();
    // The holder starts after the call: the first input waits for it.
    let deadline = Instant::now() + TIMEOUT;
    loop {
        match h
            .handle
            .rpc("pane.input", text_input(&pane, "ready\r"))
            .await
        {
            Ok(_) => return pane,
            Err(error) if error.code == ErrorCode::PaneGone && Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            Err(error) => panic!("ready: {error:?}"),
        }
    }
}

async fn typed(file: &Path, what: &str, done: impl Fn(&str) -> bool) -> String {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let text = std::fs::read_to_string(file).unwrap_or_default();
        if done(&text) {
            return text;
        }
        assert!(
            Instant::now() < deadline,
            "waited for {what}: the file has {text:?}"
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_file_goes_up_in_parts_and_a_drop_pastes_the_paths_of_the_copies() {
    let h = Harness::start();
    let server = serve(&h);
    let client = connect(&server).await;
    let file = h.dir.join("typed.txt");
    let pane = first_pane(&h, &file).await;

    // A file of 3 MiB: the hash that comes back is its digest, and the store has it.
    let shot = bytes_of(3 << 20);
    let hash = client
        .put_blob("shot.png", &shot)
        .await
        .expect("it goes up");
    assert_eq!(hash, blake3::hash(&shot).to_hex().to_string());
    let (stored, mime) = client
        .blob(&hash)
        .await
        .expect("the blob answers")
        .expect("the blob is there");
    assert_eq!(stored, shot);
    assert_eq!(mime, "image/png");
    // The same bytes again give the same blob, and an empty file goes up too.
    assert_eq!(
        client.put_blob("again.png", &shot).await.expect("again"),
        hash
    );
    let empty = client
        .put_blob("empty.txt", &[])
        .await
        .expect("an empty file");
    assert_eq!(empty, blake3::hash(&[]).to_hex().to_string());

    // The drop: two files of one name, and a name with a path in it.
    let notes = client
        .put_blob("notes.md", b"# notes\n")
        .await
        .expect("notes");
    client
        .call(
            None,
            "pane.drop_blobs",
            json!({
                "pane": pane, "input_id": next_input_id(),
                "blobs": [
                    {"hash": hash, "name": "shot.png"},
                    {"hash": notes, "name": "../../etc/notes.md"},
                    {"hash": notes, "name": "notes.md"},
                ],
            }),
        )
        .await
        .expect("the drop works");
    client
        .call(None, "pane.input", text_input(&pane, "\r"))
        .await
        .expect("enter");
    let text = typed(&file, "the pasted paths", |t| t.contains("notes.md-2")).await;
    let line = text.lines().nth(1).expect("the line of the drop");
    let paths: Vec<&str> = line
        .split(' ')
        .map(|p| p.trim_matches('\''))
        .filter(|p| !p.is_empty())
        .collect();
    assert_eq!(paths.len(), 3, "{line}");
    let data = h.paths.data_dir("test").expect("a data dir");
    for path in &paths {
        let path = Path::new(path);
        assert!(path.starts_with(data.join("drops")), "{}", path.display());
        let mode = std::fs::metadata(path)
            .expect("a file")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600, "{}", path.display());
        let dir_mode = std::fs::metadata(path.parent().expect("a dir"))
            .expect("a dir")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(dir_mode, 0o700);
    }
    assert!(paths[0].ends_with("/shot.png"));
    assert_eq!(std::fs::read(paths[0]).expect("the copy"), shot);
    assert!(paths[1].ends_with("/notes.md"), "{}", paths[1]);
    assert!(paths[2].ends_with("/notes.md-2"), "{}", paths[2]);
    assert_eq!(std::fs::read(paths[2]).expect("the copy"), b"# notes\n");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_drop_of_an_unknown_blob_or_of_no_file_is_refused() {
    let h = Harness::start();
    let server = serve(&h);
    let client = connect(&server).await;
    let file = h.dir.join("typed.txt");
    let pane = first_pane(&h, &file).await;
    for (blobs, code) in [
        (
            json!([{"hash": "0".repeat(64), "name": "a"}]),
            ErrorCode::NotFound,
        ),
        (json!([]), ErrorCode::InvalidParams),
        (
            json!([{"hash": "nothex", "name": "a"}]),
            ErrorCode::NotFound,
        ),
    ] {
        let error = client
            .call(
                None,
                "pane.drop_blobs",
                json!({"pane": pane, "input_id": next_input_id(), "blobs": blobs}),
            )
            .await
            .expect_err("refused");
        assert_eq!(error.code, code, "{error:?}");
    }
}

// --- a client that does not follow the rules -------------------------------------------------

struct Raw {
    stream: UnixStream,
    decoder: Decoder,
    pending: VecDeque<LinkMsg>,
}

impl Raw {
    async fn connect(server: &LinkServer) -> Self {
        let mut raw = Self {
            stream: UnixStream::connect(server.path()).await.expect("connects"),
            decoder: Decoder::default(),
            pending: VecDeque::new(),
        };
        raw.send(&LinkMsg::Hello(Hello {
            proto: nh_remote::msg::PROTO.to_owned(),
            api: nh_proto::version::API_VERSION.to_owned(),
            app_version: "0.0.0".to_owned(),
            device: Device {
                id: Ulid::from_parts(2, 62).to_string(),
                name: "raw".to_owned(),
            },
        }))
        .await;
        assert!(matches!(raw.recv().await, Some(LinkMsg::Welcome(_))));
        raw
    }

    async fn send(&mut self, msg: &LinkMsg) {
        let mut scheduler = Scheduler::default();
        scheduler.push(encode(msg).expect("encodes"));
        let mut wire = Vec::new();
        while scheduler.next_part(&mut wire) {}
        self.stream.write_all(&wire).await.expect("writes");
    }

    async fn recv(&mut self) -> Option<LinkMsg> {
        if let Some(msg) = self.pending.pop_front() {
            return Some(msg);
        }
        let mut buf = vec![0u8; 65_536];
        loop {
            let n = timeout(TIMEOUT, self.stream.read(&mut buf))
                .await
                .expect("an answer in time")
                .expect("reads");
            if n == 0 {
                return None;
            }
            self.pending.extend(
                self.decoder
                    .feed(&buf[..n])
                    .expect("link/1")
                    .into_iter()
                    .filter(|m| {
                        !matches!(
                            m,
                            LinkMsg::Ping { .. }
                                | LinkMsg::Pong { .. }
                                | LinkMsg::HostRequest { .. }
                        )
                    }),
            );
            if let Some(msg) = self.pending.pop_front() {
                return Some(msg);
            }
        }
    }
}

fn error_code(outcome: &BlobOutcome) -> String {
    let BlobOutcome::Err(bytes) = outcome else {
        panic!("expected an error, got {outcome:?}");
    };
    serde_json::from_slice::<Value>(bytes).expect("JSON")["code"]
        .as_str()
        .unwrap_or_default()
        .to_owned()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_core_checks_the_digest_the_order_and_the_limit_of_an_upload() {
    let h = Harness::start();
    let server = serve(&h);
    let mut raw = Raw::connect(&server).await;

    // A digest that is not the one of the bytes: nothing is stored.
    let bytes = bytes_of(100_000);
    raw.send(&LinkMsg::BlobPut {
        id: 1,
        name: "bad.bin".to_owned(),
        size: bytes.len() as u64,
        hash: blake3::hash(b"other bytes").to_hex().to_string(),
    })
    .await;
    raw.send(&LinkMsg::BlobPart {
        id: 1,
        offset: 0,
        bytes: bytes.clone(),
        last: true,
    })
    .await;
    // The part gets its credit, and then the end.
    let mut done = None;
    while let Some(msg) = raw.recv().await {
        if let LinkMsg::BlobDone { id: 1, outcome } = msg {
            done = Some(outcome);
            break;
        }
    }
    assert_eq!(error_code(&done.expect("an end")), "blob_corrupt");
    let real = blake3::hash(&bytes).to_hex().to_string();
    assert!(h.handle.blob(&real).await.expect("a read").is_none());

    // A part in the wrong place ends the upload.
    raw.send(&LinkMsg::BlobPut {
        id: 2,
        name: "order.bin".to_owned(),
        size: 10,
        hash: real.clone(),
    })
    .await;
    raw.send(&LinkMsg::BlobPart {
        id: 2,
        offset: 5,
        bytes: vec![1, 2, 3],
        last: false,
    })
    .await;
    let mut done = None;
    while let Some(msg) = raw.recv().await {
        if let LinkMsg::BlobDone { id: 2, outcome } = msg {
            done = Some(outcome);
            break;
        }
    }
    assert_eq!(error_code(&done.expect("an end")), "invalid_params");

    // A file over the limit is refused at the announcement, before any part.
    raw.send(&LinkMsg::BlobPut {
        id: 3,
        name: "huge.bin".to_owned(),
        size: nh_core::UPLOAD_MAX_BYTES + 1,
        hash: real,
    })
    .await;
    loop {
        match raw.recv().await {
            Some(LinkMsg::BlobDone { id: 3, outcome }) => {
                assert_eq!(error_code(&outcome), "too_large");
                break;
            }
            Some(_) => {}
            None => panic!("the core ended the link"),
        }
    }
}
