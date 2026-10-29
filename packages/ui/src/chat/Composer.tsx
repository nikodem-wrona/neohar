// The field under the chat of a pane. Enter sends the text with `agent.reply`, which pastes it into
// the pane and presses Enter, at most once for its `input_id`. The core takes a reply only when
// the turn of the agent ended and no interaction waits, so the field says why it cannot send. Esc
// in an empty field, or Stop, interrupts a turn. A `/` or a `$` at the start, or the button +,
// opens the list of skills that the core gives (`agent.skills`), and a pick puts the text of the
// skill in the field. The field grows with its text. The text stays in the view state of the
// window while the pane shows the terminal.

import { RpcError, type AgentValue, type Skill } from '@neohar/transport';
import { createEffect, createSignal, For, on, onCleanup, Show } from 'solid-js';
import { useApp } from '../app/context';
import { replyState, type ReplyState } from '../drawer/group';
import { moveChoice, skillQuery } from './model';

/** The line under the field when it cannot send now, and why. */
const HINTS: Record<ReplyState, string | null> = {
  ready: null,
  working: 'The agent works. Send when its turn ends, or stop it.',
  waiting: 'The agent waits for an answer. Answer the card first.',
  gone: 'The agent ended.',
};
/** The tallest the field grows, in pixels. Then it scrolls. */
const FIELD_MAX_PX = 200;

export function Composer(props: {
  pane: string;
  run: string;
  agent: AgentValue | undefined;
  /** The name of the harness, for the placeholder. */
  harness: string;
  focused: boolean;
}) {
  const { transport, input, report, run: call, view, form } = useApp();
  // The view state holds the text, so a path that drops on the pane lands in the field.
  const text = () => view.draft(props.pane);
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [skills, setSkills] = createSignal<Skill[]>([]);
  const [choice, setChoice] = createSignal(0);
  const [dismissed, setDismissed] = createSignal(false);
  let field: HTMLTextAreaElement | undefined;
  /** The id of the text that did not go out yet: a second try of the same text keeps it. */
  let pending: number | null = null;
  /** The number of the last request for skills: an older answer that comes late is dropped. */
  let asked = 0;
  const state = () => replyState(props.agent);
  const listOpen = () => !dismissed() && skills().length > 0;

  const setText = (value: string): void => {
    view.setDraft(props.pane, value);
    pending = null;
  };

  createEffect(
    on(
      () => skillQuery(text()),
      (query) => {
        const request = ++asked;
        setDismissed(false);
        if (query === null) {
          setSkills([]);
          return;
        }
        transport.call('agent.skills', { run: props.run, query }).then(
          (result) => {
            if (request !== asked) return;
            setSkills(result.skills);
            setChoice(0);
          },
          (failure: unknown) => {
            if (request === asked) setSkills([]);
            report(failure);
          },
        );
      },
    ),
  );
  onCleanup(() => {
    asked += 1;
  });

  // The field is as tall as its text, up to a limit.
  createEffect(
    on(text, () => {
      if (!field) return;
      field.style.height = 'auto';
      field.style.height = `${Math.min(field.scrollHeight, FIELD_MAX_PX)}px`;
    }),
  );

  // The field takes the keyboard while its pane has the focus, as the surface of a terminal does.
  // A phone opens its keyboard only for a tap.
  createEffect(() => {
    const overlay =
      view.paletteOpen() ||
      view.searchOpen() ||
      view.closing() !== null ||
      view.renaming() !== null;
    if (form === 'phone') return;
    if (props.focused && !overlay && field && document.activeElement !== field) {
      field.focus({ preventScroll: true });
    }
  });

  /** The button +: the list of skills, from an empty field. */
  const openSkills = (): void => {
    if (text() === '') setText('/');
    else setDismissed(false);
    field?.focus();
  };

  const pick = (skill: Skill | undefined): void => {
    if (!skill) return;
    setText(skill.insert);
    field?.focus();
  };

  const send = async (): Promise<void> => {
    const value = text();
    if (busy() || value.trim() === '' || state() !== 'ready') return;
    setBusy(true);
    setError(null);
    pending ??= input.nextId();
    try {
      await transport.call('agent.reply', { run: props.run, input_id: pending, text: value });
      setText('');
    } catch (failure) {
      setError(failure instanceof RpcError ? failure.message : String(failure));
    } finally {
      setBusy(false);
    }
  };

  const interrupt = (): void => call('agent.interrupt', { agent: props.run });

  const onKeyDown = (event: KeyboardEvent): void => {
    if (event.isComposing) return;
    if (listOpen()) {
      const count = skills().length;
      if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
        event.preventDefault();
        setChoice((at) => moveChoice(at, event.key === 'ArrowDown' ? 1 : -1, count));
        return;
      }
      if (event.key === 'Enter' || event.key === 'Tab') {
        event.preventDefault();
        pick(skills()[choice()]);
        return;
      }
      if (event.key === 'Escape') {
        event.preventDefault();
        setDismissed(true);
        return;
      }
    }
    if (event.key === 'Enter' && !event.shiftKey) {
      event.preventDefault();
      send().catch(report);
    } else if (event.key === 'Escape' && text() === '' && state() === 'working') {
      event.preventDefault();
      interrupt();
    }
  };

  return (
    <form
      class="chat-composer"
      aria-label="Message"
      onSubmit={(event) => {
        event.preventDefault();
        send().catch(report);
      }}
    >
      <Show when={listOpen()}>
        <ul class="chat-skills" role="listbox" aria-label="Skills">
          <For each={skills()}>
            {(skill, at) => (
              <li
                class="chat-skill"
                role="option"
                aria-selected={at() === choice()}
                data-source={skill.source}
                onPointerDown={(event) => {
                  // The field keeps the focus.
                  event.preventDefault();
                  pick(skill);
                }}
              >
                <span class="chat-skill-name">{skill.insert.trim()}</span>
                <Show when={skill.description}>
                  <span class="chat-skill-text">{skill.description}</span>
                </Show>
              </li>
            )}
          </For>
        </ul>
      </Show>
      <div class="chat-box">
        <button
          type="button"
          class="chat-round chat-plus"
          aria-label="Skills and commands"
          title="Skills and commands"
          onPointerDown={(event) => event.preventDefault()}
          onClick={openSkills}
        >
          <svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true">
            <path d="M8 3v10M3 8h10" stroke="currentColor" stroke-width="1.6" />
          </svg>
        </button>
        <textarea
          ref={(element) => (field = element)}
          class="chat-field"
          rows="1"
          placeholder={`Message ${props.harness}`}
          aria-label="Message"
          value={text()}
          onInput={(event) => setText(event.currentTarget.value)}
          onKeyDown={onKeyDown}
        />
        <Show
          when={state() === 'working'}
          fallback={
            <button
              type="submit"
              class="chat-round chat-send"
              aria-label="Send"
              title="Send"
              disabled={busy() || text().trim() === '' || state() !== 'ready'}
            >
              <svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true">
                <path
                  d="M8 13V3M3.5 7.5 8 3l4.5 4.5"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="1.8"
                  stroke-linecap="round"
                  stroke-linejoin="round"
                />
              </svg>
            </button>
          }
        >
          <button
            type="button"
            class="chat-round chat-stop"
            aria-label="Stop"
            title="Stop"
            onClick={interrupt}
          >
            <svg viewBox="0 0 16 16" width="12" height="12" aria-hidden="true">
              <rect x="3" y="3" width="10" height="10" rx="2" fill="currentColor" />
            </svg>
          </button>
        </Show>
      </div>
      <Show when={error()}>
        <p class="chat-error" role="alert">
          {error()}
        </p>
      </Show>
      <Show when={HINTS[state()]}>{(hint) => <p class="chat-hint">{hint()}</p>}</Show>
    </form>
  );
}
