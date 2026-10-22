import { describe, expect, test } from 'bun:test';
import type { ChangeRow, ChangesHead, ChangeStatus } from '@neohar/transport';
import { headFacts, moveTo, statusLetter, stillOpen, subjectOf, toFetch } from './model';

function row(path: string, stamp: number): ChangeRow {
  const at = path.lastIndexOf('/');
  return {
    path,
    dir: path.slice(0, at + 1),
    name: path.slice(at + 1),
    status: 'modified',
    add: 1,
    del: 0,
    binary: false,
    uncommitted: false,
    stamp,
  };
}

function head(partial: Partial<ChangesHead>): ChangesHead {
  return {
    folder: '/r/src',
    root: '/r',
    branch: 'neohar/fix-it',
    base: 'main',
    changes: { files: 1, commits: 2, merge_base: 'abc1234', truncated: false },
    diff_stats: { add: 12, del: 3 },
    ...partial,
  };
}

describe('the Changes tab', () => {
  test('a status is one letter', () => {
    const letters = (
      ['modified', 'added', 'deleted', 'renamed', 'untracked'] as ChangeStatus[]
    ).map(statusLetter);
    expect(letters).toEqual(['M', 'A', 'D', 'R', 'U']);
  });

  test('the head says the commits, the files, the lines and the merge base of the core', () => {
    expect(headFacts(head({}))).toEqual(['2 commits', '1 file', '+12 −3', 'from abc1234']);
    expect(headFacts(head({ changes: null, diff_stats: null }))).toEqual([]);
  });

  test('a worktree that is not of a task has no commits and no merge base', () => {
    const changes = { files: 3, commits: 0, merge_base: 'def5678', truncated: false };
    expect(headFacts(head({ base: null, changes }))).toEqual(['3 files', '+12 −3']);
  });

  test('the subject is the worktree and the base, not the folder', () => {
    expect(subjectOf(head({}))).toBe(subjectOf(head({ folder: '/r/docs' })));
    expect(subjectOf(head({}))).not.toBe(subjectOf(head({ base: null })));
    expect(subjectOf(head({ root: null }))).toBeNull();
    expect(subjectOf(undefined)).toBeNull();
  });

  test('an open row loads its diff once, and again only when its stamp changes', () => {
    const rows = [row('a.rs', 1), row('src/b.rs', 2)];
    const open = new Set(['src/b.rs']);
    expect(toFetch(open, rows, new Map())).toEqual(['src/b.rs']);
    const fetched = new Map([['src/b.rs', { stamp: 2 }]]);
    expect(toFetch(open, rows, fetched)).toEqual([]);
    // Another row changed: nothing to load.
    expect(toFetch(open, [row('a.rs', 9), row('src/b.rs', 2)], fetched)).toEqual([]);
    // The open row changed: its diff loads again.
    expect(toFetch(open, [row('a.rs', 1), row('src/b.rs', 3)], fetched)).toEqual(['src/b.rs']);
  });

  test('an open row whose file has no change any more closes', () => {
    const open = new Set(['a.rs', 'gone.rs']);
    expect([...stillOpen(open, [row('a.rs', 1)])]).toEqual(['a.rs']);
  });

  test('the arrow keys, Home and End move between the rows in their order', () => {
    const keys = ['a', 'b', 'c'];
    expect(moveTo(keys, null, 'ArrowDown')).toBe('a');
    expect(moveTo(keys, 'a', 'ArrowDown')).toBe('b');
    expect(moveTo(keys, 'c', 'ArrowDown')).toBe('c');
    expect(moveTo(keys, 'b', 'ArrowUp')).toBe('a');
    expect(moveTo(keys, 'a', 'ArrowUp')).toBe('a');
    expect(moveTo(keys, 'b', 'End')).toBe('c');
    expect(moveTo(keys, 'b', 'Home')).toBe('a');
    expect(moveTo(keys, 'b', 'Enter')).toBeNull();
    expect(moveTo([], null, 'ArrowDown')).toBeNull();
  });
});
