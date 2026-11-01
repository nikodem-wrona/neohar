// The state of the view that belongs to this window only and that no other client needs: whether
// the left column and the drawer are open, which tab of the drawer shows, the widths of the side
// columns, the name that the person edits, whether the next key passes through to the pane, and
// whether the Git view covers the panes. None of it is business state. Each part is a value
// (`lib/value.ts`): a part that only an action of the view changes is a `Source`.

import type { Sticky } from '../input/soft';
import { createValue, type Source } from '../lib/value';
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
  const leftOpen = createValue(true);
  const drawerOpen = createValue(true);
  const columns = createValue<Columns>(storedColumns());
  const drawerTab = createValue<DrawerTab>('inbox');
  const passNext = createValue(false);
  const paletteOpen = createValue(false);
  const sheet = createValue<PhoneSheet | null>(null);
  const typing = createValue<TypingField | null>(null);
  const pressMenu = createValue<PressMenu | null>(null);
  const barCtrl = createValue<Sticky>('off');
  const barAlt = createValue<Sticky>('off');
  const focusedCard = createValue<string | null>(null);
  const closing = createValue<Closing | null>(null);
  const renaming = createValue<RenameTarget | null>(null);
  const gitOpen = createValue(false);
  const gitTarget = createValue<GitTarget>({ folder: null, seq: 0 });
  const workflowsOpen = createValue(false);
  const workflowsTarget = createValue<GitTarget>({ folder: null, seq: 0 });
  const hermesOpen = createValue(false);
  const layers = createValue(0);
  /**
   * The side columns before the Git view, the Workflows view or the Hermes view folded them, to
   * open them again when the view closes. Only one of the views shows at a time.
   */
  let unfolded: { left: boolean; drawer: boolean } | null = null;
  const covered = (): boolean => gitOpen.get() || workflowsOpen.get() || hermesOpen.get();
  const fold = (): void => {
    if (covered()) return;
    unfolded = { left: leftOpen.get(), drawer: drawerOpen.get() };
    leftOpen.set(false);
    drawerOpen.set(false);
  };
  const unfold = (): void => {
    if (covered() || !unfolded) return;
    leftOpen.set(unfolded.left);
    drawerOpen.set(unfolded.drawer);
    unfolded = null;
  };
  const openGit = (folder: string | null = null): void => {
    fold();
    workflowsOpen.set(false);
    hermesOpen.set(false);
    gitOpen.set(true);
    gitTarget.set({ folder, seq: gitTarget.get().seq + 1 });
  };
  const closeGit = (): void => {
    if (!gitOpen.get()) return;
    gitOpen.set(false);
    unfold();
  };
  const openWorkflows = (folder: string | null = null): void => {
    fold();
    gitOpen.set(false);
    hermesOpen.set(false);
    workflowsOpen.set(true);
    workflowsTarget.set({ folder, seq: workflowsTarget.get().seq + 1 });
  };
  const closeWorkflows = (): void => {
    if (!workflowsOpen.get()) return;
    workflowsOpen.set(false);
    unfold();
  };
  const openHermes = (): void => {
    fold();
    gitOpen.set(false);
    workflowsOpen.set(false);
    hermesOpen.set(true);
  };
  const closeHermes = (): void => {
    if (!hermesOpen.get()) return;
    hermesOpen.set(false);
    unfold();
  };
  return {
    /** The widths of the left column and of the drawer. A drag of their rule changes them. */
    columns: columns as Source<Columns>,
    setColumn(column: keyof Columns, width: number): void {
      const next = { ...columns.get(), [column]: clampColumn(column, width) };
      columns.set(next);
      try {
        const stored: Stored = { monitor: next.left, drawer: next.drawer };
        globalThis.localStorage?.setItem(COLUMNS_KEY, JSON.stringify(stored));
      } catch {
        // The width stays for this page only.
      }
    },
    /** A tab or a workspace whose close waits for the person. */
    closing,
    /** The workspace or the tab whose name the person edits, and where. */
    renaming,
    /** The interaction whose card should have the focus, until the card takes it. */
    focusedCard: focusedCard as Source<string | null>,
    focusCard(interaction: string | null): void {
      focusedCard.set(interaction);
    },
    paletteOpen,
    /**
     * How many layers of shadcn/ui (a popover, a menu, a dialog) are open. While one is open, the
     * pane does not take the focus back: Radix closes a layer when the focus leaves it.
     */
    layers: layers as Source<number>,
    /** A layer opens. Returns the function that says that it closed. */
    openLayer(): () => void {
      layers.set(layers.get() + 1);
      let open = true;
      return () => {
        if (!open) return;
        open = false;
        layers.set(layers.get() - 1);
      };
    },
    /**
     * Does what a command of the page asks for. The core names it in `palette.execute`. A rename
     * edits the tab or the workspace that the window shows, and the field of a workspace opens a
     * folded left column.
     */
    pageAction(id: string, shown?: Shown): void {
      if (id === 'tab.rename' && shown?.tab) {
        renaming.set({ kind: 'tab', id: shown.tab, place: 'tabbar' });
      } else if (id === 'workspace.rename' && shown?.workspace) {
        renaming.set({ kind: 'workspace', id: shown.workspace, place: 'left' });
        leftOpen.set(true);
      } else if (id === 'inbox.open') {
        drawerTab.set('inbox');
        drawerOpen.set(true);
      } else if (id === 'git.open') {
        if (gitOpen.get()) closeGit();
        else openGit();
      } else if (id === 'workflows.open') {
        if (workflowsOpen.get()) closeWorkflows();
        else openWorkflows();
      } else if (id === 'hermes.open') {
        if (hermesOpen.get()) closeHermes();
        else openHermes();
      } else if (id === 'settings.open') {
        // A phone has no settings window: the core names this command of the page.
        sheet.set({ kind: 'settings', section: null });
      }
    },
    /** The sheet of a phone that shows, or none. */
    sheet,
    /** The menu of a long press or of the button ⋯ of a pane, or none. */
    pressMenu,
    /** The field of a pane that has the keyboard of a phone, or none. */
    typing,
    /** Ctrl and Alt of the key bar of a phone: off, for the next key, or locked. */
    barCtrl,
    barAlt,
    /** A key went out: a modifier of the bar that was for one key is off again. */
    usedBarMods(): void {
      if (barCtrl.get() === 'once') barCtrl.set('off');
      if (barAlt.get() === 'once') barAlt.set('off');
    },
    /** The left column is open, or folded to a rail. */
    leftOpen,
    drawerOpen,
    drawerTab,
    passNext,
    /** Shows a tab of the drawer, and opens the drawer if it is closed. */
    showDrawer(tab: DrawerTab): void {
      drawerTab.set(tab);
      drawerOpen.set(true);
    },
    /**
     * The Git view covers the panes in the middle of the window. While it shows, the left column
     * and the drawer are folded to their rails, and they open again when it closes.
     */
    gitOpen: gitOpen as Source<boolean>,
    /** The last request to show the Git view. */
    gitTarget: gitTarget as Source<GitTarget>,
    /** Shows the Git view on the worktree of `folder`, or of the folder of the window. */
    openGit,
    closeGit,
    toggleGit(): void {
      if (gitOpen.get()) closeGit();
      else openGit();
    },
    /**
     * The Workflows view covers the panes as the Git view does: a canvas of the workflows of the
     * person, and their executions in the folder of the window.
     */
    workflowsOpen: workflowsOpen as Source<boolean>,
    /** The last request to show the Workflows view. */
    workflowsTarget: workflowsTarget as Source<GitTarget>,
    /** Shows the Workflows view on `folder`, or on the folder of the window. */
    openWorkflows,
    closeWorkflows,
    toggleWorkflows(folder: string | null = null): void {
      if (workflowsOpen.get()) closeWorkflows();
      else openWorkflows(folder);
    },
    /** The Hermes view covers the panes as the Workflows view does: the chats with Hermes Agent. */
    hermesOpen: hermesOpen as Source<boolean>,
    openHermes,
    closeHermes,
    toggleHermes(): void {
      if (hermesOpen.get()) closeHermes();
      else openHermes();
    },
  };
}

export type ViewState = ReturnType<typeof createViewState>;
