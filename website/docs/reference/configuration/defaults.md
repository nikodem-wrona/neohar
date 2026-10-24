---
title: "Default values"
description: "Every configuration key of neohar with its default value and meaning."
sidebar_position: 2
---

:::info[Generated page]

This page is generated from [`crates/nh-core/src/config/defaults.toml`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-core/src/config/defaults.toml) by `website/scripts/gen-config.ts`.
Do not edit it by hand. Change the source and run `bun run gen` in `website/`.

:::

The compiled-in defaults of neohar. A key that `config.toml` does not have gets its value from here. Every key that the core reads is in this file, and a key of `config.toml` that is not here is reported and ignored. `~/.config/neohar/config.toml` holds only what the user changed.

Write the keys that you want to change in `~/.config/neohar/config.toml` (or `$NEOHAR_CONFIG_DIR/config.toml`), under the same table names. See [Configuration](/docs/reference/configuration) for how the file is merged, reloaded and edited.

| Table | Keys |
|---|---|
| [`[machines]`](#machines) | — |
| [`[approvals]`](#approvals) | `mode`, `gate_timeout` |
| [`[approvals.claude]`](#approvals-claude) | `persist_always`, `mode` |
| [`[approvals.codex]`](#approvals-codex) | `mode` |
| [`[policy]`](#policy) | `rule` |
| [`[agents]`](#agents) | `shims`, `resume_on_launch`, `turn_end_notice` |
| [`[agents.harness.claude]`](#agents-harness-claude) | `billing` |
| [`[agents.harness.codex]`](#agents-harness-codex) | `billing` |
| [`[events]`](#events) | `sync_retention`, `audit_retention` |
| [`[scrollback]`](#scrollback) | `max_bytes_per_pane` |
| [`[daemon]`](#daemon) | `prevent_sleep` |
| [`[git]`](#git) | `fetch_minutes` |
| [`[phone]`](#phone) | `enabled`, `port`, `https_port`, `url`, `contact`, `keep_awake`, `font_size`, `keys` |
| [`[ui]`](#ui) | `font_family`, `font_size`, `macos_option_as_alt`, `global_hotkey`, `editor`, `show_handles` |
| [`[theme]`](#theme) | `bg`, `bg_raised`, `bg_hover`, `line`, `text`, `text_dim`, `accent`, `danger` |
| [`[keys]`](#keys) | `palette_open`, `tab_next`, `tab_previous`, `pane_focus_left`, `pane_focus_right`, `pane_focus_up`, `pane_focus_down`, `pane_split_right`, `pane_split_down`, `inbox_open`, `interaction_answer_next`, `peek_sibling`, `search_open`, `git_open`, `window_new`, `pass_through` |

## `[machines]` {#machines}

This table has no keys by default.

```toml
[machines]
```

## `[approvals]` {#approvals}

Machines that this client connects to over ssh. Write one block for each:   [machines.studio]   ssh = "me@studio.example.ts.net"  # any target that ssh accepts   session = "default"               # the session on that machine   connect = "manual"                # "launch" connects at launch, "manual" only when you ask The label has 1 to 32 characters of a-z, 0-9 and "-". "local" is the label of this machine.

| Key | Default | Description |
|---|---|---|
| `mode` | `"auto"` | How the core treats a request of an agent for approval. "observe": the harness shows its dialog. "gate": the core holds the hook. "auto": it depends on whether the pane has focus. |
| `gate_timeout` | `"30m"` | How long the core holds a hook for an answer. The installer writes the same time as the timeout of the hooks. |

```toml
[approvals]
mode = "auto"
gate_timeout = "30m"
```

## `[approvals.claude]` {#approvals-claude}

| Key | Default | Description |
|---|---|---|
| `persist_always` | `false` | Write "allow always" rules to the settings of Claude Code, and not only to the session. |
| `mode` | `"inherit"` | The mode for Claude Code: "inherit" (the mode above), "observe", "gate" or "auto". |

```toml
[approvals.claude]
persist_always = false
mode = "inherit"
```

## `[approvals.codex]` {#approvals-codex}

| Key | Default | Description |
|---|---|---|
| `mode` | `"inherit"` | The mode for Codex: "inherit" (the mode above), "observe", "gate" or "auto". |

```toml
[approvals.codex]
mode = "inherit"
```

## `[policy]` {#policy}

| Key | Default | Description |
|---|---|---|
| `rule` | `[]` | Rules that decide an approval before a person is asked. Write them as `[[policy.rule]]` blocks in `config.toml`. `nh policy suggest` makes blocks from your answers. |

```toml
[policy]
rule = []
```

## `[agents]` {#agents}

| Key | Default | Description |
|---|---|---|
| `shims` | `true` | Put the shims of `claude` and `codex` first in the PATH of every pane. |
| `resume_on_launch` | `"ask"` | What to do with the agents of the last session after a reboot: "ask", "always" or "never". |
| `turn_end_notice` | `true` | A notification when the turn of an agent ends while nobody looks at its pane. The run counts in the badge in both cases. |

```toml
[agents]
shims = true
resume_on_launch = "ask"
turn_end_notice = true
```

## `[agents.harness.claude]` {#agents-harness-claude}

| Key | Default | Description |
|---|---|---|
| `billing` | `"api"` | How the runs of Claude Code are paid for. "api": each token has a price, and the usage shows a cost. "subscription": the usage shows tokens and no dollars. |

```toml
[agents.harness.claude]
billing = "api"
```

## `[agents.harness.codex]` {#agents-harness-codex}

| Key | Default | Description |
|---|---|---|
| `billing` | `"api"` | The same for Codex. |

```toml
[agents.harness.codex]
billing = "api"
```

## `[events]` {#events}

| Key | Default | Description |
|---|---|---|
| `sync_retention` | `"7d"` | How long the event log keeps the events that a client uses to catch up. The core deletes older events when it starts and then once an hour. A client with an older cursor takes a new snapshot. |
| `audit_retention` | `"90d"` | How long the log keeps the audit kinds: interactions, policy and the state changes of agents. |

```toml
[events]
sync_retention = "7d"
audit_retention = "90d"
```

## `[scrollback]` {#scrollback}

| Key | Default | Description |
|---|---|---|
| `max_bytes_per_pane` | `0` | The most bytes of archive for one pane. 0 means no limit. When the core starts and then once an hour, it deletes the oldest segments of a pane that has more. The newest segment stays. |

```toml
[scrollback]
max_bytes_per_pane = 0
```

## `[daemon]` {#daemon}

| Key | Default | Description |
|---|---|---|
| `prevent_sleep` | `"while_agents_work"` | When `nh daemon` keeps the Mac awake: "while_agents_work" (an agent run is working, or a gate holds a hook), "always" or "never". The app does not use it. A MacBook with a closed lid on battery sleeps in all cases. |

```toml
[daemon]
prevent_sleep = "while_agents_work"
```

## `[git]` {#git}

| Key | Default | Description |
|---|---|---|
| `fetch_minutes` | `10` | How often the Git view fetches the remote of a repository that a window shows, in minutes. 0 turns the fetch in the background off. A fetch in the background never asks for a passphrase or a token: it stops, and the view says so. |

```toml
[git]
fetch_minutes = 10
```

## `[phone]` {#phone}

| Key | Default | Description |
|---|---|---|
| `enabled` | `false` | The listener of the phone: the app or `nh daemon` serves the phone app on a port of 127.0.0.1, and `tailscale serve` puts it on the tailnet with HTTPS. |
| `port` | `47310` | The port on 127.0.0.1. The mapping of `tailscale serve` names it. |
| `https_port` | `8443` | The HTTPS port of `tailscale serve` on the tailnet. |
| `url` | `""` | The HTTPS origin that the phone uses, for example "https://studio.example.ts.net:8443". Empty: the name of this machine in the tailnet and `https_port`. |
| `contact` | `""` | The `sub` of a push: a "mailto:" or "https:" address. Empty: a fixed address. |
| `keep_awake` | `true` | Keep the Mac out of idle sleep for a phone with Notifications on: while an agent works, while a gate holds a hook, and for 2 hours after a push. A MacBook with a closed lid on battery sleeps in all cases. |
| `font_size` | `12` | The font size of the panes on a phone, in CSS pixels. A pinch on a pane changes it. The phone takes a size from 9 to 20. |
| `keys` | `["\|", "/", "~", "-"]` | The keys of the key bar over the keyboard of a phone, after Esc, Tab, Ctrl, Alt, Shift+Tab and the arrows. Each key types its text. |

```toml
[phone]
enabled = false
port = 47310
https_port = 8443
url = ""
contact = ""
keep_awake = true
font_size = 12
keys = ["|", "/", "~", "-"]
```

## `[ui]` {#ui}

| Key | Default | Description |
|---|---|---|
| `font_family` | `"\"Hack Nerd Font Mono\", Menlo, \"SF Mono\", \"DejaVu Sans Mono\", \"Symbols Nerd Font Mono\", monospace"` | The font of the terminal surface: families as CSS lists them, and the size in pixels. The first family is a Nerd Font, so that the icons of shells and agents show. A machine without it falls back to Menlo, and gets the icons from the symbols font when that one is installed. |
| `font_size` | `13` |  |
| `macos_option_as_alt` | `"none"` | Which Option keys of a Mac act as Alt: "none", "left", "right" or "both". |
| `global_hotkey` | `"ctrl+alt+cmd+n"` | The chord that shows neohar and the card that waits longest, from any application. The modifiers are `ctrl`, `alt`, `shift` and `cmd`, joined with "+", then a letter or a digit. An empty text turns the hotkey off. |
| `editor` | `""` | The app that opens a file or a folder of the Files tab, by its name, such as "Visual Studio Code" or "Zed". An empty text opens a file with its default app and a folder in the Finder. |
| `show_handles` | `false` | Whether the tab bar, the strip of a pane, the attention line, the Inbox and the search show the handles of tabs and panes, such as "w1:t2" and "w1:p3". Off, a pane goes by its title. |

```toml
[ui]
font_family = "\"Hack Nerd Font Mono\", Menlo, \"SF Mono\", \"DejaVu Sans Mono\", \"Symbols Nerd Font Mono\", monospace"
font_size = 13
macos_option_as_alt = "none"
global_hotkey = "ctrl+alt+cmd+n"
editor = ""
show_handles = false
```

## `[theme]` {#theme}

| Key | Default | Description |
|---|---|---|
| `bg` | `"#000000"` | The colours of the chrome of the window, as CSS colours. The palette of the terminal comes with the frames of each pane. The defaults are true black, white values, gray labels, green for what is live and the action that goes, red for an error or a destructive answer. The page makes the other shades from these colours. |
| `bg_raised` | `"#141414"` |  |
| `bg_hover` | `"#262626"` |  |
| `line` | `"#3d3d3d"` |  |
| `text` | `"#f2f2f2"` |  |
| `text_dim` | `"#989898"` |  |
| `accent` | `"#4aea7d"` |  |
| `danger` | `"#ff5251"` |  |

```toml
[theme]
bg = "#000000"
bg_raised = "#141414"
bg_hover = "#262626"
line = "#3d3d3d"
text = "#f2f2f2"
text_dim = "#989898"
accent = "#4aea7d"
danger = "#ff5251"
```

## `[keys]` {#keys}

| Key | Default | Description |
|---|---|---|
| `palette_open` | `"mod+k"` | The reserved chords of a window. The page takes them before a pane sees a key. A chord is modifiers and a key joined with "+". `mod` is Command on a Mac and Control elsewhere. The other modifiers are `shift` and `alt`. The key is a letter, a digit, `[`, `]`, `,` or an arrow (`left`, `right`, `up`, `down`). The names are the ids of the commands with `_` for `.`. |
| `tab_next` | `"mod+shift+]"` |  |
| `tab_previous` | `"mod+shift+["` |  |
| `pane_focus_left` | `"mod+alt+left"` |  |
| `pane_focus_right` | `"mod+alt+right"` |  |
| `pane_focus_up` | `"mod+alt+up"` |  |
| `pane_focus_down` | `"mod+alt+down"` |  |
| `pane_split_right` | `"mod+d"` |  |
| `pane_split_down` | `"mod+shift+d"` |  |
| `inbox_open` | `"mod+shift+i"` |  |
| `interaction_answer_next` | `"mod+shift+a"` |  |
| `peek_sibling` | `"mod+shift+p"` |  |
| `search_open` | `"mod+shift+f"` |  |
| `git_open` | `"mod+shift+g"` |  |
| `window_new` | `"mod+shift+n"` |  |
| `pass_through` | `"mod+alt+v"` | The chord that lets the next key reach the pane, also when it is reserved. |

```toml
[keys]
palette_open = "mod+k"
tab_next = "mod+shift+]"
tab_previous = "mod+shift+["
pane_focus_left = "mod+alt+left"
pane_focus_right = "mod+alt+right"
pane_focus_up = "mod+alt+up"
pane_focus_down = "mod+alt+down"
pane_split_right = "mod+d"
pane_split_down = "mod+shift+d"
inbox_open = "mod+shift+i"
interaction_answer_next = "mod+shift+a"
peek_sibling = "mod+shift+p"
search_open = "mod+shift+f"
git_open = "mod+shift+g"
window_new = "mod+shift+n"
pass_through = "mod+alt+v"
```

## The whole file

```toml
# The compiled-in defaults of neohar. A key that `config.toml` does not have gets its value from
# here. Every key that the core reads is in this file, and a key of `config.toml` that is not here
# is reported and ignored. `~/.config/neohar/config.toml` holds only what the user changed.

[machines]
# Machines that this client connects to over ssh. Write one block for each:
#   [machines.studio]
#   ssh = "me@studio.example.ts.net"  # any target that ssh accepts
#   session = "default"               # the session on that machine
#   connect = "manual"                # "launch" connects at launch, "manual" only when you ask
# The label has 1 to 32 characters of a-z, 0-9 and "-". "local" is the label of this machine.

[approvals]
# How the core treats a request of an agent for approval.
# "observe": the harness shows its dialog. "gate": the core holds the hook. "auto": it depends on
# whether the pane has focus.
mode = "auto"
# How long the core holds a hook for an answer. The installer writes the same time as the timeout
# of the hooks.
gate_timeout = "30m"

[approvals.claude]
# Write "allow always" rules to the settings of Claude Code, and not only to the session.
persist_always = false
# The mode for Claude Code: "inherit" (the mode above), "observe", "gate" or "auto".
mode = "inherit"

[approvals.codex]
# The mode for Codex: "inherit" (the mode above), "observe", "gate" or "auto".
mode = "inherit"

[policy]
# Rules that decide an approval before a person is asked. Write them as
# `[[policy.rule]]` blocks in `config.toml`. `nh policy suggest` makes blocks from your answers.
rule = []

[agents]
# Put the shims of `claude` and `codex` first in the PATH of every pane.
shims = true
# What to do with the agents of the last session after a reboot: "ask", "always" or "never".
resume_on_launch = "ask"
# A notification when the turn of an agent ends while nobody looks at its pane. The run
# counts in the badge in both cases.
turn_end_notice = true

[agents.harness.claude]
# How the runs of Claude Code are paid for. "api": each token has a price, and the usage
# shows a cost. "subscription": the usage shows tokens and no dollars.
billing = "api"

[agents.harness.codex]
# The same for Codex.
billing = "api"

[events]
# How long the event log keeps the events that a client uses to catch up. The core deletes older
# events when it starts and then once an hour. A client with an older cursor takes a new snapshot.
sync_retention = "7d"
# How long the log keeps the audit kinds: interactions, policy and the state changes of agents.
audit_retention = "90d"

[scrollback]
# The most bytes of archive for one pane. 0 means no limit. When the core starts and then once an
# hour, it deletes the oldest segments of a pane that has more. The newest segment stays.
max_bytes_per_pane = 0

[daemon]
# When `nh daemon` keeps the Mac awake: "while_agents_work" (an agent run is working,
# or a gate holds a hook), "always" or "never". The app does not use it. A MacBook with a closed
# lid on battery sleeps in all cases.
prevent_sleep = "while_agents_work"

[git]
# How often the Git view fetches the remote of a repository that a window shows, in minutes. 0
# turns the fetch in the background off. A fetch in the background never asks for a passphrase
# or a token: it stops, and the view says so.
fetch_minutes = 10

[phone]
# The listener of the phone: the app or `nh daemon` serves the phone app on a port of
# 127.0.0.1, and `tailscale serve` puts it on the tailnet with HTTPS.
enabled = false
# The port on 127.0.0.1. The mapping of `tailscale serve` names it.
port = 47310
# The HTTPS port of `tailscale serve` on the tailnet.
https_port = 8443
# The HTTPS origin that the phone uses, for example "https://studio.example.ts.net:8443". Empty:
# the name of this machine in the tailnet and `https_port`.
url = ""
# The `sub` of a push: a "mailto:" or "https:" address. Empty: a fixed address.
contact = ""
# Keep the Mac out of idle sleep for a phone with Notifications on: while an agent works,
# while a gate holds a hook, and for 2 hours after a push. A MacBook with a closed lid on battery
# sleeps in all cases.
keep_awake = true
# The font size of the panes on a phone, in CSS pixels. A pinch on a pane changes it.
# The phone takes a size from 9 to 20.
font_size = 12
# The keys of the key bar over the keyboard of a phone, after Esc, Tab, Ctrl, Alt,
# Shift+Tab and the arrows. Each key types its text.
keys = ["|", "/", "~", "-"]

[ui]
# The font of the terminal surface: families as CSS lists them, and the size in pixels. The
# first family is a Nerd Font, so that the icons of shells and agents show. A machine without
# it falls back to Menlo, and gets the icons from the symbols font when that one is installed.
font_family = "\"Hack Nerd Font Mono\", Menlo, \"SF Mono\", \"DejaVu Sans Mono\", \"Symbols Nerd Font Mono\", monospace"
font_size = 13
# Which Option keys of a Mac act as Alt: "none", "left", "right" or "both".
macos_option_as_alt = "none"
# The chord that shows neohar and the card that waits longest, from any application. The
# modifiers are `ctrl`, `alt`, `shift` and `cmd`, joined with "+", then a letter or a digit. An
# empty text turns the hotkey off.
global_hotkey = "ctrl+alt+cmd+n"
# The app that opens a file or a folder of the Files tab, by its name, such as "Visual Studio
# Code" or "Zed". An empty text opens a file with its default app and a folder in the
# Finder.
editor = ""
# Whether the tab bar, the strip of a pane, the attention line, the Inbox and the search show the
# handles of tabs and panes, such as "w1:t2" and "w1:p3". Off, a pane goes by its title.
show_handles = false

[theme]
# The colours of the chrome of the window, as CSS colours. The palette of the terminal
# comes with the frames of each pane. The defaults are true black, white values, gray labels,
# green for what is live and the action that goes, red for an error or a destructive answer. The
# page makes the other shades from these colours.
bg = "#000000"
bg_raised = "#141414"
bg_hover = "#262626"
line = "#3d3d3d"
text = "#f2f2f2"
text_dim = "#989898"
accent = "#4aea7d"
danger = "#ff5251"

[keys]
# The reserved chords of a window. The page takes them before a pane sees a key. A chord is
# modifiers and a key joined with "+". `mod` is Command on a Mac and Control elsewhere. The other
# modifiers are `shift` and `alt`. The key is a letter, a digit, `[`, `]`, `,` or an arrow (`left`,
# `right`, `up`, `down`). The names are the ids of the commands with `_` for `.`.
palette_open = "mod+k"
tab_next = "mod+shift+]"
tab_previous = "mod+shift+["
pane_focus_left = "mod+alt+left"
pane_focus_right = "mod+alt+right"
pane_focus_up = "mod+alt+up"
pane_focus_down = "mod+alt+down"
pane_split_right = "mod+d"
pane_split_down = "mod+shift+d"
inbox_open = "mod+shift+i"
interaction_answer_next = "mod+shift+a"
peek_sibling = "mod+shift+p"
search_open = "mod+shift+f"
git_open = "mod+shift+g"
window_new = "mod+shift+n"
# The chord that lets the next key reach the pane, also when it is reserved.
pass_through = "mod+alt+v"
```
