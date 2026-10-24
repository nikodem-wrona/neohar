import { expect, test } from 'bun:test';
import {
  type ChordEventLike,
  chordLabel,
  commandOfKey,
  compileKeymap,
  DEFAULT_KEYS,
  matchesChord,
  parseChord,
} from './chords';

const event = (code: string, mods: Partial<Omit<ChordEventLike, 'code'>> = {}): ChordEventLike => ({
  code,
  metaKey: false,
  shiftKey: false,
  altKey: false,
  ctrlKey: false,
  ...mods,
});

test('mod is Command on a Mac and Control elsewhere', () => {
  expect(parseChord('mod+k', 'macos')).toEqual({
    code: 'KeyK',
    meta: true,
    ctrl: false,
    shift: false,
    alt: false,
  });
  expect(parseChord('mod+k', 'other')).toEqual({
    code: 'KeyK',
    meta: false,
    ctrl: true,
    shift: false,
    alt: false,
  });
});

test('the keys that are not letters', () => {
  expect(parseChord('mod+shift+]', 'macos')?.code).toBe('BracketRight');
  expect(parseChord('mod+alt+left', 'macos')?.code).toBe('ArrowLeft');
  expect(parseChord('ctrl+7', 'macos')?.code).toBe('Digit7');
  expect(parseChord('alt+f5', 'macos')?.code).toBe('F5');
  expect(parseChord('cmd+,', 'macos')?.code).toBe('Comma');
});

test('text that is not a chord gives nothing', () => {
  expect(parseChord('k', 'macos')).toBeNull();
  expect(parseChord('', 'macos')).toBeNull();
  expect(parseChord('mod+', 'macos')).toBeNull();
  expect(parseChord('hyper+k', 'macos')).toBeNull();
  expect(parseChord('mod+kk', 'macos')).toBeNull();
  expect(parseChord('mod+f13', 'macos')).toBeNull();
});

test('a chord matches the key and exactly its modifiers', () => {
  const chord = parseChord('mod+shift+d', 'macos');
  expect(chord).not.toBeNull();
  if (!chord) return;
  expect(matchesChord(chord, event('KeyD', { metaKey: true, shiftKey: true }))).toBe(true);
  expect(matchesChord(chord, event('KeyD', { metaKey: true }))).toBe(false);
  expect(matchesChord(chord, event('KeyD', { metaKey: true, shiftKey: true, altKey: true }))).toBe(
    false,
  );
  expect(matchesChord(chord, event('KeyE', { metaKey: true, shiftKey: true }))).toBe(false);
});

test('a name of the keys section stands for a command with a dot', () => {
  expect(commandOfKey('palette_open')).toBe('palette.open');
  expect(commandOfKey('pane_focus_left')).toBe('pane.focus_left');
  expect(commandOfKey('interaction_answer_next')).toBe('interaction.answer_next');
});

test('the default keys make the default keymap', () => {
  const keymap = compileKeymap(DEFAULT_KEYS, 'macos');
  expect(keymap.commands.map((c) => c.command)).toEqual([
    'palette.open',
    'tab.next',
    'tab.previous',
    'pane.focus_left',
    'pane.focus_right',
    'pane.focus_up',
    'pane.focus_down',
    'pane.split_right',
    'pane.split_down',
    'inbox.open',
    'interaction.answer_next',
    'peek.sibling',
    'search.open',
    'git.open',
    'window.new',
  ]);
  expect(keymap.passThrough?.code).toBe('KeyV');
});

test('a chord that does not parse is left out and the others stay', () => {
  const keymap = compileKeymap(
    { palette_open: 'mod+j', tab_next: 'nonsense', window_new: 'mod+shift+n', pass_through: 'x' },
    'macos',
  );
  expect(keymap.commands.map((c) => c.command)).toEqual(['palette.open', 'window.new']);
  expect(keymap.passThrough).toBeNull();
});

test('a label shows the modifiers in the order of the platform', () => {
  const spec = (text: string, platform: 'macos' | 'other') => parseChord(text, platform)!;
  expect(chordLabel(spec('mod+shift+a', 'macos'), 'macos')).toBe('⇧⌘A');
  expect(chordLabel(spec('mod+k', 'macos'), 'macos')).toBe('⌘K');
  expect(chordLabel(spec('mod+shift+]', 'macos'), 'macos')).toBe('⇧⌘]');
  expect(chordLabel(spec('mod+alt+left', 'macos'), 'macos')).toBe('⌥⌘←');
  expect(chordLabel(spec('mod+shift+a', 'other'), 'other')).toBe('Ctrl+Shift+A');
  expect(chordLabel(spec('ctrl+f5', 'other'), 'other')).toBe('Ctrl+F5');
});
