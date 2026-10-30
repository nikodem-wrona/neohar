---
title: The desktop app
description: "The pieces of neohar.app: the thin-presentation rule and its checks, the Tauri host, the transport between the webview and the core, and the native surfaces."
sidebar_position: 1
---

`neohar.app` is one process with two halves. The **Tauri host** (Rust) finds or starts a core,
opens a native window for each row of the `windows` projection, and draws the native surfaces that
the core asks for. The **webview** (React) paints what the core sends and sends back what the
person did. The webview holds no business state. The [window](/docs/desktop/ui) page describes what
a window shows.

| Piece | Path | What it does |
|---|---|---|
| Tauri host | [`apps/desktop/src-tauri`](https://github.com/nikodem-wrona/neohar/tree/main/apps/desktop/src-tauri) | Picks the backend (its own core, or a core behind `link.sock`), follows `windows`, registers the three commands `rpc`, `subscribe` and `stream`, serves `nh://`, implements the `Host` trait, the menu, the tray, notifications, the hotkey, links, and the quit paths |
| Webview entry | [`apps/desktop/src/main.ts`](https://github.com/nikodem-wrona/neohar/blob/main/apps/desktop/src/main.ts) | `?window=<ULID>` mounts the shell and `?view=settings` the settings window. Any other address shows an error |
| UI | [`packages/ui`](https://github.com/nikodem-wrona/neohar/tree/main/packages/ui) | The shell, the stores, the keymap, the cards and the surface renderers |
| Transport | [`packages/transport`](https://github.com/nikodem-wrona/neohar/tree/main/packages/transport) | One `Transport` interface with a Tauri and a WebSocket implementation, and the API types generated from `nh-proto` |
| Frame decoder | [`packages/proto-wasm`](https://github.com/nikodem-wrona/neohar/tree/main/packages/proto-wasm) | WASM build of `nh-proto` that decodes render frames into typed arrays ([rendering and input](/docs/architecture/rendering-and-input)) |
| Lint rules | [`tools/eslint-plugin-neohar`](https://github.com/nikodem-wrona/neohar/tree/main/tools/eslint-plugin-neohar) | The ESLint rules of the thin-presentation rule |

## The thin-presentation rule

The webview paints; it does not think. The same UI must serve every host, the app must be
disposable, and agent output is untrusted, so all logic lives in the core. If the UI needs
something new, add it to the core contract first, in the same pull request.

| Rule | In practice | Held by |
|---|---|---|
| Holds no business state | One store for each projection, made by `subscribe`. Only view state (hover, scroll, an open dialog, column widths) lives in the page | `createStores`; review |
| Predicts nothing | Every action is a command; the page shows the result when the patch arrives | Review |
| Decides no facts | Counts, sort orders, risk, search results, dates and highlighting come from Rust. A pure view mapper in a `model.ts` file can turn them into rows, labels, buttons and call parameters (see below) | ESLint `neohar/no-formatting` (no sort, no `Intl`), `neohar/ui-imports` (no markdown, diff, date libraries) |
| Parses no bytes | The WASM decoder turns frames into typed arrays | The `render` stream carries frames, not PTY bytes |
| Speaks one API | The page calls the same JSON-RPC methods as the CLI | `scripts/tauri-commands.ts`, `mise run test:host`, `mise run check:gen` |
| No polling, one `innerHTML` | Everything is push; `setTimeout` lives only in `lib/timing.ts`; `RichHtml.tsx` is the only `innerHTML` or `dangerouslySetInnerHTML` | `neohar/no-polling`, `neohar/no-timers`, `neohar/no-inner-html` |
| Stays small | Under 400 KB of gzipped script, style and WASM for the desk and for the phone, and under 250 KB for the first chunk of the phone | `mise run check:bundle` (part of `mise run check`) |

`packages/ui` can import only `react`, `react-dom/client`, the packages of shadcn/ui (`radix-ui`,
`class-variance-authority`, `clsx`, `tailwind-merge`, `lucide-react`, `cmdk`, `sonner`),
`@neohar/transport`, `@neohar/proto-wasm`, `@/…` and relative modules. Lint sees
syntax only. The review rejects a reducer of business events, a store that combines two
projections, a client-side filter or an optimistic update.

**View mappers.** A feature of the UI can have a `model.ts` file next to its views, for example
[`packages/ui/src/cards/model.ts`](https://github.com/nikodem-wrona/neohar/blob/main/packages/ui/src/cards/model.ts).
A view mapper turns the facts of the core into what the view shows. Examples are the buttons of a
card, the rows of a folder tree and the params of a call. It must be a pure function with no state
and no calls, and its unit tests are in `model.test.ts`. It must not decide a fact: a count, an
order, a risk, a match or a date still comes from the core.

**The one exception** is the drag of a split divider: the page shows the new shares at once and
sends one `layout.set_ratios` on release.
View math is not derivation: pixels to cells, the nearest pane for `⌘⌥←`, and the next tab in the
order the core gave.

## The Tauri host

**Startup.** A debug build refuses to start unless `NEOHAR_CONFIG_DIR`, `NEOHAR_DATA_DIR` and
`NEOHAR_RUNTIME_DIR` are set. The single-instance plugin registers first, so a second
launch hands its arguments to the running app and ends. On macOS the plugin uses a fixed socket in
`/tmp`, and another user can make that file first. The app registers the plugin only when the path
is free or is a socket of the user. Else it logs a warning and runs without it. The app then tries
`link.sock` of its session, after a check of the runtime root. When a core answers (for example `nh daemon`), the app starts no core and is a client of
that core; else it starts its own core on its own Tokio runtime and serves `link.sock` for other
clients. Every module talks to a `Backend`,
never to the core directly. A product build without overrides also keeps `~/.local/bin/nh`
pointing at the sidecar.

**Windows follow the core.** The host keeps no window list. It opens a native window for each row
of `windows`, with the row id as its label, at `index.html?window=<id>`, and destroys it when the
row goes. A window of a remote machine has the label `<machine>/<id>`, and the commands of
that window go to that machine's backend.
The host reports focus, visibility, minimize (joined with the page's own report) and the
bounds after 500 ms without a change (`window.set_bounds`). The close button calls
`window.close`.

**Exactly three commands.** The webview may call `rpc`, `subscribe` and `stream` and nothing else.
`build.rs` declares them, the capability file grants only
them, no plugin permission is granted, and `withGlobalTauri` is false. The one exception is the
window itself: on macOS the command line is the title bar, so the capability also grants
`core:window:allow-start-dragging` and `core:window:allow-internal-toggle-maximize`, which the drag
script of Tauri calls for `data-tauri-drag-region`. `scripts/tauri-commands.ts`
checks this statically, and `mise run test:host` runs a real window that tries a plugin command, a
fourth command, an inline script and a network fetch, and requires each to fail. Native work starts
in the core: for a file to open, the contract has `fs.open` and the core asks the host.

**The `Host` trait.** What the core needs from the process that carries it
([`crates/nh-core/src/host.rs`](https://github.com/nikodem-wrona/neohar/blob/main/crates/nh-core/src/host.rs)):
notifications, the badge, the tray, focus of a window, the clipboard, opening a URL or a file
(`fs.open` with `ui.editor`), the settings window and the ssh question sheet of
`nh askpass`. `nh daemon` and most tests use `NoopHost`. Requests from a remote core go
through `MachineHost`, which adds the machine label to notifications and the tray.

**`nh://` and navigation.** `nh://blob/<hash>` serves a blob of the store (immutable, cached).
Each blob has the CSP `sandbox`, and a blob that is not a raster image (for example HTML or SVG)
is an attachment, so it never runs as a page of the app.
`nh://font/` serves the font files that the person installed, because WebKit on macOS matches no
user font by family name. A pure policy allows only the app's own pages (`tauri://localhost` on
macOS and Linux, and the Vite dev server in a debug build). `nh://open?url=` opens `http`,
`https` and `mailto` outside the app, but only with the gesture token of the host. The host puts a
random token for each launch into each page (`__NEOHAR_GESTURE__`), and the click handler of
`RichHtml` adds it to the link. A refresh or a form in hostile HTML has no script, so it cannot
know the token, and nothing opens without a click. The policy denies everything else. The CSP
also has `form-action 'none'`, `base-uri 'none'`, `object-src 'none'` and `frame-src 'none'`.

**HTML of a remote core.** The core cleans each Markdown text with `ammonia`. For a window of a
remote machine, that core is on the other machine and can send any text. Thus the link client
cleans each field `html` of every result and projection message again, with the same rules
(`nh_rich::markdown::clean`). The UI inserts HTML only through `RichHtml`, and only from such a
field.

**Quitting.**

| Path | Panes and agents |
|---|---|
| Close the last window | Keep running; the app stays in the menu bar |
| `⌘Q`, "Quit neohar (agents keep running)" | The core checkpoints each pane and ends its lease; holders keep running and the next launch restores the screens |
| `⌘⌥Q`, "Quit and stop everything…" | After a native confirmation, every program gets `TERM`/`HUP`, then `KILL` |
| `NEOHAR_STOP_ON_EXIT` set (any value) | The panes stop on any quit. `mise run app:stop` kills the panes of a dev run afterwards |
| Crash or `SIGKILL` | Holders survive and the next launch reattaches ([holders](/docs/architecture/holders)) |

**Build variants.** `tauri.conf.json` is the base: no window (the host opens them) and the CSP.
`tauri.bundle.conf.json` adds the bundle, the `nh` sidecar and the ad-hoc signature.
`mise run build:dmg` also makes the DMG of a release. The cargo feature `selftest` adds the sandbox
check of `mise run test:host`.

## The transport

The UI knows four primitives and nothing about Tauri or sockets:

| Primitive | What it does |
|---|---|
| `call(method, params)` | Runs one API method. A method that the schema does not have does not compile. Rejects with `RpcError {code, message, details}` |
| `subscribe(name, handlers)` | Follows a [projection](/docs/reference/api/projections): `onSnapshot`, then `onPatch`, and `onResync` when the core cannot send the missed patches |
| `stream(kind, params, onFrame)` | A binary stream (`render`, `peek`, `scrollback`, `input`); each frame is an `ArrayBuffer` |
| `blobUrl(hash)` | `nh://blob/<hash>` |

- **`TauriTransport`** (the app) calls the three host commands. Projections and frames arrive on
  Tauri `Channel`s, frames as raw binary bodies. A window of a remote machine uses the same
  transport: the host routes the commands over the link.
- **`WebSocketTransport`** speaks JSON-RPC to the dev WebSocket of a headless core
  (`nh daemon --dev-ws`, token in the address, `127.0.0.1` only). It is a test seam for the
  browser suites, not a product feature. It reconnects after a pause that doubles from 250 ms to
  8 s.
- **`ProjectionFollower`** (shared) drops repeats, subscribes again with `since` when a version is
  missing, and takes a new snapshot after `resync`. A Tauri channel can stall without a gap, so the
  transport asks `projection.versions` when the window gets the focus or becomes visible.
- **Generated types.** `packages/transport/src/generated.ts` comes from `nh api schema`
  (`mise run gen:api`); `mise run check:gen` fails when it is stale.
- **Input outbox.** `InputOutbox` keeps unconfirmed `pane.input` calls for 30 s and resends them
  with the same `input_id`. It is built and tested but not wired: the app's IPC has no connection
  to lose.

## Native surfaces

The core decides **when** a surface changes and **what** it says
([`crates/nh-core/src/native`](https://github.com/nikodem-wrona/neohar/tree/main/crates/nh-core/src/native));
the host draws it and reports a click as an API call. An answer from any surface is an
`interaction.answer` with a `via` and goes through the same delivery transaction as a card
([interactions](/docs/agents/interactions)).

| Surface | What it does |
|---|---|
| Notifications (macOS) | One per run for an interaction on an unfocused pane, at most one post in 10 s, updated in place while the same interaction comes first. When another interaction comes first, a new notification replaces it, so a button never answers a request that the person did not read. Approvals get Allow and Deny at a low or medium risk with the whole command in the text, and Deny only otherwise; a click on the body shows the card. The host keeps the buttons of each notification and ignores a click on a button that it did not offer. Withdrawn when the interaction ends or its pane gets the focus. A late click posts "Already answered". Needs an app bundle (`UNUserNotificationCenter`) |
| Dock badge | The number of runs in `attention` |
| Menu-bar inbox | Open interactions in the order of `interaction.next`; approvals have Deny and Open, Allow under the rule of the notifications, and "Allow `<rule>` always" when the core sends the rule. One section for each machine. The two quit items |
| Global hotkey | `ui.global_hotkey` (default `ctrl+alt+cmd+n`): show the app and the card that waits longest. Changes without a restart |
| Deep links | `neohar://pane/` and `neohar://interaction/`, with an optional machine before the name (`neohar://pane/studio/w3:p5`). Any web page can open a link, so a link shows and never answers. A link to an interaction shows its card. A link to a pane brings a window of its machine to the front, and the pane does not get the focus, because the focus would release a held approval |
| Machines menu | For each machine: New window, Connect, Disconnect ([remote machines](/docs/guides/remote-machines)) |
| Settings window | `⌘,`: edits `config.toml` through `config.set` and shows where each value comes from; one window for each machine. The section Phone shows the phone listener, "Publish with Tailscale", the paired phones with "Send a test notification" when a phone has push, and "Pair a phone"; the palette command "Pair a phone" opens it |

`interaction.reveal` shows a card without focusing its pane, because focusing the pane would
release a held hook.
Notifications, the tray and the hotkey are built and checked on macOS only.

## Not built

- The updater, Developer ID signing and notarization.
- A DMG for Intel Macs.
- Notifications on Linux and Windows, and checks of the other surfaces there.
- The resend of unconfirmed input after a reconnect (no remote webview yet).
