// The chords that the app keeps for itself while a terminal surface has focus.
//
// A surface in focus takes every key, except these. The command chords come from the `keys` section
// of the settings (`chords.ts`), and the defaults serve until the settings arrive. The menu
// accelerators are the ones of Tauri, so the page only knows them to leave them alone. The chords
// are matched by the physical key (`code`), so they stay in place on every layout.

import { type ChordEventLike, type Keymap, matchesChord } from './chords';

export type { ChordEventLike } from './chords';

/** What a reserved chord does. A command goes to the core. A menu chord belongs to the native menu. */
export type Reserved = { kind: 'command'; command: string } | { kind: 'menu'; accelerator: string };

/** The menu accelerators. Tauri handles them, so a focused terminal cannot swallow them. */
const MENU: readonly { code: string; accelerator: string }[] = [
  { code: 'KeyQ', accelerator: '⌘Q' },
  { code: 'KeyW', accelerator: '⌘W' },
  { code: 'KeyN', accelerator: '⌘N' },
  { code: 'Comma', accelerator: '⌘,' },
];

/**
 * The reserved chord of a key event, or null when the key belongs to the terminal. For a command
 * the surface calls `preventDefault` and sends the command. For a menu chord it sends nothing to the
 * pane and leaves the event to the native menu.
 */
export function resolveChord(keymap: Keymap, e: ChordEventLike): Reserved | null {
  const command = keymap.commands.find((entry) => matchesChord(entry.chord, e));
  if (command) return { kind: 'command', command: command.command };
  const menu = MENU.find(
    (entry) => e.code === entry.code && e.metaKey && !e.ctrlKey && !e.shiftKey && !e.altKey,
  );
  return menu ? { kind: 'menu', accelerator: menu.accelerator } : null;
}

/** Whether a key event is the chord that sends the next key to the pane. */
export function isPassThrough(keymap: Keymap, e: ChordEventLike): boolean {
  return keymap.passThrough !== null && matchesChord(keymap.passThrough, e);
}
