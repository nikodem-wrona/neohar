---
title: "nh search"
description: "Search the scrollback"
sidebar_position: 21
---

:::info[Generated page]

This page is generated from the `--help` output of `target/debug/nh` by `website/scripts/gen-cli.ts`. The commands are defined in [`crates/nh/src/cli`](https://github.com/nikodem-wrona/neohar/tree/main/crates/nh/src/cli). Do not edit it by hand: build `nh` and run `bun run gen` in `website/`.

:::

Search the scrollback

```text
Search the scrollback

Usage: nh search [OPTIONS] <COMMAND>

Commands:
  query  Search the scrollback of the panes

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh search query` {#search-query}

Search the scrollback of the panes

**API method:** [`search.query`](/docs/reference/api/methods/search#search-query)

```text
Search the scrollback of the panes

Usage: nh search query [OPTIONS] <QUERY>

Arguments:
  <QUERY>  

Options:
      --limit <NUMBER>     
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --pane <TEXT>        
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --workspace <TEXT>   
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```
