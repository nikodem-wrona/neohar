-- The open chats with Hermes Agent (`hermes.start`). `session` is the session id of Hermes after
-- `session/new`; the core resumes it when it starts. A closed chat has no row: the session stays
-- in the database of Hermes.

CREATE TABLE hermes_chats (
    id         TEXT PRIMARY KEY,
    cwd        TEXT NOT NULL,
    session    TEXT,
    title      TEXT,
    started_at INTEGER NOT NULL
) STRICT;
