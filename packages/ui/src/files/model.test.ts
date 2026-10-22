import { expect, test } from 'bun:test';
import type {
  FsEntry,
  FsListResult,
  PaneValue,
  TaskValue,
  WorkspaceValue,
} from '@neohar/transport';
import {
  clampSplit,
  keyAction,
  type Listing,
  menuItems,
  rootOf,
  type Row,
  truncatedNote,
  visibleRows,
  withoutFolder,
} from './model';

function entry(path: string, kind: FsEntry['kind'] = 'file'): FsEntry {
  const name = path.slice(path.lastIndexOf('/') + 1);
  const icon = kind === 'dir' ? 'folder' : 'file';
  return { name, path, kind, hidden: name.startsWith('.'), link: false, icon };
}

function listing(path: string, entries: FsEntry[], truncated = false): Listing {
  const result: FsListResult = { path, entries, truncated };
  return { result };
}

/** `/r` with `src/` (with `lib.rs` and `bin/`), `docs/` and `README.md`. */
function sample(): Map<string, Listing> {
  return new Map([
    ['/r', listing('/r', [entry('/r/docs', 'dir'), entry('/r/src', 'dir'), entry('/r/README.md')])],
    ['/r/src', listing('/r/src', [entry('/r/src/bin', 'dir'), entry('/r/src/lib.rs')])],
  ]);
}

function shape(rows: readonly Row[]): string[] {
  return rows.map((row) =>
    row.kind === 'entry'
      ? `${'  '.repeat(row.depth)}${row.entry.name}${row.open ? '/' : ''}${row.busy ? ' …' : ''}`
      : `${'  '.repeat(row.depth)}[${row.text}]`,
  );
}

test('the root is the cwd of the pane with the focus, else the worktree, else the root', () => {
  const pane = { cwd: '/Users/me/src' } as PaneValue;
  const task = { worktree: '/wt/app/fix' } as TaskValue;
  const workspace = { root: '/work/app' } as WorkspaceValue;
  expect(rootOf(pane, task, workspace)).toBe('/Users/me/src');
  expect(rootOf({} as PaneValue, task, workspace)).toBe('/wt/app/fix');
  expect(rootOf(undefined, task, workspace)).toBe('/wt/app/fix');
  expect(rootOf(undefined, undefined, workspace)).toBe('/work/app');
  expect(rootOf(undefined, {} as TaskValue, workspace)).toBe('/work/app');
  expect(rootOf(undefined, undefined, {} as WorkspaceValue)).toBeUndefined();
  expect(rootOf(undefined, undefined, undefined)).toBeUndefined();
});

test('the rows are the entries of the root, with the entries of each open folder under it', () => {
  const listings = sample();
  expect(shape(visibleRows('/r', listings, new Set()))).toEqual(['docs', 'src', 'README.md']);
  expect(shape(visibleRows('/r', listings, new Set(['/r/src'])))).toEqual([
    'docs',
    'src/',
    '  bin',
    '  lib.rs',
    'README.md',
  ]);
  // An open folder inside a closed one shows nothing.
  expect(shape(visibleRows('/r', listings, new Set(['/r/src/bin'])))).toEqual([
    'docs',
    'src',
    'README.md',
  ]);
});

test('an open folder that is not listed yet is busy, and one with no entries says so', () => {
  const listings = sample();
  listings.set('/r/src/bin', listing('/r/src/bin', []));
  const rows = visibleRows('/r', listings, new Set(['/r/docs', '/r/src', '/r/src/bin']));
  expect(shape(rows)).toEqual([
    'docs/ …',
    'src/',
    '  bin/',
    '    [Empty]',
    '  lib.rs',
    'README.md',
  ]);
});

test('an error and a cut take their place in the tree', () => {
  const listings = sample();
  listings.set('/r/docs', { error: 'cannot read /r/docs: Permission denied' });
  listings.set('/r/src', listing('/r/src', [entry('/r/src/lib.rs')], true));
  const rows = visibleRows('/r', listings, new Set(['/r/docs', '/r/src']));
  expect(shape(rows)).toEqual([
    'docs/',
    '  [cannot read /r/docs: Permission denied]',
    'src/',
    '  lib.rs',
    `  [${truncatedNote(1)}]`,
    'README.md',
  ]);
  const note = rows[1];
  expect(note?.kind === 'note' && note.error).toBe(true);
  // A root that is not listed yet has no rows.
  expect(visibleRows('/other', listings, new Set())).toEqual([]);
});

test('a folder closes with the open folders in it, and not a folder with a longer name', () => {
  const open = new Set(['/r/src', '/r/src/bin', '/r/srcs', '/r/docs']);
  expect([...withoutFolder(open, '/r/src')]).toEqual(['/r/srcs', '/r/docs']);
});

test('the keys move, open and close as in a tree', () => {
  const rows = visibleRows('/r', sample(), new Set(['/r/src']));
  const key = (path: string, name: string, shift = false) => keyAction(rows, path, name, shift);
  expect(key('/r/docs', 'ArrowDown')).toEqual({ type: 'focus', path: '/r/src' });
  expect(key('/r/docs', 'ArrowUp')).toEqual({ type: 'none' });
  expect(key('/r/src/lib.rs', 'End')).toEqual({ type: 'focus', path: '/r/README.md' });
  expect(key('/r/README.md', 'Home')).toEqual({ type: 'focus', path: '/r/docs' });
  // Right opens a closed folder, and moves into an open one.
  expect(key('/r/docs', 'ArrowRight')).toEqual({ type: 'open', path: '/r/docs' });
  expect(key('/r/src', 'ArrowRight')).toEqual({ type: 'focus', path: '/r/src/bin' });
  expect(key('/r/README.md', 'ArrowRight')).toEqual({ type: 'none' });
  // Left closes an open folder, and moves to the parent of anything else.
  expect(key('/r/src', 'ArrowLeft')).toEqual({ type: 'close', path: '/r/src' });
  expect(key('/r/src/lib.rs', 'ArrowLeft')).toEqual({ type: 'focus', path: '/r/src' });
  expect(key('/r/src/bin', 'ArrowLeft')).toEqual({ type: 'focus', path: '/r/src' });
  expect(key('/r/docs', 'ArrowLeft')).toEqual({ type: 'none' });
  // Enter opens a file, and opens or closes a folder.
  expect(key('/r/README.md', 'Enter')).toEqual({ type: 'read', path: '/r/README.md' });
  expect(key('/r/src', 'Enter')).toEqual({ type: 'close', path: '/r/src' });
  expect(key('/r/docs', 'Enter')).toEqual({ type: 'open', path: '/r/docs' });
  // The menu of the row.
  expect(key('/r/docs', 'ContextMenu')).toEqual({ type: 'menu', path: '/r/docs' });
  expect(key('/r/docs', 'F10', true)).toEqual({ type: 'menu', path: '/r/docs' });
  expect(key('/r/docs', 'F10')).toEqual({ type: 'none' });
  expect(key('/r/docs', 'a')).toEqual({ type: 'none' });
  // A row that is gone gives the focus to the first row.
  expect(key('/r/gone', 'ArrowDown')).toEqual({ type: 'focus', path: '/r/docs' });
});

test('an entry that is not a file or a folder opens nothing', () => {
  const rows = visibleRows(
    '/r',
    new Map([['/r', listing('/r', [entry('/r/s.sock', 'other')])]]),
    new Set(),
  );
  expect(keyAction(rows, '/r/s.sock', 'Enter')).toEqual({ type: 'none' });
  expect(menuItems('other').map((item) => item.id)).toEqual([
    'reveal',
    'copy',
    'copy-relative',
    'insert',
  ]);
});

test('Open shows a file in the page, and Open in app and Reveal act on the Mac', () => {
  expect(menuItems('file').map((item) => item.id)).toEqual([
    'open',
    'open-app',
    'reveal',
    'copy',
    'copy-relative',
    'insert',
  ]);
  // A folder has nothing to show in the large view.
  expect(menuItems('dir').map((item) => item.id)).toEqual([
    'open-app',
    'reveal',
    'copy',
    'copy-relative',
    'insert',
  ]);
  // A phone keeps Open, which shows the file in its own page.
  expect(menuItems('file', false).map((item) => item.id)).toEqual([
    'open',
    'copy',
    'copy-relative',
    'insert',
  ]);
  expect(menuItems('dir', false).map((item) => item.id)).toEqual([
    'copy',
    'copy-relative',
    'insert',
  ]);
});

test('the share of the tree stays in its limits', () => {
  expect(clampSplit(0.5)).toBe(0.5);
  expect(clampSplit(0)).toBe(0.15);
  expect(clampSplit(2)).toBe(0.85);
  expect(clampSplit(Number.NaN)).toBe(0.45);
});
