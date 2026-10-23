---
title: "nh phone"
description: "Phones that supervise this machine"
sidebar_position: 16
---

:::info[Generated page]

This page is generated from the `--help` output of `target/debug/nh` by `website/scripts/gen-cli.ts`. The commands are defined in [`crates/nh/src/cli`](https://github.com/nikodem-wrona/neohar/tree/main/crates/nh/src/cli). Do not edit it by hand: build `nh` and run `bun run gen` in `website/`.

:::

Phones that supervise this machine

```text
Phones that supervise this machine

Usage: nh phone [OPTIONS] <COMMAND>

Commands:
  pair-begin   Offer a pairing to a phone: print the URL of the QR code and the short code
  pair-cancel  End the offer of a pairing
  remove       Remove a paired phone and close its connections
  rename       Change the name of a paired phone
  publish      Put the phone listener on the tailnet with tailscale serve (HTTPS, never a Funnel)
  unpublish    Remove the mapping of tailscale serve for the phone listener
  push-test    Send the push "Test notification" to a paired phone

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh phone pair-begin` {#phone-pair-begin}

Offer a pairing to a phone: print the URL of the QR code and the short code

**API method:** [`phone.pair_begin`](/docs/reference/api/methods/phone#phone-pair-begin)

```text
Offer a pairing to a phone: print the URL of the QR code and the short code

Usage: nh phone pair-begin [OPTIONS]

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh phone pair-cancel` {#phone-pair-cancel}

End the offer of a pairing

**API method:** [`phone.pair_cancel`](/docs/reference/api/methods/phone#phone-pair-cancel)

```text
End the offer of a pairing

Usage: nh phone pair-cancel [OPTIONS]

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh phone remove` {#phone-remove}

Remove a paired phone and close its connections

**API method:** [`phone.remove`](/docs/reference/api/methods/phone#phone-remove)

```text
Remove a paired phone and close its connections

Usage: nh phone remove [OPTIONS] <DEVICE>

Arguments:
  <DEVICE>  The ULID of a client device, as 26 characters.

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh phone rename` {#phone-rename}

Change the name of a paired phone

**API method:** [`phone.rename`](/docs/reference/api/methods/phone#phone-rename)

```text
Change the name of a paired phone

Usage: nh phone rename [OPTIONS] <DEVICE> <NAME>

Arguments:
  <DEVICE>  The ULID of a client device, as 26 characters.
  <NAME>    1 to 64 characters.

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh phone publish` {#phone-publish}

Put the phone listener on the tailnet with tailscale serve (HTTPS, never a Funnel)

**API method:** [`phone.publish`](/docs/reference/api/methods/phone#phone-publish)

```text
Put the phone listener on the tailnet with tailscale serve (HTTPS, never a Funnel)

Usage: nh phone publish [OPTIONS]

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh phone unpublish` {#phone-unpublish}

Remove the mapping of tailscale serve for the phone listener

**API method:** [`phone.unpublish`](/docs/reference/api/methods/phone#phone-unpublish)

```text
Remove the mapping of tailscale serve for the phone listener

Usage: nh phone unpublish [OPTIONS]

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```

## `nh phone push-test` {#phone-push-test}

Send the push "Test notification" to a paired phone

**API method:** [`phone.push_test`](/docs/reference/api/methods/phone#phone-push-test)

```text
Send the push "Test notification" to a paired phone

Usage: nh phone push-test [OPTIONS] <DEVICE>

Arguments:
  <DEVICE>  The ULID of a client device, as 26 characters.

Options:
      --session <NAME>     The session to talk to. The default is $NEOHAR_SESSION, or `default`
      --socket <PATH>      The API socket to use. It wins over --session and $NEOHAR_SOCKET
      --machine <LABEL>    Run the command on this machine of config.toml, over ssh. A handle such as studio/w3:p5 does the same
      --json               Print JSON. This is the default when stdout is not a terminal
      --timeout <SECONDS>  How long to wait for the core to answer, in seconds [default: 30]
  -h, --help               Print help
```
