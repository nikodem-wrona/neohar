// The keymap of the window. It sees every key before the pane does (the capture phase). A reserved
// command is taken: the page does not pass the key on, and the core gets the command. A menu chord
// stays with the native menu. The chord for "pass the next key" lets one reserved key reach the
// pane. Composition is left alone: the keyboard layer handles it.

import type { Keymap } from '../input/chords';
import { isPassThrough, resolveChord } from '../input/reserved';
import { type ActionContext, runCommand } from './actions';

/** A key that only changes a modifier. It does not end the pass-through. */
const MODIFIER_CODES = /^(Meta|Shift|Alt|Control)(Left|Right)$/;

export interface KeyTarget {
  addEventListener(type: 'keydown', listener: (e: KeyboardEvent) => void, capture: boolean): void;
  removeEventListener(
    type: 'keydown',
    listener: (e: KeyboardEvent) => void,
    capture: boolean,
  ): void;
}

export interface PageTarget {
  addEventListener(type: 'blur', listener: () => void): void;
  removeEventListener(type: 'blur', listener: () => void): void;
}

/** The flag of the view that the keymap reads and sets. */
export interface PassThrough {
  passNext(): boolean;
  setPassNext(value: boolean): void;
}

export function installKeymap(
  keys: KeyTarget,
  page: PageTarget,
  ctx: ActionContext,
  view: PassThrough,
  /** The chords of the moment. A change of the settings changes them without a new keymap. */
  keymap: () => Keymap,
): () => void {
  const onKeyDown = (e: KeyboardEvent): void => {
    if (e.isComposing || e.keyCode === 229) return;
    if (view.passNext()) {
      // This key reaches the pane, whatever it is. A modifier alone does not use up the pass.
      if (!MODIFIER_CODES.test(e.code)) view.setPassNext(false);
      return;
    }
    const chords = keymap();
    if (isPassThrough(chords, e)) {
      e.preventDefault();
      e.stopPropagation();
      view.setPassNext(true);
      return;
    }
    const chord = resolveChord(chords, e);
    // A menu chord belongs to the native menu. The page leaves the event alone.
    if (!chord || chord.kind === 'menu') return;
    e.preventDefault();
    e.stopPropagation();
    runCommand(chord.command, ctx);
  };
  const onBlur = (): void => {
    view.setPassNext(false);
  };
  keys.addEventListener('keydown', onKeyDown, true);
  page.addEventListener('blur', onBlur);
  return () => {
    keys.removeEventListener('keydown', onKeyDown, true);
    page.removeEventListener('blur', onBlur);
  };
}
