---
title: Files and changes
description: "The Files tab that browses the folder of the focused pane, and the Changes tab that shows its git changes."
sidebar_position: 4
---

The **Files** and **Changes** tabs of the drawer show the folder that you work in and what changed
in it.

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
