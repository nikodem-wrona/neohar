---
title: "nh pane"
description: "Panes"
sidebar_position: 12
---

:::info[Generated page]

This page is generated from the `--help` output of `target/debug/nh` by `website/scripts/gen-cli.ts`. The commands are defined in [`crates/nh/src/cli`](https://github.com/nikodem-wrona/neohar/tree/main/crates/nh/src/cli). Do not edit it by hand: build `nh` and run `bun run gen` in `website/`.

:::

Panes

```text
Panes

Usage: nh pane [OPTIONS] <COMMAND>

Commands:
  list       List the panes
  show       Show a pane
  split      Split a pane
  close      Close a pane
  move       Move a pane to a tab
  float      Float a pane over the tab
  unfloat    Put a floating pane back in the layout
  popout     Move a pane to a window of its own
  restart    Start the program of a pane again
  paste      Paste text into a pane
  send-keys  Type keys into a pane: text, and names such as Enter, Escape, C-c
  scroll     Scroll the viewport of a pane
  copy       Copy the selection of a pane
  read       Print the text of a pane

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh pane list` {#pane-list}

List the panes

**API method:** [`pane.list`](/docs/reference/api/methods/pane#pane-list)

```text
List the panes

Usage: nh pane list [OPTIONS]

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --tab <TEXT>         
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --workspace <TEXT>   
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh pane show` {#pane-show}

Show a pane

**API method:** [`pane.show`](/docs/reference/api/methods/pane#pane-show)

```text
Show a pane

Usage: nh pane show [OPTIONS] <PANE>

Arguments:
  <PANE>  A reference to a pane: its ULID, or its short handle.

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh pane split` {#pane-split}

Split a pane

**API method:** [`pane.split`](/docs/reference/api/methods/pane#pane-split)

```text
Split a pane

Usage: nh pane split [OPTIONS] <PANE> <DIRECTION>

Arguments:
  <PANE>       The pane to split.
  <DIRECTION>  Which side of `pane` the new pane goes on.

Options:
      --command <JSON>     
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --ratio <NUMBER>     The share of the new pane, from 0 to 1. Without it the two panes share the room.
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh pane close` {#pane-close}

Close a pane

**API method:** [`pane.close`](/docs/reference/api/methods/pane#pane-close)

```text
Close a pane

Usage: nh pane close [OPTIONS] <PANE>

Arguments:
  <PANE>  A reference to a pane: its ULID, or its short handle.

Options:
      --force              
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh pane move` {#pane-move}

Move a pane to a tab

**API method:** [`pane.move`](/docs/reference/api/methods/pane#pane-move)

```text
Move a pane to a tab

Usage: nh pane move [OPTIONS] <PANE> <TAB>

Arguments:
  <PANE>  A reference to a pane: its ULID, or its short handle.
  <TAB>   A reference to a tab: its ULID, or its short handle.

Options:
      --beside <TEXT>      
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --direction <TEXT>   
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh pane float` {#pane-float}

Float a pane over the tab

**API method:** [`pane.float`](/docs/reference/api/methods/pane#pane-float)

```text
Float a pane over the tab

Usage: nh pane float [OPTIONS] <PANE>

Arguments:
  <PANE>  A reference to a pane: its ULID, or its short handle.

Options:
      --rect <JSON>        Where to put it. Without it the core picks a rectangle.
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh pane unfloat` {#pane-unfloat}

Put a floating pane back in the layout

**API method:** [`pane.unfloat`](/docs/reference/api/methods/pane#pane-unfloat)

```text
Put a floating pane back in the layout

Usage: nh pane unfloat [OPTIONS] <PANE>

Arguments:
  <PANE>  A reference to a pane: its ULID, or its short handle.

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh pane popout` {#pane-popout}

Move a pane to a window of its own

**API method:** [`pane.popout`](/docs/reference/api/methods/pane#pane-popout)

```text
Move a pane to a window of its own

Usage: nh pane popout [OPTIONS] <PANE>

Arguments:
  <PANE>  A reference to a pane: its ULID, or its short handle.

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh pane restart` {#pane-restart}

Start the program of a pane again

**API method:** [`pane.restart`](/docs/reference/api/methods/pane#pane-restart)

```text
Start the program of a pane again

Usage: nh pane restart [OPTIONS] <PANE>

Arguments:
  <PANE>  A reference to a pane: its ULID, or its short handle.

Options:
      --command <JSON>     
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh pane paste` {#pane-paste}

Paste text into a pane

**API method:** [`pane.paste`](/docs/reference/api/methods/pane#pane-paste)

```text
Paste text into a pane

Usage: nh pane paste [OPTIONS] <PANE> <TEXT>

Arguments:
  <PANE>  A reference to a pane: its ULID, or its short handle.
  <TEXT>  

Options:
      --input-id <NUMBER>  
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh pane send-keys` {#pane-send-keys}

Type keys into a pane: text, and names such as Enter, Escape, C-c

```text
Type keys into a pane: text, and names such as Enter, Escape, C-c

Usage: nh pane send-keys [OPTIONS] <PANE> <KEYS>...

Arguments:
  <PANE>     A reference to a pane: its ULID, or its short handle.
  <KEYS>...  

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh pane scroll` {#pane-scroll}

Scroll the viewport of a pane

**API method:** [`pane.scroll`](/docs/reference/api/methods/pane#pane-scroll)

```text
Scroll the viewport of a pane

Usage: nh pane scroll [OPTIONS] <PANE>

Arguments:
  <PANE>  A reference to a pane: its ULID, or its short handle.

Options:
      --delta <NUMBER>     
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --top-line <NUMBER>  
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh pane copy` {#pane-copy}

Copy the selection of a pane

**API method:** [`pane.copy`](/docs/reference/api/methods/pane#pane-copy)

```text
Copy the selection of a pane

Usage: nh pane copy [OPTIONS] <PANE>

Arguments:
  <PANE>  A reference to a pane: its ULID, or its short handle.

Options:
      --return-text        Also return the text, for the CLI and for a phone.
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh pane read` {#pane-read}

Print the text of a pane

**API method:** [`pane.read`](/docs/reference/api/methods/pane#pane-read)

```text
Print the text of a pane

Usage: nh pane read [OPTIONS] <PANE>

Arguments:
  <PANE>  A reference to a pane: its ULID, or its short handle.

Options:
      --lines <JSON>       
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --visible            Read the visible screen. It is the default when `lines` is absent.
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```
