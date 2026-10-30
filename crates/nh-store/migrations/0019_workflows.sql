-- A hidden tab holds the agent of a workflow node. The tab bar does not show it, and no window
-- selects it until the person opens it.

ALTER TABLE tabs ADD COLUMN hidden INTEGER NOT NULL DEFAULT 0;

-- The executions of workflows (`workflow.execute`). `graph` is the TOML text of the workflow at
-- the start, so an edit of the file does not change an execution that runs. `tree` is a hash of
-- the HEAD commit and of the changes of the worktree at the start. `dir` is the folder of the
-- reports. `state` is running, paused, done or cancelled.

CREATE TABLE workflow_executions (
    id           TEXT PRIMARY KEY,
    workflow     TEXT NOT NULL,
    name         TEXT NOT NULL,
    number       INTEGER NOT NULL,
    folder       TEXT NOT NULL,
    branch       TEXT,
    base         TEXT,
    head         TEXT,
    tree         TEXT,
    notes        TEXT,
    graph        TEXT NOT NULL,
    only_node    TEXT,
    workspace    TEXT,
    dir          TEXT NOT NULL,
    state        TEXT NOT NULL,
    tree_changed INTEGER NOT NULL DEFAULT 0,
    started_at   INTEGER NOT NULL,
    ended_at     INTEGER
) STRICT;

CREATE INDEX workflow_executions_by_start ON workflow_executions (started_at);

-- One node of an execution. `run` is the agent run of the node while it has one. `turns` counts
-- the ends of turns of that run that the core saw, so the core sends the reminder of a missing
-- report only once.

CREATE TABLE workflow_nodes (
    execution  TEXT NOT NULL REFERENCES workflow_executions (id) ON DELETE CASCADE,
    node       TEXT NOT NULL,
    state      TEXT NOT NULL,
    reason     TEXT,
    run        TEXT,
    verdict    TEXT,
    report     TEXT,
    turns      INTEGER NOT NULL DEFAULT 0,
    started_at INTEGER,
    ended_at   INTEGER,
    PRIMARY KEY (execution, node)
) STRICT;

CREATE INDEX workflow_nodes_by_run ON workflow_nodes (run);
