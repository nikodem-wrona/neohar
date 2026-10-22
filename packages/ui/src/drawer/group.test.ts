import { expect, test } from 'bun:test';
import type { AttentionValue, InteractionValue } from '@neohar/transport';
import { groupByRun, troubleOf, troubleText, troubleTitle } from './group';

const row = (id: string, run: string | null): InteractionValue =>
  ({ id, run }) as unknown as InteractionValue;

test('cards of one run sit together, in the order of the projection', () => {
  const groups = groupByRun([row('1', 'A'), row('2', 'B'), row('3', 'A'), row('4', null)]);
  expect(groups.map((g) => [g.run, g.interactions.map((i) => i.id)])).toEqual([
    ['A', ['1', '3']],
    ['B', ['2']],
    [null, ['4']],
  ]);
});

test('no interactions give no groups', () => {
  expect(groupByRun([])).toEqual([]);
});

// --- the runs that count without a card ----------------------------------------------

const waiting = (
  run: string,
  reasons: AttentionValue['reasons'],
  interactions: string[] = [],
): AttentionValue => ({ run, reasons, interactions, since: 't', urgency: 'normal' });

test('a run in error or rate limited gets a group of its own after the runs with cards', () => {
  const groups = groupByRun(
    [row('1', 'A')],
    [waiting('B', ['rate_limited']), waiting('A', ['approval'], ['1']), waiting('C', ['error'])],
  );
  expect(groups.map((g) => [g.run, g.interactions.length, g.trouble])).toEqual([
    ['A', 1, null],
    ['B', 0, 'rate_limited'],
    ['C', 0, 'error'],
  ]);
});

test('a run with cards keeps its place and gets its state', () => {
  const groups = groupByRun(
    [row('1', 'A'), row('2', 'B')],
    [waiting('B', ['approval', 'error'], ['2']), waiting('A', ['question'], ['1'])],
  );
  expect(groups.map((g) => [g.run, g.trouble])).toEqual([
    ['A', null],
    ['B', 'error'],
  ]);
});

test('the state of a row is its error before its rate limit, and nothing with cards only', () => {
  expect(troubleOf(waiting('A', ['rate_limited', 'error']))).toBe('error');
  expect(troubleOf(waiting('A', ['rate_limited']))).toBe('rate_limited');
  expect(troubleOf(waiting('A', ['approval', 'plan_review'], ['1']))).toBeNull();
  expect(troubleText('error')).toBe('error');
  expect(troubleText('rate_limited')).toBe('rate limited');
});

// --- the end of a turn that nobody saw ------------------------------------------------

test('a run whose turn ended gets a line of its own after the runs with cards', () => {
  const groups = groupByRun(
    [row('1', 'A')],
    [waiting('A', ['question'], ['1']), waiting('B', ['turn_ended'])],
  );
  expect(groups.map((g) => [g.run, g.interactions.length, g.trouble])).toEqual([
    ['A', 1, null],
    ['B', 0, 'turn_ended'],
  ]);
});

test('an error or a rate limit comes before the end of a turn', () => {
  expect(troubleOf(waiting('A', ['turn_ended']))).toBe('turn_ended');
  expect(troubleOf(waiting('A', ['turn_ended', 'rate_limited']))).toBe('rate_limited');
  expect(troubleOf(waiting('A', ['error', 'turn_ended']))).toBe('error');
  expect(troubleText('turn_ended')).toBe('turn ended');
});

test('the line of a run without a card has a title for each state', () => {
  expect(troubleTitle('error')).toBe('Error');
  expect(troubleTitle('rate_limited')).toBe('Rate limited');
  expect(troubleTitle('turn_ended')).toBe('Turn ended');
});
