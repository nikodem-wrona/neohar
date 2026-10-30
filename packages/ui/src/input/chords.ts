// The reserved chords as the `keys` section of the settings gives them. A chord is text:
// modifiers and a key joined with "+", for example `mod+shift+]`. `mod` is Command on a Mac and
// Control elsewhere. The page parses the text into the physical key (`code`) and the modifiers,
// so that a chord stays in place on every layout.

import type { Platform } from './clipboard';

/** A chord as the fields of a key event that decide it. */
export interface ChordSpec {
  code: string;
  meta: boolean;
  ctrl: boolean;
  shift: boolean;
  alt: boolean;
}

/** The fields of a `KeyboardEvent` that a chord reads. A real event fits. */
export interface ChordEventLike {
  readonly code: string;
  readonly metaKey: boolean;
  readonly shiftKey: boolean;
  readonly altKey: boolean;
  readonly ctrlKey: boolean;
}

/** The keys that are not a letter or a digit, by the name in the text. */
const NAMED_KEYS: Readonly<Record<string, string>> = {
  '[': 'BracketLeft',
  ']': 'BracketRight',
  ',': 'Comma',
  '.': 'Period',
  '/': 'Slash',
  ';': 'Semicolon',
  "'": 'Quote',
  '-': 'Minus',
  '=': 'Equal',
  '`': 'Backquote',
  '\\': 'Backslash',
  left: 'ArrowLeft',
  right: 'ArrowRight',
  up: 'ArrowUp',
  down: 'ArrowDown',
  enter: 'Enter',
  escape: 'Escape',
  space: 'Space',
  tab: 'Tab',
  backspace: 'Backspace',
};

/** The `code` of the key in a chord, or `null` when the text names no key that the page knows. */
function keyCode(key: string): string | null {
  const letter = /^[a-z]$/.exec(key);
  if (letter) return `Key${key.toUpperCase()}`;
  if (/^[0-9]$/.test(key)) return `Digit${key}`;
  if (/^f([1-9]|1[0-2])$/.test(key)) return key.toUpperCase();
  return NAMED_KEYS[key] ?? null;
}

/**
 * Parses a chord. It gives `null` for text that is not a chord: an unknown key or modifier, no
 * key, or no modifier. A chord without a modifier would take a key from the terminal.
 */
export function parseChord(text: string, platform: Platform): ChordSpec | null {
  const parts = text
    .toLowerCase()
    .split('+')
    .map((part) => part.trim());
  const key = parts.pop();
  if (key === undefined || key === '') return null;
  const code = keyCode(key);
  if (code === null || parts.length === 0) return null;
  const spec: ChordSpec = { code, meta: false, ctrl: false, shift: false, alt: false };
  for (const modifier of parts) {
    switch (modifier) {
      case 'mod':
        if (platform === 'macos') spec.meta = true;
        else spec.ctrl = true;
        break;
      case 'cmd':
      case 'meta':
        spec.meta = true;
        break;
      case 'ctrl':
        spec.ctrl = true;
        break;
      case 'shift':
        spec.shift = true;
        break;
      case 'alt':
      case 'option':
        spec.alt = true;
        break;
      default:
        return null;
    }
  }
  return spec;
}

/** Whether a key event is exactly this chord: the key and every modifier. */
export function matchesChord(spec: ChordSpec, e: ChordEventLike): boolean {
  return (
    e.code === spec.code &&
    e.metaKey === spec.meta &&
    e.ctrlKey === spec.ctrl &&
    e.shiftKey === spec.shift &&
    e.altKey === spec.alt
  );
}

/** The keys that a label shows with a sign, by `code`. */
const KEY_SIGNS: Readonly<Record<string, string>> = {
  ArrowLeft: '←',
  ArrowRight: '→',
  ArrowUp: '↑',
  ArrowDown: '↓',
  Enter: '↩',
  Escape: 'Esc',
  Space: 'Space',
  Tab: 'Tab',
  Backspace: '⌫',
};

/** The name of a key on its cap: `KeyA` is `A`, `Digit4` is `4`, `BracketRight` is `]`. */
function keyCap(code: string): string {
  if (code.startsWith('Key')) return code.slice(3);
  if (code.startsWith('Digit')) return code.slice(5);
  const named = Object.entries(NAMED_KEYS).find(([, value]) => value === code)?.[0];
  if (named !== undefined && named.length === 1) return named;
  return KEY_SIGNS[code] ?? code;
}

/** A chord as a person reads it: `⌘⇧A` on a Mac, `Ctrl+Shift+A` elsewhere. */
export function chordLabel(spec: ChordSpec, platform: Platform): string {
  const key = keyCap(spec.code);
  if (platform === 'macos') {
    return `${spec.ctrl ? '⌃' : ''}${spec.alt ? '⌥' : ''}${spec.shift ? '⇧' : ''}${spec.meta ? '⌘' : ''}${key}`;
  }
  const parts: string[] = [];
  if (spec.ctrl) parts.push('Ctrl');
  if (spec.alt) parts.push('Alt');
  if (spec.shift) parts.push('Shift');
  if (spec.meta) parts.push('Meta');
  parts.push(key);
  return parts.join('+');
}

/** The command that a name of the `keys` section stands for: the first `_` is a `.`. */
export function commandOfKey(name: string): string {
  const at = name.indexOf('_');
  return at < 0 ? name : `${name.slice(0, at)}.${name.slice(at + 1)}`;
}

/** The chords of a window. */
export interface Keymap {
  /** A chord and the command that it runs, in the order of the section. */
  commands: readonly { chord: ChordSpec; command: string }[];
  /** The chord that lets the next key reach the pane. */
  passThrough: ChordSpec | null;
}

/** The chords that the page uses until the settings arrive. They are the defaults of the core. */
export const DEFAULT_KEYS: Readonly<Record<string, string>> = {
  palette_open: 'mod+k',
  tab_next: 'mod+shift+]',
  tab_previous: 'mod+shift+[',
  pane_focus_left: 'mod+alt+left',
  pane_focus_right: 'mod+alt+right',
  pane_focus_up: 'mod+alt+up',
  pane_focus_down: 'mod+alt+down',
  pane_split_right: 'mod+d',
  pane_split_down: 'mod+shift+d',
  inbox_open: 'mod+shift+i',
  interaction_answer_next: 'mod+shift+a',
  peek_sibling: 'mod+shift+p',
  git_open: 'mod+shift+g',
  workflows_open: 'mod+shift+e',
  window_new: 'mod+shift+n',
  pass_through: 'mod+alt+v',
};

/**
 * Builds the keymap from the entries of the `keys` section. A chord that does not parse is left
 * out, so a mistake in the file takes away that one chord and no other.
 */
export function compileKeymap(keys: Readonly<Record<string, string>>, platform: Platform): Keymap {
  const commands: { chord: ChordSpec; command: string }[] = [];
  let passThrough: ChordSpec | null = null;
  for (const [name, text] of Object.entries(keys)) {
    const chord = parseChord(text, platform);
    if (chord === null) continue;
    if (name === 'pass_through') passThrough = chord;
    else commands.push({ chord, command: commandOfKey(name) });
  }
  return { commands, passThrough };
}
