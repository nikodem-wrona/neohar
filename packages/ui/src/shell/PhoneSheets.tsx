// The sheets of the desk on a phone that take the screen under the command line: More,
// with the parts of the status bar of the desk and Stop everything, and the settings. The close
// button, a swipe down on the head of a sheet and Escape close it.

import { RpcError } from '@neohar/transport';
import { Fragment, type ReactNode, useRef, useState } from 'react';
import { harnessName } from '../agents/badge';
import { useApp } from '../app/context';
import type { Keymap } from '../input/chords';
import { type ActionContext, runCommand } from '../keymap/actions';
import { useRows, useSelect, useValue } from '../lib/hooks';
import { SettingsView } from '../settings/SettingsView';
import { limitText } from '../timeline/model';
import { chordText, FUNCTION_KEYS } from './keys';

/** How far down a finger moves on the head of a sheet to close it, in CSS pixels. */
const SWIPE_CLOSE = 60;

export interface PhoneSheetsProps {
  actions: ActionContext;
  keymap: Keymap;
  /** More content of a section of the settings: This phone. */
  extraSettings?: (section: string) => ReactNode | undefined;
}

export function PhoneSheets(props: PhoneSheetsProps) {
  const { view, transport } = useApp();
  const sheet = useValue(view.sheet);
  const close = (): void => {
    view.sheet.set(null);
  };
  const startY = useRef<number | null>(null);
  const title = sheet?.kind === 'more' ? 'More' : 'Settings';
  const section = sheet?.kind === 'settings' ? sheet.section : null;
  return sheet ? (
    <section
      className="phone-sheet"
      role="dialog"
      aria-label={title}
      onKeyDown={(event) => {
        if (event.key === 'Escape') close();
      }}
    >
      <header
        className="phone-sheet-head"
        onTouchStart={(event) => {
          startY.current = event.touches[0]?.clientY ?? null;
        }}
        onTouchEnd={(event) => {
          const endY = event.changedTouches[0]?.clientY;
          if (startY.current !== null && endY !== undefined && endY - startY.current > SWIPE_CLOSE)
            close();
          startY.current = null;
        }}
      >
        <h2 className="phone-sheet-title">{title}</h2>
        <button type="button" className="phone-sheet-close" aria-label="Close" onClick={close}>
          <svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true">
            <path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" strokeWidth="1.5" />
          </svg>
        </button>
      </header>
      <div className="phone-sheet-body">
        {sheet.kind === 'more' ? (
          <More actions={props.actions} keymap={props.keymap} />
        ) : sheet.kind === 'settings' ? (
          <SettingsView
            transport={transport}
            section={section}
            onSection={(name) => view.sheet.set({ kind: 'settings', section: name })}
            {...(props.extraSettings ? { extra: props.extraSettings } : {})}
          />
        ) : null}
      </div>
    </section>
  ) : null;
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
            <button type="button" className="card-button danger" onClick={stopEverything}>
              Stop everything
            </button>
            <button type="button" className="card-button" onClick={() => setStopping(false)}>
              Cancel
            </button>
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
