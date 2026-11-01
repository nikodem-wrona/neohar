// The store of one projection. The core sends a snapshot and then patches, and the transport
// follows the versions. The store applies each message with the reducer, and it tells its
// listeners. A row that a patch did not change keeps its object, so that a component of that row
// does not render again. A component reads a store. A store never reads another store to compute
// data.

import type { ProjectionName, ProjectionValueMap, Transport } from '@neohar/transport';
import { useEffect, useMemo } from 'react';
import type { Source } from '../lib/value';
import { afterPatch, emptyState, fromSnapshot, type ProjectionState, values } from './reducer';

/** The value type of a projection by name: `tree` gives `WorkspaceValue`. */
type ValueOf<N extends ProjectionName> = N extends `${infer Base}:${string}`
  ? Base extends keyof ProjectionValueMap
    ? ProjectionValueMap[Base]
    : never
  : N extends keyof ProjectionValueMap
    ? ProjectionValueMap[N]
    : never;

export interface ProjectionStore<V> extends Source<ProjectionState<V>> {
  /** The values in the order of the projection. The array is the same until the next message. */
  readonly rows: () => V[];
  close(): void;
}

export function createProjectionStore<N extends ProjectionName>(
  transport: Transport,
  name: N,
): ProjectionStore<ValueOf<N>> {
  type V = ValueOf<N>;
  let current = emptyState<V>();
  let rows: { of: ProjectionState<V>; values: V[] } | null = null;
  const listeners = new Set<() => void>();
  const next = (state: ProjectionState<V>): void => {
    current = state;
    for (const listener of [...listeners]) listener();
  };
  const subscription = transport.subscribe(name, {
    onSnapshot: (data, version) => next(fromSnapshot<V>(data, version)),
    onPatch: (ops, version) => next(afterPatch<V>(current, ops, version)),
    onResync: () => next({ ...current, status: 'resyncing' }),
  });
  return {
    get: () => current,
    subscribe: (listener) => {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
    rows: () => {
      if (rows?.of !== current) rows = { of: current, values: values(current) };
      return rows.values;
    },
    close: () => subscription.close(),
  };
}

/**
 * A store that a component owns: it subscribes when the component mounts, or when the name
 * changes, and it ends with the component.
 */
export function useProjectionStore<N extends ProjectionName>(
  transport: Transport,
  name: N,
): ProjectionStore<ValueOf<N>> {
  const store = useMemo(() => createProjectionStore(transport, name), [transport, name]);
  useEffect(() => () => store.close(), [store]);
  return store;
}
