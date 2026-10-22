// The soft keyboard of a phone. The keys of the soft keyboard of iOS give no
// reliable `keydown` for predictions, corrections and dictation, so the text comes from the field
// of the pane:
//
// - The field keeps the text that went to the pane since the last space or Enter. iOS needs it
//   for its predictions and corrections.
// - After each `input`, the page compares the new value with that text. The characters after the
//   common start go away with Backspaces, and the rest goes as text. A correction, a word of the
//   prediction bar and dictation all take this path. The count is in code points, not in bytes or
//   UTF-16 units, because a Backspace of the terminal deletes one character.
// - Enter (`insertLineBreak`, `insertParagraph`) and a Backspace in an empty field are keys.
//
// The key bar over the keyboard gives the keys that the soft keyboard lacks: Esc, Tab, ⇧Tab, the
// arrows, and Ctrl and Alt for the next key. A second tap on Ctrl or Alt locks it. The bar sends the
// logical keys of a hardware keyboard.

import type { LogicalInput, ModName } from '@neohar/transport';
import { type KeyInput, usCharacter } from './keyboard';

/** A key with no modifier, as a hardware keyboard sends it. */
export function key(code: string, keyName: string = code, mods: ModName[] = []): KeyInput {
  return { type: 'key', code, key: keyName, mods, kind: 'press', location: 0 };
}

export const BACKSPACE = key('Backspace');
export const ENTER = key('Enter');

/** The text of the field of a pane and what went to the pane from it. */
export class FieldText {
  private sent: string[] = [];

  /** What went to the pane since the last clear. */
  get text(): string {
    return this.sent.join('');
  }

  /**
   * The events that turn what went to the pane into `value`: a Backspace for each character after
   * the common start, then the rest as text.
   */
  change(value: string): LogicalInput[] {
    const next = Array.from(value);
    let same = 0;
    while (same < this.sent.length && same < next.length && this.sent[same] === next[same]) same++;
    const events: LogicalInput[] = [];
    for (let at = same; at < this.sent.length; at++) events.push(BACKSPACE);
    const added = next.slice(same).join('');
    if (added !== '') events.push({ type: 'text', text: added });
    this.sent = next;
    return events;
  }

  /** The field starts again: after a space, an Enter, a key of the bar or a paste. */
  clear(): void {
    this.sent = [];
  }
}

/** Whether the field starts again after this value: it ends with a space or holds a line break. */
export function endsWord(value: string): boolean {
  return /\s$/.test(value);
}

/** The physical key of a character on the US layout, for a chord with Ctrl or Alt. */
export function codeOf(character: string): string | null {
  const upper = character.toUpperCase();
  if (/^[A-Z]$/.test(upper)) return `Key${upper}`;
  if (/^[0-9]$/.test(character)) return `Digit${character}`;
  for (const code of [
    'Backquote',
    'Minus',
    'Equal',
    'BracketLeft',
    'BracketRight',
    'Backslash',
    'Semicolon',
    'Quote',
    'Comma',
    'Period',
    'Slash',
    'Space',
  ]) {
    if (usCharacter(code, false) === character || usCharacter(code, true) === character) {
      return code;
    }
  }
  return null;
}

/** The state of a modifier of the key bar. */
export type Sticky = 'off' | 'once' | 'locked';

/** A tap on Ctrl or Alt: off, then for the next key, then locked, then off. */
export function nextSticky(state: Sticky): Sticky {
  return state === 'off' ? 'once' : state === 'once' ? 'locked' : 'off';
}

/** The modifiers of the bar that are on, in the order of `ModName`. */
export function stickyMods(ctrl: Sticky, alt: Sticky): ModName[] {
  const mods: ModName[] = [];
  if (ctrl !== 'off') mods.push('ctrl');
  if (alt !== 'off') mods.push('alt');
  return mods;
}

/**
 * A character typed while Ctrl or Alt of the bar is on: the key of the US layout with the
 * modifiers, as a hardware keyboard sends it. A character with no key goes as text.
 */
export function chordOf(character: string, mods: ModName[]): LogicalInput {
  const code = codeOf(character);
  if (!code) return { type: 'text', text: character };
  const shifted = usCharacter(code, true) === character && usCharacter(code, false) !== character;
  const event = withMods(key(code, character), shifted ? ['shift', ...mods] : mods);
  // With Ctrl the key makes no text. With Alt alone the text goes with the key.
  if (!mods.includes('ctrl')) event.text = character;
  return event;
}

/** A key of the bar: its label and the key that it sends. */
export interface BarKey {
  label: string;
  /** The name for a screen reader. */
  name: string;
  event: KeyInput;
}

/** The fixed keys of the bar, before Ctrl and Alt and the keys of `phone.keys`. */
export const BAR_KEYS: readonly BarKey[] = [
  { label: 'Esc', name: 'Escape', event: key('Escape') },
  { label: 'Tab', name: 'Tab', event: key('Tab') },
  { label: '⇧Tab', name: 'Shift Tab', event: key('Tab', 'Tab', ['shift']) },
  { label: '←', name: 'Left', event: key('ArrowLeft') },
  { label: '↑', name: 'Up', event: key('ArrowUp') },
  { label: '↓', name: 'Down', event: key('ArrowDown') },
  { label: '→', name: 'Right', event: key('ArrowRight') },
];

/** A key of the bar with the modifiers of the bar that are on. */
export function withMods(event: KeyInput, mods: ModName[]): KeyInput {
  if (mods.length === 0) return event;
  const all = [...(event.mods ?? [])];
  for (const mod of mods) if (!all.includes(mod)) all.push(mod);
  const order: ModName[] = ['shift', 'ctrl', 'alt', 'meta', 'caps_lock', 'num_lock'];
  return { ...event, mods: order.filter((mod) => all.includes(mod)) };
}
