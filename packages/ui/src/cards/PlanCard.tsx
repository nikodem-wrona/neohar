// The card of a plan review: the plan as sanitized Markdown, a box for feedback, and
// the buttons Approve and Request changes.

import type { InteractionValue } from '@neohar/transport';
import { createSignal, Show } from 'solid-js';
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
  ref?: ((element: HTMLElement) => void) | undefined;
}) {
  const { transport, via, report } = useCardHost();
  const [feedback, setFeedback] = createSignal('');
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const answer = async (approve: boolean) => {
    if (busy() || !canAnswer(props.interaction)) return;
    setBusy(true);
    setError(
      await sendAnswer(transport, props.interaction, planDecision(approve, feedback()), via),
    );
    setBusy(false);
  };
  return (
    <CardFrame
      interaction={props.interaction}
      harness={props.harness}
      error={error()}
      ref={props.ref}
    >
      <RichHtml html={props.payload.plan.html} />
      <Show when={props.payload.plan.truncated}>
        <p class="card-note">The plan is cut at the size limit.</p>
      </Show>
      <Show when={props.payload.plan_path}>
        <p class="card-cwd">
          <span class="card-label">File</span> {props.payload.plan_path}
        </p>
      </Show>
      <Show when={canAnswer(props.interaction)}>
        <textarea
          class="card-feedback"
          placeholder="What should change? (for Request changes)"
          aria-label="Feedback for the agent"
          value={feedback()}
          onInput={(event) => setFeedback(event.currentTarget.value)}
        />
        <div class="card-actions" role="group" aria-label="Answer">
          <button
            type="button"
            class="card-button selected"
            data-choice="approve"
            disabled={busy()}
            onClick={() => void answer(true).catch(report)}
          >
            Approve
          </button>
          <button
            type="button"
            class="card-button"
            disabled={busy()}
            onClick={() => void answer(false).catch(report)}
          >
            Request changes
          </button>
        </div>
      </Show>
    </CardFrame>
  );
}
