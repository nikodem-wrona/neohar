// The Files tab of the drawer: the tree of the folder where the pane with the
// focus is now, else of the folder of the tab, and the file that the person opens under
// it. The core lists one folder for each call (`fs.list`) in the order of the tree, and reads a
// file for the view (`fs.read`). The page lists the open folders again, and reads the open file
// again, when the tab shows, when the root changes and on Refresh. It has no timer and no watch.
//
// A row has an icon that the core names, and a menu (a right click, the context menu key or
// Shift+F10) with Open, Open in app, Reveal in Finder, Copy path, Copy relative path and Insert into
// pane. Open shows the file in a large view over the window (`FileModal.tsx`), and Open in app gives
// it to an app of the Mac. A drag of a row onto a pane inserts the path there. The open folders of
// each root stay while the page lives.

import { type FsEntry, type FsReadResult, RpcError } from '@neohar/transport';
import {
  type KeyboardEvent,
  memo,
  type PointerEvent,
  useEffect,
  useMemo,
  useRef,
  useState,
} from 'react';
import { Portal } from 'radix-ui';
import { useApp } from '../app/context';
import { useLayer } from '../app/layer';
import { onLongPress } from '../app/press';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '../components/ui/dropdown-menu';
import { writeClipboard } from '../input/clipboard';
import { dragged, pressToDrag } from '../app/drag';
import { useLatest, useSelect, useValue } from '../lib/hooks';
import { cn } from '../lib/utils';
import { FileIcon } from './FileIcon';
import { FileModal } from './FileModal';
import { FileView } from './FileView';
import { useWindowFolder } from './folder';
import {
  clampSplit,
  keepSplit,
  keyAction,
  type Listing,
  type MenuItem,
  menuItems,
  type Row,
  storedSplit,
  visibleRows,
  withoutFolder,
} from './model';

/** What the page keeps of a root while it shows another one. */
interface Kept {
  open: string[];
  file: string | null;
  focus: string | null;
}

/** The trees of the roots that the window showed, for this page only. */
const kept = new Map<string, Kept>();

function errorText(failure: unknown, fallback: string): string {
  return failure instanceof RpcError ? failure.message : fallback;
}

/** The name of a folder: the last part of its path. */
function lastPart(path: string): string {
  return path.slice(path.lastIndexOf('/') + 1) || path;
}

/** The key of a row for the list: the path of an entry, or the key of a note. */
function keyOf(row: Row): string {
  return row.kind === 'entry' ? row.entry.path : row.key;
}

/** What a row of the tree does to the tab. The functions stay the same while the tab lives. */
interface RowActions {
  activate: (entry: FsEntry) => void;
  focus: (path: string) => void;
  showMenu: (entry: FsEntry, x: number, y: number) => void;
}

export function FilesPanel() {
  const { stores, transport, run, windowId, input, form } = useApp();
  const focusedPane = useSelect(stores.windows, (state) => state.byKey[windowId]?.focused_pane);
  const root = useWindowFolder(stores, windowId);

  const [listings, setListings] = useState<ReadonlyMap<string, Listing>>(() => new Map());
  const [open, setOpen] = useState<ReadonlySet<string>>(() => new Set());
  const [file, setFile] = useState<string | null>(null);
  const [preview, setPreview] = useState<FsReadResult | null>(null);
  const [fileError, setFileError] = useState<string | null>(null);
  const [focusPath, setFocusPath] = useState<string | null>(null);
  const [message, setMessage] = useState<{ text: string; error: boolean } | null>(null);
  const [menu, setMenu] = useState<{ entry: FsEntry; x: number; y: number } | null>(null);
  const [split, setSplit] = useState(storedSplit);
  // The large view of the open file.
  const [viewing, setViewing] = useState(false);

  const rows = useMemo(
    () => (root ? visibleRows(root, listings, open) : []),
    [root, listings, open],
  );
  const rowMap = new Map(rows.map((row) => [keyOf(row), row]));
  /** The row in the tab order: the one with the focus, else the first entry. */
  const tabStop = (() => {
    if (focusPath !== null && rowMap.has(focusPath)) return focusPath;
    const first = rows.find((row) => row.kind === 'entry');
    return first?.kind === 'entry' ? first.entry.path : null;
  })();

  const tree = useRef<HTMLDivElement>(null);
  const panel = useRef<HTMLDivElement>(null);
  // An answer that comes after a newer call, or for another root, is dropped.
  const generation = useRef(0);
  const asked = useRef(new Map<string, number>()).current;
  const reads = useRef(0);
  const rootNow = useLatest(root);

  const list = (path: string): void => {
    const mine = (asked.get(path) ?? 0) + 1;
    asked.set(path, mine);
    const gen = generation.current;
    const current = () => gen === generation.current && asked.get(path) === mine;
    transport
      .call('fs.list', { path })
      .then((result) => {
        if (current()) setListings((before) => new Map(before).set(path, { result }));
      })
      .catch((failure: unknown) => {
        if (!current()) return;
        // An open folder that is gone closes.
        if (
          path !== rootNow.current &&
          failure instanceof RpcError &&
          failure.code === 'not_found'
        ) {
          setOpen((before) => withoutFolder(before, path));
          setListings((before) => {
            const next = new Map(before);
            next.delete(path);
            return next;
          });
          return;
        }
        const error = errorText(failure, 'The folder did not load.');
        setListings((before) => new Map(before).set(path, { error }));
      });
  };

  const read = (path: string): void => {
    const mine = ++reads.current;
    const gen = generation.current;
    const current = () => gen === generation.current && mine === reads.current;
    transport
      .call('fs.read', { path })
      .then((result) => {
        if (!current()) return;
        setPreview(result);
        setFileError(null);
      })
      .catch((failure: unknown) => {
        if (!current()) return;
        setPreview(null);
        setFileError(errorText(failure, 'The file did not load.'));
      });
  };

  /** Lists the root and the open folders of `at`, and reads its open file, again. */
  const refreshOf = (
    at: string | undefined,
    openNow: ReadonlySet<string>,
    shownFile: string | null,
  ): void => {
    if (!at) return;
    list(at);
    for (const path of openNow) list(path);
    if (shownFile) read(shownFile);
  };
  const refresh = (): void => refreshOf(root, open, file);

  const keep = (at: string | undefined): void => {
    if (at) kept.set(at, { open: [...open], file, focus: focusPath });
  };

  const toggle = (path: string): void => {
    if (open.has(path)) {
      setOpen((before) => withoutFolder(before, path));
    } else {
      setOpen((before) => new Set(before).add(path));
      list(path);
    }
  };

  const openFile = (path: string): void => {
    if (file === path) return;
    setFile(path);
    setPreview(null);
    setFileError(null);
    read(path);
  };

  const activate = (entry: FsEntry): void => {
    setFocusPath(entry.path);
    if (entry.kind === 'dir') toggle(entry.path);
    else if (entry.kind === 'file') openFile(entry.path);
  };

  const failed = (failure: unknown): void => {
    setMessage({ text: errorText(failure, 'The action did not work.'), error: true });
  };

  /** Gives the keyboard to a pane, so that the person can go on typing after an insert. */
  const focusPane = (pane: string): void => {
    run('pane.focus', { window: windowId, pane });
    document
      .querySelector<HTMLTextAreaElement>(`[data-pane="${CSS.escape(pane)}"] .pane-input`)
      ?.focus({ preventScroll: true });
  };

  const insert = (path: string, pane?: string): void => {
    const target = pane ?? focusedPane ?? undefined;
    if (!target) {
      setMessage({ text: 'No pane has the focus. Click a pane first.', error: true });
      return;
    }
    input.insertPaths(target, [path]).then(() => {
      setMessage(null);
      focusPane(target);
    }, failed);
  };

  /**
   * Copies a path. The desk writes the clipboard through its host. A phone writes its own clipboard
   * in the tap, with the text that the core gives.
   */
  const copyPath = (params: { path: string; relative_to?: string }, done: string): void => {
    if (form !== 'phone') {
      transport
        .call('fs.copy_path', params)
        .then(() => setMessage({ text: done, error: false }), failed);
      return;
    }
    const text = transport
      .call('fs.copy_path', { ...params, return_text: true })
      .then((result) => result.text ?? '');
    writeClipboard(text).then(() => setMessage({ text: done, error: false }), failed);
  };

  const act = (id: MenuItem['id'], path: string): void => {
    setMenu(null);
    const at = root;
    switch (id) {
      case 'open':
        viewedFrom.current = document.activeElement;
        openFile(path);
        setMessage(null);
        setViewing(true);
        break;
      case 'open-app':
        transport.call('fs.open', { window: windowId, path }).then(() => setMessage(null), failed);
        break;
      case 'reveal':
        transport
          .call('fs.open', { window: windowId, path, reveal: true })
          .then(() => setMessage(null), failed);
        break;
      case 'copy':
        copyPath({ path }, 'Copied the path.');
        break;
      case 'copy-relative':
        copyPath(at ? { path, relative_to: at } : { path }, 'Copied the relative path.');
        break;
      case 'insert':
        // The pane takes the keyboard after the insert, so the large view closes first.
        closeView();
        insert(path);
        break;
    }
  };

  /** Closes the large view. */
  const closeView = (): void => {
    if (!viewing) return;
    setViewing(false);
  };
  /**
   * The large view is out of the page. The focus goes back to the element that had it before the
   * view, also from the pane that took it when the layer closed, or to the row of the file when
   * that element is gone. A box that holds the tree, such as the panel of the tab after a click
   * on a button that WebKit gives no focus, counts as gone.
   */
  const viewedFrom = useRef<Element | null>(null);
  const viewClosed = (): void => {
    const back = viewedFrom.current;
    viewedFrom.current = null;
    const holdsTree = !!back && !!tree.current && back.contains(tree.current);
    if (back instanceof HTMLElement && back.isConnected && back !== document.body && !holdsTree) {
      back.focus({ preventScroll: true });
    } else if (file) {
      focusRow(file);
    }
  };

  /** The path that the person drags onto a pane now. */
  const item = useValue(dragged);
  const liftedPath = item?.kind === 'path' ? item.path : null;

  const rowElement = (path: string): HTMLElement | null =>
    tree.current?.querySelector<HTMLElement>(`[data-path="${CSS.escape(path)}"]`) ?? null;

  const focusRow = (path: string): void => {
    setFocusPath(path);
    const element = rowElement(path);
    element?.focus({ preventScroll: true });
    element?.scrollIntoView({ block: 'nearest' });
  };

  const showMenu = (entry: FsEntry, x: number, y: number): void => {
    focusFirst.current = true;
    setFocusPath(entry.path);
    setMenu({ entry, x, y });
  };

  // The effects and the rows read the functions of the last render.
  const latest = useLatest({ keep, refreshOf, activate, showMenu, root });
  const rowActions = useMemo<RowActions>(
    () => ({
      activate: (entry) => latest.current.activate(entry),
      focus: setFocusPath,
      showMenu: (entry, x, y) => latest.current.showMenu(entry, x, y),
    }),
    [latest],
  );

  // A new root shows its tree as the person left it, listed again.
  const shownRoot = useRef<string | undefined>(undefined);
  useEffect(() => {
    const now = latest.current;
    now.keep(shownRoot.current);
    shownRoot.current = root;
    generation.current += 1;
    const back = root ? kept.get(root) : undefined;
    const backOpen = new Set(back?.open ?? []);
    const backFile = back?.file ?? null;
    setListings(new Map());
    setOpen(backOpen);
    setFile(backFile);
    setPreview(null);
    setFileError(null);
    setFocusPath(back?.focus ?? null);
    setMessage(null);
    setMenu(null);
    setViewing(false);
    now.refreshOf(root, backOpen, backFile);
  }, [root, latest]);
  useEffect(() => () => latest.current.keep(latest.current.root), [latest]);

  const onTreeKey = (event: KeyboardEvent<HTMLDivElement>): void => {
    const target = (event.target as HTMLElement).closest<HTMLElement>('[data-path]');
    const path = target?.dataset['path'];
    if (path === undefined) return;
    const action = keyAction(rows, path, event.key, event.shiftKey);
    if (action.type === 'none') return;
    event.preventDefault();
    const row = rowMap.get(action.path);
    switch (action.type) {
      case 'focus':
        focusRow(action.path);
        break;
      case 'open':
      case 'close':
        toggle(action.path);
        break;
      case 'read':
        openFile(action.path);
        break;
      case 'menu': {
        const box = target?.getBoundingClientRect();
        if (row?.kind === 'entry' && box) showMenu(row.entry, box.left + 16, box.bottom);
        break;
      }
    }
  };

  // The menu of a row is the Dropdown Menu of shadcn/ui on an anchor of no size at the point of the
  // right click or of the long press, or under the row for a key. The Context Menu of Radix opens
  // only from its own events. The menu does not hold the page: a click elsewhere closes it and
  // reaches the page too. While it closes, it shows the entry that it had.
  useLayer(menu !== null);
  const [lastMenu, setLastMenu] = useState(menu);
  if (menu && menu !== lastMenu) setLastMenu(menu);
  const shownMenu = menu ?? lastMenu;
  // The first item takes the focus when the menu opens, also after a right click: Radix gives it
  // to the box then.
  const focusFirst = useRef(false);

  const onMenuKey = (event: KeyboardEvent<HTMLDivElement>): void => {
    // Escape closes the menu only. Tab closes it too, and the focus goes back to the row.
    if (event.key === 'Escape') {
      event.stopPropagation();
    } else if (event.key === 'Tab') {
      event.preventDefault();
      event.stopPropagation();
      setMenu(null);
    }
  };

  // The rule between the tree and the file: a drag or the arrow keys change the share.
  const onRulePointer = (event: PointerEvent<HTMLDivElement>): void => {
    const element = panel.current;
    if (event.button !== 0 || !element) return;
    const rule = event.currentTarget;
    rule.setPointerCapture(event.pointerId);
    rule.classList.add('dragging');
    const box = element.getBoundingClientRect();
    let share = split;
    const move = (moved: globalThis.PointerEvent): void => {
      share = clampSplit((moved.clientY - box.top) / Math.max(1, box.height));
      setSplit(share);
    };
    const end = (): void => {
      rule.classList.remove('dragging');
      keepSplit(share);
      rule.removeEventListener('pointermove', move);
      rule.removeEventListener('pointerup', end);
      rule.removeEventListener('pointercancel', end);
    };
    rule.addEventListener('pointermove', move);
    rule.addEventListener('pointerup', end);
    rule.addEventListener('pointercancel', end);
  };
  const onRuleKey = (event: KeyboardEvent<HTMLDivElement>): void => {
    const by = event.shiftKey ? 0.15 : 0.05;
    let next: number;
    if (event.key === 'ArrowUp') next = clampSplit(split - by);
    else if (event.key === 'ArrowDown') next = clampSplit(split + by);
    else return;
    setSplit(next);
    event.preventDefault();
    keepSplit(next);
  };

  return (
    <div className="files-panel" ref={panel}>
      {root ? (
        <>
          <header className="files-head">
            <span className="files-root" title={root}>
              <span className="files-root-name">{lastPart(root)}</span>
              <span className="files-root-path">{root}</span>
            </span>
            <button
              type="button"
              className="files-refresh"
              title="List the open folders and read the open file again"
              onClick={refresh}
            >
              Refresh
            </button>
          </header>
          {message ? (
            <p
              className={cn('files-message', { error: message.error })}
              role={message.error ? 'alert' : 'status'}
            >
              {message.text}
            </p>
          ) : null}
          <div
            className="files-tree"
            role="tree"
            aria-label="Files"
            ref={tree}
            style={file ? { flex: `0 0 ${split * 100}%` } : undefined}
            onKeyDown={onTreeKey}
          >
            {listings.has(root) ? (
              rows.map((row) =>
                row.kind === 'note' ? (
                  <div
                    key={row.key}
                    role="treeitem"
                    aria-disabled="true"
                    aria-level={row.depth + 1}
                    className={cn('files-note', { error: row.error })}
                    style={{ paddingLeft: `${row.depth * 2 + 2.5}ch` }}
                  >
                    {row.text}
                  </div>
                ) : (
                  <FileRow
                    key={row.entry.path}
                    entry={row.entry}
                    depth={row.depth}
                    open={row.open}
                    busy={row.busy}
                    current={file === row.entry.path}
                    menuOpen={menu?.entry.path === row.entry.path}
                    lifted={liftedPath === row.entry.path}
                    tabStop={tabStop === row.entry.path}
                    actions={rowActions}
                  />
                ),
              )
            ) : (
              <p className="drawer-empty">Listing…</p>
            )}
          </div>
          {file ? (
            <>
              <div
                className="files-rule"
                role="separator"
                aria-orientation="horizontal"
                aria-label="Size of the tree"
                aria-valuenow={Math.round(split * 100)}
                aria-valuemin={15}
                aria-valuemax={85}
                tabIndex={0}
                onPointerDown={onRulePointer}
                onKeyDown={onRuleKey}
              />
              <FileView
                path={file}
                preview={preview}
                error={fileError}
                onAction={act}
                onClose={() => {
                  setFile(null);
                  setPreview(null);
                  setFileError(null);
                }}
              />
              {viewing ? (
                <FileModal
                  path={file}
                  preview={preview}
                  error={fileError}
                  message={message}
                  onAction={act}
                  onClose={closeView}
                  onClosed={viewClosed}
                />
              ) : null}
            </>
          ) : null}
        </>
      ) : (
        <p className="drawer-empty">This tab has no folder.</p>
      )}
      <DropdownMenu
        open={menu !== null}
        onOpenChange={(open) => {
          if (!open) setMenu(null);
        }}
        modal={false}
      >
        {shownMenu ? (
          <>
            <Portal.Root>
              <DropdownMenuTrigger asChild>
                <span
                  aria-hidden="true"
                  className="pointer-events-none fixed size-0"
                  style={{ left: shownMenu.x, top: shownMenu.y }}
                />
              </DropdownMenuTrigger>
            </Portal.Root>
            <DropdownMenuContent
              // Beside the point, as the Context Menu of Radix: the release of a right click
              // must not fall on an item, which would choose it.
              side="right"
              align="start"
              sideOffset={2}
              aria-label={`Actions of ${shownMenu.entry.name}`}
              aria-labelledby={undefined}
              className="min-w-[22ch]"
              onKeyDown={onMenuKey}
              onFocus={(event) => {
                if (!focusFirst.current) return;
                focusFirst.current = false;
                if (event.target !== event.currentTarget) return;
                event.currentTarget.querySelector<HTMLElement>('[role="menuitem"]')?.focus();
              }}
              onCloseAutoFocus={(event) => {
                event.preventDefault();
                // The row takes the focus back, also from the pane that took it when the layer
                // closed. Only the large view keeps it.
                if (!document.activeElement?.closest('[role="dialog"]')) {
                  focusRow(shownMenu.entry.path);
                }
              }}
            >
              {menuItems(shownMenu.entry.kind, form !== 'phone').map((item) => (
                <DropdownMenuItem
                  key={item.id}
                  data-action={item.id}
                  className="py-1"
                  // A release on an item without a press on it chooses nothing: the release of
                  // the right click that opened the menu must not choose.
                  onPointerUp={(event) => event.preventDefault()}
                  onSelect={() => act(item.id, shownMenu.entry.path)}
                >
                  {item.label}
                </DropdownMenuItem>
              ))}
            </DropdownMenuContent>
          </>
        ) : null}
      </DropdownMenu>
    </div>
  );
}

/** A row of an entry of the tree. */
const FileRow = memo(function FileRow(props: {
  entry: FsEntry;
  depth: number;
  open: boolean;
  busy: boolean;
  /** The open file. */
  current: boolean;
  menuOpen: boolean;
  lifted: boolean;
  tabStop: boolean;
  actions: RowActions;
}) {
  const { entry, actions } = props;
  const key = entry.path;
  const isDir = entry.kind === 'dir';
  // A long press opens the menu of the entry of now.
  const entryNow = useLatest(entry);
  return (
    <div
      role="treeitem"
      className={cn('files-row', {
        hidden: entry.hidden,
        other: entry.kind === 'other',
        current: props.current,
        'menu-open': props.menuOpen,
        lifted: props.lifted,
      })}
      data-path={key}
      data-kind={entry.kind}
      aria-level={props.depth + 1}
      aria-expanded={isDir ? props.open : undefined}
      aria-selected={props.current}
      aria-busy={props.busy}
      tabIndex={props.tabStop ? 0 : -1}
      title={entry.path}
      style={{ paddingLeft: `${props.depth * 2 + 0.5}ch` }}
      onClick={() => actions.activate(entry)}
      onFocus={() => actions.focus(key)}
      onContextMenu={(event) => {
        event.preventDefault();
        actions.showMenu(entry, event.clientX, event.clientY);
      }}
      // A press that moves drags the path onto a pane, with pointer
      // events. A press that does not move is a click.
      onPointerDown={(event) => {
        pressToDrag(event.nativeEvent, { kind: 'path', path: key });
        // A phone has no right click: a long press opens the menu.
        onLongPress(event.nativeEvent, (x, y) => actions.showMenu(entryNow.current, x, y));
      }}
    >
      <span className="files-arrow" aria-hidden="true">
        {isDir ? (props.open ? '▾' : '▸') : ''}
      </span>
      <FileIcon icon={entry.icon} folder={isDir} open={props.open} />
      <span className="files-name">{entry.name}</span>
      {entry.link ? (
        <span className="files-link" title="A symbolic link">
          →
        </span>
      ) : null}
    </div>
  );
});
