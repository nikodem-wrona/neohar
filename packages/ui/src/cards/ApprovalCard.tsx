// The card of an approval: the command with its highlighted words, the
// directory, the risk and the reasons for it, the hint of the policy, the diff, the other input of
// the tool as text, and the last lines of the pane. The buttons are Allow, "Allow `<rule>` always"
// when the core sends a rule, Deny and Deny and stop. A high risk selects Deny, and Enter never
// answers it. The keys 1 to 4 name the buttons; a button keeps its key when "Allow always" is not
// there. At a low or medium risk a number answers. At a high risk it only selects its button and
// gives it the focus, so the answer takes a second key on the button.

import type { InteractionValue } from '@neohar/transport';
import { createSignal, For, Show } from 'solid-js';
import { Input } from '../kit/Input';
import { Diff } from '../rich/Diff';
import { RichSpans } from '../rich/Spans';
import { sendAnswer } from './answer';
import { CardFrame } from './CardFrame';
import { useCardHost } from './host';
import {
  type ApprovalChoice,
  approvalButtons,
  approvalDecision,
  buttonForKey,
  canAnswer,
  defaultChoice,
  enterAnswers,
} from './model';

type Approval = Extract<InteractionValue['payload'], { kind: 'approval' }>;

export function ApprovalCard(props: {
  interaction: InteractionValue;
  payload: Approval;
  harness: string;
  ref?: ((element: HTMLElement) => void) | undefined;
}) {
  const host = useCardHost();
  const { report } = host;
  const [choice, setChoice] = createSignal<ApprovalChoice>(defaultChoice(props.interaction.risk));
  const [message, setMessage] = createSignal('');
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const command = () => props.payload.command;
  const shownButtons = () => approvalButtons(props.interaction);
  const buttons = new Map<ApprovalChoice, HTMLButtonElement>();

  const answer = async (picked: ApprovalChoice) => {
    if (busy() || !canAnswer(props.interaction)) return;
    setChoice(picked);
    setBusy(true);
    setError(await sendAnswer(host, props.interaction, approvalDecision(picked, message())));
    setBusy(false);
  };
  // Enter on the card answers with the selected choice, unless the risk is high. A button keeps
  // its own Enter.
  const onKeyDown = (event: KeyboardEvent) => {
    const typing = event.target instanceof HTMLInputElement;
    const numbered = buttonForKey(shownButtons(), event.key);
    if (numbered && !typing && !event.metaKey && !event.ctrlKey && !event.altKey) {
      event.preventDefault();
      if (enterAnswers(props.interaction.risk)) {
        answer(numbered.id).catch(report);
      } else {
        setChoice(numbered.id);
        buttons.get(numbered.id)?.focus();
      }
      return;
    }
    if (event.key !== 'Enter' || event.target !== event.currentTarget) return;
    if (enterAnswers(props.interaction.risk)) answer(choice()).catch(report);
  };

  return (
    <CardFrame
      interaction={props.interaction}
      harness={props.harness}
      error={error()}
      onKeyDown={onKeyDown}
      ref={props.ref}
    >
      <Show when={command()}>
        {(shown) => (
          <>
            <pre class="card-command" aria-label="Command">
              <RichSpans value={shown().display} />
            </pre>
            <Show when={shown().reasons.length > 0}>
              <ul class="card-reasons" aria-label="Why this risk">
                <For each={shown().reasons}>{(reason) => <li>{reason}</li>}</For>
              </ul>
            </Show>
            <Show when={shown().policy_hint}>
              <p class="card-hint">{shown().policy_hint}</p>
            </Show>
          </>
        )}
      </Show>
      <Show when={props.payload.cwd}>
        <p class="card-cwd">
          <span class="card-label">In</span> {props.payload.cwd}
        </p>
      </Show>
      <Show when={!command() && (props.payload.paths?.length ?? 0) > 0}>
        <ul class="card-paths" aria-label="Files">
          <For each={props.payload.paths}>{(path) => <li>{path}</li>}</For>
        </ul>
      </Show>
      <Show when={props.payload.policy_note}>
        <p class="card-hint">{props.payload.policy_note}</p>
      </Show>
      <Show when={props.payload.diff}>{(diff) => <Diff value={diff()} />}</Show>
      <Show when={props.payload.input !== undefined && props.payload.input !== null}>
        <details class="card-input">
          <summary>Input of {props.payload.tool}</summary>
          <pre>{JSON.stringify(props.payload.input, null, 2)}</pre>
        </details>
      </Show>
      <Show when={(props.payload.peek?.length ?? 0) > 0}>
        <pre class="card-peek" role="region" aria-label="The last lines of the pane">
          {props.payload.peek?.join('\n')}
        </pre>
      </Show>
      <Show when={canAnswer(props.interaction)}>
        <div class="card-actions" role="group" aria-label="Answer">
          <For each={shownButtons()}>
            {(button) => (
              <button
                type="button"
                class="card-button"
                classList={{ refuses: button.refuses, selected: choice() === button.id }}
                data-choice={button.id}
                aria-pressed={choice() === button.id}
                aria-keyshortcuts={String(button.key)}
                ref={(element) => buttons.set(button.id, element)}
                disabled={busy()}
                onClick={() => void answer(button.id).catch(report)}
              >
                <kbd class="card-key" aria-hidden="true">
                  {button.key}
                </kbd>
                {button.label}
              </button>
            )}
          </For>
          <Input
            class="card-message"
            placeholder="Message for the agent (optional)"
            aria-label="Message for the agent"
            value={message()}
            onInput={(event) => setMessage(event.currentTarget.value)}
          />
        </div>
        <Show when={!enterAnswers(props.interaction.risk)}>
          <p class="card-note">
            Enter does not answer a high-risk request. Press a number, then Enter on its button.
          </p>
        </Show>
      </Show>
    </CardFrame>
  );
}
