-- The repositories that the person added to the Repository menu of the Git view
-- (`git.repo_add`). The core finds the other repositories from the workspaces and the panes, and
-- keeps no row for them.

CREATE TABLE git_repos (
    path     TEXT PRIMARY KEY,
    added_at INTEGER NOT NULL
) STRICT;
