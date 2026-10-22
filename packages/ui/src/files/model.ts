// What the Files tab needs besides the calls. The core lists each folder
// in the order of the tree and makes the text of a preview. This module only picks the root,
// puts the listings of the open folders in one list of rows, and turns a key into a move.

import type {
  FsEntry,
  FsListResult,
  PaneValue,
  TaskValue,
  WorkspaceValue,
} from '@neohar/transport';

/**
 * The root of the tree: the folder where the pane with the focus is now, else the
 * worktree of the task of the tab, else the root of the workspace of the tab.
 */
export function rootOf(
  pane: PaneValue | undefined,
  task: TaskValue | undefined,
  workspace: WorkspaceValue | undefined,
): string | undefined {
  return pane?.cwd ?? task?.worktree ?? workspace?.root ?? undefined;
}

/** What the page holds for a folder that it listed. */
export type Listing = { result: FsListResult } | { error: string };

/** A row of the tree: an entry, or a note in the place of the entries of a folder. */
export type Row =
  | { kind: 'entry'; entry: FsEntry; depth: number; open: boolean; busy: boolean }
  | { kind: 'note'; key: string; text: string; depth: number; error: boolean };

/** The text of the last row of a folder that the core cut. */
export function truncatedNote(count: number): string {
  return `The folder has more entries. The tree shows the first ${count}.`;
}

/**
 * The rows that show: the entries of `root`, and under each open folder its entries, in the
 * order of the core. A folder that is open and not listed yet shows no rows under it.
 */
export function visibleRows(
  root: string,
  listings: ReadonlyMap<string, Listing>,
  open: ReadonlySet<string>,
): Row[] {
  const rows: Row[] = [];
  const walk = (path: string, depth: number): void => {
    const listing = listings.get(path);
    if (listing === undefined) return;
    if ('error' in listing) {
      rows.push({ kind: 'note', key: `${path}#error`, text: listing.error, depth, error: true });
      return;
    }
    const { entries, truncated } = listing.result;
    if (entries.length === 0) {
      rows.push({ kind: 'note', key: `${path}#empty`, text: 'Empty', depth, error: false });
    }
    for (const entry of entries) {
      const isOpen = entry.kind === 'dir' && open.has(entry.path);
      rows.push({
        kind: 'entry',
        entry,
        depth,
        open: isOpen,
        busy: isOpen && !listings.has(entry.path),
      });
      if (isOpen) walk(entry.path, depth + 1);
    }
    if (truncated) {
      rows.push({
        kind: 'note',
        key: `${path}#truncated`,
        text: truncatedNote(entries.length),
        depth,
        error: false,
      });
    }
  };
  walk(root, 0);
  return rows;
}

/** The open folders that `close` hides: the folder and every open folder in it. */
export function withoutFolder(open: ReadonlySet<string>, folder: string): Set<string> {
  const inside = `${folder}/`;
  return new Set([...open].filter((path) => path !== folder && !path.startsWith(inside)));
}

/** What a key on a row of the tree does. */
export type TreeAction =
  | { type: 'focus'; path: string }
  | { type: 'open'; path: string }
  | { type: 'close'; path: string }
  | { type: 'read'; path: string }
  | { type: 'menu'; path: string }
  | { type: 'none' };

function entries(rows: readonly Row[]): Extract<Row, { kind: 'entry' }>[] {
  return rows.filter((row): row is Extract<Row, { kind: 'entry' }> => row.kind === 'entry');
}

/**
 * The action of a key on the row of `path` (the ARIA tree pattern): Up and Down move, Right opens
 * a folder or moves into it, Left closes a folder or moves to its parent, Enter opens a file or
 * opens and closes a folder, Home and End go to the ends.
 */
export function keyAction(
  rows: readonly Row[],
  path: string,
  key: string,
  shift = false,
): TreeAction {
  const list = entries(rows);
  const at = list.findIndex((row) => row.entry.path === path);
  if (list.length === 0) return { type: 'none' };
  if (at < 0) return { type: 'focus', path: list[0]!.entry.path };
  const row = list[at]!;
  const isDir = row.entry.kind === 'dir';
  const focus = (index: number): TreeAction => {
    const target = list[Math.max(0, Math.min(list.length - 1, index))]!;
    return target.entry.path === path
      ? { type: 'none' }
      : { type: 'focus', path: target.entry.path };
  };
  switch (key) {
    case 'ArrowDown':
      return focus(at + 1);
    case 'ArrowUp':
      return focus(at - 1);
    case 'Home':
      return focus(0);
    case 'End':
      return focus(list.length - 1);
    case 'ArrowRight': {
      if (!isDir) return { type: 'none' };
      if (!row.open) return { type: 'open', path };
      const next = list[at + 1];
      return next && next.depth > row.depth
        ? { type: 'focus', path: next.entry.path }
        : { type: 'none' };
    }
    case 'ArrowLeft': {
      if (isDir && row.open) return { type: 'close', path };
      for (let index = at - 1; index >= 0; index -= 1) {
        if (list[index]!.depth < row.depth) return { type: 'focus', path: list[index]!.entry.path };
      }
      return { type: 'none' };
    }
    case 'Enter':
      if (isDir) return row.open ? { type: 'close', path } : { type: 'open', path };
      return row.entry.kind === 'file' ? { type: 'read', path } : { type: 'none' };
    case 'ContextMenu':
      return { type: 'menu', path };
    case 'F10':
      return shift ? { type: 'menu', path } : { type: 'none' };
    default:
      return { type: 'none' };
  }
}

/** An item of the menu of a row. */
export interface MenuItem {
  id: 'open' | 'open-app' | 'reveal' | 'copy' | 'copy-relative' | 'insert';
  label: string;
}

/**
 * The items of the menu of a row. Open shows a file in the large view of the page. Open in app and
 * Reveal act on the screen of the Mac, so a phone does not have them. A socket or a broken link has
 * nothing to open.
 */
export function menuItems(kind: FsEntry['kind'], onMac = true): MenuItem[] {
  return [
    ...(kind === 'file' ? [{ id: 'open', label: 'Open' } as const] : []),
    ...(onMac && kind !== 'other' ? [{ id: 'open-app', label: 'Open in app' } as const] : []),
    ...(onMac ? [{ id: 'reveal', label: 'Reveal in Finder' } as const] : []),
    { id: 'copy', label: 'Copy path' },
    { id: 'copy-relative', label: 'Copy relative path' },
    { id: 'insert', label: 'Insert into pane' },
  ];
}

/** The share of the height of the tab that the tree takes when a file is open. */
export const SPLIT_LIMITS = { min: 0.15, max: 0.85 } as const;
const DEFAULT_SPLIT = 0.45;
const SPLIT_KEY = 'neohar.files.split';

export function clampSplit(share: number): number {
  if (!Number.isFinite(share)) return DEFAULT_SPLIT;
  return Math.min(SPLIT_LIMITS.max, Math.max(SPLIT_LIMITS.min, share));
}

/** The share that this browser kept. Storage can be missing or refuse, and then the default. */
export function storedSplit(): number {
  try {
    const kept = Number(globalThis.localStorage?.getItem(SPLIT_KEY) ?? 'NaN');
    return Number.isNaN(kept) ? DEFAULT_SPLIT : clampSplit(kept);
  } catch {
    return DEFAULT_SPLIT;
  }
}

export function keepSplit(share: number): void {
  try {
    globalThis.localStorage?.setItem(SPLIT_KEY, String(clampSplit(share)));
  } catch {
    // The share is a comfort of this view only.
  }
}
