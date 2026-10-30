// The command palette. It opens with ⌘K. Typing sends `palette.query` after 30 ms, and
// the list shows the order that the core gives. Enter, or a click, runs the command with
// `palette.execute` for this window. A command that needs a confirmation (it would stop a program)
// shows the question in the palette, and the answer calls the command again with `force`. A command
// of the page comes back as an action that the page does (`inbox.open`).

import { RpcError, type PaletteMatch } from '@neohar/transport';
import {
  Fragment,
  type KeyboardEvent,
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from 'react';
import { useApp } from '../app/context';
import { debounce, realTimers } from '../lib/timing';
import { cn } from '../lib/utils';
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
  const [text, setText] = useState('');
  const [items, setItems] = useState<PaletteMatch[]>([]);
  const [selected, setSelected] = useState(0);
  const [message, setMessage] = useState<string | null>(null);
  const [confirm, setConfirm] = useState<{ command: string; question: string } | null>(null);
  const input = useRef<HTMLInputElement>(null);
  const list = useRef<HTMLUListElement>(null);
  /** The number of the newest query, so that a late answer to an older one is dropped. */
  const newest = useRef(0);

  /** Whether keys wait for their query. Enter then asks at once and runs the best match. */
  const waiting = useRef(false);

  /** Asks the core and shows the answer, unless a newer query was asked. Gives the commands. */
  const ask = useCallback(
    (query: string): Promise<PaletteMatch[]> => {
      const mine = ++newest.current;
      return transport
        .call('palette.query', { query, limit: LIMIT, window: windowId })
        .then((result) => {
          if (mine !== newest.current) return [];
          waiting.current = false;
          setItems(result.items);
          setSelected(0);
          return result.items;
        })
        .catch((failure: unknown) => {
          if (mine === newest.current) {
            waiting.current = false;
            setMessage(
              failure instanceof RpcError ? failure.message : 'The palette cannot ask the core.',
            );
          }
          return [];
        });
    },
    [transport, windowId],
  );
  const later = useMemo(
    () => debounce((query: string) => void ask(query).catch(report), QUERY_DELAY_MS, realTimers),
    [ask, report],
  );
  useEffect(() => () => later.cancel(), [later]);
  useEffect(() => {
    ask('').catch(report);
    input.current?.focus();
  }, [ask, report]);

  // The command under the keys stays in view.
  useLayoutEffect(() => {
    list.current?.querySelector('[aria-selected="true"]')?.scrollIntoView({ block: 'nearest' });
  }, [selected]);

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
        if (result.ui) view.pageAction(result.ui, stores.windows.get().byKey[windowId]);
        view.paletteOpen.set(false);
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
    const count = items.length;
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      setSelected((at) => Math.min(at + 1, Math.max(count - 1, 0)));
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      setSelected((at) => Math.max(at - 1, 0));
    } else if (e.key === 'Enter') {
      e.preventDefault();
      if (waiting.current) {
        // The keys have not had their answer yet. Ask now, and run the best match of the answer.
        later.cancel();
        ask(text)
          .then((found) => {
            const first = found[0];
            if (first) run(first);
          })
          .catch(report);
        return;
      }
      const match = items[selected];
      if (match) run(match);
    } else if (e.key === 'Escape') {
      e.preventDefault();
      view.paletteOpen.set(false);
    }
  };

  return (
    <div
      className="palette-backdrop"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) view.paletteOpen.set(false);
      }}
    >
      <div className="palette" role="dialog" aria-modal="true" aria-label="Command palette">
        <div className="palette-field">
          <SearchIcon />
          <input
            ref={input}
            className="palette-input"
            type="text"
            role="combobox"
            aria-expanded="true"
            aria-controls="palette-list"
            aria-activedescendant={items.length > 0 ? `palette-option-${selected}` : undefined}
            placeholder="Search commands, tabs and panes"
            autoComplete="off"
            autoCapitalize="off"
            spellCheck={false}
            value={text}
            onChange={(e) => {
              setText(e.currentTarget.value);
              setConfirm(null);
              setMessage(null);
              waiting.current = true;
              later(e.currentTarget.value);
            }}
            onKeyDown={onKey}
          />
        </div>
        {confirm ? (
          <div className="palette-confirm" role="alert">
            <span>{confirm.question}</span>
            <button
              type="button"
              data-action="palette-force"
              onClick={() => {
                const match = items.find((item) => item.command.id === confirm.command);
                if (match) run(match, true);
              }}
            >
              Do it anyway
            </button>
            <button type="button" data-action="palette-keep" onClick={() => setConfirm(null)}>
              Keep it
            </button>
          </div>
        ) : null}
        {message ? (
          <p className="palette-message" role="alert">
            {message}
          </p>
        ) : null}
        <ul className="palette-list" id="palette-list" role="listbox" ref={list}>
          {items.map((item, index) => (
            <li
              key={item.command.id}
              id={`palette-option-${index}`}
              className={cn('palette-item', {
                selected: index === selected,
                disabled: !item.command.enabled,
              })}
              role="option"
              aria-selected={index === selected}
              aria-disabled={!item.command.enabled}
              title={item.command.disabled_reason ?? undefined}
              onMouseMove={() => setSelected(index)}
              onClick={() => run(item)}
            >
              <span className="palette-title">
                {segments(item.command.title, item.ranges).map((part, at) => (
                  <Fragment key={at}>{part.hit ? <mark>{part.text}</mark> : part.text}</Fragment>
                ))}
              </span>
              {item.command.shortcut ? (
                <kbd className="palette-shortcut">{item.command.shortcut}</kbd>
              ) : (
                <span />
              )}
              <span className="palette-category">{item.command.category}</span>
            </li>
          ))}
        </ul>
        {items.length === 0 ? <p className="palette-empty">No command matches.</p> : null}
        <PaletteKeys action="Run" />
      </div>
    </div>
  );
}
