use std::fmt::Write as _;
use std::path::PathBuf;

use rusqlite::types::Value;

use super::*;
use crate::Store;

/// The path that the errors of a database in memory name.
const MEMORY: &str = ":memory:";

fn memory() -> Connection {
    let conn = Connection::open_in_memory().expect("a database");
    conn.pragma_update(None, "foreign_keys", true)
        .expect("foreign keys");
    conn
}

fn run_memory(conn: &mut Connection) -> Result<(), StoreError> {
    run(conn, Path::new(MEMORY))
}

/// The value of an integer pragma.
fn pragma(conn: &Connection, name: &str) -> i64 {
    conn.pragma_query_value(None, name, |row| row.get(0))
        .expect(name)
}

fn count(conn: &Connection, table: &str) -> i64 {
    conn.query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
        row.get(0)
    })
    .expect("a count")
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("nh-migrate-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("the test directory is made");
    dir
}

// The text of a schema. -------------------------------------------------------------------------

/// A value as text: strings in quotes, so that an empty string and `NULL` are different.
fn show(value: &Value) -> String {
    match value {
        Value::Null => "NULL".to_owned(),
        Value::Integer(n) => n.to_string(),
        Value::Real(x) => x.to_string(),
        Value::Text(text) => format!("{text:?}"),
        Value::Blob(bytes) => {
            let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
            format!("x'{hex}'")
        }
    }
}

/// The rows of a query, each row as its values.
fn rows(conn: &Connection, sql: &str, param: &str) -> Vec<Vec<Value>> {
    let mut statement = conn.prepare(sql).expect(sql);
    let width = statement.column_count();
    let params: &[&dyn rusqlite::ToSql] = if sql.contains("?1") { &[&param] } else { &[] };
    statement
        .query_map(params, |row| (0..width).map(|i| row.get(i)).collect())
        .expect(sql)
        .collect::<Result<_, _>>()
        .expect(sql)
}

/// The rows of a query as lines `name value, name value`.
fn lines(conn: &Connection, sql: &str, param: &str) -> Vec<String> {
    let statement = conn.prepare(sql).expect(sql);
    let names: Vec<String> = statement
        .column_names()
        .into_iter()
        .map(str::to_owned)
        .collect();
    rows(conn, sql, param)
        .iter()
        .map(|row| {
            names
                .iter()
                .zip(row)
                .map(|(name, value)| format!("{name} {}", show(value)))
                .collect::<Vec<_>>()
                .join(", ")
        })
        .collect()
}

/// The tokens of `sql`: no white space and no comments, names without their quotes, and strings
/// with theirs.
fn tokens(sql: &str) -> Vec<String> {
    let chars: Vec<char> = sql.chars().collect();
    let at = |i: usize| chars.get(i).copied();
    let starts_comment = |i: usize| {
        matches!(
            (at(i), at(i + 1)),
            (Some('-'), Some('-')) | (Some('/'), Some('*'))
        )
    };
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(c) = at(i) {
        if c.is_whitespace() {
            i += 1;
        } else if at(i) == Some('-') && at(i + 1) == Some('-') {
            while at(i).is_some_and(|c| c != '\n') {
                i += 1;
            }
        } else if at(i) == Some('/') && at(i + 1) == Some('*') {
            i += 2;
            while at(i).is_some() && !(at(i) == Some('*') && at(i + 1) == Some('/')) {
                i += 1;
            }
            i += 2;
        } else if matches!(c, '\'' | '"' | '`' | '[') {
            // A string keeps its quotes. A quoted name loses them.
            let close = if c == '[' { ']' } else { c };
            let mut text = String::new();
            i += 1;
            while let Some(d) = at(i) {
                i += 1;
                if d == close {
                    if close != ']' && at(i) == Some(close) {
                        text.push(d);
                        i += 1;
                        continue;
                    }
                    break;
                }
                text.push(d);
            }
            out.push(if c == '\'' {
                format!("'{}'", text.replace('\'', "''"))
            } else {
                text
            });
        } else if matches!(c, '(' | ')' | ',' | ';') {
            out.push(c.to_string());
            i += 1;
        } else {
            let start = i;
            while at(i).is_some_and(|d| {
                !d.is_whitespace() && !matches!(d, '(' | ')' | ',' | ';' | '\'' | '"' | '`' | '[')
            }) && !starts_comment(i)
            {
                i += 1;
            }
            out.push(chars[start..i].iter().collect());
        }
    }
    out
}

/// `sql` in a normal form, so that the same schema gives the same text for each way to write it.
/// The form has no comments, and it has names without quotes and one space between tokens. It has
/// no space inside parentheses or before a comma, and one line for each column of a table.
fn normal_sql(sql: &str) -> String {
    let tokens = tokens(sql);
    let table = tokens.first().is_some_and(|t| t == "CREATE")
        && tokens
            .get(1)
            .is_some_and(|t| t == "TABLE" || t == "VIRTUAL");
    let mut out = String::new();
    let mut depth = 0;
    let mut previous = "";
    for token in &tokens {
        match token.as_str() {
            "(" => {
                if !out.is_empty() && !matches!(previous, "(" | ",") {
                    out.push(' ');
                }
                out.push('(');
                depth += 1;
                if table && depth == 1 {
                    out.push_str("\n    ");
                }
            }
            ")" => {
                if table && depth == 1 {
                    out.push('\n');
                }
                out.push(')');
                depth -= 1;
            }
            "," => out.push_str(if table && depth == 1 { ",\n    " } else { ", " }),
            word => {
                if !out.is_empty() && !matches!(previous, "(" | ",") {
                    out.push(' ');
                }
                out.push_str(word);
            }
        }
        previous = token;
    }
    out
}

/// The schema of `conn` as text, to compare and to freeze. The text has the pragmas that the file
/// keeps and every object of `sqlite_schema` with its SQL in a normal form. For each table, it has
/// what SQLite reports: the columns, the foreign keys, the indexes and their keys. It also has the
/// rows of each table. It does not have the data that depends on the history of the file, such as
/// the root pages.
fn schema_text(conn: &Connection) -> String {
    let mut out = String::new();
    for name in ["user_version", "application_id", "auto_vacuum", "encoding"] {
        let value: Value = conn
            .pragma_query_value(None, name, |row| row.get(0))
            .expect(name);
        let _ = writeln!(out, "pragma {name} = {}", show(&value));
    }
    let objects = rows(
        conn,
        "SELECT type, name, tbl_name, sql FROM sqlite_schema ORDER BY type, name",
        "",
    );
    for object in &objects {
        let [
            Value::Text(kind),
            Value::Text(name),
            Value::Text(table),
            sql,
        ] = object.as_slice()
        else {
            panic!("a row of sqlite_schema: {object:?}");
        };
        let _ = writeln!(out, "\n{kind} {name} on {table}");
        if let Value::Text(sql) = sql {
            for line in normal_sql(sql).lines() {
                let _ = writeln!(out, "    {line}");
            }
        }
    }
    let tables = rows(
        conn,
        "SELECT name, type, ncol, wr, strict FROM pragma_table_list WHERE schema = 'main' \
         ORDER BY name",
        "",
    );
    for table in &tables {
        let [Value::Text(name), Value::Text(kind), ncol, wr, strict] = table.as_slice() else {
            panic!("a row of pragma_table_list: {table:?}");
        };
        let _ = writeln!(
            out,
            "\n{kind} {name}: columns {}, without_rowid {}, strict {}",
            show(ncol),
            show(wr),
            show(strict)
        );
        let details = [
            (
                "column",
                "SELECT cid, name, type, \"notnull\", dflt_value, pk, hidden \
                 FROM pragma_table_xinfo(?1) ORDER BY cid",
            ),
            (
                "foreign key",
                "SELECT id, seq, \"table\", \"from\", \"to\", on_update, on_delete, \"match\" \
                 FROM pragma_foreign_key_list(?1) ORDER BY id, seq",
            ),
        ];
        for (label, sql) in details {
            for line in lines(conn, sql, name) {
                let _ = writeln!(out, "    {label}: {line}");
            }
        }
        let indexes = rows(
            conn,
            "SELECT name FROM pragma_index_list(?1) ORDER BY name",
            name,
        );
        for index in &indexes {
            let [Value::Text(index)] = index.as_slice() else {
                panic!("a row of pragma_index_list: {index:?}");
            };
            let sql = "SELECT name, \"unique\", origin, partial FROM pragma_index_list(?1) \
                       WHERE name = ?2";
            let about: String = conn
                .query_row(sql, [name, index], |row| {
                    Ok(format!(
                        "name {}, unique {}, origin {}, partial {}",
                        show(&row.get(0)?),
                        show(&row.get(1)?),
                        show(&row.get(2)?),
                        show(&row.get(3)?)
                    ))
                })
                .expect(sql);
            let _ = writeln!(out, "    index: {about}");
            let sql = "SELECT seqno, cid, name, \"desc\", coll, key FROM pragma_index_xinfo(?1) \
                       ORDER BY seqno";
            for line in lines(conn, sql, index) {
                let _ = writeln!(out, "        key: {line}");
            }
        }
        // The rows of a virtual table are in its shadow tables, and the rows of `sqlite_schema`
        // are above.
        if kind != "virtual" && name != "sqlite_schema" {
            let mut found: Vec<String> = rows(conn, &format!("SELECT * FROM \"{name}\""), "")
                .iter()
                .map(|row| row.iter().map(show).collect::<Vec<_>>().join(", "))
                .collect();
            found.sort();
            let _ = writeln!(out, "    rows: {}", found.len());
            for row in found {
                let _ = writeln!(out, "    row: {row}");
            }
        }
    }
    out
}

#[test]
fn the_normal_form_ignores_comments_quotes_and_white_space() {
    let a = "CREATE TABLE \"t\" (\n    a TEXT, -- a comment\n    b TEXT CHECK (b IN ('x', 'y--z'))\n, c INTEGER) STRICT";
    let b = "CREATE TABLE t(a TEXT,b TEXT CHECK(b IN('x','y--z')),c INTEGER)STRICT";
    assert_eq!(normal_sql(a), normal_sql(b));
    assert_eq!(
        normal_sql(b),
        "CREATE TABLE t (\n    a TEXT,\n    b TEXT CHECK (b IN ('x', 'y--z')),\n    c INTEGER\n) STRICT"
    );
    assert_ne!(normal_sql(a), normal_sql(&a.replace("'y--z'", "'y'")));
}

// The runner. ----------------------------------------------------------------------------------

#[test]
fn the_migrations_after_the_baseline_number_every_version_once_in_order() {
    for (i, migration) in MIGRATIONS.iter().enumerate() {
        assert_eq!(migration.version as usize, BASELINE as usize + 1 + i);
    }
    assert_eq!(
        SCHEMA_VERSION as usize,
        BASELINE as usize + MIGRATIONS.len()
    );
}

/// The schema of each version since the baseline is frozen in a snapshot (`schema_NNNN`). A
/// migration that shipped never changes, so the snapshot of its version never changes. A new
/// migration adds the snapshot of its version (`INSTA_UPDATE=always`), and the review reads the
/// change of the schema in it.
///
/// `schema_0015` is also the schema that the 15 steps of the development builds made: when the
/// steps became one file, a test compared the two schemas in this form and found no difference.
#[test]
fn each_version_since_the_baseline_has_its_frozen_schema() {
    for count in 0..=MIGRATIONS.len() {
        let mut conn = memory();
        let migrations = MIGRATIONS.get(..count).expect("a prefix");
        run_with(&mut conn, Path::new(MEMORY), BASELINE_SQL, migrations)
            .expect("the migrations run");
        let version = version(&conn).expect("a version");
        insta::assert_snapshot!(format!("schema_{version:04}"), schema_text(&conn));
    }
}

#[test]
fn an_empty_database_gets_the_baseline_and_every_later_migration() {
    let mut conn = memory();
    run_memory(&mut conn).expect("the migrations run");
    assert_eq!(version(&conn).expect("a version"), SCHEMA_VERSION);
    // The same schema as each step by hand.
    let by_hand = memory();
    for sql in std::iter::once(BASELINE_SQL).chain(MIGRATIONS.iter().map(|m| m.sql)) {
        by_hand.execute_batch(sql).expect("a step");
    }
    by_hand
        .pragma_update(None, "user_version", SCHEMA_VERSION)
        .expect("a version");
    assert_eq!(schema_text(&conn), schema_text(&by_hand));
}

#[test]
fn a_database_at_the_baseline_is_kept_as_it_is() {
    // A database of version 15 of a development build has the schema of the baseline, and rows.
    let mut conn = memory();
    conn.execute_batch(BASELINE_SQL).expect("the baseline");
    conn.execute_batch(
        "INSERT INTO meta (key, value) VALUES ('session_uuid', 'abc');
         INSERT INTO windows (id, sort_key, created_at, updated_at) VALUES ('w1', '1', 1, 1);",
    )
    .expect("rows");
    conn.pragma_update(None, "user_version", BASELINE)
        .expect("a version");
    let before = (pragma(&conn, "schema_version"), schema_text(&conn));
    // The runner with no migration after the baseline, as in this build, changes no part of the
    // schema. The schema cookie of SQLite counts each change.
    run_with(&mut conn, Path::new(MEMORY), BASELINE_SQL, &[]).expect("the runner accepts it");
    assert_eq!(
        (pragma(&conn, "schema_version"), schema_text(&conn)),
        before
    );
    assert_eq!(version(&conn).expect("a version"), BASELINE);
}

#[test]
fn a_database_at_the_baseline_gets_only_the_later_migrations() {
    let later = [Migration {
        version: BASELINE + 1,
        sql: "CREATE TABLE later (x INTEGER) STRICT;",
        rebuilds_tables: false,
    }];
    let mut conn = memory();
    run_with(&mut conn, Path::new(MEMORY), BASELINE_SQL, &[]).expect("the baseline");
    conn.execute("INSERT INTO meta (key, value) VALUES ('k', 'v')", [])
        .expect("a row");
    // The baseline does not run again. If it runs again, its first statement fails on `meta`.
    run_with(&mut conn, Path::new(MEMORY), BASELINE_SQL, &later).expect("the later migration");
    assert_eq!(version(&conn).expect("a version"), BASELINE + 1);
    assert_eq!((count(&conn, "meta"), count(&conn, "later")), (1, 0));
    let cookie = pragma(&conn, "schema_version");
    run_with(&mut conn, Path::new(MEMORY), BASELINE_SQL, &later).expect("a second run");
    assert_eq!(pragma(&conn, "schema_version"), cookie, "nothing ran");
}

#[test]
fn version_16_removes_the_tasks_and_keeps_the_runs_and_the_blobs() {
    // A database of 1.0 with a task that started: its port, a favorite folder, a run of the task,
    // a blob with a reference, and the handles and the projections of the tasks.
    let mut conn = memory();
    run_with(&mut conn, Path::new(MEMORY), BASELINE_SQL, &[]).expect("the baseline");
    conn.execute_batch(
        "INSERT INTO tasks (id, handle, title, slug, repo_root, worktree_path, branch, base,
             status, created_at, updated_at)
         VALUES ('t1', 'k1', 'Fix', 'fix', '/work/app', '/wt/app/fix', 'neohar/fix', 'main',
             'running', 1, 1);
         INSERT INTO port_leases (port, task_id, leased_at) VALUES (41000, 't1', 1);
         INSERT INTO favorite_folders (path, added_at, used_at) VALUES ('/work/app', 1, 1);
         INSERT INTO agent_runs (id, harness, binding, launch_mode, task_id, started_at)
             VALUES ('r1', 'claude', 'bound', 'tui', 't1', 1);
         INSERT INTO blobs (hash, bytes, mime, created_at, refs)
             VALUES ('b1', 3, 'text/plain', 1, 1);
         INSERT INTO handle_counters (scope, next) VALUES ('task', 2), ('workspace', 4);
         INSERT INTO projection_versions (name, version) VALUES ('tasks', 3), ('favorites', 1),
             ('changes:01M4F1W7EM2GN3EV6NHXZJEGGD', 2),
             ('changes_for:01M4F1W7EM2GN3EV6NHXZJEGGE', 1), ('tree', 5);",
    )
    .expect("the rows of 1.0");
    run_with(&mut conn, Path::new(MEMORY), BASELINE_SQL, &MIGRATIONS[..1]).expect("version 16");
    assert_eq!(version(&conn).expect("a version"), 16);
    let names = |sql: &str| -> Vec<String> {
        conn.prepare(sql)
            .expect("a statement")
            .query_map([], |row| row.get(0))
            .expect("a query")
            .collect::<Result<_, _>>()
            .expect("names")
    };
    assert!(
        names(
            "SELECT name FROM sqlite_schema
             WHERE name IN ('tasks', 'port_leases', 'favorite_folders')"
        )
        .is_empty()
    );
    assert_eq!((count(&conn, "agent_runs"), count(&conn, "blobs")), (1, 1));
    assert_eq!(
        names("SELECT scope FROM handle_counters ORDER BY scope"),
        ["workspace"]
    );
    assert_eq!(
        names("SELECT name FROM projection_versions ORDER BY name"),
        ["changes_for:01M4F1W7EM2GN3EV6NHXZJEGGE", "tree"]
    );
}

#[test]
fn a_database_from_before_the_baseline_is_refused_and_left_alone() {
    for found in 1..BASELINE {
        let mut conn = memory();
        conn.execute_batch("CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL) STRICT")
            .expect("a table of an old build");
        conn.pragma_update(None, "user_version", found)
            .expect("the version is set");
        let before = schema_text(&conn);
        let error = run_memory(&mut conn).expect_err("the runner refuses");
        assert!(
            matches!(error, StoreError::TooOld { found: f, baseline: BASELINE, .. } if f == found),
            "{error}"
        );
        assert_eq!(schema_text(&conn), before, "the runner changed nothing");
    }
}

#[test]
fn a_database_of_a_newer_build_is_refused_and_left_alone() {
    let mut conn = Connection::open_in_memory().expect("a database");
    conn.pragma_update(None, "user_version", SCHEMA_VERSION + 1)
        .expect("the version is set");
    let error = run_memory(&mut conn).expect_err("the runner refuses");
    assert!(matches!(error, StoreError::TooNew { found, .. } if found == SCHEMA_VERSION + 1));
    let tables: i64 = conn
        .query_row("SELECT count(*) FROM sqlite_master", [], |r| r.get(0))
        .expect("a count");
    assert_eq!(tables, 0, "the runner changed nothing");
}

#[test]
fn the_store_does_not_change_a_file_that_it_refuses() {
    for (found, name) in [(BASELINE - 1, "old"), (SCHEMA_VERSION + 1, "newer")] {
        let dir = temp_dir(name);
        let path = dir.join("state.db");
        {
            // A file as `Store::open` leaves it: in WAL mode, and closed.
            let conn = Connection::open(&path).expect("a file");
            let mode: String = conn
                .pragma_update_and_check(None, "journal_mode", "WAL", |row| row.get(0))
                .expect("WAL");
            assert_eq!(mode, "wal");
            conn.execute_batch(
                "CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL) STRICT;
                 INSERT INTO meta (key, value) VALUES ('session_uuid', 'abc');",
            )
            .expect("a row");
            conn.pragma_update(None, "user_version", found)
                .expect("a version");
        }
        let before = std::fs::read(&path).expect("the file");
        let Err(error) = Store::open(&path) else {
            panic!("the store opens a file of version {found}");
        };
        let message = error.to_string();
        if found < BASELINE {
            assert!(matches!(error, StoreError::TooOld { .. }), "{message}");
            assert_eq!(
                message,
                format!(
                    "{} has schema version 14, from a development build of neohar. This build \
                     cannot upgrade a schema older than version 15. Move the file (and its -wal \
                     and -shm files, if there are any) to a different place, and start neohar \
                     again. neohar then makes a new, empty database.",
                    path.display()
                )
            );
        } else {
            assert!(matches!(error, StoreError::TooNew { .. }), "{message}");
        }
        assert_eq!(std::fs::read(&path).expect("the file"), before, "{name}");
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[test]
fn a_rebuild_keeps_the_rows_that_hang_on_the_table() {
    let baseline = "CREATE TABLE parent (id TEXT PRIMARY KEY) STRICT;
         CREATE TABLE child (parent TEXT NOT NULL REFERENCES parent (id) ON DELETE CASCADE) STRICT;";
    let rebuild = [Migration {
        version: BASELINE + 1,
        sql: "CREATE TABLE parent_new (id TEXT PRIMARY KEY, name TEXT) STRICT;
              INSERT INTO parent_new (id) SELECT id FROM parent;
              DROP TABLE parent;
              ALTER TABLE parent_new RENAME TO parent;",
        rebuilds_tables: true,
    }];
    let mut conn = memory();
    run_with(&mut conn, Path::new(MEMORY), baseline, &[]).expect("the baseline");
    conn.execute_batch(
        "INSERT INTO parent (id) VALUES ('p1');
         INSERT INTO child (parent) VALUES ('p1');",
    )
    .expect("rows");
    run_with(&mut conn, Path::new(MEMORY), baseline, &rebuild).expect("the rebuild");
    // The drop of `parent` did not cascade, and the keys are enforced again.
    assert_eq!(count(&conn, "child"), 1);
    assert_eq!(pragma(&conn, "foreign_keys"), 1);
    conn.execute("DELETE FROM parent WHERE id = 'p1'", [])
        .expect("a delete");
    assert_eq!(count(&conn, "child"), 0, "the delete cascades as before");
}

#[test]
fn a_rebuild_that_breaks_a_foreign_key_is_rolled_back() {
    let baseline = "CREATE TABLE parent (id TEXT PRIMARY KEY) STRICT;
         CREATE TABLE child (parent TEXT NOT NULL REFERENCES parent (id) ON DELETE CASCADE) STRICT;";
    let rebuild = [Migration {
        version: BASELINE + 1,
        sql: "CREATE TABLE parent_new (id TEXT PRIMARY KEY) STRICT;
              DROP TABLE parent;
              ALTER TABLE parent_new RENAME TO parent;",
        rebuilds_tables: true,
    }];
    let mut conn = memory();
    run_with(&mut conn, Path::new(MEMORY), baseline, &[]).expect("the baseline");
    conn.execute_batch(
        "INSERT INTO parent (id) VALUES ('p1');
         INSERT INTO child (parent) VALUES ('p1');",
    )
    .expect("rows");
    let error = run_with(&mut conn, Path::new(MEMORY), baseline, &rebuild)
        .expect_err("a broken key fails the migration");
    assert!(matches!(error, StoreError::BadValue(_)), "{error}");
    assert_eq!(version(&conn).expect("a version"), BASELINE);
    assert_eq!((count(&conn, "parent"), count(&conn, "child")), (1, 1));
    assert_eq!(pragma(&conn, "foreign_keys"), 1);
}

#[test]
fn running_twice_changes_nothing_the_second_time() {
    let mut conn = memory();
    run_memory(&mut conn).expect("the first run");
    let cookie = pragma(&conn, "schema_version");
    run_memory(&mut conn).expect("the second run");
    assert_eq!(version(&conn).expect("a version"), SCHEMA_VERSION);
    assert_eq!(pragma(&conn, "schema_version"), cookie);
}

// The schema. ----------------------------------------------------------------------------------

#[test]
fn full_text_search_is_in_the_bundled_sqlite() {
    let mut conn = memory();
    run_memory(&mut conn).expect("the migrations run");
    conn.execute(
        "INSERT INTO scrollback_fts (text, pane_id, line_no) VALUES ('migration failed: no such column', 'p1', 12)",
        [],
    )
    .expect("a line");
    conn.execute(
        "INSERT INTO scrollback_fts (text, pane_id, line_no) VALUES ('all tests passed', 'p1', 13)",
        [],
    )
    .expect("a line");
    let (pane, line): (String, i64) = conn
        .query_row(
            "SELECT pane_id, line_no FROM scrollback_fts WHERE scrollback_fts MATCH 'migration'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("a hit");
    assert_eq!((pane.as_str(), line), ("p1", 12));
}

#[test]
fn the_interactions_reject_a_value_outside_their_enums() {
    let mut conn = memory();
    run_memory(&mut conn).expect("the migrations run");
    let insert = |id: &str, kind: &str, risk: &str, state: &str, gate: &str| {
        conn.execute(
            "INSERT INTO interactions (id, handle, kind, title, risk, payload_json, state,
                                       answerable, gate_mode, source, confidence, opened_at)
             VALUES (?1, ?1, ?2, 't', ?3, '{}', ?4, 1, ?5, 'structured', 1.0, 0)",
            rusqlite::params![id, kind, risk, state, gate],
        )
    };
    insert("i1", "approval", "low", "open", "gate").expect("a good interaction");
    assert!(insert("i2", "chat", "low", "open", "gate").is_err(), "kind");
    assert!(
        insert("i3", "approval", "extreme", "open", "gate").is_err(),
        "risk"
    );
    assert!(
        insert("i4", "approval", "low", "mooted", "gate").is_err(),
        "state"
    );
    assert!(
        insert("i5", "approval", "low", "open", "half").is_err(),
        "gate_mode"
    );
}
