// What a card offers and what it says. The core decides what is
// true: the risk, the state of the delivery, who may answer. This module only turns those facts
// into buttons, defaults and sentences, and builds the `Decision` that a click sends.

import type {
  Decision,
  InteractionState,
  InteractionValue,
  Question,
  QuestionAnswer,
  Risk,
} from '@neohar/transport';

export type ApprovalChoice = 'allow' | 'allow_always' | 'deny' | 'deny_stop';

export interface ChoiceButton {
  id: ApprovalChoice;
  label: string;
  /** The choice refuses the request. */
  refuses: boolean;
  /** The number key that names the button. A button keeps its key when another one is not shown. */
  key: number;
}

/**
 * The buttons of an approval, in order. "Allow always" shows the rule that it adds, such as
 * "Allow `pnpm test:*` always", and is there only when the core sends that rule: the core
 * refuses "always" for a request that no rule covers alone.
 */
export function approvalButtons(interaction: InteractionValue): ChoiceButton[] {
  const rule = interaction.always_rule;
  return [
    { id: 'allow', label: 'Allow', refuses: false, key: 1 },
    ...(rule
      ? [
          {
            id: 'allow_always' as const,
            label: `Allow \`${rule}\` always`,
            refuses: false,
            key: 2,
          },
        ]
      : []),
    { id: 'deny', label: 'Deny', refuses: true, key: 3 },
    { id: 'deny_stop', label: 'Deny and stop', refuses: true, key: 4 },
  ];
}

/** The button that a number key names, if the card shows it. */
export function buttonForKey(
  buttons: readonly ChoiceButton[],
  key: string,
): ChoiceButton | undefined {
  return buttons.find((button) => String(button.key) === key);
}

/** The choice that is selected when the card opens: Deny for a high risk. */
export function defaultChoice(risk: Risk): ApprovalChoice {
  return risk === 'high' ? 'deny' : 'allow';
}

/** Enter never answers a high-risk approval. */
export function enterAnswers(risk: Risk): boolean {
  return risk !== 'high';
}

export function approvalDecision(choice: ApprovalChoice, message?: string): Decision {
  const text = message?.trim();
  switch (choice) {
    case 'allow':
      return { kind: 'approval', behavior: 'allow' };
    case 'allow_always':
      return { kind: 'approval', behavior: 'allow', always: true };
    case 'deny':
      return { kind: 'approval', behavior: 'deny', ...(text ? { message: text } : {}) };
    case 'deny_stop':
      return {
        kind: 'approval',
        behavior: 'deny',
        stop: true,
        ...(text ? { message: text } : {}),
      };
  }
}

/** The labels chosen after a click on `label`: one choice replaces, many toggle. */
export function toggled(selected: readonly string[], label: string, multi: boolean): string[] {
  if (!multi) return [label];
  return selected.includes(label) ? selected.filter((l) => l !== label) : [...selected, label];
}

export function questionAnswer(
  question: Question,
  selected: string[],
  text: string,
): QuestionAnswer {
  const typed = text.trim();
  return { question: question.question, selected, ...(typed ? { text: typed } : {}) };
}

export function questionDecision(answers: QuestionAnswer[]): Decision {
  return { kind: 'question', answers };
}

/** Every question has a choice or a text. */
export function questionComplete(answers: readonly QuestionAnswer[], count: number): boolean {
  return (
    answers.length === count &&
    answers.every((a) => a.selected.length > 0 || (a.text?.length ?? 0) > 0)
  );
}

export function planDecision(approve: boolean, feedback: string): Decision {
  const text = feedback.trim();
  return { kind: 'plan_review', approve, ...(!approve && text ? { feedback: text } : {}) };
}

/** A card the user can answer now. */
export function canAnswer(interaction: InteractionValue): boolean {
  return interaction.state === 'open' && interaction.answerable;
}

/** The card is for reading: the core cannot reach the dialog. */
export function displayOnly(interaction: InteractionValue): boolean {
  return interaction.state === 'open' && !interaction.answerable && interaction.kind !== 'notice';
}

export type Tone = 'info' | 'ok' | 'warn' | 'error' | 'muted';

export interface Status {
  text: string;
  tone: Tone;
}

const ENDED: Partial<Record<InteractionState, Tone>> = {
  delivered: 'ok',
  resolved_elsewhere: 'muted',
  dismissed: 'muted',
  cancelled: 'muted',
  superseded: 'muted',
};

/** Who answered, for the line under a card: `ui via card`. */
export function answeredBy(interaction: InteractionValue): string {
  const { decided_by: by, via } = interaction.delivery;
  if (by === 'policy') return 'a rule of the policy';
  if (by && via) return `${by} via ${via}`;
  return by ?? via ?? 'someone';
}

/**
 * What the card says about the delivery of the answer. `harness` is the name of
 * the program, such as `Claude Code`.
 */
export function statusOf(interaction: InteractionValue, harness: string): Status | null {
  const by = answeredBy(interaction);
  switch (interaction.state) {
    case 'open':
      return displayOnly(interaction)
        ? { text: 'Display only. Link this agent to a pane to answer.', tone: 'warn' }
        : null;
    case 'decision_recorded':
    case 'delivering':
      return { text: `Sending the answer of ${by}…`, tone: 'info' };
    case 'delivered':
      return { text: `Delivered. Answered by ${by}.`, tone: ENDED.delivered ?? 'ok' };
    case 'delivery_unknown':
      return { text: `Couldn't confirm your answer reached ${harness}.`, tone: 'warn' };
    case 'failed': {
      const reason = interaction.delivery.fail_reason;
      return {
        text: `Couldn't deliver the answer to ${harness}${reason ? `: ${reason.replaceAll('_', ' ')}` : ''}.`,
        tone: 'error',
      };
    }
    case 'resolved_elsewhere':
      return { text: `Answered in ${harness}.`, tone: 'muted' };
    case 'dismissed':
      return { text: 'Dismissed.', tone: 'muted' };
    case 'cancelled':
    case 'superseded':
      return { text: 'Not needed any more.', tone: 'muted' };
  }
}
