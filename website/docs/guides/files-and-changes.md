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
| A folder outside git | "This folder is not in a git repository." |

Each row has a status letter, the folder and name, and the lines added and removed. Click a row (or
press Enter) to open its diff under it. The tab counts again a short time after a file or a commit
changes, at most once in 2 seconds.

**Open in Git view** in the head of the tab shows the same worktree in the Git view.

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
else the root of the workspace. A strip opens it on the folder of the pane of its agent.

| Part | What it shows |
|---|---|
| Repository menu | The repositories of the workspaces and of the panes, and the ones that you added. Type an absolute path to add the repository of that folder |
| Worktree menu | The worktrees of the repository, each with its branch, its changed files and its agents, for example "Claude Code · working". **Open a pane in this worktree** makes a tab there |
| Branch menu | The current branch, the recent ones, the other local ones and the remote ones. A branch that another worktree has out shows that worktree |
| Agents | "2 agents in this worktree". Click it to show the pane of the first agent |
| Fetch | The time of the last fetch, for example "Last fetched 18 minutes ago" |
| Changes | What is not committed, from `HEAD`, with a filter. Click a file to show its diff |
| History | The commits of `HEAD`, the newest first, with their branches and tags. An arrow marks a commit that no remote branch has. Click a commit to show its message and its files, and a file to show its diff |

Each menu has a filter field. Type to filter, use the arrow keys to move, and press Enter to pick.
An agent is in a worktree when the folder of its pane is in it. A worktree inside another one, as
Claude Code makes them under `.claude/worktrees/`, has its own agents.

The view reads git only. It never writes the index, so a commit of an agent at the same time does
not fail. The `nh git` commands give the same rows for the folder of your shell: `nh git status`,
`nh git worktrees`, `nh git branches`, `nh git log`, `nh git show`, `nh git diff`, `nh git repos` and
`nh git add`.
