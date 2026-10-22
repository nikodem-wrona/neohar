// Showing a pane in a window: the window shows the tab of the pane, and with it its workspace, and
// the pane gets the focus. The palette command `go.pane:<id>` does both in one command of the core.
// `pane.focus` takes only a pane that the window draws, so a click on a pane that
// can be in another tab, such as a strip of the board or Open on a card, uses this.

import type { Transport } from '@neohar/transport';

/** The command of the palette that shows a pane in the window. */
export function goToPane(pane: string): string {
  return `go.pane:${pane}`;
}

/** Shows a pane in a window, from any tab and any workspace. Rejects with an `RpcError`. */
export function showPane(
  transport: Pick<Transport, 'call'>,
  window: string,
  pane: string,
): Promise<unknown> {
  return transport.call('palette.execute', { command: goToPane(pane), window });
}
