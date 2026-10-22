// What the settings change in a window: the colours of the chrome, the font of the
// surface, the Option key of a Mac, the handles in the chrome and the reserved chords. The
// `settings` projection has the values, and these functions turn them into what the page uses. A
// setting that is missing or has the wrong type leaves the default in force.

import type { SettingsSection } from '@neohar/transport';
import type { Platform } from '../input/clipboard';
import { compileKeymap, DEFAULT_KEYS, type Keymap } from '../input/chords';
import type { OptionAsAlt } from '../input/keyboard';

/** The sections of the projection by name. */
export type Sections = Readonly<Record<string, SettingsSection | undefined>>;

function value(sections: Sections, section: string, key: string): unknown {
  return sections[section]?.entries[key]?.value;
}

function text(sections: Sections, section: string, key: string): string | undefined {
  const found = value(sections, section, key);
  return typeof found === 'string' ? found : undefined;
}

/** The colours of the chrome as CSS custom properties: `bg_raised` becomes `--bg-raised`. */
export function themeVariables(sections: Sections): Record<string, string> {
  const variables: Record<string, string> = {};
  for (const [key, entry] of Object.entries(sections['theme']?.entries ?? {})) {
    if (typeof entry.value === 'string' && entry.value !== '') {
      variables[`--${key.replaceAll('_', '-')}`] = entry.value;
    }
  }
  return variables;
}

/** The font size of a phone (`phone.font_size`): a pinch keeps it in these bounds. */
export const PHONE_FONT = { min: 9, max: 20 } as const;

/**
 * The font of the surface. A field is missing when the setting is. A phone has a size of its own
 * (`phone.font_size`).
 */
export function fontOf(
  sections: Sections,
  form: 'desk' | 'phone' = 'desk',
): { family?: string; px?: number } {
  const family = text(sections, 'ui', 'font_family');
  const font: { family?: string; px?: number } = {};
  if (family !== undefined && family !== '') font.family = family;
  if (form === 'phone') {
    const size = value(sections, 'phone', 'font_size');
    if (typeof size === 'number') font.px = clampPhoneFont(size);
    return font;
  }
  const size = value(sections, 'ui', 'font_size');
  if (typeof size === 'number' && size >= 6 && size <= 72) font.px = size;
  return font;
}

/** A font size of a phone in its bounds, in whole pixels. */
export function clampPhoneFont(size: number): number {
  return Math.min(PHONE_FONT.max, Math.max(PHONE_FONT.min, Math.round(size)));
}

/** The keys of the key bar of a phone after the fixed ones (`phone.keys`). */
export function phoneKeys(sections: Sections): string[] {
  const keys = value(sections, 'phone', 'keys');
  return Array.isArray(keys)
    ? keys.filter((item): item is string => typeof item === 'string' && item !== '').slice(0, 12)
    : [];
}

const OPTION_AS_ALT: readonly OptionAsAlt[] = ['none', 'left', 'right', 'both'];

/** Which Option keys of a Mac act as Alt. */
export function optionAsAlt(sections: Sections): OptionAsAlt {
  const found = text(sections, 'ui', 'macos_option_as_alt');
  return OPTION_AS_ALT.find((candidate) => candidate === found) ?? 'none';
}

/** Whether the chrome shows the handles of tabs and panes, such as `w1:t2`. */
export function showHandles(sections: Sections): boolean {
  return value(sections, 'ui', 'show_handles') === true;
}

/**
 * The name of a pane in the chrome: its handle when the handles show, else its title. A pane
 * with no title keeps its handle, so that the name is never empty.
 */
export function paneName(
  pane: { handle: string; title: string } | undefined,
  handles: boolean,
): string | undefined {
  if (!pane) return undefined;
  return handles || pane.title === '' ? pane.handle : pane.title;
}

/** The keymap of the settings. A chord that is missing or does not parse is the default one. */
export function keymapOf(sections: Sections, platform: Platform): Keymap {
  const keys: Record<string, string> = { ...DEFAULT_KEYS };
  for (const [name, entry] of Object.entries(sections['keys']?.entries ?? {})) {
    if (typeof entry.value === 'string') keys[name] = entry.value;
  }
  return compileKeymap(keys, platform);
}
