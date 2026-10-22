---
title: Tasks, files and changes
description: "Task workspaces with their own git worktree, branch, ports and tab; the Files tab that browses the folder of the focused pane; the Changes tab that shows its git changes."
sidebar_position: 4
---

A **task** is a unit of work with its own git worktree, branch, ports and tab, and usually an agent
that works in it. Several agents can work on one repository at the same time without touching each
other's files or ports. The **Files** and **Changes** tabs of the drawer show the folder that you
work in and what changed in it.

## Write and start a task

1. Open the **Tasks** tab of the drawer and click **New task**, or run the palette command "Create
   a task".
2. In the dialog, set the **Repository** (type a path, click **Choose…**, or click a favorite
   folder; the star adds the folder to the favorites), the **Title**, the **Agent** (None, Claude
   Code or Codex), its model and effort level, and the **Prompt**. **Add files…** or a paste
   attaches screenshots and other files.
3. **Create task** keeps a draft. Nothing runs yet. You can open the draft again to change it.
4. **Start** (in the dialog or on the row of the draft) does the work:

| Step | Result |
|---|---|
| Worktree | `~/.neohar/worktrees/<repo>/<slug>`, with the branch `neohar/<slug>` from the base (the current branch, or the one you give) |
| Ports | One port (`tasks.ports`) from 41000–41999, in `PORT` and `NEOHAR_PORTS` |
| Setup | The commands of `tasks.setup`, if any. The first failure stops the start |
| Files | Copied to `.neohar/attachments/` in the worktree, which git ignores |
| Tab | A tab in the workspace of the repository, with your shell in the worktree |
| Agent | Started in that tab with the prompt and the paths of the files |

The Tasks tab lists the drafts, the started tasks and the closed tasks. The dialog of a started
task has **Show tab**, **Close task** and **Close and remove worktree**. A worktree with changes
asks first (**Remove anyway**). The branch always stays, because it holds your commits.

:::note

Only the first pane of a task tab gets the worktree and the task variables. A pane that you split
from it starts in the root of the workspace: `cd` into the worktree there.

:::

From the command line:

```bash
cd ~/code/app
nh task create "Fix the login bug" --harness claude --prompt "Fix the login bug" --start
nh task list
nh task changes k1                      # the changed files, counted now
nh task diff k1 src/auth.rs
nh task close k1 --remove-worktree      # refused when the worktree has changes; add --force
nh favorite add ~/code/app
```

Without `--start`, `nh task create` keeps a draft; `nh task start k1` starts it later.

| Setting | Default | Meaning |
|---|---|---|
| `tasks.worktree_root` | `"~/.neohar/worktrees"` | Where the worktrees go |
| `tasks.branch_prefix` | `"neohar/"` | The branch is the prefix and the slug |
| `tasks.ports`, `tasks.port_range` | `1`, `[41000, 41999]` | How many ports a task leases, and from where |
| `tasks.setup` | `[]` | Commands that run in a new worktree, for example `["bun install"]` |
| `[tasks.env]` | empty | Variables for every pane of a task |

## The Files tab

The Files tab shows a tree of one folder. The folder is
the working directory of the pane that has the focus, so a `cd` in the pane moves the tree within a
second. Without such a pane it is the worktree of the task
of the tab, else the root of the workspace.

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
| The worktree of a started task | Everything since the merge base with the base: the commits of the branch, the changes that are not committed and the new files. The head shows the commits, files, lines and merge base |
| Any other git worktree, such as the main checkout | What is not committed, from `HEAD`, with the new files |
| A folder outside git | "This folder is not in a git repository." |

Each row has a status letter, the folder and name, and the lines added and removed. Click a row (or
press Enter) to open its diff under it. The tab counts again a short time after a file or a commit
changes, at most once in 2 seconds.
