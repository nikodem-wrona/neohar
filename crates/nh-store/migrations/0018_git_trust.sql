-- The git repositories that the person trusts (`git.trust`). The core runs git in a repository
-- only when the repository has a row here. The key is the canonical path of the common git
-- directory, so that the worktrees of a repository share the row. `settings` is a JSON array of
-- the settings of the repository that name a program, as `[key, value]` pairs at the time of the
-- trust (a value is null for a key with no value). When they change, the core asks again.

CREATE TABLE git_trust (
    common_dir TEXT PRIMARY KEY,
    settings   TEXT NOT NULL,
    trusted_at INTEGER NOT NULL
) STRICT;
