// The key bar of a phone: a row over the soft keyboard while a pane has it, in
// the place of the bottom bar. It has the keys that the soft keyboard lacks: Esc, Tab, ⇧Tab, the
// arrows, Ctrl and Alt for the next key (a second tap locks them), the keys of `phone.keys`,
// Paste and Hide. ✎ opens a field with the editing and the dictation of iOS: Send gives a run that
// waits for its next prompt `agent.reply`, and any other pane a paste and Enter. Insert pastes.
//
// A button of the bar keeps the focus in the field of the pane, so the keyboard stays.

import { type LogicalInput, RpcError } from '@neohar/transport';
import { type PointerEvent, useState } from 'react';
import { useApp } from '../app/context';
import { phoneKeys } from '../app/preferences';
import { Button } from '../components/ui/button';
import { Textarea } from '../components/ui/textarea';
import type { TypingField } from '../app/view';
import { readClipboard } from '../input/clipboard';
import { replyState } from '../drawer/group';
import { BAR_KEYS, chordOf, ENTER, nextSticky, stickyMods, withMods } from '../input/soft';
import { useSelect, useValue } from '../lib/hooks';

/** A press on a button of the bar must not take the focus from the field. */
const keepFocus = (event: PointerEvent): void => event.preventDefault();

export function KeyBar(props: { field: TypingField }) {
  const { stores, transport, input, view, report } = useApp();
  const [composing, setComposing] = useState(false);
  const [draft, setDraft] = useState('');
  const [error, setError] = useState<string | null>(null);
  const extra = useSelect(stores.settings, (state) => phoneKeys(state.byKey), sameKeys);
  const barCtrl = useValue(view.barCtrl);
  const barAlt = useValue(view.barAlt);
  const mods = stickyMods(barCtrl, barAlt);

  const send = (event: LogicalInput): void => {
    props.field.restart();
    input.send(props.field.pane, event);
    view.usedBarMods();
  };

  /** The run of the agent in the pane, when it waits for its next prompt. */
  const idleRun = (): string | undefined => {
    const run = stores.panes.get().byKey[props.field.pane]?.run;
    if (!run) return undefined;
    return replyState(stores.agents.get().byKey[run]) === 'ready' ? run : undefined;
  };

  const submit = async (enter: boolean): Promise<void> => {
    const text = draft;
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

  // The keys of the bar are a fixed list, so their place is their key.
  const barKey = (item: (typeof BAR_KEYS)[number], at: number) => (
    <button
      key={at}
      type="button"
      className="key"
      aria-label={item.name}
      onPointerDown={keepFocus}
      onClick={() => send(withMods(item.event, mods))}
    >
      {item.label}
    </button>
  );

  return (
    <div className="key-bar" role="toolbar" aria-label="Keys">
      {composing ? (
        // The Textarea and the Buttons of shadcn/ui. The field has 16 px, so that Safari does not
        // zoom, and Send is the one action of the form.
        <form
          className="flex flex-col gap-(--s1)"
          aria-label="Compose"
          onSubmit={(event) => {
            event.preventDefault();
            submit(true).catch(report);
          }}
        >
          <Textarea
            className="min-h-[88px] resize-none rounded-(--r-sm) border-muted-foreground bg-background px-2.5 py-2 font-(family-name:--font-data) text-[16px]/[1.4] field-sizing-fixed md:text-[16px]/[1.4]"
            aria-label="Text for the pane"
            rows={3}
            value={draft}
            onChange={(event) => setDraft(event.currentTarget.value)}
          />
          <div className="flex gap-(--s2)">
            <Button type="submit" className="h-(--control)" disabled={draft.trim() === ''}>
              Send
            </Button>
            <Button
              type="button"
              variant="outline"
              className="h-(--control)"
              disabled={draft.trim() === ''}
              onClick={() => void submit(false).catch(report)}
            >
              Insert
            </Button>
            <Button type="button" variant="outline" className="h-(--control)" onClick={close}>
              Cancel
            </Button>
          </div>
          {error ? (
            <p className="card-error" role="alert">
              {error}
            </p>
          ) : null}
        </form>
      ) : null}
      <div className="key-row">
        {BAR_KEYS.slice(0, 3).map(barKey)}
        <button
          type="button"
          className="key"
          aria-pressed={barCtrl !== 'off'}
          data-locked={barCtrl === 'locked'}
          onPointerDown={keepFocus}
          onClick={() => view.barCtrl.set(nextSticky(view.barCtrl.get()))}
        >
          Ctrl
        </button>
        <button
          type="button"
          className="key"
          aria-pressed={barAlt !== 'off'}
          data-locked={barAlt === 'locked'}
          onPointerDown={keepFocus}
          onClick={() => view.barAlt.set(nextSticky(view.barAlt.get()))}
        >
          Alt
        </button>
        {BAR_KEYS.slice(3).map((item, at) => barKey(item, at + 3))}
        {extra.map((character, at) => (
          <button
            key={at}
            type="button"
            className="key"
            onPointerDown={keepFocus}
            onClick={() =>
              send(mods.length > 0 ? chordOf(character, mods) : { type: 'text', text: character })
            }
          >
            {character}
          </button>
        ))}
        <button
          type="button"
          className="key"
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
          className="key"
          aria-label="Compose"
          aria-pressed={composing}
          onPointerDown={keepFocus}
          onClick={() => (composing ? close() : setComposing(true))}
        >
          ✎
        </button>
        <button
          type="button"
          className="key"
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

/** Whether two lists of keys are the same. */
function sameKeys(a: readonly string[], b: readonly string[]): boolean {
  return a.length === b.length && a.every((key, at) => key === b[at]);
}
