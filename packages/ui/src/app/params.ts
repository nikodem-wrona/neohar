// What the page address says: which window this page is, and which view it shows. The host opens
// a window of the app at `index.html?window=<ULID>`. The settings window adds
// `&view=settings`.

export type View = 'main' | 'settings';

export interface AppParams {
  /** The id of the window in the `windows` projection, or `null` when the address has none. */
  window: string | null;
  view: View;
  /** The label of the machine of the settings window, when it is not this Mac. */
  machine?: string;
}

/** A machine label: lowercase letters, digits and `-`, up to 32 characters. */
const MACHINE = /^[a-z0-9-]{1,32}$/;

/** The ULID of a window: 26 characters of Crockford base32. */
const WINDOW_ID = /^[0-9A-HJKMNP-TV-Z]{26}$/;

export function isWindowId(value: string | null): value is string {
  return value !== null && WINDOW_ID.test(value);
}

/** Reads the query string of the page. An id that is not a ULID counts as none. */
export function parseParams(search: string): AppParams {
  const params = new URLSearchParams(search);
  const window = params.get('window');
  const machine = params.get('machine');
  return {
    window: isWindowId(window) ? window : null,
    view: params.get('view') === 'settings' ? 'settings' : 'main',
    ...(machine !== null && MACHINE.test(machine) ? { machine } : {}),
  };
}
