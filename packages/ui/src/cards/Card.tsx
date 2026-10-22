// One card for an interaction of the `interactions` projection.

import type { InteractionPayload, InteractionValue } from '@neohar/transport';
import { Match, Switch } from 'solid-js';
import { ApprovalCard } from './ApprovalCard';
import { NoticeCard } from './NoticeCard';
import { PlanCard } from './PlanCard';
import { QuestionCard } from './QuestionCard';

/** The payload when it has this kind. */
function payloadOf<K extends InteractionPayload['kind']>(
  interaction: InteractionValue,
  kind: K,
): Extract<InteractionPayload, { kind: K }> | undefined {
  return interaction.payload.kind === kind
    ? (interaction.payload as Extract<InteractionPayload, { kind: K }>)
    : undefined;
}

export function Card(props: {
  interaction: InteractionValue;
  /** The name of the program that the run belongs to, such as `Claude Code`. */
  harness: string;
  ref?: ((element: HTMLElement) => void) | undefined;
}) {
  return (
    <Switch>
      <Match when={payloadOf(props.interaction, 'approval')}>
        {(payload) => (
          <ApprovalCard
            interaction={props.interaction}
            payload={payload()}
            harness={props.harness}
            ref={props.ref}
          />
        )}
      </Match>
      <Match when={payloadOf(props.interaction, 'question')}>
        {(payload) => (
          <QuestionCard
            interaction={props.interaction}
            payload={payload()}
            harness={props.harness}
            ref={props.ref}
          />
        )}
      </Match>
      <Match when={payloadOf(props.interaction, 'plan_review')}>
        {(payload) => (
          <PlanCard
            interaction={props.interaction}
            payload={payload()}
            harness={props.harness}
            ref={props.ref}
          />
        )}
      </Match>
      <Match when={payloadOf(props.interaction, 'notice')}>
        {(payload) => (
          <NoticeCard
            interaction={props.interaction}
            payload={payload()}
            harness={props.harness}
            ref={props.ref}
          />
        )}
      </Match>
    </Switch>
  );
}
