// The keyboard layer: browser `keydown` and `keyup` events become logical key events. The core
// encodes them. This layer only sorts out what the browser reports:
//
// - An input method owns the key while it composes (`isComposing`, or `keyCode` 229). The result
//   comes with `compositionend` as text.
// - A dead key (`key === "Dead"`) sends nothing. The browser delivers the result as text from the
//   composition, or as the `key` of the next event.
// - AltGr makes a character. Windows reports it as Ctrl and Alt together, Linux as the AltGraph
//   state. The character is text, so Ctrl and Alt are cleared.
// - On macOS the Option key makes a character as well. The setting `optionAsAlt` says per side
//   whether Option is Alt instead. Then the base character comes from `code` (US layout only; a
//   key that is not on the table keeps its `key`). Only this layer knows the platform and which
//   Option key is down, so the core does not decide.
//
// Nothing here keeps state about the terminal. It keeps the state of the two Alt keys.

import type { KeyKind, LogicalInput, ModName } from '@neohar/transport';

/** A key event of `pane.input`, with its tag. */
export type KeyInput = Extract<LogicalInput, { type: 'key' }>;

/** A text event of `pane.input`, with its tag. */
export type TextInput = Extract<LogicalInput, { type: 'text' }>;

/** The fields of a `KeyboardEvent` that this layer reads. A real event fits. */
export interface KeyEventLike {
  readonly type: string;
  readonly key: string;
  readonly code: string;
  readonly location: number;
  readonly repeat: boolean;
  readonly isComposing: boolean;
  readonly keyCode: number;
  readonly shiftKey: boolean;
  readonly ctrlKey: boolean;
  readonly altKey: boolean;
  readonly metaKey: boolean;
  getModifierState(key: string): boolean;
}

/** Which Option keys of a Mac act as Alt. */
export type OptionAsAlt = 'none' | 'left' | 'right' | 'both';

export interface KeyboardOptions {
  platform: 'macos' | 'other';
  optionAsAlt: OptionAsAlt;
}

/** The keys of the US layout (PC-101), without Shift and with it, by `code`. */
const PUNCTUATION: Record<string, readonly [string, string]> = {
  Backquote: ['`', '~'],
  Minus: ['-', '_'],
  Equal: ['=', '+'],
  BracketLeft: ['[', '{'],
  BracketRight: [']', '}'],
  Backslash: ['\\', '|'],
  Semicolon: [';', ':'],
  Quote: ["'", '"'],
  Comma: [',', '<'],
  Period: ['.', '>'],
  Slash: ['/', '?'],
  Space: [' ', ' '],
};
const SHIFTED_DIGITS = ')!@#$%^&*(';

/**
 * The character that the key `code` makes on the US layout, or undefined for a key that has none
 * (arrows, function keys, keys of other layouts).
 */
export function usCharacter(code: string, shift: boolean): string | undefined {
  const letter = /^Key([A-Z])$/.exec(code);
  if (letter?.[1]) return shift ? letter[1] : letter[1].toLowerCase();
  const digit = /^Digit([0-9])$/.exec(code);
  if (digit?.[1]) return shift ? SHIFTED_DIGITS[Number(digit[1])] : digit[1];
  const pair = PUNCTUATION[code];
  return pair ? pair[shift ? 1 : 0] : undefined;
}

/** A key that makes one character, as opposed to a name such as `Enter` or `ArrowUp`. */
function isCharacter(key: string): boolean {
  return [...key].length === 1;
}

const NAMES: readonly ModName[] = ['shift', 'ctrl', 'alt', 'meta', 'caps_lock', 'num_lock'];

export class Keyboard {
  private options: KeyboardOptions;
  private leftAlt = false;
  private rightAlt = false;

  constructor(options: KeyboardOptions) {
    this.options = options;
  }

  /** Changes the options, for example after a change of the setting. */
  setOptions(options: KeyboardOptions): void {
    this.options = options;
  }

  /** Forget the keys that are down. Call it when the surface loses focus. */
  reset(): void {
    this.leftAlt = false;
    this.rightAlt = false;
  }

  /**
   * The logical event for a `keydown` or `keyup`, or null when there is none: an input method owns
   * the key, it is a dead key, or the event is not a key event. Feed it every key event of the
   * surface, also the ones of the Alt keys, so that it knows which Option key is down.
   */
  toLogical(e: KeyEventLike): KeyInput | null {
    const down = e.type === 'keydown';
    if (!down && e.type !== 'keyup') return null;
    if (e.key === 'Alt') {
      if (e.location === 2) this.rightAlt = down;
      else this.leftAlt = down;
    }
    // While an input method composes, it owns the key. A dead key reports 229 as well. Option as
    // Alt overrides that: the dead key of the Mac layout is a key of the Alt chord.
    const optionDead =
      e.key === 'Dead' && this.options.platform === 'macos' && e.altKey && this.optionIsAlt();
    if (e.isComposing || (e.keyCode === 229 && !optionDead)) return null;

    let key = e.key;
    const shift = e.shiftKey;
    const meta = e.metaKey;
    let ctrl = e.ctrlKey;
    let alt = e.altKey;
    if (e.getModifierState('AltGraph') && isCharacter(key)) {
      ctrl = false;
      alt = false;
    } else if (this.options.platform === 'macos' && alt && (isCharacter(key) || key === 'Dead')) {
      if (this.optionIsAlt()) {
        key = usCharacter(e.code, shift) ?? key;
      } else {
        // Option made the character. It is text, not an Alt chord.
        alt = false;
      }
    }
    if (key === 'Dead') return null;

    const kind: KeyKind = down ? (e.repeat ? 'repeat' : 'press') : 'release';
    const on: Record<ModName, boolean> = {
      shift,
      ctrl,
      alt,
      meta,
      caps_lock: e.getModifierState('CapsLock'),
      num_lock: e.getModifierState('NumLock'),
    };
    const logical: KeyInput = {
      type: 'key',
      code: e.code,
      key,
      mods: NAMES.filter((name) => on[name]),
      kind,
      location: e.location,
    };
    if (isCharacter(key) && !ctrl && !meta && kind !== 'release') logical.text = key;
    return logical;
  }

  /** Whether an Option key that is down acts as Alt. */
  private optionIsAlt(): boolean {
    switch (this.options.optionAsAlt) {
      case 'both':
        return true;
      case 'left':
        return this.leftAlt;
      case 'right':
        return this.rightAlt;
      default:
        return false;
    }
  }
}

/** The text of a `compositionend`, which an input method or a dead key made. */
export function compositionText(e: { readonly data: string }): TextInput | null {
  return e.data === '' ? null : { type: 'text', text: e.data };
}
