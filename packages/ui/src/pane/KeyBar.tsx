// The key bar of a phone: a row over the soft keyboard while a pane has it, in
// the place of the bottom bar. It has the keys that the soft keyboard lacks: Esc, Tab, ⇧Tab, the
// arrows, Ctrl and Alt for the next key (a second tap locks them), the keys of `phone.keys`,
// Paste and Hide. ✎ opens a field with the editing and the dictation of iOS: Send gives a run that
// waits for its next prompt `agent.reply`, and any other pane a paste and Enter. Insert pastes.
//
// A button of the bar keeps the focus in the field of the pane, so the keyboard stays.

import { type LogicalInput, RpcError } from '@neohar/transport';
import { createSignal, For, Show } from 'solid-js';
import { useApp } from '../app/context';
import { phoneKeys } from '../app/preferences';
import type { TypingField } from '../app/view';
import { readClipboard } from '../input/clipboard';
import { replyState } from '../drawer/group';
import { BAR_KEYS, chordOf, ENTER, nextSticky, stickyMods, withMods } from '../input/soft';

/** A press on a button of the bar must not take the focus from the field. */
const keepFocus = (event: PointerEvent): void => event.preventDefault();

export function KeyBar(props: { field: TypingField }) {
  const { stores, transport, input, view, report } = useApp();
  const [composing, setComposing] = createSignal(false);
  const [draft, setDraft] = createSignal('');
  const [error, setError] = createSignal<string | null>(null);
  const extra = () => phoneKeys(stores.settings.state.byKey);
  const mods = () => stickyMods(view.barCtrl(), view.barAlt());

  const send = (event: LogicalInput): void => {
    props.field.restart();
    input.send(props.field.pane, event);
    view.usedBarMods();
  };

  /** The run of the agent in the pane, when it waits for its next prompt. */
  const idleRun = (): string | undefined => {
    const run = stores.panes.state.byKey[props.field.pane]?.run;
    if (!run) return undefined;
    return replyState(stores.agents.state.byKey[run]) === 'ready' ? run : undefined;
  };

  const submit = async (enter: boolean): Promise<void> => {
    const text = draft();
    if (text.trim() === '') return;
    setError(null);
    try {
      const run = enter ? idleRun() : undefined;
      if (run) {
        await transport.call('agent.reply', { run, input_id: input.nextId(), text });
      } else {
        props.field.restart();
        await input.paste(props.field.pane, text);
        if (enter) await input.send(props.field.pane, ENTER);
      }
      setDraft('');
      close();
    } catch (failure) {
      setError(failure instanceof RpcError ? failure.message : String(failure));
    }
  };

  /** Compose closes, and the field of the pane takes the keyboard again. */
  const close = (): void => {
    setComposing(false);
    props.field.focus();
  };

  return (
    <div class="key-bar" role="toolbar" aria-label="Keys">
      <Show when={composing()}>
        <form
          class="key-compose"
          aria-label="Compose"
          onSubmit={(event) => {
            event.preventDefault();
            submit(true).catch(report);
          }}
        >
          <textarea
            class="key-compose-field"
            aria-label="Text for the pane"
            rows="3"
            value={draft()}
            onInput={(event) => setDraft(event.currentTarget.value)}
          />
          <div class="key-compose-buttons">
            <button type="submit" class="card-button" disabled={draft().trim() === ''}>
              Send
            </button>
            <button
              type="button"
              class="card-button"
              disabled={draft().trim() === ''}
              onClick={() => void submit(false).catch(report)}
            >
              Insert
            </button>
            <button type="button" class="card-button" onClick={close}>
              Cancel
            </button>
          </div>
          <Show when={error()}>
            <p class="card-error" role="alert">
              {error()}
            </p>
          </Show>
        </form>
      </Show>
      <div class="key-row">
        <For each={BAR_KEYS.slice(0, 3)}>
          {(item) => (
            <button
              type="button"
              class="key"
              aria-label={item.name}
              onPointerDown={keepFocus}
              onClick={() => send(withMods(item.event, mods()))}
            >
              {item.label}
            </button>
          )}
        </For>
        <button
          type="button"
          class="key"
          aria-pressed={view.barCtrl() !== 'off'}
          data-locked={view.barCtrl() === 'locked'}
          onPointerDown={keepFocus}
          onClick={() => view.setBarCtrl(nextSticky(view.barCtrl()))}
        >
          Ctrl
        </button>
        <button
          type="button"
          class="key"
          aria-pressed={view.barAlt() !== 'off'}
          data-locked={view.barAlt() === 'locked'}
          onPointerDown={keepFocus}
          onClick={() => view.setBarAlt(nextSticky(view.barAlt()))}
        >
          Alt
        </button>
        <For each={BAR_KEYS.slice(3)}>
          {(item) => (
            <button
              type="button"
              class="key"
              aria-label={item.name}
              onPointerDown={keepFocus}
              onClick={() => send(withMods(item.event, mods()))}
            >
              {item.label}
            </button>
          )}
        </For>
        <For each={extra()}>
          {(character) => (
            <button
              type="button"
              class="key"
              onPointerDown={keepFocus}
              onClick={() =>
                send(
                  mods().length > 0
                    ? chordOf(character, mods())
                    : { type: 'text', text: character },
                )
              }
            >
              {character}
            </button>
          )}
        </For>
        <button
          type="button"
          class="key"
          onPointerDown={keepFocus}
          onClick={() => {
            readClipboard().then((text) => {
              props.field.restart();
              input.paste(props.field.pane, text);
            }, report);
          }}
        >
          Paste
        </button>
        <button
          type="button"
          class="key"
          aria-label="Compose"
          aria-pressed={composing()}
          onPointerDown={keepFocus}
          onClick={() => (composing() ? close() : setComposing(true))}
        >
          ✎
        </button>
        <button
          type="button"
          class="key"
          aria-label="Hide the keyboard"
          onPointerDown={keepFocus}
          onClick={() => props.field.hide()}
        >
          Hide
        </button>
      </div>
    </div>
  );
}
