// The sheets of the desk on a phone that take the screen under the command line: More,
// with the parts of the status bar of the desk and Stop everything, and the settings. The close
// button, a swipe down on the head of a sheet and Escape close it.

import { RpcError } from '@neohar/transport';
import { createSignal, For, type JSX, Match, Show, Switch } from 'solid-js';
import { harnessName } from '../agents/badge';
import { useApp } from '../app/context';
import type { Keymap } from '../input/chords';
import { type ActionContext, runCommand } from '../keymap/actions';
import { SettingsView } from '../settings/SettingsView';
import { limitText } from '../timeline/model';
import { chordText, FUNCTION_KEYS } from './keys';

/** How far down a finger moves on the head of a sheet to close it, in CSS pixels. */
const SWIPE_CLOSE = 60;

export interface PhoneSheetsProps {
  actions: ActionContext;
  keymap: () => Keymap;
  /** More content of a section of the settings: This phone. */
  extraSettings?: (section: string) => JSX.Element | undefined;
}

export function PhoneSheets(props: PhoneSheetsProps) {
  const { view, transport } = useApp();
  const close = (): void => {
    view.setSheet(null);
  };
  let startY: number | null = null;
  const title = () => (view.sheet()?.kind === 'more' ? 'More' : 'Settings');
  const section = () => {
    const sheet = view.sheet();
    return sheet?.kind === 'settings' ? sheet.section : null;
  };
  return (
    <Show when={view.sheet()}>
      <section
        class="phone-sheet"
        role="dialog"
        aria-label={title()}
        onKeyDown={(event) => {
          if (event.key === 'Escape') close();
        }}
      >
        <header
          class="phone-sheet-head"
          onTouchStart={(event) => {
            startY = event.touches[0]?.clientY ?? null;
          }}
          onTouchEnd={(event) => {
            const endY = event.changedTouches[0]?.clientY;
            if (startY !== null && endY !== undefined && endY - startY > SWIPE_CLOSE) close();
            startY = null;
          }}
        >
          <h2 class="phone-sheet-title">{title()}</h2>
          <button type="button" class="phone-sheet-close" aria-label="Close" onClick={close}>
            <svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true">
              <path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" stroke-width="1.5" />
            </svg>
          </button>
        </header>
        <div class="phone-sheet-body">
          <Switch>
            <Match when={view.sheet()?.kind === 'more'}>
              <More actions={props.actions} keymap={props.keymap} />
            </Match>
            <Match when={view.sheet()?.kind === 'settings'}>
              <SettingsView
                transport={transport}
                section={section}
                onSection={(name) => view.setSheet({ kind: 'settings', section: name })}
                {...(props.extraSettings ? { extra: props.extraSettings } : {})}
              />
            </Match>
          </Switch>
        </div>
      </section>
    </Show>
  );
}

/** More: what the status bar of the desk shows, as a list, and two actions. */
function More(props: { actions: ActionContext; keymap: () => Keymap }) {
  const { stores, view, transport, platform } = useApp();
  const machine = () => stores.link.state.byKey['_']?.text ?? 'local';
  const health = () => stores.health.state.byKey['_'];
  const [stopping, setStopping] = createSignal(false);
  const [stopped, setStopped] = createSignal<string | null>(null);
  const run = (command: string): void => {
    view.setSheet(null);
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
    <div class="phone-more">
      <ul class="phone-more-list" aria-label="Commands">
        <For each={FUNCTION_KEYS}>
          {(key) => (
            <li>
              <button
                type="button"
                class="phone-more-row"
                data-command={key.command}
                onClick={() => run(key.command)}
              >
                <span>{key.label}</span>
                <Show when={chordText(props.keymap(), key.command, platform)}>
                  {(chord) => <kbd class="cap">{chord()}</kbd>}
                </Show>
              </button>
            </li>
          )}
        </For>
      </ul>
      <dl class="phone-more-facts">
        <dt>Machine</dt>
        <dd>{machine()}</dd>
        <dt>Renderer</dt>
        <dd>{health()?.renderer ?? 'none'}</dd>
        <For each={stores.limits.rows()}>
          {(limit) => (
            <Show when={limitText(limit, harnessName(limit.harness))}>
              {(text) => (
                <>
                  <dt>{harnessName(limit.harness)}</dt>
                  <dd>{text()}</dd>
                </>
              )}
            </Show>
          )}
        </For>
      </dl>
      <Show when={(health()?.warnings.length ?? 0) > 0}>
        <ul class="phone-more-warnings" aria-label="Warnings">
          <For each={health()?.warnings}>{(warning) => <li>{warning.text}</li>}</For>
        </ul>
      </Show>
      <div class="phone-more-actions">
        <button
          type="button"
          class="phone-more-row"
          onClick={() => view.setSheet({ kind: 'settings', section: null })}
        >
          <span>Settings</span>
        </button>
        <Show
          when={stopping()}
          fallback={
            <button
              type="button"
              class="phone-more-row danger"
              onClick={() => {
                setStopped(null);
                setStopping(true);
              }}
            >
              <span>Stop everything</span>
            </button>
          }
        >
          <div class="phone-more-confirm" role="alert">
            <p>Stop the program of every pane? The agents get 2 s to end.</p>
            <button type="button" class="card-button danger" onClick={stopEverything}>
              Stop everything
            </button>
            <button type="button" class="card-button" onClick={() => setStopping(false)}>
              Cancel
            </button>
          </div>
        </Show>
        <Show when={stopped()}>
          <p class="phone-more-note" role="status">
            {stopped()}
          </p>
        </Show>
      </div>
    </div>
  );
}
