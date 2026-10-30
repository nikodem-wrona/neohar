// The store of one projection. The core sends a snapshot and then patches, and the transport
// follows the versions. The store applies each message with the reducer, and it tells its
// listeners. A row that a patch did not change keeps its object, so that a component of that row
// does not render again. A component reads a store. A store never reads another store to compute
// data.

import type { ProjectionName, ProjectionValueMap, Transport } from '@neohar/transport';
import { createStore, reconcile } from 'solid-js/store';
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
  /** The state as a Solid store, for the Solid components until the port to React. */
  readonly state: ProjectionState<V>;
  /** The values in the order of the projection. */
  readonly rows: () => V[];
  close(): void;
}

export function createProjectionStore<N extends ProjectionName>(
  transport: Transport,
  name: N,
): ProjectionStore<ValueOf<N>> {
  type V = ValueOf<N>;
  const [state, setState] = createStore<ProjectionState<V>>(emptyState<V>());
  // The reducer works on this plain state. The Solid store gets each new state through `reconcile`.
  let current = emptyState<V>();
  const listeners = new Set<() => void>();
  const next = (state: ProjectionState<V>): void => {
    current = state;
    for (const listener of [...listeners]) listener();
  };
  const subscription = transport.subscribe(name, {
    onSnapshot: (data, version) => {
      next(fromSnapshot<V>(data, version));
      setState(reconcile(current));
    },
    onPatch: (ops, version) => {
      next(afterPatch<V>(current, ops, version));
      setState(reconcile(current));
    },
    onResync: () => {
      next({ ...current, status: 'resyncing' });
      setState('status', 'resyncing');
    },
  });
  return {
    get: () => current,
    subscribe: (listener) => {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
    state,
    rows: () => values(state),
    close: () => subscription.close(),
  };
}
