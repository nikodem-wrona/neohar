---
title: The core
description: How nh-core works inside its host — the one door for every call, the state actor, the SQLite store with its transactional outbox, projections, configuration, jobs, health, supervision and the API server.
sidebar_position: 3
---

`nh-core` is the server of neohar. It is a library, not a process: `neohar.app` links it, and `nh daemon` runs it without windows. It owns the layout, the panes and their VT engines, the event log, the agents and their interactions, the notifications, the render server and the API server. One core serves one machine and session.

The core runs on its **own** tokio runtime (threads `nh-core-N`), separate from the runtime of Tauri, so a panic in the host cannot reach it. A host talks to it through a `CoreHandle`, which is cheap to clone and works from any thread.

## One door for every call

Every call goes through `CoreHandle::call_as(origin, method, params)`: the webview through the Tauri command `rpc`, the CLI and the hook shim through `api.sock`, a remote client through `link.sock`, and the tests directly.

1. **Authorize.** The scope of the origin (`ui`, `cli`, `pane`, `dev`) is checked against the method in the contract. An unknown name is `method_not_found`; a method that is not for the caller is `permission_denied`.
2. **Direct handlers.** Calls that act on live processes or do slow work run on the caller's task: input, resize, paste, selection, render acks, files, the doctor and others. When they change state, they still ask the actor.
3. **The state actor** runs everything else that changes state.
4. A method of the contract that nothing implements answers `unsupported`.

## The state actor

One OS thread, `nh-state`, owns the `Store` and the in-memory `State` ([actor/mod.rs](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-core/src/actor/mod.rs)). It reads one envelope at a time from a channel, so there is one order and no lock on the state. Pane tasks and other parts send it closures (update jobs and store jobs) instead of writing themselves, so the actor is the only writer.

A command runs in this order:

1. If the method needs persistence and the store is degraded, answer `degraded_storage` and stop.
2. Run the handler, a plain function `fn(&Ctx, &mut Tx, &Call)`, in one SQLite transaction. The handler can only read the state; it writes rows and appends events through the `Tx`.
3. On an error or a panic, roll back. Nothing changed, so no undo code exists.
4. After the commit, run the `Mutation` that the handler returned (it changes the in-memory state and the projections), broadcast the events, then reply.

The reply carries only ids. A client sees the change through its projection patch.

## The store and the outbox

Each session has one SQLite database, `state.db`, in WAL mode with `synchronous = NORMAL` and `STRICT` tables ([crates/nh-store](https://github.com/nikodem-wrona/neohar/tree/main/crates/nh-store/src)). Its state tables are the source of truth: layout, panes and their holders, snapshots, agent runs, interactions and deliveries, policy rules, windows and their clients, the executions of workflows and their nodes.

A new database gets the schema of version 15 from one file, [`0015_schema.sql`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-store/migrations/0015_schema.sql). Each later migration runs in its own transaction. The store refuses a database from a newer build. It also refuses a database of a development build from before version 15, and it does not change that file. The error tells you to move `state.db` to a different place.

- **Transactional outbox.** `Store::tx` inserts the events that a handler appended in the same transaction as the state rows. If the handler fails, no row and no event stays. The events table is a log for subscribers, timelines and audit; it is not used to rebuild state.
- **Cursors.** A cursor is `{machine_uuid, session_uuid, log_epoch, seq}`. A client with a cursor from another epoch, from the future or below the retention floor gets `truncated` and takes a new snapshot.
- **Redaction.** Every event payload goes through `nh-redact` before it is stored.
- **Handles.** Short handles (`w3:p5`, `i42`) come from counters in the same transaction, so a handle is never used twice in a session.
- **Blobs.** Content-addressed files under `blobs/`, named by their BLAKE3 digest, written atomically and checked on read. The webview reads them as `nh://blob/<hash>`.
- **Crash safety.** A test kills a writer with `SIGKILL` 50 times; the database passes `integrity_check` and no event exists without its row.

**Degraded mode.** A full disk or an I/O error sets a flag in the store. Writes are refused at once, a probe tries again every 10 s, and `health.storage` shows the reason. Panes, input, rendering, reads and the configuration keep working; the palette disables the commands that write.

**Scrollback archive.** Lines that leave a pane's in-memory history (at most 900 lines) go to zstd segments under `scrollback/<pane>/`, flushed at least once a second. The scrollback of a pane and `pane.read` read them.

**Retention** ([retention.rs](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-core/src/retention.rs)). When the core starts and then once an hour, it deletes events older than `events.sync_retention` (7 days), and events of the audit kinds older than `events.audit_retention` (90 days). A client with a cursor below the deleted events gets `truncated`. The same pass deletes the interactions that ended before `events.audit_retention`, with their delivery records. When `scrollback.max_bytes_per_pane` is above 0, the same pass deletes the oldest segments of each pane over the cap; the newest segment stays. The default is no cap, so the scrollback stays on disk (mode 0600) with no limit. A change of these settings starts a new pass within a minute.

:::note[Not wired yet]
Projection versions live in memory only: after a restart of the core, a client gets `resync` and a new snapshot.
:::

## Projections

`ProjectionHub` ([projection/mod.rs](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-core/src/projection/mod.rs)) keeps each projection as a keyed map plus an order.

- Changes are coalesced and flushed every 16 ms as one patch per projection, so a storm of agent events cannot starve the renderer.
- The last 1,000 patches stay in a ring, so a client that reconnects with `since` gets only what it missed; an older `since` gets `resync`.
- Reads are lock-free (`ArcSwap`). A `set` with an equal value makes no patch.
- The snapshot shape is `{items, order}`. Some projections have a parameter: `timeline:<run>`, `pane_text:<pane>`, `agents_for:<window>`.

## Clients and the host

The core never touches a window, the tray, notifications or the clipboard itself. It asks a `Host` ([host.rs](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-core/src/host.rs)): `TauriHost` in the app, `NoopHost` in the daemon and tests. A core can have several clients: the host in its process and the clients of `link.sock`. A router sends each host request to the right client, each row of `windows` belongs to one client, and a client may call the window methods only for its own windows.

## Configuration

`config.toml` in the config directory is the only store of the settings.

- `defaults.toml`, compiled in, has every key and every default. The file is merged over it key by key.
- `config.set {key, value}` edits the file in place with `toml_edit` (comments stay), checks the whole result, and renames a temporary file over it.
- A watcher reloads 200 ms after the last write. A bad file keeps the last good config and shows a warning in `health`.
- `config.*` needs no database, so it works while the store is degraded.

The keys are listed in [Configuration](/docs/reference/configuration).

## Jobs, health and supervision

- **Jobs.** A long command (`agent.start`, `agent.resume`, `integration.doctor` and others) returns `{job}` at once and reports progress in the `jobs` projection. `jobs.cancel` marks it `cancelled` at once. Jobs live in memory only.
- **Health.** One row, `_`, of the `health` projection: storage state, renderer, holder protocols, unvalidated harnesses and warnings from every part.
- **Supervision.** A panic never takes the core down, and it is never silent. The actor catches a panic per command and rolls back. Supervised tasks restart with backoff and set a `health` warning when they give up. A pane task that panics marks its pane `failed`. An adapter that panics starts again on the next signal.

## The API server

[api.rs](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-core/src/api.rs) serves newline-delimited JSON-RPC 2.0 on `<runtime>/<session>/api.sock`. The core binds it **first** at start, so a second core for the same session fails with `ApiSocketInUse`.

| Rule | Value |
|---|---|
| Peer | Same user id only, socket mode 0600 |
| Line | At most 5 MiB |
| Calls in flight | 64 per connection, then `busy` |
| Order | Calls run on their own tasks, except `adapter.*`, which run in order |
| Subscriptions | `projection.subscribe` and `events.subscribe`, at most 64 per connection |

`link.sock` (the server of `nh-remote`) gives a remote client the same calls with `ui` scope. The development WebSocket (cargo feature `dev-ws`) binds `127.0.0.1` with a token and is never in a product build. The listener of the phone (`nh-web`) uses the same JSON-RPC loop (`ws_rpc.rs`), with the cookie of a paired phone and the scope `phone`. Clients read [API concepts](/docs/reference/api/concepts) for the envelope, errors and subscriptions.

## Start and stop

`Core::start` ([runtime.rs](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-core/src/runtime.rs)) binds `api.sock`, opens `state.db`, loads the config, the harness manifests, the layout, the open runs and interactions, starts the actor and the supervised tasks, registers the stored pane tokens, takes a new fencing token, starts the API server, and only then runs the restart flow of the panes, because an agent in a restored pane can send a hook at once.

`CoreHandle::quit` checkpoints and detaches every pane; `stop_everything` stops every program; `Core::shutdown` closes the listeners and the database, and to the holders it looks like a crash with a whole database ([Holders](/docs/architecture/holders)).
