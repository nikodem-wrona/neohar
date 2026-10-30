// The frame that every card shares, as the Card of shadcn/ui (ui.shadcn.com) lays it out: the
// head with the title on the left and the risk, the state and the handle on the right, the body,
// the line for what the person must know, and the error of the last click. A card that is not
// open is one row: its title and a badge that says what became of it. A toast says that an answer
// of this window arrived (`cards/deliveries.ts`). Cards never overlay a pane; the drawer lays them
// out.

import type { InteractionValue } from '@neohar/transport';
import type { KeyboardEvent, ReactNode, Ref } from 'react';
import { Badge } from '../components/ui/badge';
import { cn } from '../lib/utils';
import { stateBadge, type Status, statusOf } from './model';

export function CardFrame(props: {
  interaction: InteractionValue;
  harness: string;
  error: string | null;
  /** The title while the card is open, when it is not the title of the interaction. */
  title?: string | undefined;
  children: ReactNode;
  onKeyDown?: ((event: KeyboardEvent<HTMLElement>) => void) | undefined;
  ref?: Ref<HTMLElement> | undefined;
}) {
  const open = props.interaction.state === 'open';
  const status: Status | null = statusOf(props.interaction, props.harness);
  const badge: Status | null = stateBadge(props.interaction, props.harness);
  return (
    <article
      className={cn('card', {
        [`card-${props.interaction.kind}`]: true,
        [`risk-${props.interaction.risk}`]: props.interaction.kind === 'approval',
        'card-ended': !open,
      })}
      data-interaction={props.interaction.id}
      data-state={props.interaction.state}
      tabIndex={0}
      aria-label={`${props.interaction.handle}: ${props.interaction.title}`}
      onKeyDown={props.onKeyDown}
      ref={props.ref}
    >
      <header className="card-head">
        <h3 className="card-title">
          {open ? (props.title ?? props.interaction.title) : props.interaction.title}
        </h3>
        <div className="card-meta">
          {open && props.interaction.kind === 'approval' ? (
            <Badge
              className="card-risk"
              data-risk={props.interaction.risk}
              variant={props.interaction.risk === 'high' ? 'error' : 'neutral'}
            >
              {props.interaction.risk} risk
            </Badge>
          ) : null}
          {badge ? (
            <Badge className="card-state" variant={badge.tone}>
              {badge.text}
            </Badge>
          ) : null}
          <span className="card-handle">{props.interaction.handle}</span>
        </div>
      </header>
      {open ? props.children : null}
      {status ? (
        <p className="card-status" data-tone={status.tone} role="status">
          {status.text}
        </p>
      ) : null}
      {props.error ? (
        <p className="card-error" role="alert">
          {props.error}
        </p>
      ) : null}
    </article>
  );
}
