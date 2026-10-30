// The command palette. It opens with ⌘K. Typing sends `palette.query` after 30 ms, and
// the list shows the order that the core gives. Enter, or a click, runs the command with
// `palette.execute` for this window. A command that needs a confirmation (it would stop a program)
// shows the question in the palette, and the answer calls the command again with `force`. A command
// of the page comes back as an action that the page does (`inbox.open`). It is the Command Dialog
// of shadcn/ui, and the Command does not filter or sort: the core gives the results and their order.

import { RpcError, type PaletteMatch } from '@neohar/transport';
import { Dialog as DialogPrimitive } from 'radix-ui';
import {
  Fragment,
  type KeyboardEvent,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from 'react';
import { useApp } from '../app/context';
import { useLayer } from '../app/layer';
import { Button } from '../components/ui/button';
import {
  Command,
  CommandEmpty,
  CommandInput,
  CommandItem,
  CommandList,
} from '../components/ui/command';
import { Dialog, DialogOverlay, DialogPortal, DialogTitle } from '../components/ui/dialog';
import { Kbd } from '../components/ui/kbd';
import { debounce, realTimers } from '../lib/timing';
import { cn } from '../lib/utils';
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
  const { stores, transport, report, windowId, view, form } = useApp();
  const [text, setText] = useState('');
  const [items, setItems] = useState<PaletteMatch[]>([]);
  /** The id of the command under the keys. */
  const [selected, setSelected] = useState('');
  const [message, setMessage] = useState<string | null>(null);
  const [confirm, setConfirm] = useState<{ command: string; question: string } | null>(null);
  /** The number of the newest query, so that a late answer to an older one is dropped. */
  const newest = useRef(0);
  useLayer(true);

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
          // The first command of the answer is under the keys.
          setSelected(result.items[0]?.command.id ?? '');
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
  // The dialog gives its field the focus when it opens.
  useEffect(() => {
    ask('').catch(report);
  }, [ask, report]);

  const close = (): void => view.paletteOpen.set(false);

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
        close();
      })
      .catch((failure: unknown) => {
        if (failure instanceof RpcError && failure.code === 'confirm_required') {
          setConfirm({ command: match.command.id, question: questionOf(failure) });
        } else {
          setMessage(failure instanceof RpcError ? failure.message : 'The command failed.');
        }
      });
  };

  // The Command moves with the arrows. Enter runs here: keys that wait for their query ask now.
  const onKey = (e: KeyboardEvent): void => {
    // Enter on a button of the question presses the button.
    if (e.key !== 'Enter' || !(e.target as Element).matches('[cmdk-input]')) return;
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
    const match = items.find((item) => item.command.id === selected);
    if (match) run(match);
  };

  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) close();
      }}
    >
      <DialogPortal>
        {/* A click beside the palette closes it: the layer under it is clear. */}
        <DialogOverlay className="bg-transparent backdrop-blur-none data-[state=open]:animate-none" />
        <DialogPrimitive.Content
          data-slot="dialog-content"
          aria-describedby={undefined}
          // The palette lies over the field of the command line, and its results drop below it.
          // It is a little wider than the field, and its middle is the middle of the field. On a
          // phone it takes the width of the screen.
          className={cn(
            'glass fixed top-(--cmd-top) left-[calc(50%+var(--traffic)/2)] z-50 flex max-h-[min(60vh,520px)] w-[min(max(var(--cmd-width),640px),calc(100vw-16px-var(--traffic)))] -translate-x-1/2 flex-col overflow-hidden rounded-(--r-lg) p-0 text-foreground outline-none',
            // It drops from the field when it opens. It does not fade in: text that is not opaque
            // yet has less contrast.
            'motion-safe:animate-in motion-safe:slide-in-from-top-1 motion-safe:duration-120',
            form === 'phone' && 'right-(--s1) left-(--s1) w-auto translate-x-0',
          )}
          onCloseAutoFocus={(event) => {
            // A field that the command opened keeps the focus. Else the focus goes back.
            const now = document.activeElement;
            if (now && now !== document.body) event.preventDefault();
          }}
        >
          <DialogTitle className="sr-only">Command palette</DialogTitle>
          <Command
            shouldFilter={false}
            value={selected}
            onValueChange={setSelected}
            onKeyDown={onKey}
            className="min-h-0 flex-1 rounded-none **:data-[slot=command-input-wrapper]:h-12 **:data-[slot=command-input-wrapper]:flex-none **:data-[slot=command-input-wrapper]:gap-3 **:data-[slot=command-input-wrapper]:px-4 **:data-[slot=command-input-wrapper]:[&_svg]:size-4 **:data-[slot=command-input-wrapper]:[&_svg]:opacity-100 **:data-[slot=command-input-wrapper]:[&_svg]:text-muted-foreground"
          >
            <CommandInput
              placeholder="Search commands, tabs and panes"
              autoComplete="off"
              autoCapitalize="off"
              spellCheck={false}
              className="h-full py-0 text-[15px]/5 caret-foreground"
              value={text}
              onValueChange={(next) => {
                setText(next);
                setConfirm(null);
                setMessage(null);
                waiting.current = true;
                later(next);
              }}
            />
            {confirm ? (
              <div className="flex items-center gap-[1ch] bg-muted px-4 py-2" role="alert">
                <span>{confirm.question}</span>
                <Button
                  size="xs"
                  variant="outline"
                  data-action="palette-force"
                  onClick={() => {
                    const match = items.find((item) => item.command.id === confirm.command);
                    if (match) run(match, true);
                  }}
                >
                  Do it anyway
                </Button>
                <Button
                  size="xs"
                  variant="outline"
                  data-action="palette-keep"
                  onClick={() => setConfirm(null)}
                >
                  Keep it
                </Button>
              </div>
            ) : null}
            {message ? (
              <p className="px-4 py-2 text-destructive" role="alert">
                {message}
              </p>
            ) : null}
            <CommandList className="max-h-none min-h-0 flex-1 p-2">
              {items.map((item) => (
                <CommandItem
                  key={item.command.id}
                  value={item.command.id}
                  disabled={!item.command.enabled}
                  title={item.command.disabled_reason ?? undefined}
                  onSelect={() => run(item)}
                  // A row: the title, then its keys and what it is, both quiet, at the right. The
                  // kinds are a column, so that they line up whether a row has keys or not. The
                  // row under the keys is a step lighter, and its fill is opaque.
                  className="grid h-8 cursor-pointer grid-cols-[minmax(0,1fr)_auto_9ch] gap-x-3 py-0 pr-2 pl-3 text-[13px]/[18px] data-[disabled=true]:pointer-events-auto data-[disabled=true]:opacity-45 data-[selected=true]:bg-(--bg-focus)"
                >
                  <span className="min-w-0 truncate">
                    {segments(item.command.title, item.ranges).map((part, at) => (
                      <Fragment key={at}>
                        {part.hit ? (
                          <mark className="bg-transparent font-semibold text-foreground">
                            {part.text}
                          </mark>
                        ) : (
                          part.text
                        )}
                      </Fragment>
                    ))}
                  </span>
                  {item.command.shortcut ? (
                    <Kbd className="h-5 min-w-[2ch] px-[5px] font-mono text-(length:--size-data)">
                      {item.command.shortcut}
                    </Kbd>
                  ) : (
                    <span />
                  )}
                  <span className="truncate text-right text-(length:--size-small) text-muted-foreground">
                    {item.command.category}
                  </span>
                </CommandItem>
              ))}
              <CommandEmpty className="px-4 py-5 text-center text-muted-foreground">
                No command matches.
              </CommandEmpty>
            </CommandList>
            <PaletteKeys action="Run" />
          </Command>
        </DialogPrimitive.Content>
      </DialogPortal>
    </Dialog>
  );
}
