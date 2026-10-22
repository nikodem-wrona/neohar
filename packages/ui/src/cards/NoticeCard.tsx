// The card of a notice: a text, the time when a limit resets, and the buttons Open and
// Dismiss.

import type { InteractionValue } from '@neohar/transport';
import { createSignal, Show } from 'solid-js';
import { sendDismiss } from './answer';
import { CardFrame } from './CardFrame';
import { useCardHost } from './host';

type Notice = Extract<InteractionValue['payload'], { kind: 'notice' }>;

export function NoticeCard(props: {
  interaction: InteractionValue;
  payload: Notice;
  harness: string;
  ref?: ((element: HTMLElement) => void) | undefined;
}) {
  const { transport, open: openPane, report } = useCardHost();
  const [error, setError] = createSignal<string | null>(null);
  const open = () => props.interaction.state === 'open';
  return (
    <CardFrame
      interaction={props.interaction}
      harness={props.harness}
      error={error()}
      ref={props.ref}
    >
      <p class="card-text">{props.payload.text}</p>
      <Show when={props.payload.resets_at}>
        {(at) => (
          <p class="card-cwd">
            <span class="card-label">Resets</span> <time datetime={at()}>{at()}</time>
          </p>
        )}
      </Show>
      <Show when={open()}>
        <div class="card-actions" role="group" aria-label="Notice">
          <Show when={openPane && props.interaction.pane}>
            {(pane) => (
              <button type="button" class="card-button" onClick={() => openPane?.(pane())}>
                Open
              </button>
            )}
          </Show>
          <button
            type="button"
            class="card-button"
            onClick={() => void sendDismiss(transport, props.interaction).then(setError, report)}
          >
            Dismiss
          </button>
        </div>
      </Show>
    </CardFrame>
  );
}
