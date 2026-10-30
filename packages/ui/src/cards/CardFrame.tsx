// The frame that every card shares, as the Card of shadcn/ui (ui.shadcn.com) lays it out: the
// head with the title on the left and the risk, the state and the handle on the right, the body,
// the line for what the person must know, and the error of the last click. A card that is not
// open is one row: its title and a badge that says what became of it. A toast says that an answer
// of this window arrived (`cards/deliveries.ts`). Cards never overlay a pane; the drawer lays them
// out.

import type { InteractionValue } from '@neohar/transport';
import { type JSX, Show } from 'solid-js';
import { Badge } from '../kit/Badge';
import { stateBadge, type Status, statusOf } from './model';

export function CardFrame(props: {
  interaction: InteractionValue;
  harness: string;
  error: string | null;
  /** The title while the card is open, when it is not the title of the interaction. */
  title?: string | undefined;
  children: JSX.Element;
  onKeyDown?: (event: KeyboardEvent) => void;
  ref?: ((element: HTMLElement) => void) | undefined;
}) {
  const open = () => props.interaction.state === 'open';
  const status = (): Status | null => statusOf(props.interaction, props.harness);
  const badge = (): Status | null => stateBadge(props.interaction, props.harness);
  return (
    <article
      class="card"
      classList={{
        [`card-${props.interaction.kind}`]: true,
        [`risk-${props.interaction.risk}`]: props.interaction.kind === 'approval',
        'card-ended': !open(),
      }}
      data-interaction={props.interaction.id}
      data-state={props.interaction.state}
      tabindex="0"
      aria-label={`${props.interaction.handle}: ${props.interaction.title}`}
      onKeyDown={props.onKeyDown}
      ref={props.ref}
    >
      <header class="card-head">
        <h3 class="card-title">
          {open() ? (props.title ?? props.interaction.title) : props.interaction.title}
        </h3>
        <div class="card-meta">
          <Show when={open() && props.interaction.kind === 'approval'}>
            <Badge
              class="card-risk"
              data-risk={props.interaction.risk}
              tone={props.interaction.risk === 'high' ? 'error' : 'neutral'}
            >
              {props.interaction.risk} risk
            </Badge>
          </Show>
          <Show when={badge()}>
            {(shown) => (
              <Badge class="card-state" tone={shown().tone}>
                {shown().text}
              </Badge>
            )}
          </Show>
          <span class="card-handle">{props.interaction.handle}</span>
        </div>
      </header>
      <Show when={open()}>{props.children}</Show>
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
