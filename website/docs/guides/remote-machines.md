---
title: Remote machines
description: "Run agents on another Mac: the nh daemon there, the [machines] table here, windows of a machine, nh --machine, and what is not built yet."
sidebar_position: 5
---

neohar can show windows of a core that runs on another Mac. On that Mac, `nh daemon` runs the core
without windows. Its panes, agents, hooks and state stay there. Your app connects to it
over the system `ssh` and shows its windows next to your local ones. A lost link, a laptop that
sleeps or a quit of the app stops no agent on the remote Mac.

:::info[Status]

Built: the daemon, the link, `[machines]`, windows of a machine, reconnect, `nh --machine`, the
client settings, port forwards and several clients. Not built
yet: the install and update of `nh` on the remote Mac by the app, the doctor checks of the remote
Mac and the "Stop everything on …" menu item. Not tested yet: link chaos, a killed daemon, sleep and
a network change.

:::

## Set up the remote Mac

1. Turn on Remote Login (ssh) on it, and check that `ssh studio` works from your Mac without a
   question that you cannot answer.
2. Install Claude Code and Codex on it and log in to them there, by hand. neohar never installs a
   harness, logs in or copies credentials. It finds the harnesses and shows their login state.
3. Put `nh` at `~/.local/bin/nh` on it. The app does not do this yet. If neohar.app runs on that
   Mac, the symlink is there already: skip this step. Else copy the `nh` of your app (both Macs must
   have the same CPU type):

   ```bash
   ssh studio 'mkdir -p ~/.local/bin && cat > ~/.local/bin/nh.tmp && chmod 0755 ~/.local/bin/nh.tmp && mv ~/.local/bin/nh.tmp ~/.local/bin/nh' < ~/Applications/neohar.app/Contents/MacOS/nh
   ```

You do not start the daemon. The first connection starts it with `nh bridge`, and it runs until you
stop it. While an agent works, it keeps the Mac from idle sleep (`daemon.prevent_sleep`). A MacBook
with its lid closed on battery sleeps in any case.

## Add the machine

Add a table to `~/.config/neohar/config.toml` on your Mac (`nh config edit`):

```toml
[machines.studio]
ssh = "me@studio.local"   # any target that ssh accepts, ~/.ssh/config applies
connect = "launch"        # connect at launch; "manual" (the default) waits for you
```

The label (`studio`) has 1 to 32 characters of `a-z`, `0-9` and `-`. `local` is this Mac. When ssh
asks for a passphrase, a password or a new host key, the app shows the question as a sheet and
stores nothing. The ssh server can write the text of a question, and the sheet says so. neohar
never needs the login password of your Mac: do not type it into this sheet. The app shows at most 5
questions of one machine in 2 minutes, and cancels the others.

## Open a window of the machine

Use the **Machines** menu: each machine has **New window**, **Connect** and **Disconnect**. Or:

```bash
nh machine list                  # this Mac and each machine, with the state of its link
nh machine connect studio
nh machine open-window studio
```

A window shows one machine. The status bar names it, and a banner shows when its link is down. The
link comes back by itself, and the screens are exact after it. The machine keeps the rows of your
windows, so they open again each time the app connects to it.

Then install the hooks on the remote Mac, and trust them in Codex there:

```bash
nh integration install all --machine studio
```

## What works in a remote window

Everything of a local window works: panes, splits, scrollback, search, agents, cards, the Inbox,
the Files and Changes tabs. Notifications, the badge and the tray show on your Mac, with the
label in the title. Also:

- A click on a link to `localhost` in a remote window opens it through an ssh forward. Only a
  click opens a link.
- The font, theme and keys come from your Mac. The other settings belong to the machine. Set
  `ui.editor` in the settings of your Mac: a remote window cannot set it.
- **Open** in the Files tab does not work for a remote file. **Copy path** and **Insert into
  pane** do.

## What a remote machine can do on your Mac

A remote machine runs its own core. If an attacker controls the remote Mac, the attacker also
controls that core. Thus the app on your Mac does not trust it, and it applies these limits:

- The core opens no address, file or program on your Mac, and it reads no file of your Mac.
- The core writes the clipboard of your Mac only within 2 s after you copy in a window of that
  machine. The app drops any other write and shows a note. Thus an OSC 52 write that a pane of
  the machine remembers as allowed is dropped too. Answer the request of each write in the pane.
- The core moves the focus to one of its windows only within 2 s after you show something of that
  machine: a click on a notification, the tray, the hotkey or a command of the palette. It can do
  this at most 4 times in 10 s.
- The core shows at most 10 notifications in 10 s, and its section of the tray has at most 50
  lines. The app opens at most 16 windows of one machine. A new window of a machine takes the
  focus only when you asked for a window of that machine on this Mac within 5 s, for example
  with **New window** or the palette.
- A core that stops reading the link and keeps sending ends the link. The app keeps a limited
  number of log lines about the refused requests of a core.

The link has one known limit. Each key that you type in a remote window goes to the machine at
once, in its own ssh packet. A person who watches the network cannot read the keys, but can see
when you type and how fast. ssh hides this timing only for a terminal session
(`ObscureKeystrokeTiming`), and the link is not a terminal session (`ssh -T`). On a network that
you do not trust, do not type a secret into a remote pane.
- The app cleans the HTML of the plans and files of the machine again, and a link opens only when
  you click it.

## The command line

`--machine` runs a command on the machine over ssh, and a handle with a prefix does the same:

```bash
nh agent list --machine studio
nh pane read studio/w3:p5
nh interaction answer studio/i42 allow
```

To stop the daemon and every program on the remote Mac:

```bash
nh session stop-everything --machine studio
ssh studio '~/.local/bin/nh daemon stop'
```

:::note

Only an app that runs its own core has links. An app that is a client of a local `nh daemon`
(`mise run app:dev:client`) shows `local` only. Claude Code can report "logged out" in a process
that ssh started, because the keychain refuses it there; this is still open.

:::
