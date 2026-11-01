// What a card offers and what it says. The core decides what is
// true: the risk, the state of the delivery, who may answer. This module only turns those facts
// into buttons, defaults and sentences, and builds the `Decision` that a click sends.

import type { Decision, InteractionValue, Question, QuestionAnswer, Risk } from '@neohar/transport';
import type { ToastText } from '../app/toasts';

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

/**
 * The title of the card of a question while it waits: the header of one question, such as
 * `Auth method`, or how many questions there are. The question itself is the body of the card.
 */
export function questionTitle(questions: readonly Question[]): string {
  if (questions.length === 1) return questions[0]?.header || 'Question';
  return `${questions.length} questions`;
}

/**
 * The badge of a card that is not open: what became of it, in a word or two. `harness` is the
 * name of the program, such as `Claude Code`.
 */
export function stateBadge(interaction: InteractionValue, harness: string): Status | null {
  switch (interaction.state) {
    case 'open':
      return null;
    case 'decision_recorded':
    case 'delivering':
      return { text: 'Sending', tone: 'info' };
    case 'delivered':
      return {
        text: interaction.delivery.decided_by === 'policy' ? 'Answered by policy' : 'Answered',
        tone: 'muted',
      };
    case 'delivery_unknown':
      return { text: 'Unconfirmed', tone: 'warn' };
    case 'failed':
      return { text: 'Not delivered', tone: 'error' };
    case 'resolved_elsewhere':
      return { text: `Answered in ${harness}`, tone: 'muted' };
    case 'dismissed':
      return { text: 'Dismissed', tone: 'muted' };
    case 'cancelled':
    case 'superseded':
      return { text: 'Not needed', tone: 'muted' };
  }
}

/** Why the core could not deliver the answer, in words: `dialog changed`. */
function failReason(interaction: InteractionValue): string | null {
  const reason = interaction.delivery.fail_reason;
  return reason ? reason.replaceAll('_', ' ') : null;
}

/**
 * The line under a card, for what the person must know or do: a card that only shows, or an
 * answer that may not have arrived. The badge says the rest, and a toast says that an answer of
 * this window arrived.
 */
export function statusOf(interaction: InteractionValue, harness: string): Status | null {
  switch (interaction.state) {
    case 'open':
      // The core keeps a card from answers for several reasons: the run has no pane, or the
      // screen showed a dialog whose request neohar could not read. Each time the person
      // answers in the harness.
      return displayOnly(interaction)
        ? { text: `Display only. Answer it in ${harness}.`, tone: 'warn' }
        : null;
    case 'delivery_unknown':
      return { text: `Couldn't confirm your answer reached ${harness}.`, tone: 'warn' };
    case 'failed': {
      const reason = failReason(interaction);
      return {
        text: `Couldn't deliver the answer to ${harness}${reason ? `: ${reason}` : ''}.`,
        tone: 'error',
      };
    }
    default:
      return null;
  }
}

/**
 * Whether an answer that this window sent is at its end: the core delivered it, could not, or
 * the interaction left the projection (`undefined`).
 */
export function answerArrived(interaction: InteractionValue | undefined): boolean {
  if (!interaction) return true;
  const { state } = interaction;
  return state !== 'open' && state !== 'decision_recorded' && state !== 'delivering';
}

/**
 * The toast for an answer of this window at its end, or `null` when there is nothing to say:
 * the answer arrived, or it may not have.
 */
export function answerToast(interaction: InteractionValue, harness: string): ToastText | null {
  switch (interaction.state) {
    case 'delivered':
      return {
        title: `Answer delivered to ${harness}`,
        description: interaction.title,
        tone: 'ok',
      };
    case 'delivery_unknown':
      return {
        title: `Couldn't confirm the answer reached ${harness}`,
        description: interaction.title,
        tone: 'warn',
      };
    case 'failed':
      return {
        title: `Couldn't deliver the answer to ${harness}`,
        description: failReason(interaction) ?? interaction.title,
        tone: 'error',
      };
    default:
      return null;
  }
}
