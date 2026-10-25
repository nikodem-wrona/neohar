use std::{fs, path::PathBuf};

use nh_proto::api::events::{Cursor, EventFilter};
use nh_store::{ArchivedLine, EventsRead, PaneLiveness, PaneRecord, TruncatedReason};
use ulid::Ulid;

use super::*;

const HOUR_MS: i64 = 3_600_000;

fn dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("nh-retention-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("a directory");
    dir
}

/// The limits of a `config.toml` with this text.
fn limits_of(name: &str, text: &str) -> Limits {
    let dir = dir(name);
    let path = dir.join(crate::config::FILE_NAME);
    fs::write(&path, text).expect("a file");
    let config = ConfigStore::load(path);
    assert!(config.warnings().is_empty(), "{:?}", config.warnings());
    let limits = Limits::of(&config.config());
    let _ = fs::remove_dir_all(dir);
    limits
}

fn append(store: &mut Store, kind: &str, at: i64) -> u64 {
    store
        .tx(at, |tx| tx.append_raw(kind, 1, None, serde_json::json!({})))
        .expect("an event")
        .0
}

fn read(store: &Store, cursor: &Cursor) -> EventsRead {
    store
        .events_since(cursor, &EventFilter::default(), 100)
        .expect("a read")
}

#[test]
fn a_pass_with_a_short_retention_deletes_old_events_and_truncates_a_cursor_behind_them() {
    let limits = limits_of(
        "events",
        "[events]\nsync_retention = \"1h\"\naudit_retention = \"3h\"\n",
    );
    assert_eq!(
        limits.retention,
        Retention {
            sync: Duration::from_secs(3_600),
            audit: Duration::from_secs(3 * 3_600),
        }
    );
    let mut store = Store::open_in_memory().expect("a store");
    let start = store.cursor_now().expect("a cursor");
    let now = 100 * HOUR_MS;
    append(&mut store, "pane.created", now - 5 * HOUR_MS); // 1: past both
    append(&mut store, "interaction.opened", now - 4 * HOUR_MS); // 2: past both
    append(&mut store, "pane.created", now - 2 * HOUR_MS); // 3: past the sync retention
    append(&mut store, "policy.added", now - 2 * HOUR_MS); // 4: inside the audit retention
    append(&mut store, "pane.created", now - HOUR_MS / 2); // 5: fresh

    let done = pass(&mut store, &limits, now).expect("a pass");
    assert_eq!(
        done,
        Pass {
            events: Swept { sync: 2, audit: 1 },
            interactions: 0,
            segments: 0,
        }
    );
    let kept: Vec<String> = store
        .recent_events(100)
        .expect("events")
        .into_iter()
        .map(|event| event.kind)
        .collect();
    assert_eq!(kept, ["policy.added", "pane.created"]);
    // A client that read nothing yet missed deleted events: it takes a new snapshot.
    let EventsRead::Truncated { reason, cursor } = read(&store, &start) else {
        panic!("the old cursor is not truncated");
    };
    assert_eq!(reason, TruncatedReason::Expired);
    assert_eq!(
        cursor.seq, 5,
        "the snapshot continues from the end of the log"
    );
    // A client that read up to the last deleted event misses nothing.
    let behind_nothing = Cursor { seq: 3, ..start };
    let EventsRead::Events { events, .. } = read(&store, &behind_nothing) else {
        panic!("a cursor at the floor goes on");
    };
    assert_eq!(
        events.iter().map(|event| event.seq).collect::<Vec<_>>(),
        [4, 5]
    );
    // A second pass at the same time has nothing to do.
    assert_eq!(
        pass(&mut store, &limits, now).expect("a pass"),
        Pass::default()
    );
}

fn add_pane(store: &Store, render_id: u32) -> Ulid {
    let id = Ulid::from_parts(1_000 + u64::from(render_id), u128::from(render_id));
    store
        .insert_pane(
            &PaneRecord {
                id,
                render_id,
                socket: "/tmp/s".to_owned(),
                holder_pid: 1,
                secret: [0; 32],
                fencing_token: 1,
                argv: vec![],
                cwd: "/".to_owned(),
                cols: 80,
                rows: 24,
                liveness: PaneLiveness::Running,
                exit: None,
            },
            1,
        )
        .expect("a pane");
    id
}

fn archive(store: &mut Store, pane: Ulid, lines: u64) {
    let mut writer = store.scrollback_writer(pane).expect("a writer");
    writer.push(
        (0..lines).map(|line_no| ArchivedLine {
            line_no,
            wrapped: false,
            text: format!("line {line_no}"),
            runs: Vec::new(),
        }),
        1,
    );
    writer.flush(store, 1).expect("a flush");
}

#[test]
fn a_pass_keeps_the_archive_of_each_pane_under_the_cap() {
    let dir = dir("scrollback");
    let mut store = Store::open(&dir.join("state.db")).expect("a store");
    let (big, small) = (add_pane(&store, 1), add_pane(&store, 2));
    // Three segments: a segment ends after `SEGMENT_LINES` lines.
    archive(&mut store, big, 2 * nh_store::SEGMENT_LINES + 50);
    archive(&mut store, small, 50);
    let segments = |store: &Store, pane| store.scrollback_segments(pane).expect("segments");
    assert_eq!(segments(&store, big).len(), 3);
    let newest = segments(&store, big)[2].clone();

    // No cap by default.
    let none = limits_of("no-cap", "");
    assert_eq!(none.max_bytes_per_pane, 0);
    assert_eq!(pass(&mut store, &none, 2).expect("a pass").segments, 0);
    assert_eq!(segments(&store, big).len(), 3);

    // A cap of one byte: the oldest segments go, and the newest segment of each pane stays.
    let tiny = limits_of("cap", "[scrollback]\nmax_bytes_per_pane = 1\n");
    assert_eq!(pass(&mut store, &tiny, 3).expect("a pass").segments, 2);
    assert_eq!(segments(&store, big), [newest]);
    assert_eq!(segments(&store, small).len(), 1);
    assert_eq!(
        store
            .read_lines(big, 0, 3 * nh_store::SEGMENT_LINES as usize)
            .expect("a read")
            .first()
            .map(|line| line.line_no),
        Some(2 * nh_store::SEGMENT_LINES),
        "the archive starts at the newest segment"
    );
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn a_pass_deletes_the_interactions_that_ended_before_the_audit_retention() {
    let limits = limits_of("interactions", "[events]\naudit_retention = \"3h\"\n");
    let mut store = Store::open_in_memory().expect("a store");
    let now = 100 * HOUR_MS;
    let row = |n: u128, state: &str, resolved: Option<i64>| nh_store::InteractionRow {
        id: Ulid::from(n),
        handle: format!("i{n}"),
        run: None,
        pane: None,
        kind: "approval".to_owned(),
        native_ref: None,
        title: "Run `ls`".to_owned(),
        risk: "low".to_owned(),
        payload: serde_json::json!({"kind": "approval", "tool": "Bash"}),
        state: state.to_owned(),
        answerable: true,
        gate_mode: "observe".to_owned(),
        source: "hook".to_owned(),
        confidence: 1.0,
        opened_at: now - 10 * HOUR_MS,
        deadline: None,
        resolved_at: resolved,
        resolution: None,
        fingerprint: None,
    };
    let rows = [
        row(1, "delivered", Some(now - 4 * HOUR_MS)), // ended before the retention: goes
        row(2, "dismissed", Some(now - 2 * HOUR_MS)), // ended inside the retention: stays
        row(3, "open", None),                         // open, also when old: stays
    ];
    store
        .tx(now, |tx| {
            rows.iter().try_for_each(|row| tx.put_interaction(row))
        })
        .expect("rows");
    let done = pass(&mut store, &limits, now).expect("a pass");
    assert_eq!(done.interactions, 1);
    let left: Vec<String> = store
        .recent_interactions(10)
        .expect("a read")
        .into_iter()
        .map(|(row, _)| row.handle)
        .collect();
    assert_eq!(left.len(), 2, "{left:?}");
    assert!(!left.contains(&"i1".to_owned()), "{left:?}");
}

#[test]
fn a_reload_that_changes_a_limit_makes_a_pass_due_at_once() {
    let dir = dir("due");
    let path = dir.join(crate::config::FILE_NAME);
    let config = ConfigStore::load(path.clone());
    let first = Limits::of(&config.config());
    let start = Instant::now();
    assert!(due(None, &first, start), "the first pass runs at the start");
    let last = (first, start);
    assert!(!due(Some(&last), &first, start + LOOK));
    assert!(!due(Some(&last), &first, start + PERIOD - LOOK));
    assert!(due(Some(&last), &first, start + PERIOD));

    fs::write(&path, "[events]\nsync_retention = \"2d\"\n").expect("a file");
    config.reload();
    let second = Limits::of(&config.config());
    assert_eq!(second.retention.sync, Duration::from_secs(2 * 86_400));
    assert!(due(Some(&last), &second, start + LOOK));
    let _ = fs::remove_dir_all(dir);
}
