//! A process that writes in a loop is killed with `SIGKILL` 50 times. After each kill the
//! database must be sound, and every committed event must have its state change, and the other
//! way round (the transactional outbox).
//!
//! The test runs itself as the writer: `writer_child` does nothing unless `NH_STORE_CRASH_DIR` is
//! set, and then it writes until it is killed.

use std::{
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};

use nh_store::Store;
use rusqlite::Connection;
use serde_json::json;

const CHILD_DIR: &str = "NH_STORE_CRASH_DIR";
/// What the writer prints when the store is open. The test harness may put its own text in front
/// of it on the same line.
const READY: &str = "NH-STORE-CRASH-WRITER-READY";
const KILLS: usize = 50;

fn db_path(dir: &Path) -> PathBuf {
    dir.join("state.db")
}

/// The writer. Each transaction inserts a group, appends an event for it, and bumps a projection.
#[test]
fn writer_child() {
    let Ok(dir) = std::env::var(CHILD_DIR) else {
        return;
    };
    let mut store = Store::open(&db_path(Path::new(&dir))).expect("the child opens the store");
    let mut next: u64 = {
        let conn = Connection::open(db_path(Path::new(&dir))).expect("a reader");
        let last: i64 = conn
            .query_row(
                "SELECT coalesce(max(cast(substr(id, 2) AS INTEGER)), 0) FROM groups",
                [],
                |row| row.get(0),
            )
            .expect("the last group");
        u64::try_from(last).expect("a count") + 1
    };
    println!("{READY}");
    loop {
        let id = format!("g{next}");
        store
            .tx(i64::try_from(next).unwrap_or(0), |tx| {
                tx.sql().execute(
                    "INSERT INTO groups (id, name, sort_key, created_at, updated_at)
                     VALUES (?1, 'group', 'a', 0, 0)",
                    [&id],
                )?;
                tx.append_raw("group.created", 1, Some(&id), json!({"n": next}))?;
                tx.bump_projection("tree")?;
                Ok(())
            })
            .expect("a commit");
        next += 1;
    }
}

/// A small generator, so that the kill times differ without a dependency.
struct Xorshift(u64);

impl Xorshift {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

/// What a check of the database found.
struct Found {
    groups: i64,
    events: i64,
    projection: i64,
}

fn check(dir: &Path) -> Found {
    // Opening the store runs the recovery of the WAL.
    let store = Store::open(&db_path(dir)).expect("the parent opens the store");
    store.integrity_check().expect("integrity_check says ok");
    drop(store);
    let conn = Connection::open(db_path(dir)).expect("a reader");
    let one = |sql: &str| -> i64 { conn.query_row(sql, [], |row| row.get(0)).expect(sql) };
    let found = Found {
        groups: one("SELECT count(*) FROM groups"),
        events: one("SELECT count(*) FROM events WHERE kind = 'group.created'"),
        projection: one(
            "SELECT coalesce((SELECT version FROM projection_versions WHERE name = 'tree'), 0)",
        ),
    };
    // Every event has its group, and every group has its event.
    let without_group = one("SELECT count(*) FROM events e WHERE kind = 'group.created'
         AND NOT EXISTS (SELECT 1 FROM groups g WHERE g.id = e.subject)");
    let without_event = one(
        "SELECT count(*) FROM groups g
         WHERE NOT EXISTS (SELECT 1 FROM events e WHERE e.subject = g.id AND e.kind = 'group.created')",
    );
    assert_eq!(
        (without_group, without_event),
        (0, 0),
        "an event or a row is alone"
    );
    // No gap: the groups are g1 to gN.
    let highest = one("SELECT coalesce(max(cast(substr(id, 2) AS INTEGER)), 0) FROM groups");
    assert_eq!(highest, found.groups, "a group is missing");
    assert_eq!(found.events, found.groups);
    assert_eq!(
        found.projection, found.groups,
        "the projection version went with the commits"
    );
    found
}

#[test]
fn the_database_is_sound_after_fifty_kills() {
    let dir = std::env::temp_dir().join(format!("nh-store-crash-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a directory");
    let exe = std::env::current_exe().expect("the test binary");
    let mut random = Xorshift(0x9E37_79B9_7F4A_7C15);
    let mut committed = 0;

    for round in 0..KILLS {
        let mut child = Command::new(&exe)
            .args(["--exact", "writer_child", "--nocapture", "--test-threads=1"])
            .env(CHILD_DIR, &dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("the writer starts");
        // The child says when the store is open and the loop begins. A thread reads its output,
        // so that the test never waits for a child that says nothing.
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let stdout = child.stdout.take().expect("a pipe");
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if line.contains(READY) {
                    let _ = ready_tx.send(());
                }
            }
        });
        if ready_rx.recv_timeout(Duration::from_secs(30)).is_err() {
            let _ = child.kill();
            let _ = child.wait();
            panic!("round {round}: the writer never got ready");
        }
        // Let it write for 1 to 40 ms, and kill it in the middle of its work.
        std::thread::sleep(Duration::from_millis(1 + random.next() % 40));
        child.kill().expect("SIGKILL");
        let status = child.wait().expect("the writer ends");
        assert!(
            !status.success(),
            "round {round}: the writer ended by itself"
        );

        let found = check(&dir);
        assert!(
            found.groups >= committed,
            "round {round}: committed rows are gone"
        );
        committed = found.groups;
    }
    eprintln!("{committed} committed rows survived {KILLS} kills");
    assert!(
        committed >= i64::try_from(KILLS).expect("a count"),
        "the writer committed only {committed} rows in {KILLS} rounds"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
