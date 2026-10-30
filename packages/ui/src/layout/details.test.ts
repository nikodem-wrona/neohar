import { describe, expect, test } from 'bun:test';
import type { AgentValue, PaneValue, Usage } from '@neohar/transport';
import { detailsTitle, paneDetails, stripTitle, tokensText } from './details';

const NO_USAGE: Usage = {
  input_tokens: 0,
  output_tokens: 0,
  cache_read_tokens: 0,
  cache_write_tokens: 0,
};

/** A pane as the core publishes it, with the parts a test changes. */
function pane(change: Partial<PaneValue> = {}): PaneValue {
  return {
    id: '01M4F1W7EM2GN3EV6NHXZJEGGB',
    handle: 'w1:p2',
    title: '✳ Fix the parser',
    cwd: '/Users/me/repos/neohar',
    cols: 120,
    rows: 40,
    ...change,
  } as unknown as PaneValue;
}

/** A run as the core publishes it, with the parts a test changes. */
function run(change: Partial<AgentValue> & { health?: string } = {}): AgentValue {
  const provenance = { source: 'structured', confidence: 1, at: 'now' };
  const { health, ...rest } = change;
  return {
    id: '01M4F1W7EM2GN3EV6NHXZJEGGA',
    harness: 'claude',
    facets: {
      execution: { value: { state: 'idle' }, provenance },
      pending: [],
      adapter_health: { value: { state: health ?? 'healthy' }, provenance },
      read_state: 'seen',
    },
    binding: { state: 'bound' },
    usage: NO_USAGE,
    ...rest,
  } as unknown as AgentValue;
}

const keys = (rows: { key: string }[]) => rows.map((row) => row.key);

describe('paneDetails', () => {
  test('a terminal has its title, folder, program and size', () => {
    const rows = paneDetails(pane({ fg_process: 'zsh' }), undefined);
    expect(keys(rows)).toEqual(['title', 'folder', 'program', 'pane']);
    expect(rows.at(-1)).toEqual({
      key: 'pane',
      label: 'Pane',
      value: 'w1:p2 · 120×40',
      data: true,
      copy: false,
    });
  });

  test('an agent has the facts of its run, and no program', () => {
    const rows = paneDetails(
      pane({ fg_process: 'claude' }),
      run({
        model: 'claude-opus-5-5',
        harness_version: '2.1.301',
        session_id: 'f3a1',
        usage: { ...NO_USAGE, input_tokens: 1200, output_tokens: 340, cost_usd: 10.634 },
      }),
    );
    expect(keys(rows)).toEqual([
      'title',
      'folder',
      'model',
      'version',
      'cost',
      'tokens',
      'session',
      'pane',
    ]);
    const value = (key: string) => rows.find((row) => row.key === key)?.value;
    expect(value('cost')).toBe('$10.63');
    expect(value('tokens')).toBe('1200 in · 340 out');
    expect(value('version')).toBe('2.1.301');
  });

  test('the folder and the session can be copied', () => {
    const rows = paneDetails(pane(), run({ session_id: 'f3a1' }));
    expect(rows.filter((row) => row.copy).map((row) => row.key)).toEqual(['folder', 'session']);
  });

  test('a title that only names the harness has no row', () => {
    expect(keys(paneDetails(pane({ title: '✳ Claude Code' }), run()))).not.toContain('title');
    expect(keys(paneDetails(pane({ title: '✳ Claude Code' }), undefined))).toContain('title');
  });

  test('a fact that the core does not give has no row', () => {
    const rows = paneDetails(pane({ title: '', cwd: null }), run());
    expect(keys(rows)).toEqual(['pane']);
  });

  test('a version that is not validated says so', () => {
    const rows = paneDetails(
      pane(),
      run({ harness_version: '9.0.0', health: 'unvalidated_version' }),
    );
    expect(rows.find((row) => row.key === 'version')?.value).toBe('9.0.0 · not validated');
  });
});

test('detailsTitle names the harness, else the terminal', () => {
  expect(detailsTitle(run({ harness: 'codex' }))).toBe('Codex');
  expect(detailsTitle(run())).toBe('Claude Code');
  expect(detailsTitle(undefined)).toBe('Terminal');
});

test('tokensText counts the cached tokens only when there are some', () => {
  expect(tokensText({ ...NO_USAGE, input_tokens: 5, output_tokens: 7 })).toBe('5 in · 7 out');
  expect(tokensText({ ...NO_USAGE, input_tokens: 5, cache_read_tokens: 9 })).toBe(
    '5 in · 0 out · 9 cached',
  );
});

describe('stripTitle', () => {
  test('a title that only names the harness is empty', () => {
    expect(stripTitle('✳ Claude Code', 'claude')).toBe('');
    expect(stripTitle('Claude Code', 'claude')).toBe('');
    expect(stripTitle('codex', 'codex')).toBe('');
  });

  test('a title with a task stays', () => {
    expect(stripTitle('✳ Fix the parser', 'claude')).toBe('✳ Fix the parser');
    expect(stripTitle('Claude Code review', 'claude')).toBe('Claude Code review');
  });

  test('a pane with no agent keeps its title', () => {
    expect(stripTitle('zsh', undefined)).toBe('zsh');
  });
});
