//! Terminal queries while no core is attached: the holder answers the ones that need
//! no screen, queues the others, and follows the modes that it needs for the answers.

mod support;

use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU32, Ordering},
    time::Duration,
};

use nh_proto::{
    answers::{self, QueryKind},
    holder::{HoldConfig, HolderEvent, HolderRequest, HolderStatus},
};
use support::{TestHolder, config};

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// A trigger file that tells the child to start. The test creates it when it wants.
fn trigger() -> PathBuf {
    static N: AtomicU32 = AtomicU32::new(0);
    let path = PathBuf::from(format!(
        "/tmp/nhh-go-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_file(&path);
    path
}

/// A child that waits for `go`, sends `send` to its terminal, reads up to `n` bytes of reply in
/// raw mode (it waits 1.5 s for each) and prints them as hexadecimal between `REPLY:` and `:END`.
fn asking(go: &Path, send: &str, n: usize) -> HoldConfig {
    let go = go.display();
    let script = format!(
        "stty raw -echo min 0 time 15; while [ ! -f {go} ]; do sleep 0.05; done; \
         printf '{send}'; reply=$(dd bs=1 count={n} 2>/dev/null | od -An -tx1); \
         echo \"REPLY:$reply:END\"; sleep 30"
    );
    config(&["/bin/sh", "-c", &script])
}

/// The hexadecimal reply that the child read, from its output.
fn reply_in(output: &[u8]) -> String {
    let text = String::from_utf8_lossy(output).into_owned();
    let start = text.find("REPLY:").expect("the child printed a reply line") + "REPLY:".len();
    let end = text.find(":END").expect("the reply line has an end");
    text[start..end].split_whitespace().collect()
}

/// Runs the child with no client attached while it asks, then reads what it got back.
fn detached_reply(send: &str, expected: &[u8]) -> String {
    let go = trigger();
    let holder = TestHolder::spawn_with(asking(&go, send, expected.len().max(1)), None);
    std::fs::write(&go, "").expect("the trigger is written");
    std::thread::sleep(Duration::from_millis(800));
    let mut client = holder.attach(0);
    let (output, _) = client.output_containing(":END");
    let _ = std::fs::remove_file(&go);
    reply_in(&output)
}

#[test]
fn the_holder_answers_the_queries_that_need_no_screen() {
    for (name, send, reply) in [
        ("DA1", "\\033[c", answers::DA1),
        ("DA1 with a zero", "\\033[0c", answers::DA1),
        ("DA2", "\\033[>c", answers::DA2),
        ("DA3", "\\033[=c", answers::DA3),
        ("XTVERSION", "\\033[>q", answers::XTVERSION),
    ] {
        assert_eq!(detached_reply(send, reply), hex(reply), "{name}");
    }
}

#[test]
fn decrqm_for_a_tracked_mode_follows_the_mode_and_other_modes_get_no_answer() {
    let set = answers::decrqm(true, 2004, Some(true));
    assert_eq!(detached_reply("\\033[?2004h\\033[?2004$p", &set), hex(&set));
    let reset = answers::decrqm(true, 1004, Some(false));
    assert_eq!(
        detached_reply("\\033[?1004h\\033[?1004l\\033[?1004$p", &reset),
        hex(&reset)
    );
    assert_eq!(
        detached_reply("\\033[?25$p", b"x"),
        "",
        "a mode that the holder does not track"
    );
}

#[test]
fn with_a_client_attached_the_holder_answers_nothing() {
    let go = trigger();
    let holder = TestHolder::spawn_with(asking(&go, "\\033[c", answers::DA1.len()), None);
    let mut client = holder.attach(0);
    std::fs::write(&go, "").expect("the trigger is written");
    let (output, _) = client.output_containing(":END");
    let _ = std::fs::remove_file(&go);
    assert_eq!(
        reply_in(&output),
        "",
        "the core answers when it is attached"
    );
    client.send(&HolderRequest::Status);
    let status = status_of(&mut client);
    assert!(
        status.queued_queries.is_empty(),
        "an attached holder queues nothing"
    );
}

fn status_of(client: &mut support::Client) -> HolderStatus {
    client.send(&HolderRequest::Status);
    client.wait_for(|e| match e {
        HolderEvent::StatusReply(s) => Some(s.clone()),
        _ => None,
    })
}

#[test]
fn the_queries_that_need_the_screen_are_queued_in_order_and_get_no_reply() {
    let go = trigger();
    let send = "\\033[6n\\033]11;?\\007\\033[14t";
    let holder = TestHolder::spawn_with(asking(&go, send, 1), None);
    std::fs::write(&go, "").expect("the trigger is written");
    std::thread::sleep(Duration::from_millis(800));
    let mut client = holder.attach(0);
    let (output, _) = client.output_containing(":END");
    let _ = std::fs::remove_file(&go);
    assert_eq!(reply_in(&output), "", "the holder does not answer these");

    let status = status_of(&mut client);
    let queued: Vec<_> = status
        .queued_queries
        .iter()
        .map(|q| (q.seq, q.kind, q.raw.clone()))
        .collect();
    assert_eq!(
        queued,
        [
            (1, QueryKind::CursorPosition, b"\x1b[6n".to_vec()),
            (
                2,
                QueryKind::OscColor { code: 11 },
                b"\x1b]11;?\x07".to_vec()
            ),
            (3, QueryKind::WindowSize { op: 14 }, b"\x1b[14t".to_vec()),
        ]
    );
    // The offsets are the ends of the sequences, in the journal.
    let offsets: Vec<u64> = status.queued_queries.iter().map(|q| q.offset).collect();
    assert!(offsets.windows(2).all(|w| w[0] < w[1]), "{offsets:?}");
    assert!(
        status
            .queued_queries
            .iter()
            .all(|q| q.received_at_ms > 1_700_000_000_000)
    );

    client.send(&HolderRequest::QueriesAnswered { up_to_seq: 2 });
    let status = status_of(&mut client);
    assert_eq!(
        status
            .queued_queries
            .iter()
            .map(|q| q.seq)
            .collect::<Vec<_>>(),
        [3]
    );
}

#[test]
fn a_queued_query_that_is_no_longer_fresh_is_dropped() {
    let go = trigger();
    let mut config = asking(&go, "\\033[6n", 1);
    config.query_fresh_ms = 200;
    let holder = TestHolder::spawn_with(config, None);
    std::fs::write(&go, "").expect("the trigger is written");
    std::thread::sleep(Duration::from_millis(1000));
    let mut client = holder.client();
    assert!(status_of(&mut client).queued_queries.is_empty());
    let _ = std::fs::remove_file(&go);
}

#[test]
fn the_holder_follows_the_modes_that_it_answers_for() {
    let script = "printf '\\033[?2004h\\033[?1004h\\033[>1u\\033[>5u\\033[=2;2u'; sleep 30";
    let holder = TestHolder::spawn(&["/bin/sh", "-c", script]);
    let mut client = holder.client();
    let status = loop {
        let status = status_of(&mut client);
        if status.journal.end > 0 {
            break status;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    assert!(status.tracked.bracketed_paste && status.tracked.focus_events);
    assert_eq!(status.tracked.kitty_flags_stack, [1, 7]);
}
