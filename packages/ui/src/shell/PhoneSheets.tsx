// The sheets of the desk on a phone that take the screen under the command line: More,
// with the parts of the status bar of the desk and Stop everything, and the settings. The close
// button, a swipe down on the head of a sheet and Escape close it. In portrait, the left column and
// the drawer are sheets too (`ColumnSheet`).
//
// Each is a Sheet of shadcn/ui that goes into the shell, not under `<body>`: the grid of the shell
// places it, and the rules of `.shell.phone` still reach what it holds. A sheet is not modal: the
// command line and the bottom bar under it still work, and a tap beside it keeps it open, as
// before. The view says when a sheet shows, so the sheet gives the focus back itself.

import { RpcError } from '@neohar/transport';
import { Fragment, type ReactNode, useRef, useState } from 'react';
import { harnessName } from '../agents/badge';
import { useApp } from '../app/context';
import { useLayer } from '../app/layer';
import type { PhoneSheet } from '../app/view';
import { Button } from '../components/ui/button';
import { Sheet, SheetClose, SheetContent, SheetTitle } from '../components/ui/sheet';
import type { Keymap } from '../input/chords';
import { type ActionContext, runCommand } from '../keymap/actions';
import { useRows, useSelect, useValue } from '../lib/hooks';
import { cn } from '../lib/utils';
import { SettingsView } from '../settings/SettingsView';
import { limitText } from '../timeline/model';
import { useFocusReturn } from './focus';
import { chordText, FUNCTION_KEYS } from './keys';

/** How far down a finger moves on the head of a sheet to close it, in CSS pixels. */
const SWIPE_CLOSE = 60;

/**
 * The classes that undo the place and the glass of a Sheet of shadcn/ui: the grid of the shell
 * places a sheet of a phone, and the sheet is a panel of the chrome, as the columns are.
 */
const IN_GRID =
  'static inset-auto z-20 h-auto w-auto max-w-none gap-0 border-0 bg-transparent shadow-none backdrop-filter-none sm:max-w-none';

/** The edge of a sheet: the hairline and the shadow of glass. */
const EDGE = 'shadow-[inset_0_0_0_1px_var(--hairline),var(--glass-shadow)]';

export interface PhoneSheetsProps {
  actions: ActionContext;
  keymap: Keymap;
  /** More content of a section of the settings: This phone. */
  extraSettings?: (section: string) => ReactNode | undefined;
  /** The shell, which the sheet goes into. */
  container: HTMLElement;
}

export function PhoneSheets(props: PhoneSheetsProps) {
  const { view, transport } = useApp();
  const sheet = useValue(view.sheet);
  // The last sheet, so that its content stays while the sheet closes.
  const [shown, setShown] = useState<PhoneSheet | null>(sheet);
  if (sheet !== null && sheet !== shown) setShown(sheet);
  const open = sheet !== null;
  useLayer(open);
  const focusBack = useFocusReturn(open);
  const close = (): void => {
    view.sheet.set(null);
  };
  const startY = useRef<number | null>(null);
  const title = shown?.kind === 'more' ? 'More' : 'Settings';
  const section = shown?.kind === 'settings' ? shown.section : null;
  return (
    <Sheet
      open={open}
      onOpenChange={(next) => {
        if (!next) close();
      }}
      modal={false}
    >
      <SheetContent
        container={props.container}
        side="bottom"
        showCloseButton={false}
        aria-describedby={undefined}
        className={cn(
          IN_GRID,
          EDGE,
          'phone-sheet z-30 min-h-0 overflow-hidden rounded-(--r-panel) bg-(--bg-raised) [grid-area:2/1/-1/-1]',
        )}
        onCloseAutoFocus={focusBack}
        onInteractOutside={(event) => event.preventDefault()}
      >
        <header
          className="phone-sheet-head"
          onTouchStart={(event) => {
            startY.current = event.touches[0]?.clientY ?? null;
          }}
          onTouchEnd={(event) => {
            const endY = event.changedTouches[0]?.clientY;
            if (
              startY.current !== null &&
              endY !== undefined &&
              endY - startY.current > SWIPE_CLOSE
            )
              close();
            startY.current = null;
          }}
        >
          <SheetTitle className="phone-sheet-title">{title}</SheetTitle>
          <SheetClose asChild>
            <button type="button" className="phone-sheet-close" aria-label="Close">
              <svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true">
                <path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" strokeWidth="1.5" />
              </svg>
            </button>
          </SheetClose>
        </header>
        <div className="phone-sheet-body">
          {shown?.kind === 'more' ? (
            <More actions={props.actions} keymap={props.keymap} />
          ) : shown?.kind === 'settings' ? (
            <SettingsView
              transport={transport}
              section={section}
              onSection={(name) => view.sheet.set({ kind: 'settings', section: name })}
              {...(props.extraSettings ? { extra: props.extraSettings } : {})}
            />
          ) : null}
        </div>
      </SheetContent>
    </Sheet>
  );
}

/**
 * The left column or the drawer of a phone in portrait: a sheet over the board and the panes, or
 * over the pane area. A pane that gets the focus, a tab that shows, the bottom bar and Escape
 * close it.
 */
export function ColumnSheet(props: {
  open: boolean;
  onClose: () => void;
  /** The name of the sheet, for a screen reader. */
  title: string;
  /** The place of the sheet in the grid of the shell. */
  className: string;
  /** The shell, which the sheet goes into. */
  container: HTMLElement;
  children: ReactNode;
}) {
  useLayer(props.open);
  const focusBack = useFocusReturn(props.open);
  return (
    <Sheet
      open={props.open}
      onOpenChange={(next) => {
        if (!next) props.onClose();
      }}
      modal={false}
    >
      <SheetContent
        container={props.container}
        side="left"
        showCloseButton={false}
        aria-describedby={undefined}
        // A column shows and goes at once, as on the desk.
        className={cn(
          IN_GRID,
          'phone-column min-h-0 animate-none data-[state=closed]:animate-none data-[state=open]:animate-none',
          props.className,
        )}
        // The sheet takes the focus itself: its first button would show its tip.
        onOpenAutoFocus={(event) => {
          event.preventDefault();
          (event.currentTarget as HTMLElement | null)?.focus({ preventScroll: true });
        }}
        onCloseAutoFocus={focusBack}
        onInteractOutside={(event) => event.preventDefault()}
      >
        <SheetTitle className="sr-only">{props.title}</SheetTitle>
        {props.children}
      </SheetContent>
    </Sheet>
  );
}

/** More: what the status bar of the desk shows, as a list, and two actions. */
function More(props: { actions: ActionContext; keymap: Keymap }) {
  const { stores, view, transport, platform } = useApp();
  const machine = useSelect(stores.link, (state) => state.byKey['_']?.text ?? 'local');
  const health = useSelect(stores.health, (state) => state.byKey['_']);
  const limits = useRows(stores.limits);
  const [stopping, setStopping] = useState(false);
  const [stopped, setStopped] = useState<string | null>(null);
  const run = (command: string): void => {
    view.sheet.set(null);
    runCommand(command, props.actions);
  };
  const stopEverything = (): void => {
    setStopping(false);
    transport.call('session.stop_everything', {}).then(
      (result) =>
        setStopped(
          `Stopped the programs of ${result.ended + result.killed} panes. ${result.left} left.`,
        ),
      (failure: unknown) =>
        setStopped(failure instanceof RpcError ? failure.message : String(failure)),
    );
  };
  return (
    <div className="phone-more">
      <ul className="phone-more-list" aria-label="Commands">
        {FUNCTION_KEYS.map((key) => {
          const chord = chordText(props.keymap, key.command, platform);
          return (
            <li key={key.command}>
              <button
                type="button"
                className="phone-more-row"
                data-command={key.command}
                onClick={() => run(key.command)}
              >
                <span>{key.label}</span>
                {chord ? <kbd className="cap">{chord}</kbd> : null}
              </button>
            </li>
          );
        })}
      </ul>
      <dl className="phone-more-facts">
        <dt>Machine</dt>
        <dd>{machine}</dd>
        <dt>Renderer</dt>
        <dd>{health?.renderer ?? 'none'}</dd>
        {limits.map((limit) => {
          const text = limitText(limit, harnessName(limit.harness));
          return text ? (
            <Fragment key={`${limit.harness}:${limit.account}`}>
              <dt>{harnessName(limit.harness)}</dt>
              <dd>{text}</dd>
            </Fragment>
          ) : null;
        })}
      </dl>
      {(health?.warnings.length ?? 0) > 0 ? (
        <ul className="phone-more-warnings" aria-label="Warnings">
          {health?.warnings.map((warning) => (
            <li key={warning.id}>{warning.text}</li>
          ))}
        </ul>
      ) : null}
      <div className="phone-more-actions">
        <button
          type="button"
          className="phone-more-row"
          onClick={() => view.sheet.set({ kind: 'settings', section: null })}
        >
          <span>Settings</span>
        </button>
        {stopping ? (
          <div className="phone-more-confirm" role="alert">
            <p>Stop the program of every pane? The agents get 2 s to end.</p>
            {/* Dark text on the red: white text on it has too little contrast. */}
            <Button
              variant="destructive"
              className="min-h-(--control) bg-destructive font-semibold text-on-signal hover:bg-(--danger-hover) dark:bg-destructive"
              onClick={stopEverything}
            >
              Stop everything
            </Button>
            <Button
              variant="outline"
              className="min-h-(--control)"
              onClick={() => setStopping(false)}
            >
              Cancel
            </Button>
          </div>
        ) : (
          <button
            type="button"
            className="phone-more-row danger"
            onClick={() => {
              setStopped(null);
              setStopping(true);
            }}
          >
            <span>Stop everything</span>
          </button>
        )}
        {stopped ? (
          <p className="phone-more-note" role="status">
            {stopped}
          </p>
        ) : null}
      </div>
    </div>
  );
}
