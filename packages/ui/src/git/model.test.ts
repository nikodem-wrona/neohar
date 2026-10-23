import { expect, test } from 'bun:test';
import type { GitAgent, GitBranchValue, GitHead, GitWorktreeValue } from '@neohar/transport';
import {
  agentLabel,
  agentsLabel,
  agentsText,
  branchFacts,
  branchGroups,
  branchTitle,
  commitLabels,
  isAbsolutePath,
  lastPart,
  matches,
  moveIndex,
  trackText,
  worktreeFacts,
} from './model';

const agent = (state: GitAgent['state'], harness: GitAgent['harness'] = 'claude'): GitAgent => ({
  run: 'R1',
  pane: 'P1',
  harness,
  state,
});

test('an agent is its harness and its state in words', () => {
  expect(agentLabel(agent('working'))).toBe('Claude Code · working');
  expect(agentLabel(agent('idle', 'codex'))).toBe('Codex · idle');
  expect(agentLabel(agent('waiting'))).toBe('Claude Code · waits for you');
  expect(agentsLabel([agent('working'), agent('idle', 'codex')])).toBe(
    'Claude Code · working, Codex · idle',
  );
  expect(agentsLabel([])).toBeNull();
  expect(agentsText([agent('working')])).toBe('1 agent in this worktree');
  expect(agentsText([agent('working'), agent('idle')])).toBe('2 agents in this worktree');
  expect(agentsText(undefined)).toBeNull();
});

test('ahead and behind are arrows, and an even branch has none', () => {
  expect(trackText(1, 3)).toBe('1↑ 3↓');
  expect(trackText(0, 2)).toBe('2↓');
  expect(trackText(0, 0)).toBe('');
  expect(trackText(undefined, undefined)).toBe('');
});

const worktree = (fields: Partial<GitWorktreeValue>): GitWorktreeValue => ({
  path: '/w/app',
  name: 'app',
  ...fields,
});

test('a worktree row says its branch, whether it is clean, and a gone folder', () => {
  expect(worktreeFacts(worktree({ branch: 'main', dirty: 0 }))).toBe('main · clean');
  expect(worktreeFacts(worktree({ branch: 'topic', dirty: 4, locked: true }))).toBe(
    'topic · 4 changed · locked',
  );
  expect(worktreeFacts(worktree({ commit: '0123456' }))).toBe('detached at 0123456');
  expect(worktreeFacts(worktree({ prunable: true, branch: 'old' }))).toBe('the folder is gone');
});

const branch = (fields: Partial<GitBranchValue>): GitBranchValue => ({
  name: 'main',
  reference: 'refs/heads/main',
  kind: 'local',
  ...fields,
});

test('the branch groups keep the order of the core and leave out the empty ones', () => {
  const rows = [
    branch({ name: 'main', current: true }),
    branch({ name: 'old', recent: true }),
    branch({ name: 'newer' }),
    branch({ name: 'origin/main', kind: 'remote', reference: 'refs/remotes/origin/main' }),
  ];
  const groups = branchGroups(rows);
  expect(groups.map((g) => [g.title, g.rows.map((r) => r.name)])).toEqual([
    ['Current', ['main']],
    ['Recent', ['old']],
    ['Local', ['newer']],
    ['Remote', ['origin/main']],
  ]);
  expect(branchGroups([branch({ name: 'x' })]).map((g) => g.title)).toEqual(['Local']);
});

test('a branch row says its track, its time and the worktree that has it out', () => {
  expect(
    branchFacts(branch({ ahead: 2, time_text: '3 days ago', worktree: '/w/app-topic/' })),
  ).toBe('2↑ · 3 days ago · out in app-topic');
  expect(branchFacts(branch({}))).toBe('');
});

test('the head names its branch, or the commit of a detached HEAD', () => {
  const head: GitHead = { folder: '/w/app', branch: 'main', commit: '0123456' };
  expect(branchTitle(head)).toBe('main');
  expect(branchTitle({ ...head, branch: null })).toBe('Detached at 0123456');
  expect(branchTitle({ folder: '/w/app' })).toBe('No commit yet');
});

test('the labels of a commit are its branches, then its tags', () => {
  expect(
    commitLabels({
      hash: 'a',
      short: 'a',
      subject: 's',
      author: 'Ada',
      time: '2026-10-24T00:00:00.000Z',
      time_text: 'just now',
      branches: ['main', 'origin/main'],
      tags: ['v1.0'],
    }),
  ).toEqual(['main', 'origin/main', 'tag v1.0']);
});

test('a filter matches when every word is in a text, in any case', () => {
  expect(matches('', ['anything'])).toBe(true);
  expect(matches('git VIEW', ['feat/git-view', 'Claude'])).toBe(true);
  expect(matches('git claude', ['feat/git-view', 'Claude · working'])).toBe(true);
  expect(matches('codex', ['feat/git-view', null, undefined])).toBe(false);
});

test('the keys move between the items and stop at the ends', () => {
  expect(moveIndex(3, 0, 'ArrowDown')).toBe(1);
  expect(moveIndex(3, 2, 'ArrowDown')).toBe(2);
  expect(moveIndex(3, 0, 'ArrowUp')).toBe(0);
  expect(moveIndex(3, 1, 'End')).toBe(2);
  expect(moveIndex(3, 2, 'Home')).toBe(0);
  expect(moveIndex(3, 1, 'Enter')).toBeNull();
  expect(moveIndex(0, 0, 'ArrowDown')).toBeNull();
});

test('a path is the last part, and an absolute path is a path to add', () => {
  expect(lastPart('/w/app/.claude/worktrees/review')).toBe('review');
  expect(lastPart('/w/app/')).toBe('app');
  expect(isAbsolutePath(' /w/app')).toBe(true);
  expect(isAbsolutePath('app')).toBe(false);
});
