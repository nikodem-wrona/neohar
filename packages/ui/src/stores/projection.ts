// A Solid store for one projection. The core sends a snapshot and then patches,
// and the transport follows the versions. The store applies each message with `reconcile`, so
// that a component reads only the rows that changed. A component reads a store. A store never
// reads another store to compute data.

import type { ProjectionName, ProjectionValueMap, Transport } from '@neohar/transport';
import { createStore, reconcile } from 'solid-js/store';
import { afterPatch, emptyState, fromSnapshot, type ProjectionState, values } from './reducer';

/** The value type of a projection by name: `tree` gives `WorkspaceValue`. */
type ValueOf<N extends ProjectionName> = N extends `${infer Base}:${string}`
  ? Base extends keyof ProjectionValueMap
    ? ProjectionValueMap[Base]
    : never
  : N extends keyof ProjectionValueMap
    ? ProjectionValueMap[N]
    : never;

export interface ProjectionStore<V> {
  /** The state. Read it in a component or an effect. */
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
  // The reducer works on this plain copy. The store gets each new state through `reconcile`.
  let current = emptyState<V>();
  const subscription = transport.subscribe(name, {
    onSnapshot: (data, version) => {
      current = fromSnapshot<V>(data, version);
      setState(reconcile(current));
    },
    onPatch: (ops, version) => {
      current = afterPatch<V>(current, ops, version);
      setState(reconcile(current));
    },
    onResync: () => {
      current = { ...current, status: 'resyncing' };
      setState('status', 'resyncing');
    },
  });
  return {
    state,
    rows: () => values(state),
    close: () => subscription.close(),
  };
}
