# Changelog

This file records the notable changes to neohar.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), with the breaking
changes first. neohar uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
`mise run release` writes each section from the
[Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/) since the last release. Do
not edit a released section. A note under Unreleased goes to the top of the next release.

## [Unreleased]

This release is the first with a DMG for Apple silicon. The DMG has an ad-hoc signature, and Apple
did not notarize it. Drag neohar to the Applications folder. Then run
`xattr -dr com.apple.quarantine /Applications/neohar.app`, because without it macOS refuses to open
the app. [Install the app](https://github.com/nikodem-wrona/neohar/blob/main/website/docs/guides/install.md)
tells more.

Read this before you update. This release closes the findings of a security review, and some of
the fixes change what you do:

- neohar runs git in a repository only after you trust it. Click **Trust** in the Changes tab or
  the Git view once for each repository, or run `nh git trust <path>`. neohar asks again when a
  setting or a hook of the repository that can start a program changes.
- "Allow always" shows only when neohar can make a rule that covers the request exactly, and the
  button shows the rule. A notification and the tray show Allow only for a command of low or
  medium risk that the title shows in full. For other requests, open the card.
- From inside a neohar pane, neohar refuses these calls: an answer to a request, a change of the
  policy, `nh git trust`, `nh config set` for the keys of `approvals`, `policy`, `phone` and
  `machines`, and input to a pane that waits for an answer. Use the app, or a terminal that is not
  a neohar pane.
- `nh --socket` refuses a socket in a folder that other users can write, for example directly in
  `/tmp`.
- neohar removes a paired phone that did not connect for 30 days. Pair it again.
- A `neohar://pane/…` link shows the app but does not select the pane.

## [1.0.0] - 2026-10-21

The first public release.

### Added

- A desktop app for macOS with workspaces, tabs and panes. A Rust core owns the terminals and
  draws them with WebGL2.
- Claude Code and Codex in panes. The board shows the state of each agent. The state comes from
  the hooks of the harness, and from the screen only when no hook is available.
- Approvals and questions as cards, as notifications or as CLI calls. Policy rules can answer them.
- Panes that continue to run when the app quits, crashes or updates. A small holder process owns
  each pane, and the next launch restores the screens.
- Tasks, each with its own git worktree.
- The Files tab and the Changes tab, and a search.
- The `nh` command line. It uses the same JSON-RPC methods as the app.
- Remote machines: the app connects over ssh to an `nh daemon` on another Mac and shows its panes.
- A phone app. The core serves it on `127.0.0.1`, and `tailscale serve` puts it on your tailnet. A
  paired phone shows the desk and answers requests. It gets a push when an agent needs you and no
  desktop window has the focus.

### Known limits

- macOS is the only tested platform.
- Builds have an ad-hoc signature. They are for local use only, and the app has no updater.

[Unreleased]: https://github.com/nikodem-wrona/neohar/compare/v1.0.0...HEAD
[1.0.0]: https://github.com/nikodem-wrona/neohar/releases/tag/v1.0.0
