// The state of the view that belongs to this window only and that no other client needs: whether
// the left column and the drawer are open, which tab of the drawer shows, the widths of the side
// columns, the name that the person edits, whether the next key passes through to the pane,
// whether the Git view covers the panes, and which panes show the chat of their agent in place of
// the terminal, with the draft of each. None of it is business state.

import { createSignal } from 'solid-js';
import type { Sticky } from '../input/soft';
import type { Closing } from '../shell/close';
import type { RenameTarget } from '../shell/rename';

export type DrawerTab = 'inbox' | 'timeline' | 'files' | 'changes';

/**
 * A sheet of the desk on a phone that takes the screen under the command line: More,
 * with the parts of the status bar, and the settings at a section.
 */
export type PhoneSheet = { kind: 'more' } | { kind: 'settings'; section: string | null };

/**
 * The field of a pane that has the keyboard of a phone: the key bar sends to its
 * pane, and it can start the text of the field again or close the keyboard.
 */
export interface TypingField {
  pane: string;
  /** The keys of the bar changed what the pane has: the field starts again. */
  restart(): void;
  /** Closes the keyboard. */
  hide(): void;
  /** Gives the field the focus again, after the field of Compose. */
  focus(): void;
}

/** An item of the menu of a long press or of the button ⋯ of a pane. */
export interface MenuAction {
  label: string;
  run: () => void;
  danger?: boolean;
}

/** The menu that a long press opened, at the point of the press. */
export interface PressMenu {
  x: number;
  y: number;
  /** What the menu acts on, for a screen reader. */
  title: string;
  items: MenuAction[];
}

/** What the window shows: the page names it to a command of the page. */
export interface Shown {
  tab?: string | null;
  workspace?: string | null;
  focused_pane?: string | null;
}

/**
 * A request to show the Git view. `folder` is the folder whose worktree it shows; without one,
 * the view shows the folder of the window. `seq` grows with each request, so that the same folder
 * can be asked for twice.
 */
export interface GitTarget {
  folder: string | null;
  seq: number;
}

/** The widths of the side columns, in characters of the chrome font. */
export interface Columns {
  left: number;
  drawer: number;
}

export const COLUMN_LIMITS: Readonly<Record<keyof Columns, { min: number; max: number }>> = {
  left: { min: 20, max: 80 },
  drawer: { min: 32, max: 96 },
};

/** The left column has the workspaces only, so it is narrower than the monitor was. */
const DEFAULT_COLUMNS: Columns = { left: 28, drawer: 44 };
/**
 * The key and the shape of the stored widths do not change: the width of the left column is in
 * the field `monitor`, so a width that the person set for the monitor stays.
 */
const COLUMNS_KEY = 'neohar.columns';
interface Stored {
  monitor?: unknown;
  drawer?: unknown;
}

/** A width in its limits, in whole characters. */
export function clampColumn(column: keyof Columns, width: number): number {
  const { min, max } = COLUMN_LIMITS[column];
  return Math.min(max, Math.max(min, Math.round(width)));
}

/** The widths that this browser kept. Storage can be missing or refuse, and then the defaults. */
function storedColumns(): Columns {
  try {
    const kept = JSON.parse(globalThis.localStorage?.getItem(COLUMNS_KEY) ?? 'null') as unknown;
    if (typeof kept !== 'object' || kept === null) return DEFAULT_COLUMNS;
    const { monitor, drawer } = kept as Stored;
    return {
      left: typeof monitor === 'number' ? clampColumn('left', monitor) : DEFAULT_COLUMNS.left,
      drawer: typeof drawer === 'number' ? clampColumn('drawer', drawer) : DEFAULT_COLUMNS.drawer,
    };
  } catch {
    return DEFAULT_COLUMNS;
  }
}

export function createViewState() {
  const [leftOpen, setLeftOpen] = createSignal(true);
  const [drawerOpen, setDrawerOpen] = createSignal(true);
  const [columns, setColumnsSignal] = createSignal<Columns>(storedColumns());
  const [drawerTab, setDrawerTab] = createSignal<DrawerTab>('inbox');
  const [passNext, setPassNext] = createSignal(false);
  const [paletteOpen, setPaletteOpen] = createSignal(false);
  const [sheet, setSheet] = createSignal<PhoneSheet | null>(null);
  const [typing, setTyping] = createSignal<TypingField | null>(null);
  const [pressMenu, setPressMenu] = createSignal<PressMenu | null>(null);
  const [barCtrl, setBarCtrl] = createSignal<Sticky>('off');
  const [barAlt, setBarAlt] = createSignal<Sticky>('off');
  const [searchOpen, setSearchOpen] = createSignal(false);
  const [focusedCard, setFocusedCard] = createSignal<string | null>(null);
  const [closing, setClosing] = createSignal<Closing | null>(null);
  const [renaming, setRenaming] = createSignal<RenameTarget | null>(null);
  const [gitOpen, setGitOpen] = createSignal(false);
  const [gitTarget, setGitTarget] = createSignal<GitTarget>({ folder: null, seq: 0 });
  const [chatPanes, setChatPanes] = createSignal<ReadonlySet<string>>(new Set());
  /** The text of the composer of each pane. It stays while the pane shows the terminal. */
  const [drafts, setDrafts] = createSignal<ReadonlyMap<string, string>>(new Map());
  const toggleChat = (pane: string): void => {
    setChatPanes((before) => {
      const next = new Set(before);
      if (!next.delete(pane)) next.add(pane);
      return next;
    });
  };
  /** The side columns before the Git view folded them, to open them again when it closes. */
  let unfolded: { left: boolean; drawer: boolean } | null = null;
  const openGit = (folder: string | null = null): void => {
    if (!gitOpen()) {
      unfolded = { left: leftOpen(), drawer: drawerOpen() };
      setLeftOpen(false);
      setDrawerOpen(false);
      setGitOpen(true);
    }
    setGitTarget((before) => ({ folder, seq: before.seq + 1 }));
  };
  const closeGit = (): void => {
    if (!gitOpen()) return;
    setGitOpen(false);
    if (unfolded) {
      setLeftOpen(unfolded.left);
      setDrawerOpen(unfolded.drawer);
      unfolded = null;
    }
  };
  return {
    /** The widths of the left column and of the drawer. A drag of their rule changes them. */
    columns,
    setColumn(column: keyof Columns, width: number): void {
      const next = { ...columns(), [column]: clampColumn(column, width) };
      setColumnsSignal(next);
      try {
        const stored: Stored = { monitor: next.left, drawer: next.drawer };
        globalThis.localStorage?.setItem(COLUMNS_KEY, JSON.stringify(stored));
      } catch {
        // The width stays for this page only.
      }
    },
    /** A tab or a workspace whose close waits for the person. */
    closing,
    setClosing,
    /** The workspace or the tab whose name the person edits, and where. */
    renaming,
    setRenaming,
    /** The interaction whose card should have the focus, until the card takes it. */
    focusedCard,
    focusCard(interaction: string | null): void {
      setFocusedCard(interaction);
    },
    paletteOpen,
    setPaletteOpen,
    /** The search of the scrollback is open. */
    searchOpen,
    setSearchOpen,
    /**
     * Does what a command of the page asks for. The core names it in `palette.execute`. A rename
     * edits the tab or the workspace that the window shows, and the field of a workspace opens a
     * folded left column.
     */
    pageAction(id: string, shown?: Shown): void {
      if (id === 'tab.rename' && shown?.tab) {
        setRenaming({ kind: 'tab', id: shown.tab, place: 'tabbar' });
      } else if (id === 'workspace.rename' && shown?.workspace) {
        setRenaming({ kind: 'workspace', id: shown.workspace, place: 'left' });
        setLeftOpen(true);
      } else if (id === 'inbox.open') {
        setDrawerTab('inbox');
        setDrawerOpen(true);
      } else if (id === 'search.open') {
        setSearchOpen(true);
      } else if (id === 'git.open') {
        if (gitOpen()) closeGit();
        else openGit();
      } else if (id === 'pane.view_toggle' && shown?.focused_pane) {
        toggleChat(shown.focused_pane);
      } else if (id === 'settings.open') {
        // A phone has no settings window: the core names this command of the page.
        setSheet({ kind: 'settings', section: null });
      }
    },
    /** The sheet of a phone that shows, or none. */
    sheet,
    setSheet,
    /** The menu of a long press or of the button ⋯ of a pane, or none. */
    pressMenu,
    setPressMenu,
    /** The field of a pane that has the keyboard of a phone, or none. */
    typing,
    setTyping,
    /** Ctrl and Alt of the key bar of a phone: off, for the next key, or locked. */
    barCtrl,
    setBarCtrl,
    barAlt,
    setBarAlt,
    /** A key went out: a modifier of the bar that was for one key is off again. */
    usedBarMods(): void {
      if (barCtrl() === 'once') setBarCtrl('off');
      if (barAlt() === 'once') setBarAlt('off');
    },
    /** The left column is open, or folded to a rail. */
    leftOpen,
    setLeftOpen,
    drawerOpen,
    setDrawerOpen,
    drawerTab,
    setDrawerTab,
    passNext,
    setPassNext,
    /** Shows a tab of the drawer, and opens the drawer if it is closed. */
    showDrawer(tab: DrawerTab): void {
      setDrawerTab(tab);
      setDrawerOpen(true);
    },
    /**
     * The Git view covers the panes in the middle of the window. While it shows, the left column
     * and the drawer are folded to their rails, and they open again when it closes.
     */
    gitOpen,
    /** The last request to show the Git view. */
    gitTarget,
    /** Shows the Git view on the worktree of `folder`, or of the folder of the window. */
    openGit,
    closeGit,
    toggleGit(): void {
      if (gitOpen()) closeGit();
      else openGit();
    },
    /** The pane shows the chat of its agent in place of the terminal. */
    chatShown(pane: string): boolean {
      return chatPanes().has(pane);
    },
    /** Shows the chat in the pane, or the terminal when the chat shows. */
    toggleChat,
    draft(pane: string): string {
      return drafts().get(pane) ?? '';
    },
    setDraft(pane: string, text: string): void {
      setDrafts((before) => {
        const next = new Map(before);
        if (text === '') next.delete(pane);
        else next.set(pane, text);
        return next;
      });
    },
  };
}

export type ViewState = ReturnType<typeof createViewState>;
