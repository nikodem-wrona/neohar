// The command palette. It opens with ⌘K. Typing sends `palette.query` after 30 ms, and
// the list shows the order that the core gives. Enter, or a click, runs the command with
// `palette.execute` for this window. A command that needs a confirmation (it would stop a program)
// shows the question in the palette, and the answer calls the command again with `force`. A command
// of the page comes back as an action that the page does (`inbox.open`).

import { RpcError, type PaletteMatch } from '@neohar/transport';
import { createEffect, createSignal, For, onCleanup, onMount, Show } from 'solid-js';
import { useApp } from '../app/context';
import { debounce, realTimers } from '../lib/timing';
import { SearchIcon } from '../shell/icons';
import { segments } from './highlight';
import { PaletteKeys } from './Keys';

/** How many commands the palette asks for. */
const LIMIT = 50;
/** The time between the last key and the query. */
const QUERY_DELAY_MS = 30;

/** What the core says about a command that needs a confirmation: the program that would stop. */
function questionOf(failure: RpcError): string {
  const details = failure.details;
  const name =
    typeof details === 'object' && details !== null && 'process' in details
      ? (details as { process: unknown }).process
      : null;
  return typeof name === 'string' && name !== ''
    ? `${name} runs in the pane, and the command stops it.`
    : 'A program runs here, and the command stops it.';
}

export function Palette() {
  const { stores, transport, report, windowId, view } = useApp();
  const [text, setText] = createSignal('');
  const [items, setItems] = createSignal<PaletteMatch[]>([]);
  const [selected, setSelected] = createSignal(0);
  const [message, setMessage] = createSignal<string | null>(null);
  const [confirm, setConfirm] = createSignal<{ command: string; question: string } | null>(null);
  let input: HTMLInputElement | undefined;
  let list: HTMLUListElement | undefined;
  /** The number of the newest query, so that a late answer to an older one is dropped. */
  let newest = 0;

  /** Whether keys wait for their query. Enter then asks at once and runs the best match. */
  let waiting = false;

  /** Asks the core and shows the answer, unless a newer query was asked. Gives the commands. */
  const ask = (query: string): Promise<PaletteMatch[]> => {
    const mine = ++newest;
    return transport
      .call('palette.query', { query, limit: LIMIT, window: windowId })
      .then((result) => {
        if (mine !== newest) return [];
        waiting = false;
        setItems(result.items);
        setSelected(0);
        return result.items;
      })
      .catch((failure: unknown) => {
        if (mine === newest) {
          waiting = false;
          setMessage(
            failure instanceof RpcError ? failure.message : 'The palette cannot ask the core.',
          );
        }
        return [];
      });
  };
  const later = debounce(
    (query: string) => void ask(query).catch(report),
    QUERY_DELAY_MS,
    realTimers,
  );
  onCleanup(() => later.cancel());
  onMount(() => {
    ask('').catch(report);
    input?.focus();
  });

  // The command under the keys stays in view.
  createEffect(() => {
    selected();
    list?.querySelector('[aria-selected="true"]')?.scrollIntoView({ block: 'nearest' });
  });

  const run = (match: PaletteMatch, force = false): void => {
    if (!match.command.enabled) return;
    setMessage(null);
    setConfirm(null);
    transport
      .call('palette.execute', {
        command: match.command.id,
        window: windowId,
        ...(force ? { args: { force: true } } : {}),
      })
      .then((result) => {
        // The action first: a field that it opens takes the focus before the palette closes.
        if (result.ui) view.pageAction(result.ui, stores.windows.state.byKey[windowId]);
        view.setPaletteOpen(false);
      })
      .catch((failure: unknown) => {
        if (failure instanceof RpcError && failure.code === 'confirm_required') {
          setConfirm({ command: match.command.id, question: questionOf(failure) });
        } else {
          setMessage(failure instanceof RpcError ? failure.message : 'The command failed.');
        }
      });
  };

  const onKey = (e: KeyboardEvent): void => {
    const count = items().length;
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      setSelected((at) => Math.min(at + 1, Math.max(count - 1, 0)));
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      setSelected((at) => Math.max(at - 1, 0));
    } else if (e.key === 'Enter') {
      e.preventDefault();
      if (waiting) {
        // The keys have not had their answer yet. Ask now, and run the best match of the answer.
        later.cancel();
        ask(text())
          .then((found) => {
            const first = found[0];
            if (first) run(first);
          })
          .catch(report);
        return;
      }
      const match = items()[selected()];
      if (match) run(match);
    } else if (e.key === 'Escape') {
      e.preventDefault();
      view.setPaletteOpen(false);
    }
  };

  return (
    <div
      class="palette-backdrop"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) view.setPaletteOpen(false);
      }}
    >
      <div class="palette" role="dialog" aria-modal="true" aria-label="Command palette">
        <div class="palette-field">
          <SearchIcon />
          <input
            ref={(element) => (input = element)}
            class="palette-input"
            type="text"
            role="combobox"
            aria-expanded="true"
            aria-controls="palette-list"
            aria-activedescendant={items().length > 0 ? `palette-option-${selected()}` : undefined}
            placeholder="Search commands, tabs and panes"
            autocomplete="off"
            autocapitalize="off"
            spellcheck={false}
            value={text()}
            onInput={(e) => {
              setText(e.currentTarget.value);
              setConfirm(null);
              setMessage(null);
              waiting = true;
              later(e.currentTarget.value);
            }}
            onKeyDown={onKey}
          />
        </div>
        <Show when={confirm()}>
          {(question) => (
            <div class="palette-confirm" role="alert">
              <span>{question().question}</span>
              <button
                type="button"
                data-action="palette-force"
                onClick={() => {
                  const match = items().find((item) => item.command.id === question().command);
                  if (match) run(match, true);
                }}
              >
                Do it anyway
              </button>
              <button type="button" data-action="palette-keep" onClick={() => setConfirm(null)}>
                Keep it
              </button>
            </div>
          )}
        </Show>
        <Show when={message()}>
          <p class="palette-message" role="alert">
            {message()}
          </p>
        </Show>
        <ul
          class="palette-list"
          id="palette-list"
          role="listbox"
          ref={(element) => (list = element)}
        >
          <For each={items()}>
            {(item, index) => (
              <li
                id={`palette-option-${index()}`}
                class="palette-item"
                classList={{
                  selected: index() === selected(),
                  disabled: !item.command.enabled,
                }}
                role="option"
                aria-selected={index() === selected()}
                aria-disabled={!item.command.enabled}
                title={item.command.disabled_reason ?? undefined}
                onMouseMove={() => setSelected(index())}
                onClick={() => run(item)}
              >
                <span class="palette-title">
                  <For each={segments(item.command.title, item.ranges)}>
                    {(part) => (part.hit ? <mark>{part.text}</mark> : part.text)}
                  </For>
                </span>
                <Show when={item.command.shortcut} fallback={<span />}>
                  <kbd class="palette-shortcut">{item.command.shortcut}</kbd>
                </Show>
                <span class="palette-category">{item.command.category}</span>
              </li>
            )}
          </For>
        </ul>
        <Show when={items().length === 0}>
          <p class="palette-empty">No command matches.</p>
        </Show>
        <PaletteKeys action="Run" />
      </div>
    </div>
  );
}
