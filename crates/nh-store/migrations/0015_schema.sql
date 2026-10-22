-- The schema of the first release: schema version 15, the baseline.
--
-- The development builds made this schema in 15 steps. This file makes the same schema in one
-- step: a database of version 15 from a development build and a new database have the same tables,
-- columns, indexes and constraints. The runner (`src/migrate.rs`) runs this file only on an empty
-- database. Do not change this file after a release. A change to the schema is a new migration with
-- the next version.
--
-- The state tables are the source of truth. Every mutation also appends its events to `events` in
-- the same transaction (the transactional outbox). Events serve sync and audit. They are never
-- used to rebuild state.
--
-- The order of the columns is the order in the development builds, where a later step added its
-- columns at the end of a table. Times are unix milliseconds.


-- The session ------------------------------------------------------------------------------------

-- Values of the session by key, for example the identity of the event log.
CREATE TABLE meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
) STRICT;

-- Short handles (`w3`, `w3:t2`, `w3:p5`, `i42`, `k7`) never repeat in a session. `scope` is the
-- kind of handle, for example `workspace`, `task` or `pane:<workspace id>`. `next` is the number
-- that the next handle gets.
CREATE TABLE handle_counters (
    scope TEXT PRIMARY KEY,
    next  INTEGER NOT NULL CHECK (next >= 1)
) STRICT;

-- The version of each projection, so that a client can ask for the patches since a version.
CREATE TABLE projection_versions (
    name    TEXT PRIMARY KEY,
    version INTEGER NOT NULL DEFAULT 0
) STRICT;

-- The event log. `seq` only grows. A client keeps a cursor.
CREATE TABLE events (
    seq          INTEGER PRIMARY KEY AUTOINCREMENT,
    at           INTEGER NOT NULL,
    kind         TEXT NOT NULL,
    v            INTEGER NOT NULL DEFAULT 1,
    subject      TEXT,
    payload_json TEXT NOT NULL
) STRICT;
CREATE INDEX events_by_at ON events (at);
CREATE INDEX events_by_subject ON events (subject, seq) WHERE subject IS NOT NULL;


-- The layout and the windows ---------------------------------------------------------------------

CREATE TABLE groups (
    id         TEXT PRIMARY KEY,
    name       TEXT NOT NULL,
    parent_id  TEXT REFERENCES groups (id) ON DELETE SET NULL,
    sort_key   TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
) STRICT;

CREATE TABLE workspaces (
    id           TEXT PRIMARY KEY,
    handle       TEXT NOT NULL UNIQUE,
    name         TEXT NOT NULL,
    root_path    TEXT,
    machine      TEXT NOT NULL DEFAULT 'local',
    group_id     TEXT REFERENCES groups (id) ON DELETE SET NULL,
    sort_key     TEXT NOT NULL,
    closed_at    INTEGER,
    created_at   INTEGER NOT NULL,
    updated_at   INTEGER NOT NULL,
    -- The tab that the workspace shows when no window gives a different one.
    selected_tab TEXT REFERENCES tabs (id) ON DELETE SET NULL
) STRICT;

CREATE TABLE tabs (
    id            TEXT PRIMARY KEY,
    workspace_id  TEXT NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    handle        TEXT NOT NULL UNIQUE,
    name          TEXT NOT NULL,
    sort_key      TEXT NOT NULL,
    -- `LayoutNode` of `nh-proto::api::model`, as JSON.
    layout_json   TEXT NOT NULL,
    -- The floating panes with their rectangles, as JSON.
    floating_json TEXT NOT NULL DEFAULT '[]',
    created_at    INTEGER NOT NULL,
    updated_at    INTEGER NOT NULL,
    -- The pane that fills the tab.
    zoomed_pane   TEXT
) STRICT;

-- One row for each pane. A pane is in the layout before its holder starts. The layout sets the
-- placement (`workspace_id`, `tab_id`, `handle`, `render_id` and the size). The pane runtime sets
-- the columns of the holder (`socket` to `exit_value`) when the holder runs. `secret` is the secret
-- of the holder: the next core needs it to say `Hello`. `fencing_token` is the token of the core
-- that attached last.
CREATE TABLE panes (
    id                  TEXT PRIMARY KEY,
    render_id           INTEGER NOT NULL,
    workspace_id        TEXT REFERENCES workspaces (id) ON DELETE SET NULL,
    tab_id              TEXT REFERENCES tabs (id) ON DELETE SET NULL,
    handle              TEXT,
    pane_token          TEXT,
    socket              TEXT,
    holder_pid          INTEGER,
    secret              BLOB,
    fencing_token       INTEGER,
    argv_json           TEXT NOT NULL DEFAULT '[]',
    -- What the layout asked for (`ProcessSpec` as JSON): the program, the cwd and the extra
    -- environment. `NULL` means the login shell. The pane runtime starts the holder from it.
    spec_json           TEXT,
    cwd                 TEXT,
    cols                INTEGER NOT NULL,
    rows                INTEGER NOT NULL,
    -- `created`: the layout made the pane, and no holder runs yet.
    liveness            TEXT NOT NULL CHECK (liveness IN ('created', 'running', 'exited', 'lost')),
    exit_kind           TEXT CHECK (exit_kind IN ('code', 'signal', 'exec_failed')),
    exit_value          INTEGER,
    -- How the screen came back after the last restart: `snapshot`, `journal`, `ring_only` or
    -- `fresh`.
    recovery_method     TEXT,
    -- A clear of the scrollback hides the lines before this one.
    cleared_before_line INTEGER,
    last_archived_line  INTEGER,
    -- `ask`, `allow` or `deny` for OSC 52 clipboard writes.
    clipboard_policy    TEXT,
    created_at          INTEGER NOT NULL,
    updated_at          INTEGER NOT NULL
) STRICT;
CREATE UNIQUE INDEX panes_handle ON panes (handle) WHERE handle IS NOT NULL;
CREATE UNIQUE INDEX panes_token ON panes (pane_token) WHERE pane_token IS NOT NULL;
CREATE INDEX panes_tab ON panes (tab_id);

CREATE TABLE windows (
    id                    TEXT PRIMARY KEY,
    bounds_json           TEXT,
    -- The tab that the window shows in each workspace, as JSON.
    tab_by_workspace_json TEXT NOT NULL DEFAULT '{}',
    visible_panes_json    TEXT NOT NULL DEFAULT '[]',
    focused_pane          TEXT REFERENCES panes (id) ON DELETE SET NULL,
    open                  INTEGER NOT NULL DEFAULT 1,
    sort_key              TEXT NOT NULL,
    created_at            INTEGER NOT NULL,
    updated_at            INTEGER NOT NULL,
    -- The pane of a window that shows one popped-out pane. The window goes away with the pane.
    popout_pane           TEXT REFERENCES panes (id) ON DELETE CASCADE,
    -- The device id of the client that owns the window: the app that hosts the core, or a client
    -- over a link. The first client that attaches gets a window that has no client.
    client                TEXT
) STRICT;

-- The time when each client was last seen. The core removes the windows of a client that did not
-- connect for 30 days.
CREATE TABLE clients (
    device    TEXT PRIMARY KEY,
    name      TEXT NOT NULL,
    last_seen INTEGER NOT NULL
) STRICT;


-- The contents of the panes ----------------------------------------------------------------------

-- The engine snapshots. `offset` is the offset in the journal of the holder where the snapshot was
-- taken. The store keeps the newest two for each pane.
CREATE TABLE snapshots (
    pane       TEXT NOT NULL REFERENCES panes (id) ON DELETE CASCADE,
    offset     INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    data       BLOB NOT NULL,
    PRIMARY KEY (pane, offset)
) STRICT;

-- The scrollback archive. A segment is a zstd file of lines.
CREATE TABLE scrollback_segments (
    pane_id    TEXT NOT NULL REFERENCES panes (id) ON DELETE CASCADE,
    segment_no INTEGER NOT NULL,
    first_line INTEGER NOT NULL,
    last_line  INTEGER NOT NULL,
    path       TEXT NOT NULL,
    bytes      INTEGER NOT NULL,
    PRIMARY KEY (pane_id, segment_no)
) STRICT;

-- The full-text index of the archived lines.
CREATE VIRTUAL TABLE scrollback_fts USING fts5 (text, pane_id UNINDEXED, line_no UNINDEXED);


-- The agents -------------------------------------------------------------------------------------

CREATE TABLE agent_runs (
    id                        TEXT PRIMARY KEY,
    pane_id                   TEXT REFERENCES panes (id) ON DELETE SET NULL,
    harness                   TEXT NOT NULL CHECK (harness IN ('claude', 'codex')),
    name                      TEXT UNIQUE,
    harness_session_id        TEXT,
    previous_session_ids_json TEXT NOT NULL DEFAULT '[]',
    transcript_path           TEXT,
    -- The core read the transcript file up to this position.
    transcript_offset         INTEGER NOT NULL DEFAULT 0,
    -- `bound`, `suggested` or `unlinked`.
    binding                   TEXT NOT NULL,
    binding_confidence        REAL NOT NULL DEFAULT 1,
    launch_mode               TEXT NOT NULL CHECK (launch_mode IN ('tui', 'headless')),
    version                   TEXT,
    capabilities_json         TEXT NOT NULL DEFAULT '{}',
    facets_json               TEXT NOT NULL DEFAULT '{}',
    yolo                      INTEGER NOT NULL DEFAULT 0,
    resume_json               TEXT,
    model                     TEXT,
    task_id                   TEXT,
    started_at                INTEGER NOT NULL,
    ended_at                  INTEGER,
    -- The core follows the process of a run: `pid` and `pid_start` identify it, and `manifest` is
    -- the id of the manifest that matched (the id of a wrapper when the user has one). After a
    -- restart, the core finds the same process in the pane and continues the run.
    manifest                  TEXT,
    pid                       INTEGER,
    pid_start                 INTEGER,
    -- The data that the count of tokens keeps across a restart: the usage of the latest messages
    -- and the cost that it counted for each model. With the usage, the count includes a message on
    -- several lines only one time.
    transcript_ledger_json    TEXT,
    -- The place of the run on the board, counted from 0. A run with no place comes after the runs
    -- that have one, the oldest first.
    board_rank                INTEGER
) STRICT;
CREATE INDEX agent_runs_pane ON agent_runs (pane_id);

-- The tokens and the cost of each run, for each day and model.
CREATE TABLE usage (
    run_id      TEXT NOT NULL REFERENCES agent_runs (id) ON DELETE CASCADE,
    day         TEXT NOT NULL,
    model       TEXT NOT NULL,
    input       INTEGER NOT NULL DEFAULT 0,
    output      INTEGER NOT NULL DEFAULT 0,
    cache_read  INTEGER NOT NULL DEFAULT 0,
    cache_write INTEGER NOT NULL DEFAULT 0,
    cost_usd    REAL,
    PRIMARY KEY (run_id, day, model)
) STRICT;

-- The counters of the disagreement telemetry, for each version of a harness. `raised` is 1 after
-- the core sent `agent.drift_detected` for the version: the notification goes one time.
CREATE TABLE drift (
    harness      TEXT NOT NULL,
    version      TEXT NOT NULL,
    observations INTEGER NOT NULL DEFAULT 0,
    bad_signals  INTEGER NOT NULL DEFAULT 0,
    raised       INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (harness, version)
) STRICT;


-- The interactions and the policy ----------------------------------------------------------------

-- Approvals, questions, plan reviews and notices of the agents.
CREATE TABLE interactions (
    id              TEXT PRIMARY KEY,
    handle          TEXT NOT NULL UNIQUE,
    run_id          TEXT REFERENCES agent_runs (id) ON DELETE SET NULL,
    pane_id         TEXT REFERENCES panes (id) ON DELETE SET NULL,
    kind            TEXT NOT NULL CHECK (kind IN ('approval', 'question', 'plan_review', 'notice')),
    native_ref      TEXT,
    title           TEXT NOT NULL,
    risk            TEXT NOT NULL CHECK (risk IN ('low', 'medium', 'high', 'unknown')),
    -- The rich payload that `nh-rich` made when the interaction opened.
    payload_json    TEXT NOT NULL,
    state           TEXT NOT NULL CHECK (state IN (
        'open', 'decision_recorded', 'delivering', 'delivered', 'delivery_unknown', 'failed',
        'superseded', 'resolved_elsewhere', 'dismissed', 'cancelled')),
    answerable      INTEGER NOT NULL,
    gate_mode       TEXT NOT NULL CHECK (gate_mode IN ('gate', 'observe')),
    source          TEXT NOT NULL,
    confidence      REAL NOT NULL,
    opened_at       INTEGER NOT NULL,
    deadline        INTEGER,
    resolved_at     INTEGER,
    resolution_json TEXT,
    fingerprint     TEXT
) STRICT;
CREATE INDEX interactions_run ON interactions (run_id);
CREATE INDEX interactions_open ON interactions (state) WHERE state IN ('open', 'decision_recorded', 'delivering');

-- The delivery record of an interaction: how the answer goes to the harness.
CREATE TABLE deliveries (
    interaction_id  TEXT PRIMARY KEY REFERENCES interactions (id) ON DELETE CASCADE,
    native_ref      TEXT,
    decision_rev    INTEGER NOT NULL DEFAULT 0,
    decision_json   TEXT,
    decided_by      TEXT,
    via             TEXT,
    idempotency_key TEXT,
    channel         TEXT CHECK (channel IN ('native', 'keystroke')),
    lease_json      TEXT,
    deadline        INTEGER,
    state           TEXT NOT NULL,
    attempts_json   TEXT NOT NULL DEFAULT '[]'
) STRICT;

CREATE TABLE notifications (
    id             TEXT PRIMARY KEY,
    text           TEXT NOT NULL,
    urgency        TEXT NOT NULL CHECK (urgency IN ('low', 'normal', 'high')),
    interaction_id TEXT REFERENCES interactions (id) ON DELETE SET NULL,
    run_id         TEXT REFERENCES agent_runs (id) ON DELETE SET NULL,
    seen           INTEGER NOT NULL DEFAULT 0,
    created_at     INTEGER NOT NULL
) STRICT;

-- The rules of the approval policy: from the configuration, or for this session only.
CREATE TABLE policy_rules (
    id         TEXT PRIMARY KEY,
    scope      TEXT NOT NULL CHECK (scope IN ('config', 'session')),
    rule_json  TEXT NOT NULL,
    source     TEXT NOT NULL,
    created_at INTEGER NOT NULL
) STRICT;

-- One row for each answered approval. The core makes a suggestion for a rule from these rows
-- only, so it does not need the interactions.
CREATE TABLE fingerprints (
    fingerprint    TEXT NOT NULL,
    decision       TEXT NOT NULL CHECK (decision IN ('allow', 'deny')),
    interaction_id TEXT,
    at             INTEGER NOT NULL,
    harness        TEXT,
    tool           TEXT,
    -- The command prefix, for example `pnpm test:*`, or the glob of the paths.
    pattern        TEXT,
    workspace      TEXT,
    -- 1 when the core can suggest a rule for it. A high risk, a line with many commands, a
    -- destructive program and a secret path are not eligible.
    eligible       INTEGER NOT NULL DEFAULT 0
) STRICT;
CREATE INDEX fingerprints_by_fingerprint ON fingerprints (fingerprint, at);


-- The tasks --------------------------------------------------------------------------------------

-- A task can be a draft: the person writes the task first and starts it later. A draft has its
-- repository, its title, the agent with its model, effort level and prompt, and its files. The
-- worktree, the branch, the ports and the tab come with the start. A draft has '' in `slug` and
-- `worktree_path`, and in `branch` and `base` if the person did not ask for them.
CREATE TABLE tasks (
    id               TEXT PRIMARY KEY,
    handle           TEXT NOT NULL UNIQUE,
    title            TEXT NOT NULL,
    slug             TEXT NOT NULL,
    workspace_id     TEXT REFERENCES workspaces (id) ON DELETE SET NULL,
    tab_id           TEXT REFERENCES tabs (id) ON DELETE SET NULL,
    repo_root        TEXT NOT NULL,
    worktree_path    TEXT NOT NULL,
    branch           TEXT NOT NULL,
    base             TEXT NOT NULL,
    status           TEXT NOT NULL CHECK (status IN (
        'draft', 'creating', 'setup', 'running', 'idle', 'setup_failed', 'closed')),
    env_json         TEXT NOT NULL DEFAULT '{}',
    -- The blob with the output of the setup commands.
    setup_blob       TEXT,
    harness          TEXT CHECK (harness IN ('claude', 'codex')),
    model            TEXT,
    effort           TEXT,
    prompt           TEXT,
    ports            INTEGER CHECK (ports BETWEEN 0 AND 255),
    -- A list of `{id, name, mime, bytes, blob, path?}`. Each blob has a reference in `blobs`.
    attachments_json TEXT NOT NULL DEFAULT '[]',
    created_at       INTEGER NOT NULL,
    updated_at       INTEGER NOT NULL
) STRICT;

CREATE TABLE port_leases (
    port      INTEGER PRIMARY KEY CHECK (port BETWEEN 1 AND 65535),
    task_id   TEXT NOT NULL REFERENCES tasks (id) ON DELETE CASCADE,
    leased_at INTEGER NOT NULL
) STRICT;

-- The favorite folders of the person, for the form of a new task. `path` is the root of a git
-- repository. `used_at` is the time of the last task that the person made or changed with this
-- repository. A new favorite gets the time when the person added it. This table is not STRICT,
-- because it is not STRICT in the databases of the development builds.
CREATE TABLE favorite_folders (
    path     TEXT PRIMARY KEY,
    added_at INTEGER NOT NULL,
    used_at  INTEGER NOT NULL
);


-- The files --------------------------------------------------------------------------------------

-- Content-addressed files under the state directory. `bytes` is the size of the file. `refs`
-- counts the rows that name the blob, for a later collection of the files that no row uses.
CREATE TABLE blobs (
    hash       TEXT PRIMARY KEY,
    bytes      INTEGER NOT NULL,
    mime       TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    refs       INTEGER NOT NULL DEFAULT 0
) STRICT;


-- The phones -------------------------------------------------------------------------------------

-- The paired phones. `token_hash` is the BLAKE3 hash of the token in the cookie of the phone, as
-- hex: the core never keeps the token. `seen_at` is the time of the last connection of the phone.
-- The push columns hold the push subscription. They are empty until the phone subscribes.
CREATE TABLE phone_devices (
    device        TEXT PRIMARY KEY,
    name          TEXT NOT NULL,
    token_hash    TEXT NOT NULL UNIQUE,
    paired_at     INTEGER NOT NULL,
    seen_at       INTEGER,
    push_endpoint TEXT,
    push_p256dh   TEXT,
    push_auth     TEXT
) STRICT;
