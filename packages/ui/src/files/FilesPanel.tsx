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
import { createEffect, createMemo, createSignal, For, on, onCleanup, Show } from 'solid-js';
import { useApp } from '../app/context';
import { onLongPress } from '../app/press';
import { writeClipboard } from '../input/clipboard';
import { pressToDrag } from '../app/drag';
import { FileIcon } from './FileIcon';
import { FileModal } from './FileModal';
import { FileView } from './FileView';
import { windowFolder } from './folder';
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
import { dragged } from '../lib/solid';

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

export function FilesPanel() {
  const { stores, transport, run, windowId, input, form } = useApp();
  const shown = () => stores.windows.state.byKey[windowId];
  const root = createMemo(() => windowFolder(stores, windowId));

  const [listings, setListings] = createSignal<ReadonlyMap<string, Listing>>(new Map());
  const [open, setOpen] = createSignal<ReadonlySet<string>>(new Set());
  const [file, setFile] = createSignal<string | null>(null);
  const [preview, setPreview] = createSignal<FsReadResult | null>(null);
  const [fileError, setFileError] = createSignal<string | null>(null);
  const [focusPath, setFocusPath] = createSignal<string | null>(null);
  const [message, setMessage] = createSignal<{ text: string; error: boolean } | null>(null);
  const [menu, setMenu] = createSignal<{ entry: FsEntry; x: number; y: number } | null>(null);
  const [split, setSplit] = createSignal(storedSplit());
  // The large view of the open file.
  const [viewing, setViewing] = createSignal(false);

  const rows = createMemo(() => {
    const at = root();
    return at ? visibleRows(at, listings(), open()) : [];
  });
  const rowMap = createMemo(() => new Map(rows().map((row) => [keyOf(row), row])));
  const keys = createMemo(() => rows().map(keyOf));
  /** The row in the tab order: the one with the focus, else the first entry. */
  const tabStop = createMemo(() => {
    const path = focusPath();
    if (path !== null && rowMap().has(path)) return path;
    const first = rows().find((row) => row.kind === 'entry');
    return first?.kind === 'entry' ? first.entry.path : null;
  });

  let tree: HTMLDivElement | undefined;
  let menuBox: HTMLDivElement | undefined;
  let panel: HTMLDivElement | undefined;
  // An answer that comes after a newer call, or for another root, is dropped.
  let generation = 0;
  const asked = new Map<string, number>();
  let reads = 0;

  const list = (path: string): void => {
    const mine = (asked.get(path) ?? 0) + 1;
    asked.set(path, mine);
    const gen = generation;
    const current = () => gen === generation && asked.get(path) === mine;
    transport
      .call('fs.list', { path })
      .then((result) => {
        if (current()) setListings((before) => new Map(before).set(path, { result }));
      })
      .catch((failure: unknown) => {
        if (!current()) return;
        // An open folder that is gone closes.
        if (path !== root() && failure instanceof RpcError && failure.code === 'not_found') {
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
    const mine = ++reads;
    const gen = generation;
    const current = () => gen === generation && mine === reads;
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

  const refresh = (): void => {
    const at = root();
    if (!at) return;
    list(at);
    for (const path of open()) list(path);
    const shownFile = file();
    if (shownFile) read(shownFile);
  };

  const keep = (at: string | undefined): void => {
    if (at) kept.set(at, { open: [...open()], file: file(), focus: focusPath() });
  };

  // A new root shows its tree as the person left it, listed again.
  createEffect(
    on(root, (next, previous) => {
      keep(previous);
      generation += 1;
      const back = next ? kept.get(next) : undefined;
      setListings(new Map());
      setOpen(new Set(back?.open ?? []));
      setFile(back?.file ?? null);
      setPreview(null);
      setFileError(null);
      setFocusPath(back?.focus ?? null);
      setMessage(null);
      setMenu(null);
      setViewing(false);
      refresh();
    }),
  );
  onCleanup(() => keep(root()));

  const toggle = (path: string): void => {
    if (open().has(path)) {
      setOpen((before) => withoutFolder(before, path));
    } else {
      setOpen((before) => new Set(before).add(path));
      list(path);
    }
  };

  const openFile = (path: string): void => {
    if (file() === path) return;
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
    const target = pane ?? shown()?.focused_pane ?? undefined;
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
    const at = root();
    switch (id) {
      case 'open':
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

  /** Closes the large view. The focus goes back to the row when the element that had it is gone. */
  const closeView = (): void => {
    if (!viewing()) return;
    setViewing(false);
    const shownFile = file();
    if (shownFile && document.activeElement === document.body) focusRow(shownFile);
  };

  /** The path that the person drags onto a pane now. */
  const liftedPath = (): string | null => {
    const item = dragged();
    return item?.kind === 'path' ? item.path : null;
  };

  const rowElement = (path: string): HTMLElement | null =>
    tree?.querySelector<HTMLElement>(`[data-path="${CSS.escape(path)}"]`) ?? null;

  const focusRow = (path: string): void => {
    setFocusPath(path);
    const element = rowElement(path);
    element?.focus({ preventScroll: true });
    element?.scrollIntoView({ block: 'nearest' });
  };

  const showMenu = (entry: FsEntry, x: number, y: number): void => {
    setFocusPath(entry.path);
    setMenu({ entry, x, y });
  };

  const closeMenu = (): void => {
    const shownMenu = menu();
    setMenu(null);
    if (shownMenu) focusRow(shownMenu.entry.path);
  };

  const onTreeKey = (event: KeyboardEvent): void => {
    const target = (event.target as HTMLElement).closest<HTMLElement>('[data-path]');
    const path = target?.dataset['path'];
    if (path === undefined) return;
    const action = keyAction(rows(), path, event.key, event.shiftKey);
    if (action.type === 'none') return;
    event.preventDefault();
    const row = rowMap().get(action.path);
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

  // The menu is a popover in the top layer: a click elsewhere closes it. It stays in the
  // window and takes the focus.
  createEffect(() => {
    if (!menu() || !menuBox) return;
    if (!menuBox.matches(':popover-open')) menuBox.showPopover();
    const box = menuBox.getBoundingClientRect();
    if (box.right > window.innerWidth - 4) {
      menuBox.style.left = `${Math.max(4, window.innerWidth - box.width - 4)}px`;
    }
    if (box.bottom > window.innerHeight - 4) {
      menuBox.style.top = `${Math.max(4, window.innerHeight - box.height - 4)}px`;
    }
    menuBox.querySelector<HTMLElement>('[role="menuitem"]')?.focus();
  });

  const onMenuKey = (event: KeyboardEvent): void => {
    const items = [...(menuBox?.querySelectorAll<HTMLElement>('[role="menuitem"]') ?? [])];
    const at = items.indexOf(document.activeElement as HTMLElement);
    if (event.key === 'Escape' || event.key === 'Tab') {
      event.preventDefault();
      event.stopPropagation();
      closeMenu();
    } else if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      event.preventDefault();
      const step = event.key === 'ArrowDown' ? 1 : -1;
      items[(at + step + items.length) % items.length]?.focus();
    } else if (event.key === 'Home' || event.key === 'End') {
      event.preventDefault();
      (event.key === 'Home' ? items[0] : items.at(-1))?.focus();
    }
  };

  // The rule between the tree and the file: a drag or the arrow keys change the share.
  const onRulePointer = (event: PointerEvent): void => {
    if (event.button !== 0 || !panel) return;
    const rule = event.currentTarget as HTMLElement;
    rule.setPointerCapture(event.pointerId);
    rule.classList.add('dragging');
    const box = panel.getBoundingClientRect();
    const move = (moved: PointerEvent): void => {
      setSplit(clampSplit((moved.clientY - box.top) / Math.max(1, box.height)));
    };
    const end = (): void => {
      rule.classList.remove('dragging');
      keepSplit(split());
      rule.removeEventListener('pointermove', move);
      rule.removeEventListener('pointerup', end);
      rule.removeEventListener('pointercancel', end);
    };
    rule.addEventListener('pointermove', move);
    rule.addEventListener('pointerup', end);
    rule.addEventListener('pointercancel', end);
  };
  const onRuleKey = (event: KeyboardEvent): void => {
    const by = event.shiftKey ? 0.15 : 0.05;
    if (event.key === 'ArrowUp') setSplit(clampSplit(split() - by));
    else if (event.key === 'ArrowDown') setSplit(clampSplit(split() + by));
    else return;
    event.preventDefault();
    keepSplit(split());
  };

  return (
    <div class="files-panel" ref={(element) => (panel = element)}>
      <Show when={root()} fallback={<p class="drawer-empty">This tab has no folder.</p>}>
        {(at) => (
          <>
            <header class="files-head">
              <span class="files-root" title={at()}>
                <span class="files-root-name">{lastPart(at())}</span>
                <span class="files-root-path">{at()}</span>
              </span>
              <button
                type="button"
                class="files-refresh"
                title="List the open folders and read the open file again"
                onClick={refresh}
              >
                Refresh
              </button>
            </header>
            <Show when={message()}>
              {(note) => (
                <p
                  class="files-message"
                  classList={{ error: note().error }}
                  role={note().error ? 'alert' : 'status'}
                >
                  {note().text}
                </p>
              )}
            </Show>
            <div
              class="files-tree"
              role="tree"
              aria-label="Files"
              ref={(element) => (tree = element)}
              style={file() ? { flex: `0 0 ${split() * 100}%` } : undefined}
              onKeyDown={onTreeKey}
            >
              <Show when={listings().has(at())} fallback={<p class="drawer-empty">Listing…</p>}>
                <For each={keys()}>
                  {(key) => (
                    <Show when={rowMap().get(key)}>
                      {(row) => {
                        const first = row();
                        if (first.kind === 'note') {
                          return (
                            <div
                              role="treeitem"
                              aria-disabled="true"
                              aria-level={first.depth + 1}
                              class="files-note"
                              classList={{ error: first.error }}
                              style={{ 'padding-left': `${first.depth * 2 + 2.5}ch` }}
                            >
                              {first.text}
                            </div>
                          );
                        }
                        const entry = () => {
                          const now = row();
                          return now.kind === 'entry' ? now : first;
                        };
                        const isDir = () => entry().entry.kind === 'dir';
                        return (
                          <div
                            role="treeitem"
                            class="files-row"
                            classList={{
                              hidden: entry().entry.hidden,
                              other: entry().entry.kind === 'other',
                              current: file() === key,
                              'menu-open': menu()?.entry.path === key,
                              lifted: liftedPath() === key,
                            }}
                            data-path={key}
                            data-kind={entry().entry.kind}
                            aria-level={entry().depth + 1}
                            aria-expanded={isDir() ? entry().open : undefined}
                            aria-selected={file() === key}
                            aria-busy={entry().busy}
                            tabIndex={tabStop() === key ? 0 : -1}
                            title={entry().entry.path}
                            style={{ 'padding-left': `${entry().depth * 2 + 0.5}ch` }}
                            onClick={() => activate(entry().entry)}
                            onFocus={() => setFocusPath(key)}
                            onContextMenu={(event) => {
                              event.preventDefault();
                              showMenu(entry().entry, event.clientX, event.clientY);
                            }}
                            // A press that moves drags the path onto a pane, with pointer
                            // events. A press that does not move is a click.
                            onPointerDown={(event) => {
                              pressToDrag(event, { kind: 'path', path: key });
                              // A phone has no right click: a long press opens the menu.
                              onLongPress(event, (x, y) => showMenu(entry().entry, x, y));
                            }}
                          >
                            <span class="files-arrow" aria-hidden="true">
                              {isDir() ? (entry().open ? '▾' : '▸') : ''}
                            </span>
                            <FileIcon
                              icon={entry().entry.icon}
                              folder={isDir()}
                              open={entry().open}
                            />
                            <span class="files-name">{entry().entry.name}</span>
                            <Show when={entry().entry.link}>
                              <span class="files-link" title="A symbolic link">
                                →
                              </span>
                            </Show>
                          </div>
                        );
                      }}
                    </Show>
                  )}
                </For>
              </Show>
            </div>
            <Show when={file()}>
              {(path) => (
                <>
                  <div
                    class="files-rule"
                    role="separator"
                    aria-orientation="horizontal"
                    aria-label="Size of the tree"
                    aria-valuenow={Math.round(split() * 100)}
                    aria-valuemin={15}
                    aria-valuemax={85}
                    tabindex="0"
                    onPointerDown={onRulePointer}
                    onKeyDown={onRuleKey}
                  />
                  <FileView
                    path={path()}
                    preview={preview()}
                    error={fileError()}
                    onAction={act}
                    onClose={() => {
                      setFile(null);
                      setPreview(null);
                      setFileError(null);
                    }}
                  />
                  <Show when={viewing()}>
                    <FileModal
                      path={path()}
                      preview={preview()}
                      error={fileError()}
                      message={message()}
                      onAction={act}
                      onClose={closeView}
                    />
                  </Show>
                </>
              )}
            </Show>
          </>
        )}
      </Show>
      <Show when={menu()}>
        {(shownMenu) => (
          <div
            class="files-menu"
            role="menu"
            popover="auto"
            aria-label={`Actions of ${shownMenu().entry.name}`}
            ref={(element) => {
              menuBox = element;
              // A click outside the menu closes the popover, and the menu goes with it.
              element.addEventListener('toggle', (event) => {
                if ((event as ToggleEvent).newState === 'closed') setMenu(null);
              });
            }}
            style={{ left: `${shownMenu().x}px`, top: `${shownMenu().y}px` }}
            onKeyDown={onMenuKey}
          >
            <For each={menuItems(shownMenu().entry.kind, form !== 'phone')}>
              {(item) => (
                <button
                  type="button"
                  role="menuitem"
                  class="files-menu-item"
                  data-action={item.id}
                  tabIndex={-1}
                  onClick={() => act(item.id, shownMenu().entry.path)}
                >
                  {item.label}
                </button>
              )}
            </For>
          </div>
        )}
      </Show>
    </div>
  );
}
