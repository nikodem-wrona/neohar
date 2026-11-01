// The hooks through which a component reads the state of the window: a value of `lib/value.ts`, a
// part of a projection store, or the rows of a store. A component renders again only when what it
// reads changes.

import { useLayoutEffect, useRef, useSyncExternalStore, type RefObject } from 'react';
import type { Source } from './value';

/** The current value. The component renders again when it changes. */
export function useValue<T>(value: Source<T>): T {
  return useSyncExternalStore(value.subscribe, value.get);
}

/**
 * A part of a store. The component renders again only when the part changes: a pick that is
 * `equal` to the last one gives the last one. The last pick is kept for each state, so that a
 * pick that makes a new array does not render without end.
 */
export function useSelect<S, T>(
  source: Source<S>,
  pick: (state: S) => T,
  equal: (a: T, b: T) => boolean = Object.is,
): T {
  const last = useRef<{ state: S; pick: (state: S) => T; value: T } | null>(null);
  const read = (): T => {
    const state = source.get();
    const before = last.current;
    if (before && before.state === state && before.pick === pick) return before.value;
    const picked = pick(state);
    const value = before && equal(before.value, picked) ? before.value : picked;
    last.current = { state, pick, value };
    return value;
  };
  return useSyncExternalStore(source.subscribe, read);
}

/** Whether two arrays, or the values of two objects, are the same items. */
export function shallowEqual<T>(a: T, b: T): boolean {
  if (Object.is(a, b)) return true;
  if (typeof a !== 'object' || typeof b !== 'object' || a === null || b === null) return false;
  const left = Object.keys(a) as (keyof T)[];
  const right = Object.keys(b);
  if (left.length !== right.length) return false;
  return left.every((key) => Object.hasOwn(b, key) && Object.is(a[key], b[key]));
}

/** A store that gives its rows in the order of the projection. */
interface Rows<V> {
  rows(): V[];
  subscribe(listener: () => void): () => void;
}

/** The rows of a store. The array is the same until the store changes. */
export function useRows<V>(store: Rows<V>): V[] {
  return useSyncExternalStore(store.subscribe, store.rows);
}

/**
 * A ref to the value of the last render. A native listener or a callback that lives longer than a
 * render reads it, and so sees the props of now.
 */
export function useLatest<T>(value: T): RefObject<T> {
  const ref = useRef(value);
  useLayoutEffect(() => {
    ref.current = value;
  });
  return ref;
}
