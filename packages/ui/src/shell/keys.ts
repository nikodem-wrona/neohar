// The function keys of the window: the reserved chords that the chrome shows as key caps, so that a
// person can learn them and also click them. The keymap of the settings gives the chords; a command
// whose chord was removed has no cap.

import type { Platform } from '../input/clipboard';
import { chordLabel, type Keymap } from '../input/chords';

export interface FunctionKey {
  command: string;
  label: string;
  /** The cap is amber while an agent waits for a person. Every other cap is gray. */
  waits?: boolean;
  /** A key that the strip leaves out first when the window is narrow. */
  optional?: boolean;
}

/** The caps of the status bar, in their order. */
export const FUNCTION_KEYS: readonly FunctionKey[] = [
  { command: 'palette.open', label: 'Command' },
  { command: 'interaction.answer_next', label: 'Answer', waits: true },
  { command: 'inbox.open', label: 'Inbox' },
  { command: 'search.open', label: 'Search' },
  { command: 'pane.split_right', label: 'Split', optional: true },
  { command: 'tab.next', label: 'Next tab', optional: true },
];

/** The chord of a command as a person reads it, or `null` when no chord runs it. */
export function chordText(keymap: Keymap, command: string, platform: Platform): string | null {
  const found = keymap.commands.find((entry) => entry.command === command);
  return found ? chordLabel(found.chord, platform) : null;
}
