---
title: Files, changes and Git
description: "The Files tab that browses the folder of the focused pane, the Changes tab that shows its git changes, and the Git view of its repository."
sidebar_position: 4
---

The **Files** and **Changes** tabs of the drawer show the folder that you work in and what changed
in it. The **Git view** shows its repository: the worktrees, the branches, the changes and the
history.

## The Files tab

The Files tab shows a tree of one folder. The folder is
the working directory of the pane that has the focus, so a `cd` in the pane moves the tree within a
second. Without such a pane it is the root of the workspace.

- Each row has the icon of its file type, in the colours of the Material Icon Theme. The core
  picks the icon from the name of the file or folder.
- Click a folder to open it, and a file to preview it under the tree: text with highlighting,
  Markdown as formatted text (a switch shows the lines), and images.
- **Open** in the head of the preview, or in the menu of a file, shows the file in a large view over
  the window. Escape closes it.
- Use the arrow keys, Enter, Home and End in the tree.
- The menu of a row (right click), the head of the preview and the large view have **Reveal in
  Finder**, **Copy path**, **Copy relative path** and **Insert into pane**, which pastes the quoted
  path into the focused pane. The menu and the large view also have **Open in app**: the app of
  `ui.editor`, else the default app of the file.
- Drag a row onto a pane to insert its path there.
- The tree does not watch the disk. It reads the folders again when the tab shows, when the folder
  changes, and on **Refresh**.

## The Changes tab

The Changes tab shows the git changes of the worktree of the folder of the Files tab.
It is read-only.

| Folder | The changes |
|---|---|
| A git worktree, such as the main checkout | What is not committed, from `HEAD`, with the new files |
| A git repository that you do not trust | The path of the repository and a **Trust** button. See [Trust a repository](#trust-a-repository) |
| A folder outside git | "This folder is not in a git repository." |

Each row has a status letter, the folder and name, and the lines added and removed. Click a row (or
press Enter) to open its diff under it. The tab counts again a short time after a file or a commit
changes, at most once in 2 seconds.

**Open in Git view** in the head of the tab shows the same worktree in the Git view.

## Trust a repository

The settings of a git repository can name programs that git runs, for example a filter or
`core.fsmonitor`. A repository from an archive, a shared folder or a clone can bring such
settings. Thus neohar runs no git command in a repository until you trust it.

Until then, the Changes tab and the Git view show the path of the repository and a **Trust**
button. They do not show its branch, its changes or its history. neohar finds the repository
without git: it looks for `.git` in the folder and in the folders above it.

- Click **Trust** only when you know where the repository comes from. Then neohar reads the
  repository and shows its changes.
- A trust is for the whole repository, with all its worktrees. neohar keeps it after a restart.
- neohar keeps the settings of the repository that name a program at the time of the trust: the
  local settings, and the files that they include. Before each look, neohar reads them again.
  When one of them changes, for example because an agent wrote `.git/config`, the tab and the
  view ask again and name the changed settings. Your global settings, for example in
  `~/.gitconfig`, do not count.
- `nh git trust` trusts the repository of the current directory, and `nh git untrust` takes the
  trust back. An `nh git` command in a repository that you do not trust fails with
  `permission_denied`.
- A paired phone can also trust a repository, because it can type commands into a pane.

## The Git view

The Git view takes the middle of the window, where the tab bar and the panes are. The command line,
the board and the status bar stay. The left column and the drawer fold to their rails, and they
open again when the view closes. The panes and their agents continue to run, and the core sends no
frames while the view shows.

To open it, use one of these:

- Press `⌘⇧G`, or click **Git** in the status bar. Do the same, or press Escape, to close it.
- Type "Git" in the palette.
- Right-click a strip of the board, and click **Show in Git view**.

The view opens on the worktree of the folder of the window: the folder of the pane with the focus,
else the root of the workspace. A strip opens it on the folder of the pane of its agent. For a
repository that you do not trust, the view shows the prompt of
[Trust a repository](#trust-a-repository) in place of the changes and the history.

| Part | What it shows |
|---|---|
| Repository menu | The repositories of the workspaces and of the panes, and the ones that you added. Type an absolute path to add the repository of that folder |
| Worktree menu | The worktrees of the repository, each with its branch, its changed files and its agents, for example "Claude Code · working". **Open a pane in this worktree** makes a tab there |
| Branch menu | The current branch, the recent ones, the other local ones and the remote ones. Pick a branch to switch the worktree to it. A remote branch with no local branch gets a local branch that tracks it |
| Agents | "2 agents in this worktree". Click it to show the pane of the first agent |
| Fetch | **Fetch origin**, or **Pull origin 3↓** when the branch is behind, with the time of the last fetch under it. A pull is a fast-forward only |
| Changes | What is not committed, from `HEAD`, with a filter. Click a file to show its diff |
| History | The commits of `HEAD`, the newest first, with their branches and tags. An arrow marks a commit that no remote branch has. Click a commit to show its message and its files, and a file to show its diff |

Each menu has a filter field. Type to filter, use the arrow keys to move, and press Enter to pick.
An agent is in a worktree when the folder of its pane is in it. A worktree inside another one, as
Claude Code makes them under `.claude/worktrees/`, has its own agents.

The reads never write the index, so a commit of an agent at the same time does not fail.

The settings of a repository cannot change these rules of neohar for each git command of the
Changes tab, the Git view and `nh git`, also in a repository that you trust:

- Git runs no `core.fsmonitor` program, and it uses no bare repository that it finds in a folder.
- A remote cannot use the `ext::` transport.
- The reads and the fetch in the background run no hooks. A switch, a pull, a restore and a fetch
  from the button run the hooks of the repository.
- A read stops after 60 seconds.
- A submodule is a change only when its commit changed.
- neohar uses the `git` of an absolute folder of your `PATH`. It never uses a `git` of the
  repository, such as `bin/git`.

### Switch, fetch and pull

Each write is one git command, and the timeline of events records each write that works. One write
at a time runs in a worktree.

| Case | What the view does |
|---|---|
| An agent starts, works or waits for you in the worktree | Before a switch or a pull, a dialog names the agent. Click **Switch anyway** or **Cancel** |
| The agents of the worktree are idle or exited | The write runs |
| The worktree has changes | A switch asks: **Bring my changes** to the other branch, or **Leave them** in a stash of the current branch |
| Another worktree has the branch out | **Show that worktree** shows it instead |
| The branch and its upstream diverged | The view offers Fetch only. Pull with a merge or a rebase in a pane |

When you switch back to a branch with a stash of its changes, a line offers **Restore them**.

A fetch from the button asks for a passphrase or a token in a sheet of the app, as an ssh link
does. The view also fetches in the background every 10 minutes, but only for the repositories that
a window shows and that you trust. A fetch in the background never asks: when it needs a secret, it stops, and the
view says "The fetch needs your passphrase or a token. Click Fetch." To change the interval, set
`git.fetch_minutes`. The value 0 turns the fetch in the background off.

The `nh git` commands give the same rows and writes for the folder of your shell: `nh git status`,
`nh git worktrees`, `nh git branches`, `nh git log`, `nh git show`, `nh git diff`, `nh git repos`,
`nh git add`, `nh git trust`, `nh git untrust`, `nh git switch`, `nh git unstash`, `nh git fetch`
and `nh git pull`. A write that asks in the view answers with `confirm_required` in the CLI, and
`--force` writes anyway.
