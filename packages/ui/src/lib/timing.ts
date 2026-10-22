// Timers of the webview. This is the only file that may call `setTimeout`.
// Everything else is push: the core sends, and the UI reacts. A timer here is a delay for the
// UI's own input, for example the 50 ms between the last change of a pane's size and the
// `pane.resize` call. It never polls.

/** The two timer functions, so that a test can drive time by hand. */
export interface Timers {
  set(callback: () => void, ms: number): unknown;
  clear(handle: unknown): void;
}

/** The timers of the page. The transport takes them for its pause before a new connection. */
export const realTimers: Timers = {
  set: (callback, ms) => setTimeout(callback, ms),
  clear: (handle) => clearTimeout(handle as ReturnType<typeof setTimeout>),
};

/** A function that runs `ms` after its last call. */
export interface Debounced<A extends unknown[]> {
  (...args: A): void;
  /** Drops the pending call, if any. */
  cancel(): void;
}

/**
 * Calls `fn` once, `ms` after the last call of the result, with the arguments of that last call.
 * A burst of calls less than `ms` apart makes one call at its end.
 */
export function debounce<A extends unknown[]>(
  fn: (...args: A) => void,
  ms: number,
  timers: Timers = realTimers,
): Debounced<A> {
  let pending: unknown;
  let waiting = false;
  const debounced = (...args: A): void => {
    if (waiting) timers.clear(pending);
    waiting = true;
    pending = timers.set(() => {
      waiting = false;
      fn(...args);
    }, ms);
  };
  debounced.cancel = (): void => {
    if (waiting) timers.clear(pending);
    waiting = false;
  };
  return debounced;
}
