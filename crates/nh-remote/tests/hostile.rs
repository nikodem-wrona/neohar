//! A fake core that does not follow the rules ("Hostile host requests"): it sends host
//! requests that a link does not carry and requests over the limits of the client, and the client
//! refuses each one. It also stops answering pings, and the client calls the link down. It sends
//! a blob or a message over the limits of the client, and messages that are not `Welcome` for
//! longer than the client waits.

use std::{collections::VecDeque, path::PathBuf, sync::Arc, time::Duration};

use nh_core::{ErrorCode, RecordingHost, host::HostCall};
use nh_proto::api::ids::WindowId;
use nh_remote::{
    Action, ClientHello, ClientOptions, HostOutcome, HostRequest, Level, LinkClient, LinkMsg,
    LinkTiming, Notification, Welcome, Window,
    codec::{Decoder, Scheduler, encode},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, DuplexStream, ReadHalf, WriteHalf},
    time::timeout,
};
use ulid::Ulid;

const TIMEOUT: Duration = Duration::from_secs(10);

/// The side of the fake core.
struct FakeCore {
    read: ReadHalf<DuplexStream>,
    write: WriteHalf<DuplexStream>,
    decoder: Decoder,
    /// Messages that came in one read with the one that `recv` gave.
    pending: VecDeque<LinkMsg>,
    next: u64,
}

impl FakeCore {
    async fn send(&mut self, msg: &LinkMsg) {
        let mut scheduler = Scheduler::default();
        scheduler.push(encode(msg).expect("encodes"));
        let mut wire = Vec::new();
        while scheduler.next_part(&mut wire) {}
        self.write.write_all(&wire).await.expect("writes");
    }

    /// The next message of the client that is not a ping.
    async fn recv(&mut self) -> Option<LinkMsg> {
        if let Some(msg) = self.pending.pop_front() {
            return Some(msg);
        }
        let mut buf = vec![0u8; 65_536];
        loop {
            let n = timeout(TIMEOUT, self.read.read(&mut buf))
                .await
                .expect("in time")
                .ok()?;
            if n == 0 {
                return None;
            }
            self.pending.extend(
                self.decoder
                    .feed(&buf[..n])
                    .expect("link/1")
                    .into_iter()
                    .filter(|m| !matches!(m, LinkMsg::Ping { .. } | LinkMsg::Pong { .. })),
            );
            if let Some(msg) = self.pending.pop_front() {
                return Some(msg);
            }
        }
    }

    /// Sends the raw body of a host request and returns the answer of the client.
    async fn ask_raw(&mut self, request: Vec<u8>) -> HostOutcome {
        self.next += 1;
        let id = self.next;
        self.send(&LinkMsg::HostRequest { id, request }).await;
        match self.recv().await {
            Some(LinkMsg::HostReply {
                id: answered,
                result,
            }) if answered == id => result,
            other => panic!("expected the reply to {id}, got {other:?}"),
        }
    }

    async fn ask(&mut self, request: &HostRequest) -> HostOutcome {
        self.ask_raw(postcard::to_allocvec(request).expect("encodes"))
            .await
    }
}

/// A client linked to a fake core.
async fn linked(timing: LinkTiming) -> (LinkClient, FakeCore, Arc<RecordingHost>) {
    linked_with(ClientOptions {
        timing,
        ..ClientOptions::default()
    })
    .await
}

fn client_hello() -> ClientHello {
    ClientHello {
        device: Ulid::from_parts(5, 5),
        name: "laptop".to_owned(),
        app_version: "0.0.0".to_owned(),
    }
}

/// A fake core and the two halves of the stream of its client.
fn fake_core() -> (FakeCore, ReadHalf<DuplexStream>, WriteHalf<DuplexStream>) {
    let (client_side, core_side) = tokio::io::duplex(1 << 20);
    let (read, write) = tokio::io::split(core_side);
    let core = FakeCore {
        read,
        write,
        decoder: Decoder::default(),
        pending: VecDeque::new(),
        next: 0,
    };
    let (client_read, client_write) = tokio::io::split(client_side);
    (core, client_read, client_write)
}

/// A client with `options`, linked to a fake core.
async fn linked_with(options: ClientOptions) -> (LinkClient, FakeCore, Arc<RecordingHost>) {
    let (mut core, client_read, client_write) = fake_core();
    let host = Arc::new(RecordingHost::new(PathBuf::from("/bin/nh"), true));
    let connect = LinkClient::connect(
        client_read,
        client_write,
        client_hello(),
        host.clone(),
        options,
    );
    let accept = async {
        assert!(matches!(core.recv().await, Some(LinkMsg::Hello(_))));
        core.send(&LinkMsg::Welcome(Welcome {
            core_version: "0.0.0".to_owned(),
            machine_uuid: "m".to_owned(),
            session_uuid: "s".to_owned(),
            hostname: "studio".to_owned(),
            core_epoch: "e".to_owned(),
        }))
        .await;
    };
    let (client, ()) = tokio::join!(connect, accept);
    (client.expect("the link opens"), core, host)
}

fn note(title: String, body: String) -> Notification {
    Notification {
        title,
        body,
        urgency: Level::Normal,
        interaction: None,
        actions: vec![Action::Allow, Action::Deny],
        pane: None,
    }
}

fn is_refused(outcome: &HostOutcome) -> bool {
    matches!(outcome, HostOutcome::Refused(_))
}

#[tokio::test]
async fn the_client_refuses_every_request_outside_the_rules() {
    let (client, mut core, host) = linked(LinkTiming::default()).await;
    let mine = WindowId::new(Ulid::from_parts(9, 1));
    client.set_owned_windows([mine]);

    // A request that is no variant of the enum: an open of a URL, a pick of a path.
    let mut open_url = vec![77u8];
    open_url.extend(postcard::to_allocvec("file:///etc/passwd").expect("encodes"));
    assert!(is_refused(&core.ask_raw(open_url).await));
    assert!(is_refused(&core.ask_raw(Vec::new()).await));

    // Text over the limits.
    let long_title = note("t".repeat(201), "b".to_owned());
    let long_body = note("t".to_owned(), "b".repeat(1_001));
    for notification in [long_title, long_body] {
        let outcome = core
            .ask(&HostRequest::Notify {
                id: "n".to_owned(),
                notification,
            })
            .await;
        assert!(is_refused(&outcome));
    }
    let clipboard = HostRequest::ClipboardWrite {
        text: "x".repeat((1 << 20) + 1),
    };
    assert!(is_refused(&core.ask(&clipboard).await));

    // A window that this client does not own, and one with a bad id.
    let other = WindowId::new(Ulid::from_parts(9, 2));
    for window in [other.to_string(), "../../etc".to_owned()] {
        let open = HostRequest::OpenWindow {
            window: Window {
                id: window.clone(),
                bounds: None,
                panes: Vec::new(),
                tab: None,
            },
        };
        assert!(is_refused(&core.ask(&open).await));
        assert!(is_refused(
            &core.ask(&HostRequest::FocusWindow { window }).await
        ));
    }
    // An update or a withdrawal of a notification that the client never showed.
    let update = HostRequest::UpdateNotification {
        id: "never".to_owned(),
        notification: note("t".to_owned(), "b".to_owned()),
    };
    assert!(is_refused(&core.ask(&update).await));

    // At most 10 notifications in 10 s.
    let mut outcomes = Vec::new();
    for n in 0..11 {
        let notify = HostRequest::Notify {
            id: format!("n{n}"),
            notification: note(format!("n{n}"), "b".to_owned()),
        };
        outcomes.push(core.ask(&notify).await);
    }
    assert!(outcomes[..10].iter().all(|o| *o == HostOutcome::Done));
    assert!(is_refused(&outcomes[10]));

    // What is allowed still works.
    assert_eq!(
        core.ask(&HostRequest::FocusWindow {
            window: mine.to_string()
        })
        .await,
        HostOutcome::Done
    );
    let calls = host.calls();
    assert_eq!(calls.len(), 11, "{calls:?}");
    assert!(
        calls[..10]
            .iter()
            .all(|c| matches!(c, HostCall::Notify(..)))
    );
    assert_eq!(calls[10], HostCall::FocusWindow(mine));
    assert!(client.is_up(), "a refused request does not end the link");
}

#[tokio::test]
async fn a_core_that_answers_no_ping_is_down() {
    let timing = LinkTiming {
        ping_every: Duration::from_millis(50),
        down_after: Duration::from_millis(200),
        hello_within: Duration::from_secs(5),
    };
    let (client, _core, _) = linked(timing).await;
    // The fake core reads nothing more, so no `Pong` comes.
    let reason = timeout(TIMEOUT, client.closed()).await.expect("in time");
    assert!(reason.contains("Pong"), "{reason}");
    let error = client
        .call(None, "session.info", serde_json::json!({}))
        .await
        .expect_err("down");
    assert_eq!(error.code, ErrorCode::LinkDown);
}

#[tokio::test]
async fn a_call_in_flight_fails_with_link_down_when_the_core_says_bye() {
    let (client, mut core, _) = linked(LinkTiming::default()).await;
    let call = {
        let client = client.clone();
        tokio::spawn(async move {
            client
                .call(None, "task.create", serde_json::json!({}))
                .await
        })
    };
    assert!(matches!(core.recv().await, Some(LinkMsg::Call { .. })));
    core.send(&LinkMsg::Bye {
        reason: "update".to_owned(),
    })
    .await;
    let error = call.await.expect("joins").expect_err("down");
    assert_eq!(error.code, ErrorCode::LinkDown);
    let reason = client.closed().await;
    assert!(reason.contains("update"), "{reason}");
}

#[tokio::test]
async fn a_blob_over_the_limit_of_the_client_fails_and_its_parts_are_dropped() {
    let (client, mut core, _) = linked_with(ClientOptions {
        max_blob: 256 * 1024,
        ..ClientOptions::default()
    })
    .await;
    let blob = {
        let client = client.clone();
        tokio::spawn(async move { client.blob(&"0".repeat(64)).await })
    };
    let Some(LinkMsg::BlobGet { id, .. }) = core.recv().await else {
        panic!("expected BlobGet");
    };
    // The core sends parts for ever: the fifth one is over 256 KiB.
    for n in 0..8u64 {
        core.send(&LinkMsg::BlobPart {
            id,
            offset: n * 65_536,
            bytes: vec![1; 65_536],
            last: false,
        })
        .await;
    }
    let error = timeout(TIMEOUT, blob)
        .await
        .expect("in time")
        .expect("joins")
        .expect_err("too large");
    assert_eq!(error.code, ErrorCode::TooLarge);
    // The parts after it are dropped, and they still get credit, so a core can end the blob.
    let mut credits = 0;
    while credits < 8 {
        match core.recv().await {
            Some(LinkMsg::BlobCredit {
                id: credited,
                parts,
            }) if credited == id => {
                credits += parts;
            }
            other => panic!("expected credit, got {other:?}"),
        }
    }
    core.send(&LinkMsg::BlobDone {
        id,
        outcome: nh_remote::msg::BlobOutcome::Ok {
            hash: "0".repeat(64),
            mime: "image/png".to_owned(),
        },
    })
    .await;
    assert!(
        client.is_up(),
        "a blob over the limit does not end the link"
    );
}

#[tokio::test]
async fn a_small_message_that_decompresses_over_the_limit_of_the_client_ends_the_link() {
    let (client, mut core, _) = linked(LinkTiming::default()).await;
    // 65 MiB of zeros is a few kilobytes after zstd, and more than a client takes from a core.
    let frame = LinkMsg::Frame {
        stream: 1,
        bytes: vec![0; 65 << 20],
    };
    let encoded = encode(&frame).expect("encodes");
    assert!(encoded.compressed && encoded.bytes.len() < 64 * 1024);
    core.send(&frame).await;
    let reason = timeout(TIMEOUT, client.closed()).await.expect("in time");
    assert!(reason.contains("size limit"), "{reason}");
}

#[tokio::test]
async fn a_core_that_sends_no_welcome_does_not_keep_the_client_waiting() {
    let (mut core, read, write) = fake_core();
    let options = ClientOptions {
        timing: LinkTiming {
            hello_within: Duration::from_millis(300),
            ..LinkTiming::default()
        },
        ..ClientOptions::default()
    };
    let connect = LinkClient::connect(
        read,
        write,
        client_hello(),
        Arc::new(RecordingHost::new(PathBuf::from("/bin/nh"), true)),
        options,
    );
    // A whole message of a kind that no version has: the client skips it and waits on.
    let unknown = {
        let body = [1u8, 1, 250];
        let mut part = u32::try_from(body.len())
            .expect("small")
            .to_le_bytes()
            .to_vec();
        part.extend_from_slice(&body);
        part
    };
    let noise = async {
        loop {
            core.write.write_all(&unknown).await.expect("writes");
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    };
    let outcome = tokio::select! {
        outcome = timeout(Duration::from_secs(5), connect) => outcome,
        () = noise => unreachable!("the noise never ends"),
    };
    let error = outcome
        .expect("connect gives up in time")
        .expect_err("no Welcome");
    assert!(matches!(error, nh_remote::LinkError::Timeout), "{error}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_upload_stops_at_eight_parts_until_the_core_gives_credit() {
    let (client, mut core, _host) = linked(LinkTiming::default()).await;
    let bytes = vec![7u8; 3 << 20];
    let hash = blake3::hash(&bytes).to_hex().to_string();
    let sent = {
        let client = client.clone();
        let bytes = bytes.clone();
        tokio::spawn(async move { client.put_blob("big.bin", &bytes).await })
    };
    let Some(LinkMsg::BlobPut {
        id,
        name,
        size,
        hash: announced,
    }) = core.recv().await
    else {
        panic!("expected BlobPut");
    };
    assert_eq!((name.as_str(), size), ("big.bin", 3 << 20));
    assert_eq!(announced, hash);
    // The core gives no credit: eight parts of 64 KiB come, and no ninth.
    let mut parts = Vec::new();
    while let Ok(Some(msg)) = timeout(Duration::from_millis(400), core.recv()).await {
        let LinkMsg::BlobPart {
            offset,
            bytes,
            last,
            ..
        } = msg
        else {
            panic!("expected a part");
        };
        assert_eq!(offset, (parts.len() * (64 << 10)) as u64);
        assert!(!last);
        parts.push(bytes.len());
    }
    assert_eq!(parts, [64 << 10; 8], "eight parts in flight and no more");
    // One credit, one more part.
    core.send(&LinkMsg::BlobCredit { id, parts: 1 }).await;
    let Some(LinkMsg::BlobPart { offset, .. }) = core.recv().await else {
        panic!("expected the ninth part");
    };
    assert_eq!(offset, (8 * (64 << 10)) as u64);
    assert!(
        timeout(Duration::from_millis(300), core.recv())
            .await
            .is_err(),
        "a credit allows one part"
    );
    // The core answers with a hash that is not the digest of the bytes: the client says so.
    core.send(&LinkMsg::BlobDone {
        id,
        outcome: nh_remote::msg::BlobOutcome::Ok {
            hash: "0".repeat(64),
            mime: "application/octet-stream".to_owned(),
        },
    })
    .await;
    let error = sent.await.expect("task").expect_err("the hash is wrong");
    assert_eq!(error.code, ErrorCode::BlobCorrupt);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_file_over_the_limit_is_refused_before_a_byte_goes() {
    let (client, mut core, _host) = linked(LinkTiming::default()).await;
    let bytes = vec![0u8; (nh_core::UPLOAD_MAX_BYTES + 1) as usize];
    let error = client
        .put_blob("huge.bin", &bytes)
        .await
        .expect_err("over the limit");
    assert_eq!(error.code, ErrorCode::TooLarge);
    assert!(
        timeout(Duration::from_millis(300), core.recv())
            .await
            .is_err(),
        "the core heard nothing"
    );
}
