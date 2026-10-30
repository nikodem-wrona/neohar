import { describe, expect, it } from 'bun:test';
import type { InteractionValue } from '@neohar/transport';
import {
  answerArrived,
  answerToast,
  approvalButtons,
  approvalDecision,
  buttonForKey,
  canAnswer,
  defaultChoice,
  displayOnly,
  enterAnswers,
  planDecision,
  questionAnswer,
  questionComplete,
  questionDecision,
  questionTitle,
  stateBadge,
  statusOf,
  toggled,
} from './model';

const card = (over: Partial<InteractionValue> = {}): InteractionValue =>
  ({
    id: '01M4F1W7EM2GN3EV6NHXZJEGGA',
    handle: 'i1',
    kind: 'approval',
    state: 'open',
    mode: 'gate',
    title: 'Run `ls`',
    risk: 'low',
    opened_at: '2026-10-10T10:00:00.000Z',
    payload: { kind: 'approval', tool: 'Bash' },
    delivery: { rev: 0, attempts: 0 },
    answerable: true,
    channels: ['native'],
    ...over,
  }) as InteractionValue;

describe('the buttons of an approval', () => {
  it('are Allow, Allow always with its rule, Deny and Deny and stop, in this order', () => {
    const buttons = approvalButtons(card({ always_rule: 'pnpm test:*' }));
    expect(buttons.map((b) => b.label)).toEqual([
      'Allow',
      'Allow `pnpm test:*` always',
      'Deny',
      'Deny and stop',
    ]);
    expect(buttons.map((b) => b.key)).toEqual([1, 2, 3, 4]);
    expect(buttons.filter((b) => b.refuses).map((b) => b.id)).toEqual(['deny', 'deny_stop']);
  });

  it('have no Allow always without a rule from the core, and keep their keys', () => {
    for (const over of [{}, { always_rule: null }, { always_rule: '' }]) {
      const buttons = approvalButtons(card(over));
      expect(buttons.map((b) => b.id)).toEqual(['allow', 'deny', 'deny_stop']);
      expect(buttons.map((b) => b.key)).toEqual([1, 3, 4]);
      // Key 2 names nothing, so it cannot allow by accident.
      expect(buttonForKey(buttons, '2')).toBeUndefined();
      expect(buttonForKey(buttons, '3')?.id).toBe('deny');
    }
  });

  it('select Deny for a high risk and Allow for the others', () => {
    expect(defaultChoice('high')).toBe('deny');
    for (const risk of ['low', 'medium', 'unknown'] as const) {
      expect(defaultChoice(risk)).toBe('allow');
    }
  });

  it('are not answered by Enter when the risk is high', () => {
    expect(enterAnswers('high')).toBe(false);
    expect(enterAnswers('low')).toBe(true);
    expect(enterAnswers('unknown')).toBe(true);
  });
});

describe('the decisions', () => {
  it('say allow, allow always, deny and deny and stop', () => {
    expect(approvalDecision('allow')).toEqual({ kind: 'approval', behavior: 'allow' });
    expect(approvalDecision('allow_always')).toEqual({
      kind: 'approval',
      behavior: 'allow',
      always: true,
    });
    expect(approvalDecision('deny')).toEqual({ kind: 'approval', behavior: 'deny' });
    expect(approvalDecision('deny_stop', '  not now ')).toEqual({
      kind: 'approval',
      behavior: 'deny',
      stop: true,
      message: 'not now',
    });
  });

  it('carry the answers of a question', () => {
    const question = { question: 'Color?', options: [{ label: 'Red' }, { label: 'Blue' }] };
    const answers = [questionAnswer(question, ['Red'], ' ')];
    expect(answers).toEqual([{ question: 'Color?', selected: ['Red'] }]);
    expect(questionDecision(answers)).toEqual({ kind: 'question', answers });
    expect(questionAnswer(question, [], ' teal ')).toEqual({
      question: 'Color?',
      selected: [],
      text: 'teal',
    });
  });

  it('approve a plan, or request changes with the feedback', () => {
    expect(planDecision(true, 'ignored')).toEqual({ kind: 'plan_review', approve: true });
    expect(planDecision(false, ' smaller steps ')).toEqual({
      kind: 'plan_review',
      approve: false,
      feedback: 'smaller steps',
    });
    expect(planDecision(false, '')).toEqual({ kind: 'plan_review', approve: false });
  });
});

describe('the choices of a question', () => {
  it('replace one another when one is allowed, and toggle when many are', () => {
    expect(toggled(['Red'], 'Blue', false)).toEqual(['Blue']);
    expect(toggled(['Red'], 'Blue', true)).toEqual(['Red', 'Blue']);
    expect(toggled(['Red', 'Blue'], 'Red', true)).toEqual(['Blue']);
  });

  it('are complete when every question has a choice or a text', () => {
    const q = { question: 'a', options: [] };
    expect(questionComplete([questionAnswer(q, ['x'], '')], 1)).toBe(true);
    expect(questionComplete([questionAnswer(q, [], 'text')], 1)).toBe(true);
    expect(questionComplete([questionAnswer(q, [], '')], 1)).toBe(false);
    expect(questionComplete([questionAnswer(q, ['x'], '')], 2)).toBe(false);
  });
});

describe('what a card says', () => {
  it('can be answered only while it is open and the core can reach it', () => {
    expect(canAnswer(card())).toBe(true);
    expect(canAnswer(card({ answerable: false }))).toBe(false);
    expect(canAnswer(card({ state: 'delivered' }))).toBe(false);
  });

  it('is for reading when the core cannot answer: display only, without answer buttons', () => {
    const unlinked = card({ answerable: false });
    expect(displayOnly(unlinked)).toBe(true);
    expect(statusOf(unlinked, 'Claude Code')).toEqual({
      text: 'Display only. Answer it in Claude Code.',
      tone: 'warn',
    });
    expect(displayOnly(card({ answerable: false, kind: 'notice' }))).toBe(false);
    expect(statusOf(card(), 'Claude Code')).toBeNull();
  });

  it('has a line only for what the person must know: an answer that may not have arrived', () => {
    const answered = (state: InteractionValue['state'], extra = {}) =>
      card({ state, delivery: { rev: 1, attempts: 1, decided_by: 'ui', via: 'card', ...extra } });
    expect(statusOf(answered('delivery_unknown'), 'Claude Code')).toEqual({
      text: "Couldn't confirm your answer reached Claude Code.",
      tone: 'warn',
    });
    expect(statusOf(answered('failed', { fail_reason: 'dialog_changed' }), 'Codex')).toEqual({
      text: "Couldn't deliver the answer to Codex: dialog changed.",
      tone: 'error',
    });
    // A toast says that an answer arrived, and the badge says how a card ended.
    for (const state of [
      'decision_recorded',
      'delivering',
      'delivered',
      'resolved_elsewhere',
      'dismissed',
      'cancelled',
      'superseded',
    ] as const) {
      expect(statusOf(answered(state), 'Codex')).toBeNull();
    }
  });

  it('has a badge that says what became of it, once it is not open', () => {
    const ended = (state: InteractionValue['state'], decided_by = 'ui') =>
      card({ state, delivery: { rev: 1, attempts: 1, decided_by, via: 'card' } });
    expect(stateBadge(card(), 'Codex')).toBeNull();
    expect(stateBadge(ended('delivering'), 'Codex')).toEqual({ text: 'Sending', tone: 'info' });
    expect(stateBadge(ended('delivered'), 'Codex')).toEqual({ text: 'Answered', tone: 'muted' });
    expect(stateBadge(ended('delivered', 'policy'), 'Codex')?.text).toBe('Answered by policy');
    expect(stateBadge(ended('delivery_unknown'), 'Codex')).toEqual({
      text: 'Unconfirmed',
      tone: 'warn',
    });
    expect(stateBadge(ended('failed'), 'Codex')).toEqual({ text: 'Not delivered', tone: 'error' });
    expect(stateBadge(ended('resolved_elsewhere'), 'Codex')?.text).toBe('Answered in Codex');
    expect(stateBadge(ended('dismissed'), 'Codex')?.text).toBe('Dismissed');
    expect(stateBadge(ended('superseded'), 'Codex')?.text).toBe('Not needed');
  });
});

describe('the title of a question', () => {
  const question = (text: string, header?: string) => ({
    question: text,
    options: [],
    ...(header ? { header } : {}),
  });

  it('is the header of one question, or how many questions there are', () => {
    expect(questionTitle([question('Which build should I install?', 'Build from')])).toBe(
      'Build from',
    );
    expect(questionTitle([question('Which build should I install?')])).toBe('Question');
    expect(questionTitle([question('a'), question('b')])).toBe('2 questions');
  });
});

describe('the toast of an answer that this window sent', () => {
  const sent = (state: InteractionValue['state'], extra = {}) =>
    card({
      state,
      title: 'Which build should I install?',
      delivery: { rev: 1, attempts: 1, decided_by: 'ui', via: 'card', ...extra },
    });

  it('waits while the answer travels, and ends when the interaction left', () => {
    expect(answerArrived(sent('open'))).toBe(false);
    expect(answerArrived(sent('decision_recorded'))).toBe(false);
    expect(answerArrived(sent('delivering'))).toBe(false);
    expect(answerArrived(sent('delivered'))).toBe(true);
    expect(answerArrived(sent('failed'))).toBe(true);
    expect(answerArrived(sent('resolved_elsewhere'))).toBe(true);
    expect(answerArrived(undefined)).toBe(true);
  });

  it('says that the answer arrived, or that it may not have', () => {
    expect(answerToast(sent('delivered'), 'Claude Code')).toEqual({
      title: 'Answer delivered to Claude Code',
      description: 'Which build should I install?',
      tone: 'ok',
    });
    expect(answerToast(sent('delivery_unknown'), 'Codex')).toEqual({
      title: "Couldn't confirm the answer reached Codex",
      description: 'Which build should I install?',
      tone: 'warn',
    });
    expect(answerToast(sent('failed', { fail_reason: 'dialog_changed' }), 'Codex')).toEqual({
      title: "Couldn't deliver the answer to Codex",
      description: 'dialog changed',
      tone: 'error',
    });
    expect(answerToast(sent('resolved_elsewhere'), 'Codex')).toBeNull();
  });
});
