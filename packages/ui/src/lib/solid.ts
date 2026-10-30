// The bridge from the values of the state modules (`lib/value.ts`) to the Solid components. It is
// for the time of the port to React only: the React components read the values with hooks, and
// then this file goes.

import { type Accessor, createRoot, from } from 'solid-js';
import { dragged as draggedValue } from '../app/drag';
import type { Source, Value } from './value';

/** A Solid accessor that follows `source`. Make it under an owner, which stops it. */
export function accessor<T>(source: Source<T>): Accessor<T> {
  return from((set) => source.subscribe(() => set(() => source.get())), source.get());
}

/** The value that a setter gets: the next value, or a function of the current one. */
type Next<T> = T | ((current: T) => T);

/**
 * An object of values and functions, as the Solid components read it: each source `x` becomes the
 * accessor `x()`, and each value also gets the setter `setX(next)`. The functions stay as they are.
 */
export type Bridged<O> = {
  [K in keyof O]: O[K] extends Source<infer T> ? Accessor<T> : O[K];
} & {
  [
    K in keyof O as O[K] extends Value<unknown> ? `set${Capitalize<K & string>}` : never
  ]: O[K] extends Value<infer T> ? (next: Next<T>) => void : never;
};

function isSource(item: unknown): item is Value<unknown> {
  return typeof item === 'object' && item !== null && 'get' in item && 'subscribe' in item;
}

/** The Solid form of `object`. Make it under an owner, which stops the accessors. */
export function bridge<O extends object>(object: O): Bridged<O> {
  const out: Record<string, unknown> = {};
  for (const [key, item] of Object.entries(object)) {
    if (!isSource(item)) {
      out[key] = item;
      continue;
    }
    out[key] = accessor(item);
    out[`set${key.charAt(0).toUpperCase()}${key.slice(1)}`] = (next: unknown) => {
      item.set(
        typeof next === 'function' ? (next as (current: unknown) => unknown)(item.get()) : next,
      );
    };
  }
  return out as Bridged<O>;
}

/** What the person drags now, as a Solid accessor. */
export const dragged: Accessor<ReturnType<typeof draggedValue.get>> = createRoot(() =>
  accessor(draggedValue),
);
