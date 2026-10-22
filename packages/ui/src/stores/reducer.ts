// The state of a projection and how a message changes it. These are plain functions: the
// Solid store in `projection.ts` applies them, and a test can check them without a browser.

import type { PatchOp, ProjectionData } from '@neohar/transport';

/** Where a store is in its life. */
export type Status = 'loading' | 'live' | 'resyncing';

/** A projection as a keyed map with an order. */
export interface ProjectionState<V> {
  byKey: Record<string, V>;
  order: string[];
  /** The version of the last message, or `null` before the first snapshot. */
  version: number | null;
  status: Status;
}

export function emptyState<V>(): ProjectionState<V> {
  return { byKey: {}, order: [], version: null, status: 'loading' };
}

/** The state that a snapshot gives. */
export function fromSnapshot<V>(data: ProjectionData, version: number): ProjectionState<V> {
  return {
    byKey: { ...(data.items as Record<string, V>) },
    order: [...data.order],
    version,
    status: 'live',
  };
}

/**
 * The state after a patch. The result is a new state, and the argument stays as it was. A key
 * that is new goes to the end of the order, unless an `order` op in the same patch says more.
 */
export function afterPatch<V>(
  state: ProjectionState<V>,
  ops: readonly PatchOp[],
  version: number,
): ProjectionState<V> {
  const byKey = { ...state.byKey };
  let order = [...state.order];
  for (const op of ops) {
    if (op.op === 'set') {
      if (!(op.key in byKey)) order.push(op.key);
      byKey[op.key] = op.value as V;
    } else if (op.op === 'remove') {
      delete byKey[op.key];
      order = order.filter((key) => key !== op.key);
    } else {
      // The keys of the op, then the keys that it does not name, in their old order.
      const named = op.keys.filter((key) => key in byKey);
      const rest = order.filter((key) => !named.includes(key));
      order = [...named, ...rest];
    }
  }
  return { byKey, order, version, status: 'live' };
}

/** The values in the order of the projection. */
export function values<V>(state: ProjectionState<V>): V[] {
  const out: V[] = [];
  for (const key of state.order) {
    const value = state.byKey[key];
    if (value !== undefined) out.push(value);
  }
  return out;
}
