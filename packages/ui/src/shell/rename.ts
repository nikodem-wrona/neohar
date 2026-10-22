// Renaming a workspace or a tab from the chrome. A double click on the name, F2 on a tab or
// the palette command opens a field in place of the name. Enter or a click outside the field sends
// the name with `workspace.rename` or `tab.rename`, and Escape keeps the old name. The core trims
// the name and checks it; the window only skips a call that cannot change the name.

import { RpcError, type Transport } from '@neohar/transport';

/** The most characters that the core keeps in a name (`NAME_MAX` of the layout). */
export const NAME_MAX = 100;

/** What gets a new name, and where its field shows: in the left column or in the tab bar. */
export interface RenameTarget {
  kind: 'workspace' | 'tab';
  id: string;
  place: 'left' | 'tabbar';
}

/** Whether two targets are the same field. */
export function sameTarget(a: RenameTarget | null, b: RenameTarget | null): boolean {
  return a !== null && b !== null && a.kind === b.kind && a.id === b.id && a.place === b.place;
}

/**
 * Asks the core to give the target the typed name. Gives `null` when the field can close: the core
 * took the name, or the text is empty or the old name, and then nothing is sent. Else it gives the
 * message of the failure.
 */
export async function requestRename(
  transport: Pick<Transport, 'call'>,
  target: RenameTarget,
  typed: string,
  old: string,
): Promise<string | null> {
  const name = typed.trim();
  if (name === '' || name === old) return null;
  try {
    if (target.kind === 'tab') await transport.call('tab.rename', { tab: target.id, name });
    else await transport.call('workspace.rename', { workspace: target.id, name });
    return null;
  } catch (failure: unknown) {
    return failure instanceof RpcError ? failure.message : `The ${target.kind} kept its name.`;
  }
}
