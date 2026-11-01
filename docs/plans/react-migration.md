# Replace Solid with React and shadcn/ui

Oct 30, 2026 · @Nikodem Wrona

This plan replaces SolidJS with React in the webview: `packages/ui`, `apps/desktop`, `apps/web` and the Playwright pages under `tests/`. Then it uses [shadcn/ui](https://ui.shadcn.com/) (Radix and Tailwind CSS) for each component where shadcn/ui has an equivalent.

The work has two stages:

- **Stage A: React, 1:1.** The DOM, the class names, the CSS and the API calls stay the same. The Playwright specs pass with no edits, and that proves the port.
- **Stage B: shadcn/ui.** Area by area, the hand-made controls become shadcn components. The DOM and the look of these parts change.

All the work goes directly on `feat/workflows`, into its open PR #11. There is no other branch and no other PR. See "Where the work goes".

## Answers of 2026-10-30

| Question | Answer |
| --- | --- |
| The bundle limits | Raise them: 400 KB for each bundle, 250 KB for the first chunk of the phone |
| Radix | Allowed in `packages/ui` |
| Components | shadcn/ui for every component where possible. `AGENTS.md` gets the rule (text below) |
| React Compiler | Only if WP4 shows too many renders (the recommendation) |
| Sidebar and tab bar | WP9 tries shadcn Sidebar and Tabs. It keeps custom code if the rail of 36 px, the stored width or the drag to reorder do not fit |
| Phone browser | iOS 16.4 or later |
| Where | One PR: the commits go directly on `feat/workflows`, into PR #11 |

## What changes and what stays

| Changes | Stays |
| --- | --- |
| The 77 components in `packages/ui/src` (`*.tsx`, 11,682 lines) | The core, the API, `generated.ts`, `packages/transport`, `packages/proto-wasm` |
| 9 state modules on Solid primitives: `stores/projection.ts`, `app/view.ts`, `app/toasts.ts`, `app/failures.ts`, `app/drag.ts`, `app/context.ts`, `cards/host.ts`, `cards/deliveries.ts`, `phone/context.ts` | The renderer: `render/` and `surface/` (WebGL2, Canvas2D, atlas). They do not import Solid |
| The build: `vite-plugin-solid` becomes `@vitejs/plugin-react`, and `@tailwindcss/vite` comes in Stage B | The pure modules and their `bun test`: `*/model.ts`, `input/`, `keymap/`, `layout/tree.ts`, `stores/reducer.ts` |
| `kit/` and `kit.css` become `components/ui/` (shadcn) | The thin-presentation rule |
| The lint rules: the allowed imports, `no-inner-html`, the hooks rules | The pane, the layout splits, the board, the canvas of the Workflows view and the rich content: they stay custom (see Stage B) |
| The bundle limits, and the docs that name Solid or the limits | |

## The facts today

Measured on `feat/workflows` on 2026-10-30.

| Solid API | Files |
| --- | --- |
| `Show` | 61 |
| `createSignal` | 48 (206 calls) |
| `For` | 44 |
| `onCleanup` | 26 |
| `createEffect` | 24 (44 calls) |
| `onMount` | 17 |
| `on` | 16 |
| `createMemo` | 15 |
| `Switch`, `Match` | 9 |
| `createContext`, `useContext`, `splitProps` | 3 |
| `createStore`, `reconcile` | 1 (`stores/projection.ts`) |
| `render`, `createRoot` | 2 (`app/mount.tsx`, `phone/mount.tsx`) |

| JSX that is different in React | Lines | Files |
| --- | --- | --- |
| `class=` | 757 | 73 |
| `classList` | 57 | 33 |
| `ref=` | 61 | 37 |
| `onChange` (the native `change` event in Solid) | 32 | 9 |
| `style={{…}}` (`workflows/Canvas.tsx` has kebab-case keys) | 20 | 12 |
| `keyed` on `Show` | 13 | |
| `popover` | 21 | 6 |
| `for=` | 4 | 4 |
| `innerHTML` | 2 | 1 (`rich/RichHtml.tsx`) |

The UI uses no `Portal`, `Index`, `Dynamic`, `Suspense`, `ErrorBoundary`, `lazy` or directives.

## The bundle

`scripts/check-bundle.ts`, gzipped bytes:

| Bundle | Now | Old limit | New limit | React (estimate) | React and shadcn (estimate) |
| --- | --- | --- | --- | --- | --- |
| Desk | 172,078 | 300,000 | 400,000 | about 225,000 | about 300,000–330,000 |
| Phone, all files | 180,486 | 300,000 | 400,000 | about 235,000 | about 310,000–340,000 |
| Phone, first chunk | 96,053 | 150,000 | 250,000 | about 150,000 | about 200,000–230,000 |

The estimate: `react` and `react-dom` add about 55–60 KB. The Radix primitives in use and their shared parts (Floating UI, focus scope, scroll lock) add about 40–50 KB. Sonner, `tailwind-merge`, `cmdk`, the icons and the Tailwind classes add about 30–40 KB. The Solid runtime (7–10 KB) and `kit.css` go away. WP0 and WP10 measure the real numbers.

WP2 raises the limits, because Stage A alone puts the first chunk of the phone at the old limit.

## Decisions

| Question | Decision | Why |
| --- | --- | --- |
| Version | React 19, the latest 19.x, with an exact pin as `solid-js` has now. `react-dom/client` | `ref` as a prop, ref cleanups, `useSyncExternalStore`. The version that shadcn/ui uses |
| How | On `feat/workflows`: first a framework-neutral state layer while Solid still renders (WP1). Then the 1:1 port (WP2–WP5). Then shadcn/ui area by area (WP6–WP10) | Solid and React cannot share one JSX transform in a package. The state and the effects are the hard part of the port, and they stay when shadcn replaces the markup. The 1:1 port keeps the specs as a safety net |
| State | Own small stores with `subscribe` and `get`, read through `useSyncExternalStore`. No Redux, Zustand, Jotai or signal library | The reducer already makes immutable states and keeps the object of each row that did not change. The state stays testable in `bun test` without React |
| Fine-grained updates | A selector for each row, and `memo` on the row components: board strip, card, pane box, tab, file row, node | Solid changed one text node. React renders a component again. A row that did not change keeps its object, so `memo` skips it |
| React Compiler | Not now | It needs Babel in the Vite build. WP4 measures first |
| `StrictMode` | Off | `StrictMode` runs each effect two times in development. A second `render.attach` of a pane loses its screen, and the core does not send it again (`pane/Surface.tsx`) |
| Events | JSX handlers for clicks, pointer and forms. Native `addEventListener` for the input of a pane | React's `onBeforeInput` is a synthetic event from `keypress` and `textInput`. It has no `inputType` and no event for `deleteContentBackward`. The typing on a phone needs the native event |
| Components | shadcn/ui, added with its CLI to `packages/ui/src/components/ui/`. Radix through the `radix-ui` package | The answer of 2026-10-30. The CLI default folder, so that `bunx shadcn add` works with no change |
| Styles | Tailwind CSS 4 through `@tailwindcss/vite`, and `tw-animate-css`. The shadcn theme variables come from `styles/tokens.css` | shadcn/ui needs Tailwind. The colors, fonts and the glass look of neohar stay in the tokens |
| The old CSS files | They stay for the parts that stay custom, in the cascade layer `components`. A file loses its rules when its part moves to shadcn | No rewrite of the layout CSS for its own sake. Unlayered CSS beats every Tailwind class, so the old files go in a layer below `utilities` |
| Icons | `lucide-react`, as shadcn/ui uses. `shell/icons.tsx` stays for the icons that Lucide does not have | The shadcn components import Lucide |
| Toasts | Sonner. `app/toasts.ts` keeps the text of each delivery, and Sonner shows it | The shadcn toast |
| Import alias | `@/` = `packages/ui/src`, in each tsconfig and Vite config that compiles `packages/ui` | The shadcn CLI writes `@/lib/utils` and `@/components/ui/…` |
| `classList` | `cn()` in `lib/utils.ts`. A small own function in Stage A, the shadcn `cn` (`clsx` and `tailwind-merge`) in WP6 | The call sites do not change between the stages |
| Phone browser | Safari 16.4 or later (now 16) | Tailwind CSS 4 needs it. An iPhone on iOS 16.0–16.3 cannot open the phone app |
| Limits | 400 KB for each bundle, 250 KB for the first chunk of the phone, from WP2 | The answer of 2026-10-30 |
| Commit type | `refactor(ui)` for Stage A. In Stage B, `feat(ui)` for a commit that changes what you see or do, else `refactor(ui)`. `build(deps)` for a change of the dependencies only | "A change of the window only is not a breaking change" (CONTRIBUTING.md) |

## Stage A: React, 1:1

### The state layer (WP1)

`lib/value.ts` has no framework in it:

```ts
/** A value that the page reads, and that a listener follows. */
export interface Value<T> {
  get(): T;
  set(next: T): void;
  subscribe(listener: () => void): () => void;
}

export function createValue<T>(initial: T, equal: (a: T, b: T) => boolean = Object.is): Value<T>;
```

- **Projection stores.** `ProjectionStore<V>` gets `get(): ProjectionState<V>` and `subscribe()`. The state is the `current` of the reducer: no `reconcile`. `rows()` keeps its array for each state, so that a selector gets the same array until the next message.
- **View state and the other modules.** Each `createSignal` of `app/view.ts` becomes a `Value`, with the same names. `toasts`, `failures`, `drag` and `deliveries` do the same. `deliveries` follows the stores with `subscribe`, not with `createEffect`.
- **Solid in the meantime.** In WP1 the Solid components read the new values through one bridge (`from()` of `solid-js`), so no component changes. The projection stores keep their Solid `state` beside `get()` until WP3. WP3 deletes the bridge and the Solid `state`.
- **Tests.** `app/view.test.ts`, `app/toasts.test.ts`, `app/failures.test.ts` and `app/drag.test.ts` test the `Value` API with no framework.

The hooks come in WP3 (`lib/hooks.ts`):

```ts
/** The current value. The component renders again when it changes. */
export function useValue<T>(value: Value<T>): T;
/** A part of a store. The component renders again only when the part changes. */
export function useSelect<S, T>(source: Source<S>, pick: (state: S) => T, equal?: (a: T, b: T) => boolean): T;
```

`useSelect` keeps the last pick for each state, so a pick that makes a new array does not loop.

A trade-off: `reconcile` kept the object of a row that the core sent again with equal content. `afterPatch` makes a new object for each `set`, so that row renders again. If WP4 shows too many renders, `afterPatch` can keep the old object when the new one is deep-equal.

### Port rules (WP3)

| Solid | React | Note |
| --- | --- | --- |
| `createSignal` | `useState`, or `useRef` when nothing renders from it | |
| `createMemo` | An expression in the body. `useMemo` only for a cost or for an object that a `memo` child needs | |
| `createEffect(fn)` | `useEffect(fn, deps)`, with the deps that Solid tracked | The hooks lint finds missing deps |
| `createEffect` that measures or writes the DOM before the paint | `useLayoutEffect` | The field of `Surface`, `Scrollbar`, `Floating`, `board/fit` |
| `on(x, fn, { defer: true })` | `useEffect` on `[x]` that skips its first run | 16 files |
| `onMount`, `onCleanup` | `useEffect(() => …, [])` and its cleanup | |
| `untrack` | A ref, or a value that is not in the deps | 2 places in `Surface` |
| `<Show when fallback>` | `when ? a : b` | |
| `<Show keyed>` | The element with a `key` | A new key makes a new component, as `keyed` did |
| `<For each>` | `.map()` with `key` = the id of the row | Never the index for panes, strips, cards, tabs and nodes |
| `<Switch>`, `<Match>` | `if` or `switch` in a small function | |
| `class`, `classList` | `className`, `cn()` | Codemod |
| `for` | `htmlFor` | Codemod |
| `style={{ 'background-size': … }}` | `style={{ backgroundSize: … }}`. The `--x` keys stay | Codemod |
| `ref={el}`, `ref={(el) => …}` | `useRef`, or a callback ref | |
| `onChange` on a text field (native `change`) | `onBlur` and Enter, or a native `change` listener | React's `onChange` fires for each key. `SettingsView` would write the config for each key |
| `onChange` on a checkbox, a radio or a select | `onChange` | The same event |
| `onInput` | `onInput` | React has it |
| `innerHTML` | `dangerouslySetInnerHTML` | Only in `rich/RichHtml.tsx`. The lint keeps it there |
| `JSX.Element` | `ReactNode` | |
| `splitProps` | Rest destructuring | |
| `props.x` read in a handler | The value of this render. A handler that needs the latest value reads a ref | A Solid component runs one time. A React component runs at each render |

A one-off codemod (not committed) does the mechanical part first: `class`, `for`, `classList`, the kebab-case style keys and `JSX.Element`. Then the reactive part is done by hand, file by file.

The order of the port, leaves first:

1. `lib/hooks.ts`, `lib/utils.ts`, `kit/` (6), `rich/` (3), `shell/icons.tsx`, `agents/AgentBadge.tsx`, `files/FileIcon.tsx`
2. `cards/` (6), `drawer/` (2), `timeline/` (2)
3. `pane/` (3), `layout/` (4)
4. `files/` (4), `changes/` (1), `git/` (8)
5. `board/` (3), `palette/` (2), `shell/` (16)
6. `workflows/` (6), `settings/` (2)
7. `phone/` (5), `app/mount.tsx`, `phone/mount.tsx`

After step 1, the areas are separate folders. Two or three agents can port them at the same time.

### Places that need care

1. **The pane surface** (`pane/Surface.tsx`, 568 lines). `render.attach` runs one time for each `render_id`: its effect depends only on `renderId`, and it reads the size from a ref. The `<textarea>` gets native listeners for `keydown`, `keyup`, `compositionend`, `paste`, `beforeinput` and `input` in one effect. `test:keyboard` and `test:phone` prove it.
2. **The layout tree** (`layout/LayoutView.tsx`, `SplitView.tsx`, `PaneBox.tsx`). The keys are the pane id and the path of the split. A change of the tree must not mount a pane again when the pane stays at its place. A pane that moves to another split mounts again, as in Solid: check that its screen comes back.
3. **Focus.** The effect that moves the DOM focus to the focused pane depends on `focused` and on the overlays (palette, close question, rename). Check each overlay.
4. **Hot projections.** `panes`, `attention` and `agents_for` change often while agents write. WP4 counts the renders with the React Profiler during `test:gpu`.
5. **Timers.** The lint keeps timers in `lib/timing.ts`. An effect with a timeout uses `realTimers`, as now.
6. **Mounts.** `mountApp`, `mountSettings` and `mountPhone` keep their signatures. `createRoot` of `react-dom/client` replaces `render`, and `dispose()` calls `root.unmount()`. The pages in `apps/` and `tests/` do not change.

## Stage B: shadcn/ui

### The foundation (WP6)

1. **Specs first.** Change the class selectors of the Playwright specs to roles, labels and `data-testid` (`getByRole`, `getByLabel`), on the DOM of Stage A. They must pass before a part changes.
2. **Tailwind.** `@tailwindcss/vite` in the two Vite configs and in `tests/ui/serve.ts`. Check that it works with `css.transformer: 'lightningcss'`. The phone targets Safari 16.4.
3. **The theme.** Map the shadcn variables (`--background`, `--foreground`, `--card`, `--popover`, `--primary`, `--secondary`, `--muted`, `--accent`, `--destructive`, `--border`, `--input`, `--ring`, `--radius`, `--sidebar-*`) to the tokens of `styles/tokens.css`, for each theme of the tokens. Put them on `:root`: Radix shows its layers in a portal under `<body>`, outside the root of the app.
4. **The cascade.** `@layer theme, base, components, utilities`. The old CSS files go in `components`, so the Tailwind classes win over them and they win over the preflight. Move the element rules of `base.css` that fight the preflight.
5. **The CLI.** `components.json` in `packages/ui`, the `@/` alias, `lib/utils.ts` with the shadcn `cn`.
6. **The kit.** `kit/` becomes Button, Badge, Input, Radio Group, Popover and Sonner. Delete `kit/` and `kit.css`.
7. **The lint.** `neohar/ui-imports` allows `react`, `react-dom/client`, `radix-ui`, `class-variance-authority`, `clsx`, `tailwind-merge`, `lucide-react`, `cmdk`, `sonner` and `@/…`. A file in `components/ui/` passes the same rules as the rest: change the file, do not turn a rule off.
8. **The rules.** The `AGENTS.md` and `CONTRIBUTING.md` text below, and the budget line of `.claude/skills/pr-review/SKILL.md`.

### What becomes shadcn

| Part now | shadcn/ui | Note |
| --- | --- | --- |
| `kit/Button` | Button | WP6 |
| `kit/Badge`, `agents/AgentBadge.tsx` | Badge | WP6 |
| `kit/Input`, `shell/RenameField.tsx` | Input | WP6 |
| `kit/Choice`, the options of `cards/QuestionCard.tsx` | Radio Group | WP6 |
| `kit/Toaster` | Sonner | WP6 |
| `kit/Popover` | Popover | WP6 |
| `shell/CloseConfirm.tsx`, `git/GitConfirm.tsx`, `git/TrustPrompt.tsx` | Alert Dialog | WP7 |
| `files/FileModal.tsx`, `workflows/ExecuteDialog.tsx` | Dialog | WP7 |
| `git/Menu.tsx`, the ⋯ menu of `layout/PaneBox.tsx`, the menu of a board strip | Dropdown Menu | WP7 |
| `shell/PressMenu.tsx` (long press) | Context Menu | Radix opens it on a long press. WP7 |
| `palette/Palette.tsx` | Command Dialog, Kbd | `shouldFilter={false}`: the core gives the results and their order. WP7 |
| `workflows/NodePicker.tsx` | Popover and Command | `shouldFilter={false}`. WP7 |
| The tips of icon buttons (`title`) | Tooltip | WP7 |
| `shell/PhoneSheets.tsx` | Sheet | WP7 |
| `settings/*`, `workflows/NodeDetails.tsx`, the box "remember" of `layout/PaneBox.tsx`, `git/GitChanges.tsx` | Field, Label, Input, Textarea, Select, Switch, Checkbox | WP8 |
| `cards/CardFrame.tsx` and the cards | Card | WP8 |
| The tabs of the drawer, the sections of the settings | Tabs | Controlled by `view`. WP9 |
| `shell/FailureBanner.tsx`, `HealthBanner.tsx`, `LinkBanner.tsx` | Alert | WP9 |
| `pane/KeyBar.tsx` | Toggle Group, Button | Ctrl and Alt are sticky toggles. WP9 |
| `shell/StatusBar.tsx` | Button, Badge, Tooltip | WP9 |
| `shell/Sidebar.tsx` | Sidebar (try) | Keep custom code if the rail of 36 px or the stored width do not fit. WP9 |
| `shell/TabBar.tsx` | Tabs (try) | Keep a custom bar of shadcn Buttons and a Context Menu if the drag to reorder or the rename do not fit. WP9 |
| Lists that scroll, with no virtual rows | Scroll Area | Not in `git/VirtualDiff.tsx`. WP9 |

These parts stay custom, with shadcn controls inside them:

- `pane/Surface.tsx` and `pane/Scrollbar.tsx`: the renderer owns them.
- `layout/`: the core owns the ratios of the splits. Resizable keeps its own sizes, and that breaks the thin-presentation rule.
- `board/`, `timeline/`, `rich/`, the tree of `files/`, `git/DiffPane.tsx`, `git/VirtualDiff.tsx`, `workflows/Canvas.tsx`: shadcn/ui has no equivalent.

### The rule for AGENTS.md (WP6)

In "Scope and fixed choices":

> - **UI:** React and TypeScript, shadcn/ui on Radix and Tailwind CSS, Vite, Bun.

In "Conventions", in place of the bullet "UI components":

> - **UI components:** use [shadcn/ui](https://ui.shadcn.com/) for each component that it has. To add one, run `bunx shadcn@latest add <name>` in `packages/ui`. The command writes it to `packages/ui/src/components/ui/`. Before you add a component, look in `components/ui/` for one that you can use or extend. Do not make a button, a field, a dialog, a menu or another control by hand when shadcn/ui has it. When shadcn/ui has no equivalent, make the component from shadcn components and Radix primitives. Use Tailwind classes and the theme variables of `styles/tokens.css`. You can change a file in `components/ui/` for neohar, but keep its props. A component shows the results and the order that the core gives: do not filter or sort in it (for example, set `shouldFilter={false}` on `Command`). `neohar/ui-imports` allows only the packages of shadcn/ui: `radix-ui`, `class-variance-authority`, `clsx`, `tailwind-merge`, `lucide-react`, `cmdk` and `sonner`. A new package needs a change of that list.

## Work packages

| WP | Content | Size | Done when |
| --- | --- | --- | --- |
| 0 Spike | A throwaway worktree of `feat/workflows`, not committed: React and Tailwind in the two Vite builds, `Surface.tsx`, one card and a shadcn Dialog | S | The bundle numbers of React and of React with shadcn are known. The phone typing and a card answer work. Tailwind builds for Safari 16.4 with Lightning CSS |
| 1 Neutral state | `lib/value.ts`, `get` and `subscribe` on the projection stores, the 8 other state modules on `Value`, the Solid bridge, their tests | M | `mise run check`, `mise run test`, `test:shell` and `test:phone` pass on `feat/workflows` with Solid |
| 2 Tooling | The dependencies of `packages/ui`, `apps/desktop`, `apps/web` and `tests/ui`. The Vite plugin in the two configs and `tests/ui/serve.ts`. `"jsx": "react-jsx"` in the tsconfigs. ESLint: the allowed imports, `eslint-plugin-react-hooks` (both rules as errors), `no-inner-html` on `dangerouslySetInnerHTML`, a ban on `onBeforeInput`. The notices. The new limits in `check-bundle.ts`, its test and the description of `check:bundle` in `mise.toml` | S | One commit with WP3: the tools and the components do not work without each other |
| 3 Components | The port in the order above. The mounts. Delete the bridge and the Solid `state` | L | `tsc`, ESLint and `bun test` pass. `mise run app:dev` opens the window |
| 4 Verify | Every suite of "Tests", the Profiler, the bundle, a day of real use | M | Every row of "Tests" passes |
| 5 Docs | Solid becomes React in the pages of "Docs to change". The kit bullet of `AGENTS.md` says React until WP6 | S | `mise run check:docs` passes |
| 6 shadcn foundation | The steps of "The foundation" | M | The specs pass. Only the kit parts look different. `mise run check` passes |
| 7 Overlays | Dialog, Alert Dialog, Dropdown Menu, Context Menu, Tooltip, Command, Popover, Sheet | M | `test:shell`, `test:phone` and `test:keyboard` pass. Esc closes the layer, and the focus goes back to the pane |
| 8 Forms and cards | Card, Field, Label, Input, Textarea, Select, Switch, Checkbox, Radio Group | M | A card answer and a settings edit work on the desk and on the phone |
| 9 Chrome | Tabs, Alert, Toggle Group, Sidebar (try), the tab bar (try), Scroll Area, the status bar | M | The suites pass. Screenshots of the desk and the phone are in the description of #11 |
| 10 Clean up | Delete the CSS rules and the native `popover` code that nothing uses. Measure the bundle. The guide pages about the look | S | `check:bundle` passes. No old CSS rule is without a user |

## Where the work goes

All the work goes directly on `feat/workflows`, into its open PR #11. Make no other branch and no other PR.

- **The order of the commits.** First commit the work in progress (the kit, the toasts and the card changes) and this plan. Then one commit for each WP, in the order of the table. WP2 and WP3 are one commit.
- **Each commit works.** After each commit, `mise run check` and `mise run test` pass, and the app opens.
- **Checkpoints.** The commit of WP1 (Solid on the neutral state) and the commit of WP5 (the end of Stage A) are the points to go back to with `git bisect` or `git revert`.
- **Commit messages.** Each commit follows the commit rules: `refactor(ui)`, `feat(ui)`, `build(deps)` or `docs`. Squash each `fixup!` commit before a push.
- **The description of #11.** It gets a section for this plan: the WPs and their commits, the bundle numbers, and the screenshots of Stage B.

## Tests

| Check | What it proves |
| --- | --- |
| `mise run check` | Types, lint with the hooks rules, notices, bundle limits |
| `mise run test` | The state modules and the pure models, with no change and no React |
| `test:shell`, `test:shell:link` | The DOM follows the projections. A click sends its command. axe |
| `test:phone` | Pairing, desk, cards, reply, sheets, and the typing through `beforeinput` |
| `test:keyboard` | The keyboard matrix through the `<textarea>` |
| `test:gpu` | Renderer equality, context loss, no allocation, frame budgets. The chrome must not take the frame time of the panes |
| `test:host` | The sandbox and the CSP. React needs no `unsafe-eval`. The two CSPs already allow inline styles, which Radix and Sonner add |
| `test:transport` | A smoke test. The path does not change |
| Real use | `/install-app`, then a day with Claude Code and Codex panes: approvals, cards, the board, a restart that restores the screens. After Stage A and after WP9 |

In Stage A the DOM stays, so the specs must pass with no edits. A spec that needs an edit shows a change of the DOM: examine it before you change the spec. In Stage B, WP6 moves the specs to roles and labels first.

New specs:

- A settings text field writes the config one time, when you commit the value, not for each key (WP3).
- Esc closes each Radix layer, and the focus goes back to the pane that had it (WP7).

## Docs to change

- `AGENTS.md` and `CONTRIBUTING.md`: the UI line and the bullet "UI components" (WP5, then the final text in WP6).
- `README.md`: the diagram, the table of folders and the credits.
- `website/docs/`: `intro.md`, `architecture/overview.md` (also the bundle row), `desktop/overview.md` (also the bundle row), `desktop/ui.md`, `code/index.md`, `code/tools.md`, `testing/overview.md`, `getting-started/setup.md`, `getting-started/development-workflow.md`. `getting-started/mise-tasks.md` is generated: change `mise.toml` and run `bun run gen`.
- `.claude/skills/pr-review/SKILL.md`: the budget line (300 KB, and "a UI dependency beyond `packages/transport` and `packages/proto-wasm`").
- `THIRD_PARTY_LICENSES.md` through `scripts/gen-notices.ts`.
- `tools/eslint-plugin-neohar`: the message of the allowed imports and the tests of both rules.
- The `docs-updater` agent can do WP5 and the docs part of WP6.

## Risks

| Risk | Effect | Mitigation |
| --- | --- | --- |
| Too many renders from hot projections | The chrome takes frame time. `test:gpu` fails | Row selectors and `memo`. The Profiler in WP4. The React Compiler after that |
| Effects with wrong deps | Old values, a second attach, a lost screen | `react-hooks/exhaustive-deps` as an error. A careful review of `Surface` |
| Synthetic events are different | Phone typing, input methods or paste stop working | Native listeners in `Surface`. The lint ban on `onBeforeInput`. `test:keyboard`, `test:phone` |
| Radix and the keymap both read Esc and Tab | A key closes a layer and also goes to the pane, or the pane takes the focus from a dialog | Each open Radix layer sets a flag in `view`. The keymap and the focus effect of `Surface` step back while it is set |
| The Tailwind preflight fights `base.css` | Small changes of the look in the custom parts | The old files in the layer `components`. Screenshots before and after for each Stage B commit |
| Tailwind with Lightning CSS as the Vite transformer | The build fails or loses styles | WP0 checks it |
| The bundle grows past the estimate | `check:bundle` fails at 400 KB or 250 KB | WP0 measures. Load the overlays of the phone with `import()` |
| iOS 16.0–16.3 | The phone app does not open | The decision above. The phone page can show a message for an old Safari |
| `exactOptionalPropertyTypes` with `@types/react` and the shadcn files | Type errors when a prop gets `undefined` | Conditional spreads, as the code does now. Small edits in `components/ui/` |
| A very large PR | #11 has the workflows, the removal of the search, React and shadcn. A review is hard | One commit for each WP. The description of #11 lists the WPs and their commits. Two checkpoints (WP1 and WP5) |
| `main` moves during the work | Conflicts in `packages/ui` | Rebase `feat/workflows` on `main`. No merge commits |
| The work in progress now | `kit/`, the toasts and the card changes are not committed | Commit them on `feat/workflows` before WP1 |

## Out of scope

- React Flow for the Workflows canvas. The canvas port is 1:1. `workflows.md` kept the canvas custom because of the bundle. React Flow is a separate decision with its own budget.
- A rewrite of the custom layout CSS (panes, board, frame) to Tailwind for its own sake.
- A router, a data-fetching library, server components.

