---
title: Configuration
description: "config.toml: where it is, how the core merges it over the defaults, hot reload, config.get, config.set and config.reload, the settings window, and a summary of every table."
sidebar_position: 1
---

neohar has one configuration file, `config.toml`, that holds only what you changed. The core compiles in [`defaults.toml`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-core/src/config/defaults.toml), the only place where a default is written. [Defaults](/docs/reference/configuration/defaults) lists every key with its default and comment.

## Where the file is

`~/.config/neohar/config.toml` on macOS and Linux, or `$XDG_CONFIG_HOME/neohar/` when that is set, or `$NEOHAR_CONFIG_DIR/` in tests and the dev app. Every session reads the same file. A missing file means the defaults. The same directory holds `harnesses/*.toml`, the manifests of wrapper scripts around `claude` and `codex`.

## How the core reads it

- **Merge.** Tables merge key by key at every depth. Every other value replaces the default, also a list: `setup = ["make"]` is the whole list.
- **Types.** The merged table is checked against typed structs. A wrong type or an out-of-range value makes the whole file invalid. Times are text with a unit: `"500ms"`, `"90s"`, `"30m"`, `"7d"`.
- **Last good configuration.** A file with an error never takes a good configuration away. The core keeps the last good one and shows one warning in `health`. At the start, a bad file gives the defaults and the warning.
- **Unknown keys.** A key that `defaults.toml` does not have is ignored with a warning; the rest of the file applies. An invalid `[[policy.rule]]` is a warning too. The keys that older versions had (`input.confirm_multiline_paste_to_agents`, `search.index_transcripts`, `update.manifest_check`) are ignored with no warning and one line in the log.
- **Hot reload.** The core watches the directory and reads the file when it has been quiet for 200 ms. Most settings apply at their next use; `[ui]`, `[theme]` and `[keys]` change the open windows at once.

Each value has a source: `default` (not in the file), `file`, or `ui` (written by `config.set` in this session).

## Change it

```bash
nh config get approvals --json        # each value with its source
nh config set approvals.mode gate     # a word is text
nh config set tasks.ports 2           # JSON when it parses
nh config set tasks.setup '["bun install"]'
nh config set tasks.ports null        # remove the key: the default applies again
nh config edit                        # $VISUAL or $EDITOR, then a reload
nh config reload
```

`config.set` takes a leaf of `defaults.toml`, or `machines.<label>` and its fields. It edits the file in place with `toml_edit`, so comments and order stay, checks the whole new text before it writes, and writes atomically. It refuses to edit a file with a syntax error. The three methods need no database, so they work while the store is degraded.

**Settings…** in the app menu (⌘,) opens the settings window. It lists the sections of the `settings` projection; an edit calls `config.set`, and **Reset** sets the key to `null`.

## An example

```toml
[approvals]
mode = "gate"            # hold every hook for an answer in neohar

[agents.harness.claude]
billing = "subscription" # count tokens, show no dollars

[tasks]
worktree_root = "~/work/trees"
ports = 2
setup = ["git submodule update --init"]

[tasks.env]
RUST_LOG = "debug"

[[policy.rule]]
decision = "allow"
tool = "Bash"
command = "cargo test*"

[machines.studio]
ssh = "me@studio.example.ts.net"
connect = "launch"

[ui]
font_size = 14
editor = "Zed"
```

## The tables

| Table | What it sets |
|---|---|
| `[machines.<label>]` | The machines that the app connects to over ssh: `ssh`, `session`, `connect` (`launch` or `manual`). See [Remote machines](/docs/guides/remote-machines) |
| `[approvals]` | `mode` (`observe`, `gate` or `auto`, default `auto`) and `gate_timeout` (`30m`) |
| `[approvals.claude]`, `[approvals.codex]` | A `mode` of the harness (`inherit` by default); for Claude Code also `persist_always` |
| `[policy]` | The `[[policy.rule]]` blocks. See [Agents and approvals](/docs/guides/agents-and-approvals) |
| `[agents]` | `shims` (put the `claude` and `codex` shims first in the `PATH` of each pane) and `resume_on_launch` (`ask`, `always` or `never`) |
| `[agents.harness.claude]`, `[agents.harness.codex]` | `billing`: `api` (show a cost) or `subscription` (tokens only) |
| `[tasks]`, `[tasks.env]` | `worktree_root`, `branch_prefix`, `ports`, `port_range`, `setup`, and the variables of every task pane |
| `[daemon]` | `prevent_sleep`: when `nh daemon` keeps the Mac awake |
| `[phone]` | The phone listener: `enabled` (default `false`), `port` on `127.0.0.1` (`47310`), `https_port` of `tailscale serve` (`8443`), `url` (the HTTPS origin of the phone), `contact` (the `sub` of a push), `keep_awake`, and the `font_size` and `keys` of the phone app |
| `[ui]` | `font_family`, `font_size`, `macos_option_as_alt`, `global_hotkey`, `editor` (the app that opens a file of the Files tab), `show_handles` (show `w3:t2` and `w3:p5` in the window) |
| `[theme]` | Eight CSS colours of the window chrome |
| `[keys]` | The reserved chords of a window, named by command id with `_` for `.` (`palette_open = "mod+k"`) |
| `[events]` | `sync_retention` (`7d`) and `audit_retention` (`90d`, for interactions, policy, tasks and agent state): how long the event log keeps events. The core deletes older events at the start and then once an hour |
| `[scrollback]` | `max_bytes_per_pane`: the most bytes of archive for one pane. Above it, the same pass deletes the oldest segments. `0` (the default) is no limit |
