---
title: Code map
description: The crates, the packages, the dependency rules and the conventions of the codebase, and where to start reading.
sidebar_position: 1
---

The repository is a Cargo workspace and a Bun workspace in one tree. Rust does the work: the core, the holder, the VT engine, the store, the adapters and every tool. TypeScript paints: a SolidJS UI, a transport and a thin wrapper around a WASM decoder. The members are listed in [`Cargo.toml`](https://github.com/nikodem-wrona/neohar/blob/main/Cargo.toml) and [`package.json`](https://github.com/nikodem-wrona/neohar/blob/main/package.json). The tools are on [Tools](/docs/code/tools).

How each part works is in [Architecture](/docs/architecture/overview), [Agents](/docs/agents/overview) and [Desktop app](/docs/desktop/overview). This page is the map.

## Rust crates

| Crate | What it owns | Start with |
|---|---|---|
| [`nh-proto`](https://github.com/nikodem-wrona/neohar/tree/main/crates/nh-proto) | Every wire type: frames, the holder protocol, input, the API contract (methods, projections, events, streams, errors), paths. Compiles to `wasm32-unknown-unknown`. | [`api/methods.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/api/methods.rs), [`frame.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/frame.rs), [`holder.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/holder.rs) |
| [`nh-hold`](https://github.com/nikodem-wrona/neohar/tree/main/crates/nh-hold) | The holder: PTY spawn, the journal ring, cut points, checkpoints, the lease, exit. At most 2,500 lines of code. | [`journal.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-hold/src/journal.rs), [`cutpoint.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-hold/src/cutpoint.rs) |
| [`nh-term`](https://github.com/nikodem-wrona/neohar/tree/main/crates/nh-term) | The `VtEngine` trait on `alacritty_terminal`, damage and frames, history, snapshot and restore, selection, the input encoder. | [`engine.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-term/src/engine.rs), [`frame_builder.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-term/src/frame_builder.rs) |
| [`nh-store`](https://github.com/nikodem-wrona/neohar/tree/main/crates/nh-store) | SQLite: migrations, the event outbox and cursors, retention, the scrollback archive, blobs, degraded mode, the rows of each domain. | [`lib.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-store/src/lib.rs), [`events.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-store/src/events.rs) |
| [`nh-rich`](https://github.com/nikodem-wrona/neohar/tree/main/crates/nh-rich) | Rich content: diffs, sanitized markdown, highlight spans, the command reader and its risk rules, the project scan. | [`diff.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-rich/src/diff.rs), [`command.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-rich/src/command.rs) |
| [`nh-redact`](https://github.com/nikodem-wrona/neohar/tree/main/crates/nh-redact) | Redaction of secrets in logs, event payloads and debug bundles. | [`patterns.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-redact/src/patterns.rs) |
| [`nh-agents`](https://github.com/nikodem-wrona/neohar/tree/main/crates/nh-agents) | Harness manifests, detection, the arbiter, the Claude Code and Codex adapters, the hook installer, screen manifests, the keystroke verifier, transcripts and prices. | [`manifests/`](https://github.com/nikodem-wrona/neohar/tree/main/crates/nh-agents/manifests), [`matcher.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-agents/src/matcher.rs) |
| [`nh-tasks`](https://github.com/nikodem-wrona/neohar/tree/main/crates/nh-tasks) | Task workspaces: slugs, git worktrees and branches, the changes of a task, port leases, setup commands. | [`git.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-tasks/src/git.rs), [`ports.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-tasks/src/ports.rs) |
| [`nh-core`](https://github.com/nikodem-wrona/neohar/tree/main/crates/nh-core) | The core: the state actor and command bus, projections, the API server, panes and the render server, config, jobs, health, interactions, policy, tasks, files, search, the palette, doctor. | [`actor/`](https://github.com/nikodem-wrona/neohar/tree/main/crates/nh-core/src/actor), [`api.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-core/src/api.rs), [`pane/`](https://github.com/nikodem-wrona/neohar/tree/main/crates/nh-core/src/pane) |
| [`nh-remote`](https://github.com/nikodem-wrona/neohar/tree/main/crates/nh-remote) | Remote machines: the protocol `link/1`, the server of `link.sock` and its client, the link over `ssh`, reconnect, port forwards. | [`lib.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-remote/src/lib.rs), [`link.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-remote/src/link.rs), [`ssh.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-remote/src/ssh.rs) |
| [`nh-web`](https://github.com/nikodem-wrona/neohar/tree/main/crates/nh-web) | The listener of the phone: the phone app, its JSON-RPC WebSocket and the pairing, on a port of `127.0.0.1`, the mapping of `tailscale serve`, and the sender of the web push. | [`lib.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-web/src/lib.rs), [`http.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-web/src/http.rs), [`tailscale.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-web/src/tailscale.rs), [`push/`](https://github.com/nikodem-wrona/neohar/tree/main/crates/nh-web/src/push) |
| [`nh`](https://github.com/nikodem-wrona/neohar/tree/main/crates/nh) | The sidecar binary. Its roles: `nh hold`, `nh hook` (the shim), `nh daemon`, `nh bridge` (the remote end of a link), `nh askpass`, and the CLI. | [`main.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh/src/main.rs), [`shim.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh/src/shim.rs) |
| [`neohar-desktop`](https://github.com/nikodem-wrona/neohar/tree/main/apps/desktop/src-tauri) | The Tauri host. It links `nh-core`, owns the windows, the menu, the tray, notifications and the three Tauri commands. | [`lib.rs`](https://github.com/nikodem-wrona/neohar/blob/main/apps/desktop/src-tauri/src/lib.rs), [`host.rs`](https://github.com/nikodem-wrona/neohar/blob/main/apps/desktop/src-tauri/src/host.rs) |

## UI packages

| Package | What it is | Start with |
|---|---|---|
| [`@neohar/desktop`](https://github.com/nikodem-wrona/neohar/tree/main/apps/desktop) | The webview entry. It mounts the shell or the settings view. | [`src/main.ts`](https://github.com/nikodem-wrona/neohar/blob/main/apps/desktop/src/main.ts) |
| [`@neohar/web`](https://github.com/nikodem-wrona/neohar/tree/main/apps/web) | The entry of the phone app. `mise run build:web` builds it into `target/web-dist`, and the build of `nh-web` puts it into the binary. | [`src/main.ts`](https://github.com/nikodem-wrona/neohar/blob/main/apps/web/src/main.ts) |
| [`@neohar/ui`](https://github.com/nikodem-wrona/neohar/tree/main/packages/ui) | The SolidJS shell (sidebar, tabs, layout, drawer with the inbox and the Files and Changes tabs, cards, palette, search, settings), the keyboard layer, and the WebGL2 and Canvas2D renderers. `phone/` is the second shell, for the phone, with no renderer. | [`shell/`](https://github.com/nikodem-wrona/neohar/tree/main/packages/ui/src/shell), [`surface/`](https://github.com/nikodem-wrona/neohar/tree/main/packages/ui/src/surface), [`phone/`](https://github.com/nikodem-wrona/neohar/tree/main/packages/ui/src/phone) |
| [`@neohar/transport`](https://github.com/nikodem-wrona/neohar/tree/main/packages/transport) | The `Transport` interface, `TauriTransport`, `WebSocketTransport`, the projection follower, the input outbox, and the generated API types. | [`index.ts`](https://github.com/nikodem-wrona/neohar/blob/main/packages/transport/src/index.ts), [`projection.ts`](https://github.com/nikodem-wrona/neohar/blob/main/packages/transport/src/projection.ts) |
| [`proto-wasm`](https://github.com/nikodem-wrona/neohar/tree/main/packages/proto-wasm) | The frame decoder: `nh-proto` frames into typed arrays in WASM memory. `mise run build:wasm` builds it into `pkg/` (not committed). | [`decoder.rs`](https://github.com/nikodem-wrona/neohar/blob/main/packages/proto-wasm/src/decoder.rs) |

The test packages under [`tests/`](https://github.com/nikodem-wrona/neohar/tree/main/tests) (`render`, `keyboard`, `transport`, `ui`, `phone`) are Bun workspace members too. See [Testing](/docs/testing/overview).

## Generated types

`packages/transport/src/generated.ts` is written, never edited. `mise run gen:api` runs [`tools/gen-ts`](https://github.com/nikodem-wrona/neohar/tree/main/tools/gen-ts) over `nh api schema`, the JSON Schema of the contract in `nh-proto`. `mise run check:gen` fails when the file is stale. After a change to the API types, run `gen:api` and accept the `insta` snapshot of the contract.

## Dependency rules

`mise run check:deps` ([`scripts/check-deps.ts`](https://github.com/nikodem-wrona/neohar/blob/main/scripts/check-deps.ts)) checks these rules over `cargo tree` in every `mise run check`. A new crate needs a row there, or the check fails.

| Crate | May depend on (workspace) | Must not pull in |
|---|---|---|
| `nh-proto` | — | tokio |
| `nh-hold` | `nh-proto` | tokio, tracing, serde_json |
| `nh-term` | `nh-proto` | tokio |
| `nh-store` | `nh-proto`, `nh-redact` | tokio |
| `nh-rich`, `nh-redact` | — | tokio |
| `nh-agents` | `nh-proto`, `nh-term`, `nh-rich`, `nh-redact` | `nh-core`, tauri |
| `nh-tasks` | `nh-proto` | `nh-core`, tauri |
| `nh-core` | every crate above | tauri |
| `nh-remote` | `nh-core`, `nh-proto` | tauri |
| `nh-web` | `nh-core`, `nh-proto` | tauri |
| `nh` | `nh-proto`, `nh-hold`, `nh-core`, `nh-remote`, `nh-web` | tauri |
| `neohar-desktop` | `nh-core`, `nh-proto`, `nh-remote`, `nh-web` | — |
| `proto-wasm` | `nh-proto` | tokio |

The same check fails when the `wasm-bindgen` command line in `mise.toml` differs from the crate in `Cargo.lock`. On the TypeScript side, ESLint lets `packages/ui` import only `solid-js`, `@neohar/transport` and `@neohar/proto-wasm`, and [`scripts/tauri-commands.ts`](https://github.com/nikodem-wrona/neohar/blob/main/scripts/tauri-commands.ts) lets only `packages/transport` call Tauri, and only `rpc`, `subscribe` and `stream`.

## Conventions

- Rust edition 2024 with the toolchain of `mise.toml`. Workspace dependencies are declared once in the root `Cargo.toml`.
- `thiserror` in library crates, `anyhow` in binaries and tools. No `unwrap()` outside tests.
- `tracing` everywhere except `nh-hold` and the hook shim, which log to a plain file.
- `ulid` for ids. RFC 3339 with milliseconds in the API, unix milliseconds in SQLite.
- `unsafe` is denied by the workspace lints. The few modules that need it allow it locally, with a `// SAFETY:` comment on each block.
- `tokio` is the async runtime of `nh-core`, `nh-remote`, `nh-tasks`, the desktop host and some tools. It is never in `nh-proto`, `nh-hold`, `nh-term`, `nh-store`, `nh-rich` or `nh-redact`.
- Wire types live in `nh-proto` only. Frame and holder types change by appending; fields and variants are never reordered or removed.
- Features `dev-ws` and `selftest` are for development builds and tests. A release never has them. The feature `debug` of `nh-core` adds the `debug.*` methods: `nh` has it only with `dev-ws`, and the tests, `nh-chaos` and `nh-golden` turn it on. The desktop host does not, so a release has no debug methods.

## Where to start reading

| To understand | Read |
|---|---|
| How a process starts and stays alive | [`crates/nh/src/main.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh/src/main.rs), then [`crates/nh-hold/src/lib.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-hold/src/lib.rs) |
| How a command changes state | [`crates/nh-core/src/actor`](https://github.com/nikodem-wrona/neohar/tree/main/crates/nh-core/src/actor), [`crates/nh-core/src/api.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-core/src/api.rs) |
| How bytes become pixels | [`crates/nh-term/src/frame_builder.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-term/src/frame_builder.rs), then [`packages/ui/src/surface`](https://github.com/nikodem-wrona/neohar/tree/main/packages/ui/src/surface) |
| How an approval reaches a person and back | [`crates/nh/src/shim.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh/src/shim.rs), [`crates/nh-core/src/interaction`](https://github.com/nikodem-wrona/neohar/tree/main/crates/nh-core/src/interaction), [`packages/ui/src/cards`](https://github.com/nikodem-wrona/neohar/tree/main/packages/ui/src/cards) |
| How a harness is recognized | [`crates/nh-agents/manifests`](https://github.com/nikodem-wrona/neohar/tree/main/crates/nh-agents/manifests), [`crates/nh-agents/src/matcher.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-agents/src/matcher.rs) |
| How a remote machine connects | [`crates/nh-remote/src/lib.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-remote/src/lib.rs), [`crates/nh/src/bridge.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh/src/bridge.rs) |
