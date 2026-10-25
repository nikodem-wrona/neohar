// The links of the text of an agent. On a phone, an address of the loopback interface means the
// Mac, not the phone. In the app, a click on a link goes to the host as a navigation that carries
// the gesture token of the host: a refresh or a form in hostile HTML has no script, so it cannot
// know the token, and the host opens nothing for it.

/** An address of the loopback interface. */
export function isLoopback(href: string): boolean {
  try {
    const host = new URL(href).hostname;
    return host === 'localhost' || host === '127.0.0.1' || host === '[::1]' || host === '::1';
  } catch {
    return false;
  }
}

/** The start of each link that the core writes: the host opens the address outside the app. */
const OPEN = 'nh://open?url=';

/**
 * The navigation for a click on a link in the app: the link of the core with the gesture token of
 * the host. `null` when the link is not one of the core or when the page has no token (a browser).
 */
export function clickedLink(href: string, token: string | undefined): string | null {
  if (!token || !href.startsWith(OPEN) || href.includes('&')) return null;
  return `${href}&gesture=${encodeURIComponent(token)}`;
}

/** The gesture token that the host gives each window of the app, if the page has one. */
export function gestureToken(): string | undefined {
  const token = (globalThis as { __NEOHAR_GESTURE__?: unknown }).__NEOHAR_GESTURE__;
  return typeof token === 'string' && token.length > 0 ? token : undefined;
}
