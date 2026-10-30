// The folder of a window: the folder where the pane with the focus is
// now, else the folder of the tab. The Files tab shows its tree, and the Changes tab shows the
// changes of the git worktree that holds it.

import { useSelect } from '../lib/hooks';
import type { Stores } from '../stores';
import { rootOf } from './model';

/**
 * The pane that counts for the folder. A window keeps the focus of a pane of the tab that it
 * showed before, so only a pane that the window draws counts.
 */
function countedPane(win: { focused_pane?: string | null; visible_panes: string[] } | undefined) {
  const focused = win?.focused_pane;
  return focused && win.visible_panes.includes(focused) ? focused : undefined;
}

/** The folder of the window now, for a command. */
export function windowFolder(stores: Stores, windowId: string): string | undefined {
  const win = stores.windows.get().byKey[windowId];
  const workspace = win?.workspace ? stores.tree.get().byKey[win.workspace] : undefined;
  const focused = countedPane(win);
  const pane = focused ? stores.panes.get().byKey[focused] : undefined;
  return rootOf(pane, workspace);
}

/** The folder of the window. The component renders again when it changes. */
export function useWindowFolder(stores: Stores, windowId: string): string | undefined {
  const win = useSelect(stores.windows, (state) => state.byKey[windowId]);
  const workspace = useSelect(stores.tree, (state) =>
    win?.workspace ? state.byKey[win.workspace] : undefined,
  );
  const focused = countedPane(win);
  const pane = useSelect(stores.panes, (state) => (focused ? state.byKey[focused] : undefined));
  return rootOf(pane, workspace);
}
