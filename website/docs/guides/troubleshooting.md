---
title: Troubleshooting
description: "nh doctor, the debug bundle, the logs, and the common problems: the core, hooks that do not arrive, Codex and the shared daemon, inherited variables, leaked holders."
sidebar_position: 7
---

## Start here

```bash
nh doctor                    # checks the install and the core; works also when neohar does not run
nh debug bundle              # a zip for a bug report; add --include-scrollback for the pane text
```

`nh doctor` prints one line for each check, with a hint under a warning or a failure. The exit code
is 1 when a check fails. The health banner of the window shows the same warnings. The debug bundle
removes the secrets that it can recognize, but it can miss one: read the files before you send
them.

| Log | Where |
|---|---|
| Core of the app | `~/Library/Application Support/dev.neohar.app/default/logs/core.<date>.log` (the last 7 days) |
| Core of `mise run app:dev` | The terminal that runs it |
| Holder of a pane | `logs/holder-<pane-ulid>.log` in the same folder |

`NEOHAR_LOG=nh_core=debug` makes the core log more.

## The core and the command line

| You see | Cause | What to do |
|---|---|---|
| `nh: command not found` | `~/.local/bin` is not on your `PATH`, or the app never ran | [Install the app](/docs/guides/install) |
| `nh: neohar is not running: nothing listens at …` | No core, or a socket file of a killed core | Start neohar. The next start removes the old file |
| The app quits at once: "another core serves …" | An `nh daemon` or another app serves the session | Stop the other core, or use another `NEOHAR_SESSION` |
| A debug build does not start | The three `NEOHAR_*_DIR` variables are not set | Use `mise run app:dev` ([Setup](/docs/getting-started/setup)) |
| A red banner: "Storage is degraded" | The disk is full or the database cannot be written. Nothing is saved | Free disk space. The banner goes at the first write that works |
| No notifications | macOS permission refused, or a run outside the app bundle (`app:dev`) | System Settings › Notifications › neohar |

## Agents

| You see | Cause | What to do |
|---|---|---|
| `~` on the badge, or "… is detected from the process list only" | No hook reaches neohar | `nh integration status`, then `nh integration install all`. Start a new session of the harness |
| The same for Codex after the install | Codex did not trust the new hooks | Start Codex and choose "Trust all and continue", or run `/hooks` |
| "Codex in … started without `--no-daemon`…" | Codex attached to its shared app-server, so its hooks go there | Quit it and start `codex` again in a neohar pane, where the shim adds the flag |
| The shim does not run (`nh doctor` names an alias) | A shell alias or function `codex` hides it, or a profile puts another folder first | Remove the alias. zsh keeps the shims first by itself. In bash or fish, put `$NEOHAR_SHIMS` first at the end of your startup file |
| A Codex pane shows as Claude Code, or panes have `CLAUDECODE=1` | The app was opened from a Claude Code session (`open` passes its variables). Fixed for new panes | Open a new pane, or `unset CLAUDECODE` in the old one |
| `?` after the harness in a pane strip | neohar has not validated this version of the harness, so it only observes | Use a version that neohar knows, or update neohar |
| "… behaves differently than neohar expects" | The drift alarm of a harness version | `nh debug bundle`, and report the version |
| `nh agent prompt` gives `not_bound` or `busy` | The run is only suggested, or you typed in the pane at that moment, or a dialog is open | Install the hooks or `nh agent link`; try again |
| "Couldn't confirm your answer reached …" | The app or the hook ended at the moment of the answer | Look at the pane, and answer there if the dialog is still open |

## Panes that keep running

Each pane has a holder process (`nh hold`) that outlives the app on purpose: a quit, a crash or an
update leaves the programs running ([Holders](/docs/architecture/holders)).

- **Stop them with the app:** **Quit and stop everything…** (`⌘⌥Q`).
- **The dev app:** `mise run app:stop` after the app ended kills the panes that it knows.
  `NEOHAR_STOP_ON_EXIT=1` makes every exit of the app stop them.
- **New panes fail to start:** macOS allows 511 pseudo-terminals in total, and leaked holders count.

```bash
pgrep -fl 'hold --pane' | wc -l   # holders that run now
sysctl kern.tty.ptmx_max          # the limit
```

A holder that you kill by hand takes the program of its pane with it.

:::warning[Known issue]

A pane that you move out of a tab, so that the tab closes, can be missing after the next launch.
Its program keeps running in a holder that no core owns, and a close does not stop it. Find it with
`pgrep -fl 'hold --pane'` and stop it by hand. A fix is not merged yet.

:::

## Hooks

- The hooks call `~/.local/bin/nh`. If `nh doctor` says that the symlink is missing, is a file, or
  points to a missing app, start the app that you want once: it makes the link again. It never
  replaces a file of yours at that path.
- A harness reads its hooks when it starts. Start it again after an install.
- Outside a neohar pane, a hook exits at once and does nothing. That is correct.
