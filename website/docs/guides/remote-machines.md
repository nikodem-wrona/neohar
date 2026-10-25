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
client settings, port forwards, dropped files and several clients. Not built
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
stores nothing.

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

- Files that you drop on a remote pane are copied to the remote Mac first.
- A click on a link to `localhost` in a remote window opens it through an ssh forward. Only a
  click opens a link.
- The font, theme and keys come from your Mac. The other settings belong to the machine.
- **Open** in the Files tab does not work for a remote file. **Copy path** and **Insert into
  pane** do.

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
