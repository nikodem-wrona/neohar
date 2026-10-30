// The card of a notice: a text, the time when a limit resets, and the buttons Open and
// Dismiss.

import type { InteractionValue } from '@neohar/transport';
import { type Ref, useState } from 'react';
import { sendDismiss } from './answer';
import { CardFrame } from './CardFrame';
import { useCardHost } from './host';

type Notice = Extract<InteractionValue['payload'], { kind: 'notice' }>;

export function NoticeCard(props: {
  interaction: InteractionValue;
  payload: Notice;
  harness: string;
  ref?: Ref<HTMLElement> | undefined;
}) {
  const { transport, open: openPane, report } = useCardHost();
  const [error, setError] = useState<string | null>(null);
  const open = props.interaction.state === 'open';
  const pane = props.interaction.pane;
  return (
    <CardFrame
      interaction={props.interaction}
      harness={props.harness}
      error={error}
      ref={props.ref}
    >
      <p className="card-text">{props.payload.text}</p>
      {props.payload.resets_at ? (
        <p className="card-cwd">
          <span className="card-label">Resets</span>{' '}
          <time dateTime={props.payload.resets_at}>{props.payload.resets_at}</time>
        </p>
      ) : null}
      {open ? (
        <div className="card-actions" role="group" aria-label="Notice">
          {openPane && pane ? (
            <button type="button" className="card-button" onClick={() => openPane(pane)}>
              Open
            </button>
          ) : null}
          <button
            type="button"
            className="card-button"
            onClick={() => void sendDismiss(transport, props.interaction).then(setError, report)}
          >
            Dismiss
          </button>
        </div>
      ) : null}
    </CardFrame>
  );
}
