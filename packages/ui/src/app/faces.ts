// The faces of the fonts that the person installed. On macOS the webview matches no
// such font by its family name, so the app serves their files over `nh://font/`, and the page
// declares them with a stylesheet from there. A phone gets the same sheet from the listener of the
// core: `Transport.fontsUrl` gives the address. The surface measures and draws the font only once
// the faces are loaded: a canvas draws a face that still loads with the next family of the list.

import { realTimers, type Timers } from '../lib/timing';
import { fontString } from '../surface/metrics';

/** The id of the stylesheet element of the faces. */
export const FACES_LINK_ID = 'nh-faces';

/** How long the page waits for the stylesheet and for the faces, in ms. Then it draws with what it has. */
export const FACES_WAIT_MS = 3000;

/** The variants of a font that the atlas draws: bit 0 is bold and bit 1 is italic. */
const VARIANTS = [0, 1, 2, 3];

/** The CSS fonts whose faces the page loads before it measures: each variant of the family. */
export function fontStrings(family: string, px: number): string[] {
  return VARIANTS.map((variant) => fontString({ family, sizePx: px }, variant));
}

/** `promise`, or nothing after `ms` milliseconds, or nothing when the promise fails. */
export function within<T>(
  promise: Promise<T>,
  ms: number,
  timers: Timers = realTimers,
): Promise<T | undefined> {
  return new Promise((resolve) => {
    const timer = timers.set(() => resolve(undefined), ms);
    promise.then(
      (value) => {
        timers.clear(timer);
        resolve(value);
      },
      () => {
        timers.clear(timer);
        resolve(undefined);
      },
    );
  });
}

/**
 * Puts the stylesheet of the faces of `family` in the page and waits until the faces that the
 * variants of the font need are loaded, or until the wait is over. A page without the `nh://`
 * scheme, as the tests have, gets an error for the stylesheet and goes on at once.
 */
export async function loadFaces(href: string, family: string, px: number): Promise<void> {
  await within(stylesheet(href), FACES_WAIT_MS);
  const loads = fontStrings(family, px).map((font) => document.fonts.load(font));
  await within(Promise.allSettled(loads), FACES_WAIT_MS);
}

/** The stylesheet that the page has or loads: its address and the end of its load. */
let sheet: { href: string; loaded: Promise<void> } | undefined;

/**
 * Points the stylesheet element at `href` and resolves when the sheet is loaded or failed. A
 * second call with the same address, as the settings make at the start of a window while the
 * first load runs, waits for that same load.
 */
function stylesheet(href: string): Promise<void> {
  if (sheet?.href === href) return sheet.loaded;
  const found = document.getElementById(FACES_LINK_ID);
  const link = found instanceof HTMLLinkElement ? found : makeLink();
  const loaded = new Promise<void>((resolve) => {
    link.addEventListener('load', () => resolve(), { once: true });
    link.addEventListener('error', () => resolve(), { once: true });
    link.setAttribute('href', href);
  });
  sheet = { href, loaded };
  return loaded;
}

/** The stylesheet element, in the head of the page. */
function makeLink(): HTMLLinkElement {
  const link = document.createElement('link');
  link.id = FACES_LINK_ID;
  link.rel = 'stylesheet';
  document.head.appendChild(link);
  return link;
}
