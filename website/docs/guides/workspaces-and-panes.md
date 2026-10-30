---
title: Workspaces and panes
description: "Workspaces, tabs and panes, the command line of the window, the default shortcuts, and how to search the scrollback."
sidebar_position: 2
---

A window shows one tab of one workspace. A workspace holds tabs, a tab holds a layout of panes, and
each pane runs one program in a terminal: a shell, Claude Code, Codex or any other program. The
window and the `nh` command line send the same commands to the core, so you can do each step in
either place.

## The window

| Part | What it shows |
|---|---|
| Top row | The command line (it opens the palette) and the attention line: how many agents wait, and the first request |
| Board | Under the top row: a strip for each agent, with its state, its work, its harness and its cost. A strip is a card: its glyph is green while the agent works, and its border is amber while it waits for you and red after an error. The agents that do not fit are behind **+N**, which is amber when one of them waits. Drag a strip onto another strip to change the order; the order stays after a restart. The board is always shown |
| Left column | The workspaces and their tabs. **New workspace** is at the bottom. The arrow at the end of **Workspaces** folds it to a rail, and under 1000 px it starts folded |
| Middle | The tab bar and the panes of the tab, the [Git view](/docs/guides/files-and-changes#the-git-view) or the [Workflows view](/docs/guides/workflows) |
| Right | The drawer: Inbox, Timeline, Files and Changes. Its arrow folds it to a rail, and under 1280 px it starts folded |
| Bottom row | Function keys (a click runs the command), the limits of the harnesses, warnings, the machine, and the **Workspaces**, **Drawer**, **Git** and **Workflows** switches |

Each part is a panel with round corners. Drag the gap beside a side column to change its width,
and the gap between two panes to change their shares. The pane in focus has a lighter title strip. A click
on a strip of the board shows the pane of its agent, in any tab or workspace. The strips keep
their places when the state of an agent changes.

## Workspaces, tabs and panes

Every object has a short handle that stays the same: workspace `w3`, tab `w3:t2`, pane `w3:p5`.
The window does not show the handles of tabs and panes. To show them in the tab bar and in the
strip of each pane, turn on **Show handles** in the settings (`⌘,`, section Ui), or set
`ui.show_handles = true`. `nh pane list` shows the handles of the panes.

| Task | In the window | Command line |
|---|---|---|
| New workspace | **New workspace**, or the palette | `nh workspace create app --root ~/code/app` |
| New tab | The plus button after the tabs | `nh tab create w3` |
| Rename | Double-click the name, or `F2` on a tab | `nh workspace rename w3 api`, `nh tab rename w3:t2 build` |
| Split a pane | `⌘D` (right), `⌘⇧D` (down) | `nh pane split w3:p5 right` |
| Focus a pane | Click it, or `⌘⌥` and an arrow | — |
| Resize | Drag a divider; double-click it to make the panes equal | `nh layout set-ratios`, `nh layout equalize` |
| Zoom or float a pane | The buttons in the strip of the pane | `nh layout zoom w3:t2 --pane w3:p5`, `nh pane float w3:p5` |
| Move a pane | Drag its strip onto a tab | `nh pane move w3:p5 w3:t1` |
| Close | The cross of the pane, tab or workspace | `nh pane close w3:p5 --force` |

A workspace with a root starts its panes in that folder. When a program runs in a pane that you
close, a dialog asks first, and the close stops every process that the pane started. When the program of a pane ends, the pane shows the exit code and a
**Restart** button (`nh pane restart w3:p5`).

## When the app quits

Each pane has its own holder process, so the programs do not belong to the app
([Holders](/docs/architecture/holders)).

| Action | What happens to the programs |
|---|---|
| `⌘Q`, **Quit neohar (agents keep running)** | They keep running. The next launch restores every screen |
| The app crashes | The same |
| `⌘⌥Q`, **Quit and stop everything…** | After a confirmation, every program stops |
| The Mac restarts | The holders are gone. Each pane starts its login shell again in its last folder. `agents.resume_on_launch` says what happens to the agents |

Closing the last window does not quit the app.

## The palette

Press `⌘K`, or click the command line. Type a part of a name (`spr` finds "Split pane right"),
then press Enter. The core ranks the commands. The palette also has "Go to" commands for every
workspace, tab and pane. A command that would stop a program asks first.

## Shortcuts

`⌘` is Control on Linux. You can change the window chords in the settings (`⌘,`, section Keys) or
with `nh config set keys.palette_open mod+j`.

| Shortcut | Command |
|---|---|
| `⌘K` | Open the palette |
| `⌘⇧]`, `⌘⇧[` | Next tab, previous tab |
| `⌘⌥←` `→` `↑` `↓` | Focus the pane in that direction |
| `⌘D`, `⌘⇧D` | Split the pane right, down |
| `⌘⇧I` | Show the Inbox |
| `⌘⇧A` | Answer next: show the most urgent card |
| `⌘⇧F` | Search the scrollback |
| `⌘⇧G` | Open or close the Git view |
| `⌘⇧E` | Open or close the Workflows view |
| `⌘⇧N`, `⌘N` | New window |
| `⌘⌥V` | Send the next key to the pane, also a reserved one |
| `⌘W` | Close the window (the panes stay) |
| `⌘,` | Settings |
| `⌃⌥⌘N` | From any app: show neohar and the card that waits longest (`ui.global_hotkey`) |
| `⌘C`, `⌘V` in a pane | Copy the selection, paste |

In a pane, drag to select. A double click selects a word and a triple click a line. Hold Shift to
select in a program that uses the mouse.

## Search and scrollback

A pane keeps about 900 lines above the screen in memory, which the wheel and the scrollbar scroll.
Every line that leaves the screen also goes to an archive on disk, which the search reads. The
archive survives quits, crashes and updates. It goes when the pane closes.

Press `⌘⇧F` and type words. Choose **All panes**, **This workspace** or **This pane**. Enter on a
hit shows its pane at that line.

- Each word of two letters or more also finds longer words that start with it: `migr fail` finds
  "migration failed". Case does not matter.
- The search finds lines that left the screen. A line that is still on the screen is not in the
  archive yet.
- A hit older than the history in memory opens the pane at its oldest line in memory. Read such a
  line with `nh pane read`.

```bash
nh search query "migration failed" --workspace w3
nh pane read w3:p5 --lines '{"from": 0, "to": 200}'   # absolute lines from the archive
```
