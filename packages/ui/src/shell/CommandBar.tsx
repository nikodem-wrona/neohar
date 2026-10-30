// The command line of the window: the top row of the desk. Its search field, in the middle of
// the row, opens the palette in its place, so the palette reads as a field that is
// always there. A person types commands, tabs, panes and handles in it. On the right, the attention line says how many agents wait, in an amber pill, and
// what the first one asks, in the order of the core; a click answers it as ⌘⇧A does. With no wait
// the line is empty. A first row without an
// interaction (`error`, `rate_limited`, `turn_ended`) shows its pane and its state, and a click
// shows the pane, in any tab. The line shows the handle of the pane only with `ui.show_handles`.

import { useApp } from '../app/context';
import { showHandles } from '../app/preferences';
import { showPane } from '../app/show';
import { troubleOf, troubleText } from '../drawer/group';
import type { Keymap } from '../input/chords';
import { type ActionContext, runCommand } from '../keymap/actions';
import { useRows, useSelect, useValue } from '../lib/hooks';
import { SearchIcon } from './icons';
import { chordText } from './keys';

export function CommandBar(props: { actions: ActionContext; keymap: Keymap }) {
  const { stores, view, platform, transport, report, windowId, form } = useApp();
  const count = useSelect(stores.attention, (state) => state.order.length);
  const handles = useSelect(stores.settings, (state) => showHandles(state.byKey));
  const leftOpen = useValue(view.leftOpen);
  const paletteOpen = useValue(view.paletteOpen);
  const row = useRows(stores.attention)[0];
  /** The interaction that waits first: the first interaction of the first row of `attention`. */
  const firstId = row?.interactions[0];
  const first = useSelect(stores.interactions, (state) =>
    firstId ? state.byKey[firstId] : undefined,
  );
  /** The first row when it has no interaction: a run in `error`, `rate_limited` or `turn_ended`. */
  const troubleState = row ? troubleOf(row) : null;
  const troublePane = useSelect(stores.agents, (state) =>
    row && troubleState ? (state.byKey[row.run]?.pane ?? null) : null,
  );
  const trouble =
    row && troubleState ? { pane: troublePane, text: troubleText(troubleState) } : undefined;
  // The handle of the pane that the line shows: of the interaction, or else of the trouble.
  const linePane = first ? first.pane : trouble?.pane;
  const lineHandle = useSelect(stores.panes, (state) =>
    linePane ? state.byKey[linePane]?.handle : undefined,
  );
  const paletteChord = chordText(props.keymap, 'palette.open', platform);
  const answerChord = chordText(props.keymap, 'interaction.answer_next', platform);
  return (
    // A press on a part of the line that is not a control moves the window, and a double click
    // zooms it, when the window has no title bar. The attribute does nothing in a browser.
    <header className="commandbar" data-tauri-drag-region="deep">
      {/* A phone opens the left column as a sheet from this button. */}
      {form === 'phone' ? (
        <button
          type="button"
          className="phone-workspaces"
          aria-label="Workspaces"
          aria-pressed={leftOpen}
          onClick={() => {
            view.sheet.set(null);
            view.leftOpen.set(!view.leftOpen.get());
          }}
        >
          <svg viewBox="0 0 16 16" width="16" height="16" aria-hidden="true">
            <path d="M2 4h12M2 8h12M2 12h12" stroke="currentColor" strokeWidth="1.5" />
          </svg>
        </button>
      ) : (
        <span className="brand">neohar</span>
      )}
      <button
        type="button"
        className="cmdline"
        title="Open the command palette"
        aria-haspopup="dialog"
        aria-expanded={paletteOpen}
        onClick={() => view.paletteOpen.set(true)}
      >
        <SearchIcon />
        <span className="cmdline-hint">Search or run a command</span>
        {paletteChord ? <kbd className="cap">{paletteChord}</kbd> : null}
      </button>
      <div className="ticker" aria-label="Needs attention" role="status">
        {count > 0 ? <span className="badge">{count} waiting</span> : null}
        {first ? (
          <button
            type="button"
            className="ticker-next"
            title={first.title}
            onClick={() => runCommand('interaction.answer_next', props.actions)}
          >
            {handles ? <span className="handle">{lineHandle ?? first.handle}</span> : null}
            <span className="ticker-title">{first.title}</span>
            {answerChord ? <kbd className="cap cap-wait">{answerChord}</kbd> : null}
            <span className="ticker-action">Answer</span>
          </button>
        ) : trouble ? (
          <button
            type="button"
            className="ticker-next"
            title={trouble.text}
            disabled={trouble.pane === null}
            onClick={() => {
              const pane = trouble.pane;
              if (pane) showPane(transport, windowId, pane).catch(report);
            }}
          >
            {handles ? <span className="handle">{lineHandle ?? '?'}</span> : null}
            <span className="ticker-title">{trouble.text}</span>
            <span className="ticker-action">Open</span>
          </button>
        ) : null}
      </div>
    </header>
  );
}
