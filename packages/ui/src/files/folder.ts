// The folder of a window: the folder where the pane with the focus is
// now, else the folder of the tab. The Files tab shows its tree, and the Changes tab shows the
// changes of the git worktree that holds it.

import type { Stores } from '../stores';
import { rootOf } from './model';

export function windowFolder(stores: Stores, windowId: string): string | undefined {
  const win = stores.windows.state.byKey[windowId];
  const workspace = win?.workspace ? stores.tree.state.byKey[win.workspace] : undefined;
  // A window keeps the focus of a pane of the tab that it showed before, so only a pane that the
  // window draws counts.
  const focused = win?.focused_pane;
  const pane =
    focused && win.visible_panes.includes(focused) ? stores.panes.state.byKey[focused] : undefined;
  return rootOf(pane, workspace);
}
