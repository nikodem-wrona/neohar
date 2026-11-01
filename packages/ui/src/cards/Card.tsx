// One card for an interaction of the `interactions` projection.

import type { InteractionPayload, InteractionValue } from '@neohar/transport';
import { memo, type Ref } from 'react';
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

export const Card = memo(function Card(props: {
  interaction: InteractionValue;
  /** The name of the program that the run belongs to, such as `Claude Code`. */
  harness: string;
  ref?: Ref<HTMLElement> | undefined;
}) {
  const approval = payloadOf(props.interaction, 'approval');
  if (approval) {
    return (
      <ApprovalCard
        interaction={props.interaction}
        payload={approval}
        harness={props.harness}
        ref={props.ref}
      />
    );
  }
  const question = payloadOf(props.interaction, 'question');
  if (question) {
    return (
      <QuestionCard
        interaction={props.interaction}
        payload={question}
        harness={props.harness}
        ref={props.ref}
      />
    );
  }
  const plan = payloadOf(props.interaction, 'plan_review');
  if (plan) {
    return (
      <PlanCard
        interaction={props.interaction}
        payload={plan}
        harness={props.harness}
        ref={props.ref}
      />
    );
  }
  const notice = payloadOf(props.interaction, 'notice');
  if (notice) {
    return (
      <NoticeCard
        interaction={props.interaction}
        payload={notice}
        harness={props.harness}
        ref={props.ref}
      />
    );
  }
  return null;
});
