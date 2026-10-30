// The card of a plan review: the plan as sanitized Markdown, a box for feedback, and
// the buttons Approve and Request changes.

import type { InteractionValue } from '@neohar/transport';
import { type Ref, useState } from 'react';
import { RichHtml } from '../rich/RichHtml';
import { sendAnswer } from './answer';
import { CardFrame } from './CardFrame';
import { useCardHost } from './host';
import { canAnswer, planDecision } from './model';

type Plan = Extract<InteractionValue['payload'], { kind: 'plan_review' }>;

export function PlanCard(props: {
  interaction: InteractionValue;
  payload: Plan;
  harness: string;
  ref?: Ref<HTMLElement> | undefined;
}) {
  const host = useCardHost();
  const { report } = host;
  const [feedback, setFeedback] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const answer = async (approve: boolean) => {
    if (busy || !canAnswer(props.interaction)) return;
    setBusy(true);
    setError(await sendAnswer(host, props.interaction, planDecision(approve, feedback)));
    setBusy(false);
  };
  return (
    <CardFrame
      interaction={props.interaction}
      harness={props.harness}
      error={error}
      ref={props.ref}
    >
      <RichHtml html={props.payload.plan.html} />
      {props.payload.plan.truncated ? (
        <p className="card-note">The plan is cut at the size limit.</p>
      ) : null}
      {props.payload.plan_path ? (
        <p className="card-cwd">
          <span className="card-label">File</span> {props.payload.plan_path}
        </p>
      ) : null}
      {canAnswer(props.interaction) ? (
        <>
          <textarea
            className="card-feedback"
            placeholder="What should change? (for Request changes)"
            aria-label="Feedback for the agent"
            value={feedback}
            onChange={(event) => setFeedback(event.currentTarget.value)}
          />
          <div className="card-actions" role="group" aria-label="Answer">
            <button
              type="button"
              className="card-button selected"
              data-choice="approve"
              disabled={busy}
              onClick={() => void answer(true).catch(report)}
            >
              Approve
            </button>
            <button
              type="button"
              className="card-button"
              disabled={busy}
              onClick={() => void answer(false).catch(report)}
            >
              Request changes
            </button>
          </div>
        </>
      ) : null}
    </CardFrame>
  );
}
