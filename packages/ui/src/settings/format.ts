// The text of the settings window: how a name reads, and what an edit of a value means. The core
// owns the settings and checks every value (`config.set` answers `invalid_params` for a wrong
// one). The page only turns the text of a field into a value of the type that the setting has.

/** A name of a section or a key as a label: `font_size` reads `Font size`. */
export function humanize(name: string): string {
  const text = name.replaceAll('_', ' ');
  return text.charAt(0).toUpperCase() + text.slice(1);
}

export type Edit = { ok: true; value: unknown } | { ok: false; error: string };

/**
 * The value that the text of a field means for a setting that has `current`. A number needs a
 * number. A list or a table needs JSON. A string is the text.
 */
export function parseEdit(text: string, current: unknown): Edit {
  if (typeof current === 'number') {
    const value = Number(text);
    return text.trim() !== '' && Number.isFinite(value)
      ? { ok: true, value }
      : { ok: false, error: 'This setting is a number.' };
  }
  if (typeof current === 'object' && current !== null) {
    try {
      return { ok: true, value: JSON.parse(text) as unknown };
    } catch {
      return { ok: false, error: 'This setting is a list. Write it as JSON, for example [1, 2].' };
    }
  }
  return { ok: true, value: text };
}

/** The text that a field shows for a value. */
export function fieldText(value: unknown): string {
  return typeof value === 'string' ? value : JSON.stringify(value);
}
