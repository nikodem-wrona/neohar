---
title: API concepts
description: How a client talks to the core - transports, the JSON-RPC envelope, errors, versions, scopes, commands and jobs, projections, events, binary streams and idempotency.
sidebar_position: 1
---

The UI, the `nh` CLI, the hook shim and remote clients use **one contract**: the same JSON-RPC methods, projections, events and error codes. The generated pages list every item: [methods](/docs/reference/api), [projections](/docs/reference/api/projections), [events](/docs/reference/api/events), [streams](/docs/reference/api/streams), [errors](/docs/reference/api/errors) and [types](/docs/reference/api/types).

The contract is Rust code in [`crates/nh-proto/src/api`](https://github.com/nikodem-wrona/neohar/tree/main/crates/nh-proto/src/api). `nh api schema` prints it as one JSON document, and a snapshot test ([`api_schema__contract.snap`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/tests/snapshots/api_schema__contract.snap)) shows every change as a diff. `mise run gen:api` writes the TypeScript types of [`packages/transport/src/generated.ts`](https://github.com/nikodem-wrona/neohar/blob/main/packages/transport/src/generated.ts) from it, and `mise run check` fails when that file is stale.

## Transports

| Transport | Who uses it | Framing |
|---|---|---|
| Tauri IPC | The webview of an app that runs the core | Three commands: `rpc`, `subscribe` and `stream`, with binary channels |
| `api.sock` | The `nh` CLI, the hook shim, scripts | Newline-delimited JSON-RPC 2.0. No binary streams |
| `link.sock` | An app that is a client of a daemon, and `nh bridge` over ssh | `link/1`: length-prefixed `postcard` messages that carry the same calls, projections and frames |
| Dev WebSocket | Browser tests | JSON-RPC in text messages, frames in binary messages. `127.0.0.1` only, with a token, never in a release |

The sockets are under `<runtime>/<session>/` with mode 0600, and the core checks the UID of each peer. In the UI, `TauriTransport` and `WebSocketTransport` implement one `Transport` interface, so the UI code does not know which one it has.

## The envelope

On `api.sock` each message is one JSON object on one line:

```text
→ {"jsonrpc":"2.0","id":4,"method":"pane.split","params":{"pane":"w1:p1","direction":"right"}}
← {"jsonrpc":"2.0","id":4,"result":{"pane":"01JB…","handle":"w1:p2"}}
← {"jsonrpc":"2.0","id":5,"error":{"code":-32000,"message":"…","data":{"code":"confirm_required","details":{"process":"zsh","is_agent":false}}}}
```

- A request without `id` runs and gets no reply. Absent `params` count as `{}`.
- A line has at most 5 MiB. A connection runs at most 64 calls at the same time; more get `busy`.
- **Replies come in the order in which the calls end.** Match them by `id`. The `adapter.*` calls of the shim are the exception.
- The core pushes notifications (`projection.message`, `events.event`, `events.truncated`) on the same connection after a subscribe.

## Errors

A caller sees `{code, message, details}`. On the wire, the JSON-RPC protocol errors keep their integers (-32700, -32600, -32601, -32602, -32603), and every application code is -32000 with the text code in `error.data.code`. The UI acts on five codes and shows `message` for the rest:

| Code | What a client does |
|---|---|
| `confirm_required` | A close would stop a running program. Ask the person, then call again with `force: true` |
| `degraded_storage` | The store is full or read-only and the method needs it. Show the reason |
| `already_answered` | Another client answered the interaction first. Show who |
| `delivery_unknown` | The core cannot say whether the answer reached the harness. Keep the card |
| `input_unconfirmed` | An input was in flight when the holder or the core went away. Mark the pane |

## Versions

| What | Version | Where to read it |
|---|---|---|
| API | `neohar/1`, additive within the major version | `session.info.api_version` |
| Frames | 1 | `session.info.protocols.frame` |
| Holder protocol | `holder/2` | `session.info.protocols.holder` |
| Event payloads | `v` in each event | the event |

## Methods and scopes

A method is `<noun>.<verb>`: `pane.split`, `interaction.answer`, `fs.list`. A method that names a workspace, tab, pane or interaction takes the ULID or the short handle (`w3`, `w3:t2`, `w3:p5`, `i42`). Each method lists who may call it:

| Scope | Who | Notes |
|---|---|---|
| `ui` | The webview, and the dev WebSocket | Input, resize, windows, palette and rich text are UI only |
| `cli` | A client of `api.sock` with no pane token | Every `cli` method has an `nh` verb |
| `pane` | A request with the token of a pane | Only the four `adapter.*` methods. A hook can wait for a decision but can never answer one |
| `dev` | Builds with the feature `debug` | The `debug.*` methods |

A method that is not for the caller is `permission_denied`. The CLI never sends the token of the pane it runs in, so `nh` in a pane has the CLI scope.

## Commands and jobs

A method that changes state returns after its transaction committed. The result carries only ids; the client sees the change in a projection patch. The UI makes no optimistic update. A long command returns `{job}` at once (`agent.start`, `agent.resume`, `git.fetch`, `git.pull`, `integration.doctor`), and its progress is in the `jobs` projection. A command that needs the database gets `degraded_storage` while the store is degraded; reads, input and rendering go on.

## Projections

A projection is a keyed map plus an order in the memory of the core:

```json
{"type": "snapshot", "version": 12, "data": {"items": {"01JA…": {}}, "order": ["01JA…"]}}
{"type": "patch", "version": 13, "ops": [{"op": "set", "key": "01JA…", "value": {}}, {"op": "remove", "key": "01JB…"}]}
{"type": "resync"}
```

- The snapshot comes first. Then at most one patch per 16 ms, with the last value of each changed key.
- The version grows by 1 for each patch. Subscribe with `since` to get the patches you missed from a ring of 1,000. `resync` means: subscribe again without `since`. Versions start at 0 when the core restarts.
- `palette` and `health` have one key, `_`. Some names take a parameter: `timeline:<run>`, `pane_text:<pane>`, `agents_for:<window>`.
- The UI never joins two projections. A join it needs is its own projection.

On `api.sock`, `projection.subscribe {name, since?}` returns `{subscription}`. A connection has at most 64 subscriptions, and a slow reader gets `resync`. A slow reader on a WebSocket (the phone, the dev WebSocket) also gets `resync`.

## Events

Each committed command appends its events in the same transaction. They are for sync, history and audit, never to rebuild state. An event has `seq`, `at`, `kind` (`pane.created`, `interaction.decided`), `v`, `subject` and a typed `payload` with secrets redacted.

`events.subscribe {cursor?, filter?}` reads the events after the cursor from the database, then follows the live ones, with no gap and no repeat. A filter has `kinds` (a trailing `*` is a prefix) and `subject`. A cursor that the log cannot continue gets `events.truncated` with a new cursor: take a fresh snapshot, then go on from it. `nh events tail` is the CLI for it.

## Binary streams

Streams carry `postcard` bytes that the WASM decoder reads. They exist on the Tauri channels, `link.sock` and the dev WebSocket, not on `api.sock`.

| Kind | Direction | What |
|---|---|---|
| `render` | core → UI | Frames of the visible panes of a window; the UI acks with `render.ack` |
| `peek` | core → UI | Keyframes of the bottom rows of a pane, at most 4 a second |
| `scrollback` | core → UI | Pages of up to 200 lines |
| `input` | UI → core | In the contract only. Keys use `pane.input` |

A stream that falls behind gets fewer, larger frames, never a growing queue. Images are blobs at `nh://blob/<hash>`.

## Idempotency

- **`input_id`** on `pane.input`, `pane.paste`, `pane.send_bytes` and `pane.drop_files`: the holder writes an id once, so a retry with the same id is safe. Input is at-most-once; an input that no holder confirmed is reported as unconfirmed.
- **`idempotency_key`** on the decisions that the core sends to a held hook: the shim acks with it, and the core re-sends a decision only with the same key. The first answer to an interaction wins; a later one gets `already_answered`.

## Raw calls

```bash
nh api call session.info
nh api call pane.show '{"pane":"w1:p2"}'
echo '{"pane":"w1:p2"}' | nh api call pane.read -
nh api schema > contract.json        # needs no running core
```
