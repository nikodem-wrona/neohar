---
title: The window
description: "What a neohar window shows and how packages/ui builds it: the layout of the shell, the stores, panes and drags, the drawer and its cards, the surface renderer, keys and focus, theme and accessibility."
sidebar_position: 2
---

A window is the React shell of [`packages/ui`](https://github.com/nikodem-wrona/neohar/tree/main/packages/ui),
mounted by `mountApp` with a transport and the id of a row of `windows`. It paints the projections
of the core and sends a command for each action of the person
([thin presentation](/docs/desktop/overview)). For how to use it, see
[workspaces and panes](/docs/guides/workspaces-and-panes).

## Layout

```text
┌ command line ···· [⌕ search field: palette ⌘K] ··········· attention: 1 waiting  ⇧⌘A Answer ┐
├ board ────────────────────────────────────────────────────────────────────────────────────┤
│▌● WORK work of a run  │▌● WAIT work of a run  │▌○ IDLE work of a run  │ +3                │
│▌  Claude Code   $0.42 │▌  Codex         $1.10 │▌  Claude Code   $0.08 │                   │
├ left column ─────┬ tab bar ────────────────────────────────────┬ drawer ──────────────────┤
│ workspaces       │ ┌ pane ───────────┬ pane ───────────┐       │ Inbox Timeline Files     │
│  ▸ tabs          │ │ title strip     │                 │       │ Changes                  │
│ New workspace    │ │ surface (grid)  ├ pane ───────────┤       │ cards, tree, diffs       │
│                  │ └─────────────────┴─────────────────┘       │                          │
├──────────────────┴─────────────────────────────────────────────┴──────────────────────────┤
│ status bar: keys · limits · warnings · machine · git · workflows                          │
└───────────────────────────────────────────────────────────────────────────────────────────┘
```

| Part | Code | What it shows |
|---|---|---|
| Command line | `shell/CommandBar.tsx` | On macOS also the title bar: the window buttons lie over its left end, and a press on its empty parts moves the window. A search field in the middle that opens the palette in place, and the attention line: the count of `attention` as an amber Badge and the first interaction; a click answers it as `⌘⇧A` does. With no wait the line is empty. The field and the attention line are Buttons with key caps |
| Board | `board/` | A strip for each agent run, in the order of the core: a card with the border and tint of its state, the badge, the state in four letters and the work, then the harness and the cost. A click shows the pane in any tab and workspace, and a right click opens a menu with Show the pane, Show in Git view and Execute a workflow here. A drag of a strip onto another strip puts its run at that place (`agent.move`); the core keeps the order for every window and across a restart, and a new run in a pane takes the place of the run that ended there. When every run fits, the strips share the board up to 48 characters. The runs that do not fit are behind `+N` (`board/fit.ts`), whose popover lists every agent and turns amber when one of them waits. The board is two lines high, or one when it is narrower than 100 characters; a new agent never changes its height |
| Left column | `shell/Sidebar.tsx` | The workspaces and their tabs. It folds to a rail, as the drawer does, and under 1000 px it starts folded. A rail button is an icon (`shell/icons.tsx`) with its name as the tooltip. It is not the Sidebar of shadcn/ui. That Sidebar is fixed to the edge of the window and has widths in rem and its own state. Thus it does not fit the grid of the shell, the width in characters and the rail of 36 px. The controls of the column are Buttons with Tooltips |
| Tab bar and layout | `shell/TabBar.tsx`, `layout/` | The tabs of the workspace and the layout tree of the shown tab: splits, floating panes, zoom. Each tab is a Button with `role="tab"`, not the Tabs of shadcn/ui. Radix Tabs selects a tab on a press and when an arrow key moves the focus. That breaks the drags and the selection through the core. A right click on a tab opens a Context Menu with Rename, Move left, Move right and Close, the same items as the long press on a phone |
| Git view | `git/` | It takes the place of the tab bar and the layout (`⌘⇧G`): the Repository, Worktree and Branch menus, the changes or the history, and a diff. Both side columns fold while it shows, and the focus report has `panes_hidden`, so the core sends no frames |
| Workflows view | `workflows/` | It takes the same place as the Git view (`⌘⇧E`), and the two never show at the same time: the workflows and the executions, the canvas of a workflow, the node details and the panel of an execution. It loads as a lazy chunk when it opens the first time. See [Workflows](/docs/guides/workflows) |
| Drawer | `shell/Drawer.tsx` | Four tabs (below), the Tabs of shadcn/ui that `view.drawerTab` controls. Inbox, Timeline and Changes scroll in a Scroll Area. Under 1280 px it starts folded to a rail. It never covers a pane |
| Status bar | `shell/StatusBar.tsx` | Function keys (the reserved chords as Buttons with gray key caps, `shell/Cap.tsx` on Kbd; Answer is amber while an agent waits), harness limits, health warnings, `canvas2d` after a fallback, the machine, and after a rule the buttons of the Git view and the Workflows view in the group "Views". The limits, the renderer, "Next key goes to the pane" and the count of red warnings are Badges. The text of the usage and of the warnings is in Tooltips. The Git and Workflows buttons are pressed while their view shows. The left column and the drawer fold and open from their arrows and rails. The Workflows button has a dot while an execution is paused or waits for the person |
| Banners | `shell/FailureBanner.tsx`, `shell/HealthBanner.tsx`, `shell/LinkBanner.tsx` | Alerts of shadcn/ui: a command of the window that failed; degraded storage and doctor notes; a link to a remote machine that is down (input is off while it is) |
| Toasts | `app/toasts.ts`, `shell/Toasts.tsx` | Short messages in the bottom right corner, such as an answer that arrived, in the Toaster of shadcn/ui (Sonner). `app/toasts.ts` keeps the text, the time and the limit of 3. Each one goes after 5 s, but not while the pointer is on the toasts |
| Layers | `palette/`, `shell/CloseConfirm.tsx`, `shell/PressMenu.tsx`, `shell/PhoneSheets.tsx`, `shell/Tip.tsx` | The layers of shadcn/ui on Radix over the window (see below) |

**Layers.** Each layer over the window is a component of shadcn/ui on Radix:

| Component | Where |
|---|---|
| Command Dialog | The palette (`⌘K`) |
| Alert Dialog | The close confirmation (`shell/CloseConfirm.tsx`), the question of the Git view before a switch or a pull (`git/GitConfirm.tsx`) |
| Dialog | The large view of a file (`files/FileModal.tsx`), the execute dialog and the node details of the Workflows view, the pairing of a phone |
| Dropdown Menu | The menu of a Files row, the menu of a long press (`shell/PressMenu.tsx`), the `⋯` menu of a pane on a phone |
| Context Menu | The menu of a board strip and the menu of a tab (right click) |
| Popover with Command | The Repository, Worktree and Branch menus of the Git view, the node picker of the Workflows view, the list behind `+N` of the board |
| Sheet | The sheets of the phone. They are not modal, and they go into `.shell`, not under the body |
| Tooltip | The name of an icon button (`shell/Tip.tsx`, with one `TooltipProvider` in the shell) |

A Command never filters or sorts (`shouldFilter={false}`): the lists keep the order of the core
and of the models. A button hides its tooltip while its own layer is open, so that Escape closes
the layer and not the tooltip. The trust prompt of the Git view is not a layer: it stays in its
tab.

The widths of the left column and the drawer are in characters; a drag of the rule changes them,
and the page keeps them in local storage because they are not business state.

Each region is a panel with round corners, with a gap of 8 px between them. The board is not
a panel: its strips lie on the ground, with no border or padding around them. The canvas of
the renderer lies under the chrome and paints the ground, the main panel and the background of each
pane with its round corners (`surface/gl/shaders.ts`, `render/host.ts`). A pane has a padding of
8 px around its cells. The title strip of the pane in focus is one step lighter. The strip
of its agent on the board has a lighter fill and keeps the colour of its state, so a wait stays
louder than the focus.

**Showing a pane.** `pane.focus` takes only a pane that the window draws. A click on a
strip of the board, on Open of a notice or an inbox line or on the attention line therefore
runs the palette command `go.pane:<id>` (`app/show.ts`): the window shows the tab
of the pane, in any workspace, and the pane gets the focus.

## Stores

`createStores` makes one store for each projection the shell reads: `windows`, `tree`,
`panes`, `attention`, `interactions`, `agents_for:<window>`, `harnesses`, `jobs`, `limits`,
`health`, `link` and `settings`. A store applies snapshots and patches with a pure reducer and keeps the order of the core. Some subscriptions open only while they are needed:
`pane_text:<pane>` for the text mirror, `timeline:<run>` while the Timeline shows, and
`changes_for:<window>` and `changes_head:<window>` while Changes shows, and `git_head:<window>`,
`git_worktrees:<window>`, `git_branches:<window>`, `git_changes:<window>` and `git_repos` while the
Git view shows. No store derives data
from another one; a lookup by id is allowed.

A store has `get`, `subscribe` and `rows`. A component reads its rows with `useRows`
(`lib/hooks.ts`), or opens its own store with `useProjectionStore`. The view state of the window
(`app/view.ts`, the drags, the toasts, the failures) is in small values with `get`, `set` and
`subscribe` (`lib/value.ts`), with no framework in them. A component reads a value with `useValue`
or `useSelect`. The window has
no `StrictMode` of React: it runs each effect two times, and a second attach of a pane loses its
screen.

The status bar follows `workflow_executions` for the dot of its Workflows button. The Workflows
view also follows `workflows`, and `pane_text:<pane>` for the screen of a working agent.

## Panes, drags and closing

A pane (`layout/PaneBox.tsx`) is a title strip, the surface with a scrollbar, and strips for
requests. The title strip says what the board does not: the dot of the state, the
handle, `WAIT` or `ERR`, a badge with the mark and the name of the harness (Claude Code or Codex),
a dim `?` for an unvalidated version, a `yolo` tag, the title (not when it only names the
harness), the unconfirmed-input marker, and buttons for the details, float, zoom and close. The
details button (`i`) opens a popover (`components/ui/popover.tsx`, rows from `layout/details.ts`) with the
title, the working directory, the model, the version, the cost, the tokens, the session, and the
handle and size of the pane. The working directory and the session have a copy button. A click on
`i` again, a click outside or Escape closes the popover. Strips in the pane answer
requests that are not interactions of a run: a clipboard write by OSC 52 (Allow, Deny, "Remember
for this pane"), a held multi-line paste into an agent, and an ended program with
its exit code and Restart.

- **Dividers.** A drag shows the new shares at once and sends one `layout.set_ratios` on release.
  A double click equalizes; arrow keys move a focused divider by 0.02.
- **Floating panes** move by the strip and resize by the corner. The page sends at most one
  `pane.set_float_rect` per animation frame and moves the pane only when the projection says so.
- **Drags** of a tab onto a tab (`tab.move`), of a pane strip onto a tab (`pane.move`), and of a
  Files row onto a pane (`pane.insert_paths`) use pointer events (`app/drag.ts`). The drag events
  of HTML never reach the page in the app, because the host's drag handler takes every drag that
  enters the window.
- **Closing.** A cross, a middle click or Delete sends `pane.close`, `tab.close` or
  `workspace.close` without `force`. On `confirm_required` a dialog asks, and "Close" sends the
  call again with `force`.
- **Renaming.** A double click on a workspace or tab name, or F2 on a tab, opens a field; Enter
  sends `workspace.rename` or `tab.rename`.

## The drawer

| Tab | Shows |
|---|---|
| Inbox | The cards of `interactions`, grouped by run in the order of the core, plus a line for each run in `error` or `rate_limited` |
| Timeline | The turns of the agent in the focused pane; "Older items" pages back |
| Files | The tree of the focused pane's working directory, else of the root of the workspace, with file-type icons, a file view, a large view of the file and actions |
| Changes | The git worktree of the Files folder, with a diff per file, read-only |

The how-to for the last two and for the Git view is in
[files, changes and Git](/docs/guides/files-and-changes).

**Cards.** An approval shows the command with highlight spans, why the risk, the directory, the
diff and the last lines of the pane, with Allow, "Allow `<rule>` always" (only when the core sends
`always_rule`), Deny, and Deny and stop. The keys 1 to 4 name the four answers, and a key stays
with its button when "Allow always" is not there. At a high risk Deny is preselected, a number
only selects its button, and only Enter on a button answers. A question shows each option as a
choice with a radio button (a checkbox for a multi-select question), with a field for free text. A choice clears the text and text clears the choices,
because keys type one of the two into the dialog of the harness. A plan review has the plan and a
text area for feedback, and a notice has Open and Dismiss.

A card is a Card of shadcn/ui on its `article` (`cards/CardFrame.tsx`): a header with the title
and the badge, the content and a footer. Each answer is a Button with its key in a Kbd. The answers
that let the agent go on have the colour of the accent (green by default), and Deny and stop is red.
The line of a run in the Inbox is a Card too.

A card never changes itself after a click: it sends `interaction.answer {via: "card"}` and then
shows the state from the projection. A card that is not open is one row, with a badge such as
"Sending", "Answered" or "Not delivered". When the answer of a card arrives, a toast in the bottom
right corner says so (`cards/deliveries.ts`). A line under the card stays only for what the
person must know: "Display only", "Couldn't confirm your answer reached …" or a failure. The
mechanism behind it is in [interactions](/docs/agents/interactions).

**Rich content.** Spans and diffs are drawn as text nodes. Plan Markdown comes from the core as
HTML sanitized with `ammonia`, and `rich/RichHtml.tsx` is the only place that sets `innerHTML`.

## The surface renderer

A window has one canvas behind the chrome and one GL context; every pane is drawn into its own
rectangle with a scissor. `render/host.ts` opens the `render` stream, gives each frame to
the WASM decoder, paints in the next animation frame, and sends one `render.ack` per painted
frame. A new pane size goes to the core as `pane.resize` 50 ms after the last change, and at most
every 100 ms during a drag. WebGL2 is used unless it is missing or software; then Canvas2D.
A lost GL context is rebuilt from the decoder without a keyframe. The frame format, the
atlas and the shaders are in [rendering and input](/docs/architecture/rendering-and-input).

## Keys and focus

A key goes to one of three places:

1. **The native menu** takes `⌘Q`, `⌘⌥Q`, `⌘W` (close window), `⌘N` and `⌘,` before the page.
2. **The keymap** (`keymap/install.ts`) listens in the capture phase and runs the reserved chords
   of the `[keys]` settings ([defaults](/docs/reference/configuration/defaults#keys)), also while
   a dialog or card has the focus. A change of a chord applies at once in every window.
3. **The focused pane** gets every other key through the keyboard layer, as `pane.input`.

`⌘⌥V` lets the next key through to the pane even when it is reserved. `⌘⇧P` is reserved and does
nothing yet. The full list of chords is in [workspaces and panes](/docs/guides/workspaces-and-panes).

**Focus follows the core.** A click or a chord sends `pane.focus`; the lighter strip and the DOM focus move
when `windows` names the new `focused_pane`. The page sends `window.focus_report` (document
focus, visibility, the active pane, the refresh rate), and the host joins it with the native
state. The core counts a pane as focused after 500 ms. That decides the gate: an approval
for an unfocused pane is held as a card or notification, and focusing the pane releases it to the
harness's own dialog ([interactions](/docs/agents/interactions)). Looking at a card never focuses
its pane. While a layer of shadcn/ui (a popover, a menu, a dialog) is open, the pane does not take
the focus back. Radix closes a layer when the focus leaves it. The layer tells the view
with `useLayer` (`app/layer.ts`), and `view.layers` counts the open layers. Escape closes the top
layer, and the focus goes back to the element that had it, or to the pane. A layer that the page
opens with no trigger (the close confirmation, the menu of a long press, the sheets of the phone)
gives the focus back with `useFocusReturn` (`shell/focus.ts`).

## Theme, fonts and accessibility

- **Theme.** The `[theme]` section holds the colours of the chrome; the shell sets them as CSS
  variables (`bg_raised` becomes `--bg-raised`) over the defaults of `styles/tokens.css`. The
  default is black and white, with green for what is live, amber for an agent that waits and red
  for an error; `styles/contrast.test.ts` checks every pair at 4.5:1. The terminal palette comes
  with the frames of each pane.
- **Styles.** The styles are in `packages/ui/src/styles/`, one file for each area. `styles/index.css`
  puts them in cascade layers in this order: `theme`, `reset`, `tokens`, `base`, `layout`,
  `components`, `states`, `phone`, `utilities`. A later layer wins, so the Tailwind utilities win
  over the rules of the chrome. The phone app adds `phone.css` in the layer `phone`
  (`phone/layer.css`).
- **Components.** The controls are shadcn/ui components in `packages/ui/src/components/ui/`
  (alert, alert dialog, badge, button, card, checkbox, command, context menu, dialog, dropdown menu,
  field, input, kbd, label, popover, radio group, scroll area, select, separator, sheet, sonner,
  switch, tabs, textarea, toggle, toggle group, tooltip). The chrome, the cards, the forms, the Git
  view, the Workflows view and the phone use them. The sections of the settings are Tabs (a column
  on the desk, a row that scrolls to the side on a phone). Changes and History of the Git view are
  Tabs, and its lists scroll in a Scroll Area. The lists and the execution panel of the Workflows
  view scroll in Scroll Areas. On the key bar of a phone, Ctrl, Alt and Compose are Toggles. The
  badge has the tones of neohar as variants: neutral (the default), muted, info, ok, warn and
  error. `cn()` in `lib/utils.ts` joins classes with `clsx` and `tailwind-merge`. `@/` is
  `packages/ui/src`.
- **Tailwind CSS 4.** `@tailwindcss/vite` builds it for the desk, the phone and the shell test.
  `styles/theme.css` holds the theme, the utilities and `tw-animate-css`. Its `@theme inline` maps
  the colour names of shadcn/ui to the tokens of `tokens.css`, so a theme of the person changes the
  components too. The names are not variables on the root element, because `--accent` is the
  accent of the chrome that the core sets. There is no Tailwind preflight. The parts of a shadcn/ui
  component (`data-slot`) get a small reset in the layer `reset`. The chrome is dark only, so the
  variant `dark:` always applies. The root font is 13 px, so `theme.css` sets the scale: a step of
  `--spacing` is 4 px, and `text-xs`, `text-sm`, `text-base` and `text-lg` are 11/14, 13/18, 16/24
  and 18/28 px (size/line height).
- **Fonts.** `ui.font_family` and `ui.font_size` set the surface font; the default list starts
  with Hack Nerd Font Mono and falls back to Menlo. The page loads the faces that the host serves
  over `nh://font/` before it measures cells. The data of the chrome (state codes, costs,
  paths, key caps) uses the same family. Titles and text use the face of the system.
- **Text mirror.** The canvas is `aria-hidden`. `shell/Mirror.tsx` follows `pane_text:<pane>` for
  the focused pane in an off-screen `role="log"`, and only its last row is a polite live region.
- **ARIA.** The tab bar and the drawer are tab lists, dialogs are modal with labels, the Files tree
  is an ARIA tree, and dividers are focusable separators. An axe smoke test runs in the shell test
  on every drawer tab and dialog.

## Tests

| Command | What it checks |
|---|---|
| `mise run test:ui` (`bun test`) | Plain modules: the reducer, drags, the divider and float math, the keymap and chords, the focus report, the card model, the Files and Changes models, the canvas of the Workflows view, the renderer helpers |
| `mise run test:shell` | The shell in WebKit against `nh daemon --dev-ws`: the DOM follows the projections, drags, chords, painted pixels, resizes, cards with hostile text, axe |
| `mise run test:gpu` | Both renderers paint equal cells, context loss, no allocation per frame |

The browser suites are not part of `mise run test`: they need `node` and the Playwright browsers. See [local suites](/docs/testing/local-suites).

## Not built

- Pop-out panes in the shell, and machines and groups in the left column.
- The peek of a sibling agent (`⌘⇧P`).
- A light theme and a setting for the terminal palette.
