// A value of the page that a listener follows. The state modules of the window keep their state in
// values, with no framework in them: a test reads them with `get`, and a component follows them
// with `subscribe`.

/** Something that the page reads, and that a listener follows. */
export interface Source<T> {
  get(): T;
  /** Calls `listener` after each change. Returns the function that stops it. */
  subscribe(listener: () => void): () => void;
}

/** A value that the page reads, and that a listener follows. */
export interface Value<T> extends Source<T> {
  set(next: T): void;
}

/**
 * A value that starts at `initial`. A `set` to a value that is `equal` to the current one changes
 * nothing and calls no listener.
 */
export function createValue<T>(initial: T, equal: (a: T, b: T) => boolean = Object.is): Value<T> {
  let current = initial;
  const listeners = new Set<() => void>();
  return {
    get: () => current,
    set: (next) => {
      if (equal(current, next)) return;
      current = next;
      // A copy, so that a listener that stops or starts another one does not change this loop.
      for (const listener of [...listeners]) listener();
    },
    subscribe: (listener) => {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
  };
}
