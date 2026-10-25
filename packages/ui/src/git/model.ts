// What the Git view needs from `git_head:<window>`, `git_worktrees:<window>`,
// `git_branches:<window>` and `git_repos`. The core finds the worktree, reads git, orders every
// list and writes every time text. This module only turns those facts into the words of a row,
// keeps the rows that match the filter of a menu in their order, groups the branches by the flags
// of the core, and moves between items with the keys.

import type {
  DiffLine,
  FileDiff,
  GitAgent,
  GitAgentState,
  GitBranchValue,
  GitCommit,
  GitHead,
  GitRepoValue,
  GitTrustAsk,
  GitWorktreeValue,
} from '@neohar/transport';
import { harnessName } from '../agents/badge';

/** The state of an agent in words. */
export function agentStateWord(state: GitAgentState): string {
  switch (state) {
    case 'starting':
      return 'starting';
    case 'working':
      return 'working';
    case 'waiting':
      return 'waits for you';
    case 'idle':
      return 'idle';
    case 'exited':
      return 'exited';
    default:
      return 'state unknown';
  }
}

/** An agent as a worktree row shows it: "Claude · working". */
export function agentLabel(agent: GitAgent): string {
  return `${harnessName(agent.harness)} · ${agentStateWord(agent.state)}`;
}

/** The agents of a worktree in one text, or `null` when it has none. */
export function agentsLabel(agents: readonly GitAgent[] | undefined): string | null {
  if (!agents || agents.length === 0) return null;
  return agents.map(agentLabel).join(', ');
}

/** The note of the head: "2 agents in this worktree", or `null`. */
export function agentsText(agents: readonly GitAgent[] | undefined): string | null {
  const count = agents?.length ?? 0;
  if (count === 0) return null;
  return count === 1 ? '1 agent in this worktree' : `${count} agents in this worktree`;
}

function count(n: number, one: string, many: string): string {
  return `${n} ${n === 1 ? one : many}`;
}

/** Ahead and behind as arrows: "1↑ 3↓". Empty when the branch is even with its upstream. */
export function trackText(ahead: number | undefined, behind: number | undefined): string {
  const parts: string[] = [];
  if (ahead) parts.push(`${ahead}↑`);
  if (behind) parts.push(`${behind}↓`);
  return parts.join(' ');
}

/** The facts of a worktree row: "main · clean", "detached at 0123456 · 4 changed". */
export function worktreeFacts(row: GitWorktreeValue): string {
  if (row.prunable) return 'the folder is gone';
  const facts: string[] = [row.branch ?? `detached at ${row.commit ?? '?'}`];
  if (row.dirty !== undefined) facts.push(row.dirty === 0 ? 'clean' : `${row.dirty} changed`);
  if (row.locked) facts.push('locked');
  return facts.join(' · ');
}

/** The facts of a branch row: its upstream track, the time of its commit, where it is out. */
export function branchFacts(row: GitBranchValue): string {
  const facts: string[] = [];
  const track = trackText(row.ahead, row.behind);
  if (track) facts.push(track);
  if (row.time_text) facts.push(row.time_text);
  if (row.worktree) facts.push(`out in ${lastPart(row.worktree)}`);
  return facts.join(' · ');
}

/** The facts of a repository row. */
export function repoFacts(row: GitRepoValue): string {
  return row.agents > 0 ? count(row.agents, 'agent', 'agents') : '';
}

/** The last part of a path. */
export function lastPart(path: string): string {
  const trimmed = path.endsWith('/') ? path.slice(0, -1) : path;
  return trimmed.slice(trimmed.lastIndexOf('/') + 1) || path;
}

/**
 * The text of the branch of the head: its name, or the commit of a detached `HEAD`. A repository
 * that waits for a trust has no branch that the core read.
 */
export function branchTitle(head: GitHead): string {
  if (head.trust) return '—';
  if (head.branch) return head.branch;
  return head.commit ? `Detached at ${head.commit}` : 'No commit yet';
}

/** The labels of a commit of the History: its branches, then its tags. */
export function commitLabels(commit: GitCommit): string[] {
  return [...(commit.branches ?? []), ...(commit.tags ?? []).map((tag) => `tag ${tag}`)];
}

/** Whether an item matches the filter of a menu: every word of the filter is in one of its texts. */
export function matches(filter: string, texts: readonly (string | null | undefined)[]): boolean {
  const words = filter.toLowerCase().split(/\s+/).filter(Boolean);
  if (words.length === 0) return true;
  const hay = texts
    .filter((text): text is string => !!text)
    .join(' ')
    .toLowerCase();
  return words.every((word) => hay.includes(word));
}

/** A group of the Branch menu, with the rows of the core in their order. */
export interface BranchGroup {
  title: string;
  rows: GitBranchValue[];
}

/**
 * The groups of the Branch menu: the current branch, the recent ones, the other local ones and
 * the remote ones. The core gives the rows in that order and flags them, so a group keeps the order
 * of the core. An empty group is left out.
 */
export function branchGroups(rows: readonly GitBranchValue[]): BranchGroup[] {
  const groups: BranchGroup[] = [
    { title: 'Current', rows: rows.filter((row) => row.current) },
    { title: 'Recent', rows: rows.filter((row) => !row.current && row.recent) },
    {
      title: 'Local',
      rows: rows.filter((row) => !row.current && !row.recent && row.kind === 'local'),
    },
    { title: 'Remote', rows: rows.filter((row) => row.kind === 'remote') },
  ];
  return groups.filter((group) => group.rows.length > 0);
}

/** The index that a key moves to among `count` items from `at`, or `null` for another key. */
export function moveIndex(count: number, at: number, key: string): number | null {
  if (count === 0) return null;
  switch (key) {
    case 'ArrowDown':
      return Math.min(at + 1, count - 1);
    case 'ArrowUp':
      return Math.max(at - 1, 0);
    case 'Home':
      return 0;
    case 'End':
      return count - 1;
    default:
      return null;
  }
}

/** Whether a text that the person typed in the Repository menu is a path to add. */
export function isAbsolutePath(text: string): boolean {
  return text.trim().startsWith('/');
}

/** What the button after the menus does: fetch, pull, or wait for the job that runs. */
export interface FetchButton {
  kind: 'fetch' | 'pull' | 'busy';
  label: string;
}

/**
 * The button of the remote: "Pull origin 3↓" when the branch is behind and has no commit of its
 * own, "Fetching…" while a job runs, else "Fetch origin". A repository with no remote has none.
 * A branch that diverged gets Fetch only: this version has no merge.
 */
export function fetchButton(head: GitHead): FetchButton | null {
  if (head.job) return { kind: 'busy', label: head.job.kind === 'pull' ? 'Pulling…' : 'Fetching…' };
  if (!head.remote) return null;
  const behind = head.behind ?? 0;
  if (behind > 0 && !head.ahead && head.upstream) {
    return { kind: 'pull', label: `Pull ${head.remote} ${behind}↓` };
  }
  return { kind: 'fetch', label: `Fetch ${head.remote}` };
}

/** A question of the core before a write: the reason, from `details.kind`. */
export type GitQuestion =
  | { kind: 'agents'; text: string }
  | { kind: 'changes'; text: string; from: string }
  | { kind: 'worktree'; text: string; worktree: string };

/**
 * The question of a write that the core refused with `confirm_required` (busy agents, changes to
 * bring or leave) or with `conflict` for a branch that another worktree has out. Any other
 * refusal is no question.
 */
export function questionOf(code: string, message: string, details: unknown): GitQuestion | null {
  const facts = (details ?? {}) as Record<string, unknown>;
  const text = (key: string): string | null =>
    typeof facts[key] === 'string' ? (facts[key] as string) : null;
  if (code === 'confirm_required' && facts['kind'] === 'agents') {
    return { kind: 'agents', text: message };
  }
  if (code === 'confirm_required' && facts['kind'] === 'changes') {
    return { kind: 'changes', text: message, from: text('from') ?? 'HEAD' };
  }
  const worktree = text('worktree');
  if (code === 'conflict' && facts['kind'] === 'worktree' && worktree) {
    return { kind: 'worktree', text: message, worktree };
  }
  return null;
}

/** The words of the prompt of a repository that the core runs no git command in. */
export interface TrustPromptText {
  title: string;
  /** Why the core asks, one sentence for each line. */
  lines: string[];
  /** The settings that changed after the trust, or `null`. */
  changed: string | null;
}

/**
 * The prompt of `trust` of `changes_head` or `git_head`, from its reason: the person never
 * trusted the repository, or a setting that names a program changed after the trust.
 */
export function trustPrompt(ask: GitTrustAsk): TrustPromptText {
  const why = 'Git can run the hooks of this repository, and programs that its settings name.';
  const advice = 'Trust a repository only when you know where it comes from.';
  if (ask.reason === 'changed') {
    const keys = ask.changed ?? [];
    return {
      title: 'The settings or the hooks of this repository changed',
      lines: [
        why,
        'neohar runs git in this repository again only after you trust it again.',
        advice,
      ],
      changed: keys.length > 0 ? `Changed: ${keys.join(', ')}` : null,
    };
  }
  return {
    title: 'Do you trust this repository?',
    lines: [why, 'neohar runs git in this repository only after you trust it.', advice],
    changed: null,
  };
}

/** A row of a diff in the Git view: the header of a hunk, or one line. */
export type DiffRow = { kind: 'hunk'; header: string } | { kind: 'line'; line: DiffLine };

/** The hunks of a file as one list of rows, in their order, for a list that paints only some. */
export function diffRows(file: FileDiff): DiffRow[] {
  return file.hunks.flatMap((hunk): DiffRow[] => [
    { kind: 'hunk', header: hunk.header },
    ...hunk.lines.map((line): DiffRow => ({ kind: 'line', line })),
  ]);
}

/**
 * The rows that a list of rows of one height paints: the ones on screen and `extra` more on each
 * side. `end` is past the last one.
 */
export function visibleRows(
  count: number,
  scrollTop: number,
  viewHeight: number,
  rowHeight: number,
  extra: number,
): { start: number; end: number } {
  const first = Math.floor(Math.max(0, scrollTop) / rowHeight);
  const shown = Math.ceil(Math.max(0, viewHeight) / rowHeight) + 1;
  return {
    start: Math.max(0, Math.min(count, first - extra)),
    end: Math.max(0, Math.min(count, first + shown + extra)),
  };
}
