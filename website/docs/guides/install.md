---
title: Install the app
description: "Build neohar.app from source, install it in ~/Applications, connect Claude Code and Codex, check the install, and update or remove it."
sidebar_position: 1
---

There is no download of neohar yet. You build the app from the repository on your Mac. The build
has an ad-hoc signature: it runs on the Mac that built it and is not notarized, because there is no
Apple Developer ID yet. The app is for macOS only for now.

To run a development build from the working tree, see [Setup](/docs/getting-started/setup).

## Build and install

You need the Xcode Command Line Tools (`xcode-select --install`), git and
[mise](https://mise.jdx.dev). Run each command from the root of the repository.

```bash
git clone https://github.com/nikodem-wrona/neohar.git && cd neohar
mise install          # the pinned Rust, Bun and cargo tools
bun install           # the packages of the webview
mise run build:app    # target/release/bundle/macos/neohar.app, with nh inside
mise run app:install  # copy it to ~/Applications/neohar.app
```

`app:install` checks the signature, copies the app and puts it in place in one step. It never
replaces an installed app unless you add `-- --yes`, because that app can have running panes.
`NEOHAR_APPLICATIONS_DIR` changes the destination folder.

The file `neohar.app/Contents/Resources/THIRD_PARTY_LICENSES.md` gives the licenses of the code and
data of other authors in the app.

:::tip[With Claude Code]

In a Claude Code session in this repository, the `/install-app` skill builds the app, installs it,
restarts a running app (its panes keep running), checks that the panes came back and installs the
hooks again.

:::

## The first launch

```bash
open ~/Applications/neohar.app
```

- macOS asks if neohar can show notifications. Allow it if you want to answer approvals from a
  notification.
- The window shows no tab. Click **New workspace** at the bottom of the left column. You get one
  tab with one pane that runs your login shell.
- The launch makes `~/.local/bin/nh`, a symlink to the `nh` inside the app. Hooks call this path,
  so an update of the app never breaks them.

| Path | What it holds |
|---|---|
| `~/Library/Application Support/dev.neohar.app/default/` | The data of the session: `state.db`, scrollback, blobs, logs |
| `~/.config/neohar/config.toml` | Your settings. It does not exist until you change a setting |
| `/tmp/neohar-$UID/` | The sockets of the core and of the panes |

For a shell outside neohar to find `nh`, put `~/.local/bin` on your `PATH`:

```bash
echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.zshrc
nh session info
```

## Connect Claude Code and Codex

Without hooks, neohar finds an agent from its process and its screen, but it only guesses its state
and cannot answer its dialogs. The hooks give the exact state and let you answer from neohar.

```bash
nh integration install all --dry-run   # show the change
nh integration install all             # write it
nh integration status                  # what is installed and trusted
```

| Harness | File | Then |
|---|---|---|
| Claude Code | `~/.claude/settings.json` | Start a new session |
| Codex | `~/.codex/hooks.json` | Start Codex and choose "Trust all and continue", or run `/hooks` once. neohar never writes the trust |

A file that changes gets a backup next to it first: `<file>.neohar-bak-<seconds>`.
`nh integration uninstall all` removes the hooks. Then see
[Agents and approvals](/docs/guides/agents-and-approvals).

## Check the install

```bash
nh doctor
```

Each line checks one part: the symlink, the runtime folder, the sockets, the hooks, the storage and
the notifications. The command works also when neohar does not run. The exit code is 1 when a check
fails. [Troubleshooting](/docs/guides/troubleshooting) has the common problems.

## Update

```bash
git pull && mise install && bun install && mise run build:app
```

Quit neohar with **Quit neohar (agents keep running)** (`⌘Q`), then install and open the new app:

```bash
mise run app:install -- --yes
open ~/Applications/neohar.app
```

A quit leaves every pane and agent running. The new app connects to them again and restores the
screens ([Holders](/docs/architecture/holders)). There is no updater in the app yet.

:::warning

An update over running panes of an older build is not tested yet. If you do not need the running
agents, choose **Quit and stop everything…** (`⌘⌥Q`) before you install.

:::

## Remove

1. Remove the hooks while neohar runs: `nh integration uninstall all`.
2. Stop every pane: **Quit and stop everything…** (`⌘⌥Q`). A plain quit leaves them running.
3. Remove the app and the symlink: `rm -rf ~/Applications/neohar.app` and `rm ~/.local/bin/nh`.
4. If you want the data gone too, remove the three paths of the table above. This cannot be undone.
