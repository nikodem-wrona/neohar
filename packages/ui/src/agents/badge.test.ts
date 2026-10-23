import { describe, expect, test } from 'bun:test';
import type { AgentValue } from '@neohar/transport';
import { badgeOf, harnessName, workOf } from './badge';

/** A run as the core publishes it, with the parts a test changes. */
function run(change: {
  execution?: string;
  source?: string;
  health?: string;
  read?: 'seen' | 'unseen';
  binding?: string;
  yolo?: boolean;
  harness?: 'claude' | 'codex';
  pending?: string[];
}): AgentValue {
  const provenance = { source: change.source ?? 'structured', confidence: 1, at: 'now' };
  return {
    id: '01M4F1W7EM2GN3EV6NHXZJEGGA',
    harness: change.harness ?? 'claude',
    pane: '01M4F1W7EM2GN3EV6NHXZJEGGB',
    binding: { state: change.binding ?? 'bound' },
    facets: {
      liveness: { value: { state: 'alive' }, provenance },
      execution: { value: { state: change.execution ?? 'idle' }, provenance },
      pending: change.pending ?? [],
      adapter_health: { value: { state: change.health ?? 'healthy' }, provenance },
      read_state: change.read ?? 'seen',
    },
    capabilities: {
      observe: true,
      gate: false,
      answer_native: [],
      answer_keystroke: false,
      reconcile: false,
      resume: false,
      steer: false,
      survive_disconnect: false,
    },
    usage: { input_tokens: 0, output_tokens: 0, cache_read_tokens: 0, cache_write_tokens: 0 },
    yolo: change.yolo ?? false,
    resumable: false,
    started_at: 'now',
  } as unknown as AgentValue;
}

describe('badgeOf', () => {
  test('a glyph for each execution state', () => {
    const glyph = (state: string) => badgeOf(run({ execution: state })).glyph;
    expect(glyph('working')).toBe('●');
    expect(glyph('idle')).toBe('○');
    expect(glyph('error')).toBe('▲');
    expect(glyph('rate_limited')).toBe('◷');
    expect(glyph('exited')).toBe('×');
    expect(glyph('starting')).toBe('◌');
    expect(glyph('something new')).toBe('·');
  });

  test('a state that the screen or the process told is inferred, unless the run ended', () => {
    expect(badgeOf(run({ source: 'structured' })).inferred).toBe(false);
    expect(badgeOf(run({ source: 'screen' })).inferred).toBe(true);
    expect(badgeOf(run({ source: 'process' })).inferred).toBe(true);
    expect(badgeOf(run({ source: 'holder', execution: 'exited' })).inferred).toBe(false);
    expect(badgeOf(run({ source: 'screen', execution: 'exited' })).inferred).toBe(false);
    expect(badgeOf(run({ source: 'screen' })).tooltip).toContain('inferred from screen');
    expect(badgeOf(run({ source: 'process' })).tooltip).toContain('inferred from process');
  });

  test('a version that is not validated is marked', () => {
    expect(badgeOf(run({ health: 'healthy' })).unvalidated).toBe(false);
    const marked = badgeOf(run({ health: 'unvalidated_version' }));
    expect(marked.unvalidated).toBe(true);
    expect(marked.tooltip).toContain('version not validated');
    // A disconnected channel is not an unvalidated version.
    expect(badgeOf(run({ health: 'disconnected' })).unvalidated).toBe(false);
  });

  test('done is idle and unseen, and only that', () => {
    expect(badgeOf(run({ execution: 'idle', read: 'unseen' })).done).toBe(true);
    expect(badgeOf(run({ execution: 'idle', read: 'seen' })).done).toBe(false);
    expect(badgeOf(run({ execution: 'working', read: 'unseen' })).done).toBe(false);
    expect(badgeOf(run({ execution: 'idle', read: 'unseen' })).tooltip).toContain('done');
  });

  test('a run that the process list showed is marked as not linked', () => {
    expect(badgeOf(run({ binding: 'suggested' })).suggested).toBe(true);
    expect(badgeOf(run({ binding: 'suggested' })).tooltip).toContain('not linked');
    expect(badgeOf(run({ binding: 'bound' })).suggested).toBe(false);
  });

  test('the harness and the yolo mark', () => {
    expect(badgeOf(run({ harness: 'codex', yolo: true }))).toMatchObject({
      harness: 'Codex',
      yolo: true,
    });
    expect(badgeOf(run({ yolo: true })).tooltip).toContain('yolo');
    expect(harnessName('claude')).toBe('Claude Code');
    expect(harnessName('other')).toBe('other');
  });
});

describe('the code of the board', () => {
  test('a state has four letters or fewer', () => {
    expect(badgeOf(run({ execution: 'working' })).code).toBe('WORK');
    expect(badgeOf(run({ execution: 'idle' })).code).toBe('IDLE');
    expect(badgeOf(run({ execution: 'rate_limited' })).code).toBe('RATE');
  });

  test('an open interaction is WAIT, whatever the state of the execution', () => {
    const badge = badgeOf(run({ execution: 'working', pending: ['i1'] }));
    expect(badge.code).toBe('WAIT');
    expect(badge.waiting).toBe(true);
    expect(badge.tooltip).toContain('waits for you');
  });

  test('an idle run that the window has not seen is DONE', () => {
    expect(badgeOf(run({ execution: 'idle', read: 'unseen' })).code).toBe('DONE');
  });

  test('an ended run does not wait', () => {
    const badge = badgeOf(run({ execution: 'exited', pending: ['i1'] }));
    expect(badge.waiting).toBe(false);
    expect(badge.code).toBe('END');
  });
});

describe('workOf', () => {
  test('the title of the pane, else the harness', () => {
    expect(workOf(run({}), { title: 'zsh' })).toBe('zsh');
    expect(workOf(run({}), { title: '' })).toBe('Claude Code');
    expect(workOf(run({ harness: 'codex' }), undefined)).toBe('Codex');
  });
});
