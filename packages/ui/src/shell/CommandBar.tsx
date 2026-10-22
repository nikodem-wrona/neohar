// The command line of the window: the top row of the desk. Its search field, in the middle of
// the row, opens the palette in its place, so the palette reads as a field that is
// always there. A person types commands, tabs, panes and handles in it. On the right, the attention line says how many agents wait, in an amber pill, and
// what the first one asks, in the order of the core; a click answers it as ⌘⇧A does. With no wait
// the line is empty. A first row without an
// interaction (`error`, `rate_limited`, `turn_ended`) shows its pane and its state, and a click
// shows the pane, in any tab. The line shows the handle of the pane only with `ui.show_handles`.

import { Show } from 'solid-js';
import { useApp } from '../app/context';
import { showHandles } from '../app/preferences';
import { showPane } from '../app/show';
import { troubleOf, troubleText } from '../drawer/group';
import type { Keymap } from '../input/chords';
import { type ActionContext, runCommand } from '../keymap/actions';
import { SearchIcon } from './icons';
import { chordText } from './keys';

export function CommandBar(props: { actions: ActionContext; keymap: () => Keymap }) {
  const { stores, view, platform, transport, report, windowId, form } = useApp();
  const count = () => stores.attention.state.order.length;
  const handles = () => showHandles(stores.settings.state.byKey);
  /** The interaction that waits first: the first interaction of the first row of `attention`. */
  const first = () => {
    const id = stores.attention.rows()[0]?.interactions[0];
    return id ? stores.interactions.state.byKey[id] : undefined;
  };
  /** The first row when it has no interaction: a run in `error`, `rate_limited` or `turn_ended`. */
  const trouble = () => {
    const row = stores.attention.rows()[0];
    const state = row ? troubleOf(row) : null;
    if (!row || !state) return undefined;
    const pane = stores.agents.state.byKey[row.run]?.pane ?? null;
    return { pane, text: troubleText(state) };
  };
  const paneHandle = (pane: string | null | undefined) =>
    pane ? stores.panes.state.byKey[pane]?.handle : undefined;
  const paletteChord = () => chordText(props.keymap(), 'palette.open', platform);
  const answerChord = () => chordText(props.keymap(), 'interaction.answer_next', platform);
  return (
    // A press on a part of the line that is not a control moves the window, and a double click
    // zooms it, when the window has no title bar. The attribute does nothing in a browser.
    <header class="commandbar" data-tauri-drag-region="deep">
      {/* A phone opens the left column as a sheet from this button. */}
      <Show when={form === 'phone'} fallback={<span class="brand">neohar</span>}>
        <button
          type="button"
          class="phone-workspaces"
          aria-label="Workspaces"
          aria-pressed={view.leftOpen()}
          onClick={() => {
            view.setSheet(null);
            view.setLeftOpen((open) => !open);
          }}
        >
          <svg viewBox="0 0 16 16" width="16" height="16" aria-hidden="true">
            <path d="M2 4h12M2 8h12M2 12h12" stroke="currentColor" stroke-width="1.5" />
          </svg>
        </button>
      </Show>
      <button
        type="button"
        class="cmdline"
        title="Open the command palette"
        aria-haspopup="dialog"
        aria-expanded={view.paletteOpen()}
        onClick={() => view.setPaletteOpen(true)}
      >
        <SearchIcon />
        <span class="cmdline-hint">Search or run a command</span>
        <Show when={paletteChord()}>{(chord) => <kbd class="cap">{chord()}</kbd>}</Show>
      </button>
      <div class="ticker" aria-label="Needs attention" role="status">
        <Show when={count() > 0}>
          <span class="badge">{count()} waiting</span>
        </Show>
        <Show
          when={first()}
          fallback={
            <Show when={trouble()}>
              {(row) => (
                <button
                  type="button"
                  class="ticker-next"
                  title={row().text}
                  disabled={row().pane === null}
                  onClick={() => {
                    const pane = row().pane;
                    if (pane) showPane(transport, windowId, pane).catch(report);
                  }}
                >
                  <Show when={handles()}>
                    <span class="handle">{paneHandle(row().pane) ?? '?'}</span>
                  </Show>
                  <span class="ticker-title">{row().text}</span>
                  <span class="ticker-action">Open</span>
                </button>
              )}
            </Show>
          }
        >
          {(waiting) => (
            <button
              type="button"
              class="ticker-next"
              title={waiting().title}
              onClick={() => runCommand('interaction.answer_next', props.actions)}
            >
              <Show when={handles()}>
                <span class="handle">{paneHandle(waiting().pane) ?? waiting().handle}</span>
              </Show>
              <span class="ticker-title">{waiting().title}</span>
              <Show when={answerChord()}>
                {(chord) => <kbd class="cap cap-wait">{chord()}</kbd>}
              </Show>
              <span class="ticker-action">Answer</span>
            </button>
          )}
        </Show>
      </div>
    </header>
  );
}
