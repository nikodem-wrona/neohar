import { expect, test } from 'bun:test';
import { compileKeymap, DEFAULT_KEYS } from './chords';
import { isPassThrough, resolveChord, type ChordEventLike } from './reserved';

/** The keymap of the defaults, for a Mac. */
const keymap = compileKeymap(DEFAULT_KEYS, 'macos');
const reservedChord = (e: ChordEventLike) => resolveChord(keymap, e);
const isPassThroughChord = (e: ChordEventLike) => isPassThrough(keymap, e);

const chord = (code: string, mods: Partial<Omit<ChordEventLike, 'code'>> = {}): ChordEventLike => ({
  code,
  metaKey: true,
  shiftKey: false,
  altKey: false,
  ctrlKey: false,
  ...mods,
});

test('the reserved commands', () => {
  expect(reservedChord(chord('KeyK'))).toEqual({ kind: 'command', command: 'palette.open' });
  expect(reservedChord(chord('BracketRight', { shiftKey: true }))).toEqual({
    kind: 'command',
    command: 'tab.next',
  });
  expect(reservedChord(chord('ArrowLeft', { altKey: true }))).toEqual({
    kind: 'command',
    command: 'pane.focus_left',
  });
  expect(reservedChord(chord('KeyD'))).toEqual({ kind: 'command', command: 'pane.split_right' });
  expect(reservedChord(chord('KeyD', { shiftKey: true }))).toEqual({
    kind: 'command',
    command: 'pane.split_down',
  });
  expect(reservedChord(chord('KeyN', { shiftKey: true }))).toEqual({
    kind: 'command',
    command: 'window.new',
  });
});

test('the menu accelerators are not commands', () => {
  expect(reservedChord(chord('KeyQ'))).toEqual({ kind: 'menu', accelerator: '⌘Q' });
  expect(reservedChord(chord('KeyW'))).toEqual({ kind: 'menu', accelerator: '⌘W' });
  expect(reservedChord(chord('KeyN'))).toEqual({ kind: 'menu', accelerator: '⌘N' });
  expect(reservedChord(chord('Comma'))).toEqual({ kind: 'menu', accelerator: '⌘,' });
});

test('everything else reaches the terminal', () => {
  expect(reservedChord(chord('KeyC'))).toBeNull();
  expect(reservedChord(chord('KeyK', { metaKey: false }))).toBeNull();
  expect(reservedChord(chord('KeyK', { shiftKey: true }))).toBeNull();
  expect(reservedChord(chord('KeyK', { ctrlKey: true }))).toBeNull();
  expect(reservedChord(chord('ArrowLeft'))).toBeNull();
});

test('the pass-through chord is ⌘⌥V and nothing else', () => {
  expect(isPassThroughChord(chord('KeyV', { altKey: true }))).toBe(true);
  expect(isPassThroughChord(chord('KeyV'))).toBe(false);
  expect(isPassThroughChord(chord('KeyV', { altKey: true, shiftKey: true }))).toBe(false);
  expect(isPassThroughChord(chord('KeyV', { altKey: true, metaKey: false }))).toBe(false);
  // It is not a reserved command, so it does not reach the command table.
  expect(reservedChord(chord('KeyV', { altKey: true }))).toBeNull();
});
