// The search of the scrollback. It opens with ⌘⇧F. Typing sends `search.query` after
// 100 ms, and the list shows the order that the core gives, each hit with its pane (the handle with
// `ui.show_handles`, else the title), its line and the text with the match marked. Enter,
// or a click, shows the pane in this window with `palette.execute` and scrolls it to the line with
// `pane.scroll`.

import { type SearchHit, RpcError } from '@neohar/transport';
import { createEffect, createSignal, For, onCleanup, onMount, Show } from 'solid-js';
import { useApp } from '../app/context';
import { paneName, showHandles } from '../app/preferences';
import { showPane } from '../app/show';
import { debounce, realTimers } from '../lib/timing';
import { PaletteKeys } from '../palette/Keys';
import { SearchIcon } from '../shell/icons';
import { hitSegments, paramsOf, SCOPES, type Scope } from './model';

/** The time between the last key and the query. */
const QUERY_DELAY_MS = 100;

export function SearchOverlay() {
  const { stores, transport, windowId, view } = useApp();
  const [text, setText] = createSignal('');
  const [scope, setScope] = createSignal<Scope>('all');
  const [hits, setHits] = createSignal<SearchHit[]>([]);
  const [truncated, setTruncated] = createSignal(false);
  const [selected, setSelected] = createSignal(0);
  const [message, setMessage] = createSignal<string | null>(null);
  /** A query was asked and has no answer yet. */
  const [asking, setAsking] = createSignal(false);
  let input: HTMLInputElement | undefined;
  let list: HTMLUListElement | undefined;
  /** The number of the newest query, so that a late answer to an older one is dropped. */
  let newest = 0;

  const place = () => {
    const shown = stores.windows.state.byKey[windowId];
    return { pane: shown?.focused_pane, workspace: shown?.workspace };
  };

  const ask = (): void => {
    const params = paramsOf(text(), scope(), place());
    const mine = ++newest;
    setMessage(null);
    if (!params) {
      setHits([]);
      setTruncated(false);
      setAsking(false);
      return;
    }
    setAsking(true);
    transport
      .call('search.query', params)
      .then((result) => {
        if (mine !== newest) return;
        setAsking(false);
        setHits(result.hits);
        setTruncated(result.truncated === true);
        setSelected(0);
      })
      .catch((failure: unknown) => {
        if (mine !== newest) return;
        setAsking(false);
        setMessage(failure instanceof RpcError ? failure.message : 'The search failed.');
      });
  };
  const later = debounce(ask, QUERY_DELAY_MS, realTimers);
  onCleanup(() => later.cancel());
  onMount(() => input?.focus());

  // The hit under the keys stays in view.
  createEffect(() => {
    selected();
    list?.querySelector('[aria-selected="true"]')?.scrollIntoView({ block: 'nearest' });
  });

  const paneOf = (pane: string): string =>
    paneName(stores.panes.state.byKey[pane], showHandles(stores.settings.state.byKey)) ?? '';

  const open = (hit: SearchHit): void => {
    setMessage(null);
    showPane(transport, windowId, hit.pane)
      .then(() => transport.call('pane.scroll', { pane: hit.pane, top_line: hit.line }))
      .then(() => view.setSearchOpen(false))
      .catch((failure: unknown) => {
        setMessage(failure instanceof RpcError ? failure.message : 'The pane did not open.');
      });
  };

  const onKey = (e: KeyboardEvent): void => {
    const count = hits().length;
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      setSelected((at) => Math.min(at + 1, Math.max(count - 1, 0)));
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      setSelected((at) => Math.max(at - 1, 0));
    } else if (e.key === 'Enter') {
      e.preventDefault();
      const hit = hits()[selected()];
      if (hit) open(hit);
    } else if (e.key === 'Escape') {
      e.preventDefault();
      view.setSearchOpen(false);
    }
  };

  return (
    <div
      class="palette-backdrop"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) view.setSearchOpen(false);
      }}
    >
      <div class="palette search" role="dialog" aria-modal="true" aria-label="Search scrollback">
        <div class="palette-field">
          <SearchIcon />
          <input
            ref={(element) => (input = element)}
            class="palette-input"
            type="text"
            aria-label="Words to find"
            placeholder="Search the scrollback"
            autocomplete="off"
            autocapitalize="off"
            spellcheck={false}
            value={text()}
            onInput={(e) => {
              setText(e.currentTarget.value);
              later();
            }}
            onKeyDown={onKey}
          />
        </div>
        <div class="search-scopes" role="group" aria-label="Where to search">
          <For each={SCOPES}>
            {(choice) => (
              <button
                type="button"
                class="search-scope"
                aria-pressed={scope() === choice.id}
                onClick={() => {
                  setScope(choice.id);
                  later.cancel();
                  ask();
                  input?.focus();
                }}
              >
                {choice.label}
              </button>
            )}
          </For>
        </div>
        <Show when={message()}>
          <p class="palette-message" role="alert">
            {message()}
          </p>
        </Show>
        <ul
          class="palette-list"
          role="listbox"
          aria-label="Matching lines"
          ref={(element) => (list = element)}
        >
          <For each={hits()}>
            {(hit, index) => (
              <li
                class="palette-item search-hit"
                classList={{ selected: index() === selected() }}
                role="option"
                aria-selected={index() === selected()}
                data-pane={hit.pane}
                data-line={hit.line}
                onMouseMove={() => setSelected(index())}
                onClick={() => open(hit)}
              >
                <span class="palette-category">
                  {paneOf(hit.pane)} · {hit.line}
                </span>
                <span class="palette-title">
                  <For each={hitSegments(hit.text)}>
                    {(part) => (part.hit ? <mark>{part.text}</mark> : part.text)}
                  </For>
                </span>
              </li>
            )}
          </For>
        </ul>
        <Show when={truncated()}>
          <p class="palette-empty">More lines match. Add a word to narrow the search.</p>
        </Show>
        <Show when={hits().length === 0 && !asking() && text().trim() !== '' && !message()}>
          <p class="palette-empty">No line matches.</p>
        </Show>
        <PaletteKeys action="Open" />
      </div>
    </div>
  );
}
