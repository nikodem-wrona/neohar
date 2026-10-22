// The frame that every card shares: the title with the risk and the handle, the body, the line
// that says what became of the answer, and the error of the last click. Cards never overlay a
// pane; the drawer lays them out.

import type { InteractionValue } from '@neohar/transport';
import { type JSX, Show } from 'solid-js';
import { type Status, statusOf } from './model';

export function CardFrame(props: {
  interaction: InteractionValue;
  harness: string;
  error: string | null;
  children: JSX.Element;
  onKeyDown?: (event: KeyboardEvent) => void;
  ref?: ((element: HTMLElement) => void) | undefined;
}) {
  const status = (): Status | null => statusOf(props.interaction, props.harness);
  return (
    <article
      class="card"
      classList={{
        [`card-${props.interaction.kind}`]: true,
        [`risk-${props.interaction.risk}`]: props.interaction.kind === 'approval',
        'card-ended': props.interaction.state !== 'open',
      }}
      data-interaction={props.interaction.id}
      data-state={props.interaction.state}
      tabindex="0"
      aria-label={`${props.interaction.handle}: ${props.interaction.title}`}
      onKeyDown={props.onKeyDown}
      ref={props.ref}
    >
      <header class="card-head">
        <h3 class="card-title">{props.interaction.title}</h3>
        <Show when={props.interaction.kind === 'approval'}>
          <span class="card-risk" data-risk={props.interaction.risk}>
            {props.interaction.risk} risk
          </span>
        </Show>
        <span class="card-handle">{props.interaction.handle}</span>
      </header>
      {props.children}
      <Show when={status()}>
        {(line) => (
          <p class="card-status" data-tone={line().tone} role="status">
            {line().text}
          </p>
        )}
      </Show>
      <Show when={props.error}>
        {(message) => (
          <p class="card-error" role="alert">
            {message()}
          </p>
        )}
      </Show>
    </article>
  );
}
