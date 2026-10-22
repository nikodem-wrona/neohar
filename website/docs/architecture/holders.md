---
title: Holders and durability
description: How one nh hold process per pane keeps programs alive across a quit, crash or update of the app, the journal and cut points, at-most-once input, and the restart flow that brings screens back.
sidebar_position: 2
---

A holder, `nh hold`, is a small process with one job per pane: it owns the PTY master, it is the parent of the pane's program, and it keeps a journal of everything the program printed. The app and the core can stop at any time; the holder and its child go on. When a core starts again, it connects to each holder, restores the screen from a snapshot and replays the journal after it.

## Why a separate process

- **Independent.** It forks twice and calls `setsid`, so no exit of the app reaches it. The core starts it through `/bin/sh` with `tokio::process`, never through `tauri-plugin-shell`, which kills its children when the app exits.
- **Small and stable.** A holder runs as long as its pane, across updates, so its code must rarely change. It has no VT parser beyond a byte scanner, no async runtime and no logging framework. `mise run check:holder-size` keeps `crates/nh-hold/src` under 2,500 lines of code, and `check:deps` forbids `tokio`, `tracing` and `serde_json` in it.
- **Cheap.** 0.01% of a core idle, 0.08% with 10 output events a second.

## Start

The core runs `nh hold --pane <ulid> --config-fd 3`. Descriptor 3 carries one `HoldConfig` message: the secret, the command, the environment, the working directory, the size, the socket path and the checkpoint timings. None of them appears in `argv` or the environment. The holder binds its socket **before** it forks, then writes `READY <pid>` (or `ERROR <message>`) on descriptor 1, so "ready" means "listening".

The child gets the login environment that the core captured at start, the terminal variables and the `NEOHAR_*` variables of the pane, including its pane token. The default command is `$SHELL -l`.

## Socket and the lease

The socket is `<runtime>/<session>/holders/<pane-ulid>.sock`, mode 0600 in a 0700 directory. A client must be the same user, send `Hello` within 2 s with protocol 2, the 32-byte secret (compared in constant time) and a **fencing token** higher than every token the holder accepted.

The holder serves one client at a time. A `Hello` with a higher token takes the lease, and the old client gets `LeaseLost`. A core takes one token for all its panes: the clock in milliseconds, or one above the highest token in `state.db`, whichever is higher. So a core that starts later always wins, and an orphaned old core cannot write to the PTY.

macOS deletes old files in `/tmp`. Every 60 s, and on `SIGUSR1`, the holder checks its socket file and binds again if it is gone.

## The journal and cut points

The journal ([journal.rs](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-hold/src/journal.rs)) is a byte ring (16 MiB) with absolute `u64` offsets and a second ring of markers: `Resize`, `InputAck`, `CutPoint`. Replay sends the markers of an offset before its byte, so output, resizes and acks keep their order.

- **Pull, not push.** A client's offset advances only when its socket has room. A slow client never blocks the child; if it falls behind the ring start, it gets `Truncated`.
- **Exit order.** `ChildExited` goes out only after every journal byte. The holder keeps the journal and the status until a core sends `ExitAck`, so a core that was down still gets the final output.

A **cut point** is an offset that is not inside a UTF-8 sequence or an escape sequence. The scanner in [cutpoint.rs](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-hold/src/cutpoint.rs) mirrors the state table of `vte` 0.15, the parser of `alacritty_terminal`, and a property test checks it against `vte` itself. Checkpoints and replays start only at cut points, so a snapshot never needs parser state.

## Checkpoints

A checkpoint is a snapshot of the pane's VT engine at a cut point, stored in `state.db`. The holder asks with `CheckpointWanted` when half the ring was written since the last one, after 2 s without output, or after 30 s. The pane task takes the snapshot at the next `CutPoint` marker, flushes the scrollback archive, stores the snapshot (the newest two are kept) and answers `CheckpointTaken` after the commit.

`AlacrittyEngine::checkpoint` also rebuilds the running engine from the snapshot, so the engine that goes on is the one a restart would make. Without the rebuild, 2.2% of restored screens differed in the style of blank cells.

## Terminal queries without a core

While no core is attached, the holder answers the queries that do not depend on the screen (DA1, DA2, DA3, XTVERSION, the modes 2004 and 1004) with the same bytes as the core, from `nh_proto::answers`. It queues the others (cursor position, colour queries, window size), at most 64 for 5 s, and the next core answers them during its replay.

## Input ids: at most once

Every `Input` carries an `input_id`. The holder writes the bytes, records an `InputAck` marker and answers. It remembers the last 4,096 ids across cores, and a repeated id is not written again. So an input is never applied twice; an input without an ack may be lost, and the client is told with the event `pane.input_unconfirmed`:

| Reason | When |
|---|---|
| `holder_lost` | The connection to the holder ended before the ack |
| `not_written` | No ack after 30 s |
| `server_restarted` | The client sees a new core. Keystrokes are never stored, so a new core does not know them |

The core's own writes (query replies) carry ids with bit 63 set and the fencing token above bit 20, so the ids of two cores never collide. A shared counter was a real defect that the chaos test found.

## Quit, crash and update

| Event | What happens |
|---|---|
| Quit (⌘Q, "Quit neohar (agents keep running)") | Each pane asks for a checkpoint at its next cut point (at most 1 s), flushes the archive and detaches. Holders and agents keep running |
| Quit and stop everything (⌘⌥Q, confirmed) | Agents get their quit keys (2 s), children get `Term` and `Hup`, then `Kill`; the core waits until the holders are gone |
| Crash | Holders see a closed connection and keep everything. The next launch is a normal launch |
| Update | A quit and a launch. A new core must speak to N−1 holders: `holder/2` is append-only and 47 frozen fixtures guard it |
| Close a pane, tab or workspace | Asks first if a program runs (`confirm_required`). Then the core stops and kills the whole process family of the pane, also jobs that left the session |

`NEOHAR_STOP_ON_EXIT=1` makes a quit stop the panes instead. `mise run app:stop` kills the panes of the dev app.

## The restart flow

[pane/restore.rs](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-core/src/pane/restore.rs) runs for every pane that `state.db` lists as running, all in parallel:

1. Connect with the new fencing token, then `Status` for the journal range and the queued queries.
2. Pick the start: the newest snapshot inside the ring that restores, else the whole journal if the ring still starts at 0, else the ring start (`ring_only`, not exact for programs that do not repaint).
3. `Attach` from that offset and replay up to the `Status` end with every side effect off: no bell, notification, clipboard write or archive write. Fresh queued queries get their reply when the replay passes them.
4. Resize to `cols − 1`, then back after 50 ms. Programs that repaint on `SIGWINCH` (Claude Code, Codex, `vim`, `less`) then show exactly their screen.
5. Ask for a checkpoint, so the next restart replays less. The pane leaves `restoring` after its first frame.

Panes publish their rows in render-id order, so the layout of a window stays the same. A holder that does not answer stays `running` for the next try.

**After a reboot** the holders are gone. A pane of the layout whose child had not ended comes back with a login shell in its old directory and the event `pane.restarted`. Agent runs end as `rebooted`, and `agents.resume_on_launch` (`ask`, `always`, `never`) decides whether to resume them.

## Proof by chaos

The chaos test killed a core with `SIGKILL` 100 times under 21 live panes (`vim`, `less`, `htop`, fixtures) and compared every restored screen with a reference engine that never restarted: 3,759 screens, 0 differ; 0 of 10,539 inputs applied twice; 20 panes back in a median of 16 ms. The real app, killed and relaunched, gave 60 screens with 0 differences. Not covered: programs that do not repaint and `ring_only`. How to run it is on [Local suites](/docs/testing/local-suites).

## Key files

| Topic | Code |
|---|---|
| Holder | [crates/nh-hold/src](https://github.com/nikodem-wrona/neohar/tree/main/crates/nh-hold/src): `daemon.rs` (start), `event_loop.rs` (lease, poll loop), `journal.rs`, `cutpoint.rs`, `checkpoint.rs`, `sequences.rs` |
| Protocol | [crates/nh-proto/src/holder.rs](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/src/holder.rs), [crates/nh-proto/tests/holder2.rs](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-proto/tests/holder2.rs) |
| Core side | [crates/nh-core/src/holder](https://github.com/nikodem-wrona/neohar/tree/main/crates/nh-core/src/holder), [pane/task.rs](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-core/src/pane/task.rs), [pane/inputs.rs](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-core/src/pane/inputs.rs), [pane/family.rs](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-core/src/pane/family.rs) |
| Tests | [crates/nh/tests](https://github.com/nikodem-wrona/neohar/tree/main/crates/nh/tests) (`hold_*.rs`, `core_restart.rs`), [tools/nh-chaos](https://github.com/nikodem-wrona/neohar/tree/main/tools/nh-chaos) |
