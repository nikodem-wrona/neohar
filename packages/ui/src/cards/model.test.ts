import { describe, expect, it } from 'bun:test';
import type { InteractionValue } from '@neohar/transport';
import {
  answeredBy,
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

  it('shows the delivery of the answer, also the amber one', () => {
    const answered = (state: InteractionValue['state'], extra = {}) =>
      card({ state, delivery: { rev: 1, attempts: 1, decided_by: 'ui', via: 'card', ...extra } });
    expect(statusOf(answered('delivering'), 'Claude Code')).toEqual({
      text: 'Sending the answer of ui via card…',
      tone: 'info',
    });
    expect(statusOf(answered('delivered'), 'Claude Code')).toEqual({
      text: 'Delivered. Answered by ui via card.',
      tone: 'ok',
    });
    expect(statusOf(answered('delivery_unknown'), 'Claude Code')).toEqual({
      text: "Couldn't confirm your answer reached Claude Code.",
      tone: 'warn',
    });
    expect(statusOf(answered('failed', { fail_reason: 'dialog_changed' }), 'Codex')).toEqual({
      text: "Couldn't deliver the answer to Codex: dialog changed.",
      tone: 'error',
    });
    expect(statusOf(answered('resolved_elsewhere'), 'Codex')?.text).toBe('Answered in Codex.');
    expect(statusOf(answered('dismissed'), 'Codex')?.text).toBe('Dismissed.');
    expect(statusOf(answered('cancelled'), 'Codex')?.text).toBe('Not needed any more.');
  });

  it('names who answered, also a rule of the policy', () => {
    expect(
      answeredBy(card({ delivery: { rev: 1, attempts: 1, decided_by: 'policy', via: 'policy' } })),
    ).toBe('a rule of the policy');
    expect(
      answeredBy(card({ delivery: { rev: 1, attempts: 1, decided_by: 'cli', via: 'cli' } })),
    ).toBe('cli via cli');
    expect(answeredBy(card())).toBe('someone');
  });
});
