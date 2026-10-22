# Changelog

This file records the notable changes to neohar.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), with the breaking
changes first. neohar uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
`mise run release` writes each section from the
[Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/) since the last release. Do
not edit a released section. A note under Unreleased goes to the top of the next release.

## [Unreleased]

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
