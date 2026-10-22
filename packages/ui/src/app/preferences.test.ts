import { expect, test } from 'bun:test';
import type { SettingsSection } from '@neohar/transport';
import {
  fontOf,
  keymapOf,
  optionAsAlt,
  paneName,
  showHandles,
  themeVariables,
} from './preferences';

const section = (entries: Record<string, unknown>): SettingsSection => ({
  entries: Object.fromEntries(
    Object.entries(entries).map(([key, value]) => [key, { value, source: 'default' as const }]),
  ),
});

test('the theme gives a custom property for each colour', () => {
  expect(
    themeVariables({ theme: section({ bg: '#000', bg_raised: '#111', bad: 3, empty: '' }) }),
  ).toEqual({ '--bg': '#000', '--bg-raised': '#111' });
  expect(themeVariables({})).toEqual({});
});

test('the font takes the settings that have the right type', () => {
  expect(fontOf({ ui: section({ font_family: 'Iosevka', font_size: 15 }) })).toEqual({
    family: 'Iosevka',
    px: 15,
  });
  expect(fontOf({ ui: section({ font_family: '', font_size: 2 }) })).toEqual({});
  expect(fontOf({ ui: section({ font_size: '15' }) })).toEqual({});
  expect(fontOf({})).toEqual({});
});

test('the option key has four values and none is the fallback', () => {
  expect(optionAsAlt({ ui: section({ macos_option_as_alt: 'left' }) })).toBe('left');
  expect(optionAsAlt({ ui: section({ macos_option_as_alt: 'both' }) })).toBe('both');
  expect(optionAsAlt({ ui: section({ macos_option_as_alt: 'weird' }) })).toBe('none');
  expect(optionAsAlt({})).toBe('none');
});

test('the handles show only when the setting is true', () => {
  expect(showHandles({ ui: section({ show_handles: true }) })).toBe(true);
  expect(showHandles({ ui: section({ show_handles: false }) })).toBe(false);
  expect(showHandles({ ui: section({ show_handles: 'true' }) })).toBe(false);
  expect(showHandles({})).toBe(false);
});

test('a pane goes by its title when the handles do not show, and by its handle else', () => {
  const pane = { handle: 'w1:p3', title: 'build' };
  expect(paneName(pane, false)).toBe('build');
  expect(paneName(pane, true)).toBe('w1:p3');
  expect(paneName({ handle: 'w1:p3', title: '' }, false)).toBe('w1:p3');
  expect(paneName(undefined, false)).toBeUndefined();
});

test('the keymap has the default chords and the ones that the settings changed', () => {
  const none = keymapOf({}, 'macos');
  expect(none.commands.find((c) => c.command === 'palette.open')?.chord.code).toBe('KeyK');
  const changed = keymapOf({ keys: section({ palette_open: 'mod+j' }) }, 'macos');
  expect(changed.commands.find((c) => c.command === 'palette.open')?.chord.code).toBe('KeyJ');
  // A chord that does not parse is left out, and the others stay.
  const broken = keymapOf({ keys: section({ palette_open: 'x' }) }, 'macos');
  expect(broken.commands.some((c) => c.command === 'palette.open')).toBe(false);
  expect(broken.commands.some((c) => c.command === 'tab.next')).toBe(true);
});
