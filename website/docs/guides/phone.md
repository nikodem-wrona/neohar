---
title: The phone
description: "Supervise your agents from a phone: turn on the phone listener, publish it with Tailscale, pair a phone, get a push when an agent needs you, and what keeps the listener safe."
sidebar_position: 6
---

The phone app shows the desk of your Mac on a phone. You can read the panes, answer approvals and
questions from the Inbox, and reply to an agent that ended its turn. A paired phone gets a push
when an agent needs you and no desktop window has the focus.

The core of the app (or of `nh daemon`) serves the phone app on a port of `127.0.0.1`. Tailscale
puts that port on your tailnet with HTTPS. neohar has no TLS and no network listener of its own.
The phone app was tested as an iPhone in Safari.

## Before you start

- Install [Tailscale](https://tailscale.com) on the Mac and on the phone, and log in to the same
  tailnet on both.
- In the admin console of the tailnet, turn on MagicDNS and HTTPS certificates.

## Turn on the listener

Open the settings (`⌘,`) and go to the section **Phone**, or run:

```bash
nh config set phone.enabled true
```

The listener starts at once on `127.0.0.1:47310` (`phone.port`). The section **Phone** shows its
state and what to do next.

## Publish it with Tailscale

Click **Publish with Tailscale** in the section **Phone**, or run:

```bash
nh phone publish
```

neohar runs `tailscale serve` and maps the HTTPS port 8443 of the tailnet (`phone.https_port`) to the
listener. It never runs `tailscale funnel`, so the page stays inside your tailnet. It never changes
a mapping that serves something else. `nh phone unpublish` removes the mapping.

If your Mac has an HTTPS address of another kind, set `phone.url` to it and do not publish.

## Pair a phone

1. Click **Pair a phone** in the section **Phone**, or run `nh phone pair-begin`. neohar shows a QR
   code and a short code.
2. Scan the QR code with the camera of the phone. Safari opens the phone app.
3. In Safari, add neohar to the Home Screen, and open it from the Home Screen. The app pairs by
   itself. You can also type the short code.

The code is good for 10 minutes and for one phone. Five wrong codes end the offer.
`nh phone pair-cancel` ends it before that.

To remove a phone, click **Remove** next to it in the section **Phone**, or tap **Unpair this
phone** on the phone. `nh phone remove <device>` does the same. The removal closes the connections
of the phone at once.

## Push notifications

On the phone, open **This phone** and turn on **Notifications**. Safari sends a push only to an app
of the Home Screen.

- A phone gets a push when an agent needs you and no desktop window has the focus. An agent needs
  you for an approval, a question, a plan review, an error, a rate limit or the end of a turn.
- A phone that shows the phone app gets no push.
- A push has the harness, the work of the agent and the reason in a few words. It has no text of the
  agent.
- The core sends pushes only to the push services of Apple, Google and Mozilla.

**Send a test notification** in the settings and `nh phone push-test <device>` send a test push.
While a phone has notifications on, `phone.keep_awake` keeps the Mac out of idle sleep: while an
agent works, while neohar holds a request, and for 2 hours after a push. A MacBook with a closed lid
on battery sleeps in all cases.

## Security

- **Loopback only.** The listener accepts connections only on `127.0.0.1`. Tailscale brings the
  HTTPS requests of the tailnet to it.
- **Pairing.** Each pairing gives the phone a cookie with a random token. The cookie is
  `HttpOnly`, `Secure` and `SameSite=Strict`. The core keeps only a hash of the token.
- **Checks on each request.** Every route that is not a file of the app needs the cookie. Every
  request needs the host of the public address in `Host`, so that a page of another site cannot
  reach the port. The connections and each `POST` also need that address in `Origin`.
- **A scope of its own.** The calls of a phone have the scope `phone`. The
  [methods](/docs/reference/api/methods) page tells which methods each scope permits.
- **No logs of secrets.** The listener logs no token, no code and no query.

Each device in your tailnet can open the page, but only a paired phone can see the desk. Remove a
phone that you lost.

## Configuration

| Key | Default | Meaning |
|---|---|---|
| `phone.enabled` | `false` | Turns the listener on |
| `phone.port` | `47310` | The port on `127.0.0.1` |
| `phone.https_port` | `8443` | The HTTPS port of `tailscale serve` on the tailnet |
| `phone.url` | empty | The HTTPS address that the phone uses. Empty: the name of the Mac in the tailnet and `phone.https_port` |
| `phone.keep_awake` | `true` | Keep the Mac out of idle sleep for a phone with notifications on |

[Configuration defaults](/docs/reference/configuration/defaults) has every key of `[phone]`, and
[`nh phone`](/docs/reference/cli/commands/phone) has every command.
