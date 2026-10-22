import { expect, test } from 'bun:test';
import { isCopyChord, isPasteChord } from './clipboard';
import type { ChordEventLike } from './reserved';

const key = (code: string, mods: Partial<ChordEventLike> = {}): ChordEventLike => ({
  code,
  metaKey: false,
  shiftKey: false,
  altKey: false,
  ctrlKey: false,
  ...mods,
});

test('on macOS copy and paste are ⌘C and ⌘V', () => {
  expect(isCopyChord(key('KeyC', { metaKey: true }), 'macos')).toBe(true);
  expect(isPasteChord(key('KeyV', { metaKey: true }), 'macos')).toBe(true);
  expect(isCopyChord(key('KeyC', { ctrlKey: true }), 'macos')).toBe(false);
  expect(isCopyChord(key('KeyC', { metaKey: true, shiftKey: true }), 'macos')).toBe(false);
  expect(isCopyChord(key('KeyC', { metaKey: true, altKey: true }), 'macos')).toBe(false);
});

test('elsewhere they are Ctrl+Shift+C and Ctrl+Shift+V, and Ctrl+C is the interrupt', () => {
  expect(isCopyChord(key('KeyC', { ctrlKey: true, shiftKey: true }), 'other')).toBe(true);
  expect(isPasteChord(key('KeyV', { ctrlKey: true, shiftKey: true }), 'other')).toBe(true);
  expect(isCopyChord(key('KeyC', { ctrlKey: true }), 'other')).toBe(false);
  expect(isPasteChord(key('KeyV', { ctrlKey: true }), 'other')).toBe(false);
});

test('other keys are not these chords', () => {
  expect(isCopyChord(key('KeyV', { metaKey: true }), 'macos')).toBe(false);
  expect(isPasteChord(key('KeyC', { metaKey: true }), 'macos')).toBe(false);
});
