// The stores of a window and of a phone: one for each projection that the shell reads. Each
// projection has its own store, and no store reads another one.

import type { Transport } from '@neohar/transport';
import { createProjectionStore } from './projection';

export { createProjectionStore, useProjectionStore } from './projection';
export type { ProjectionStore } from './projection';
export { afterPatch, emptyState, fromSnapshot, values } from './reducer';
export type { ProjectionState, Status } from './reducer';

export function createStores(transport: Transport, windowId: string) {
  const stores = {
    windows: createProjectionStore(transport, 'windows'),
    tree: createProjectionStore(transport, 'tree'),
    panes: createProjectionStore(transport, 'panes'),
    attention: createProjectionStore(transport, 'attention'),
    // The interactions that wait for an answer, and the last ones that ended.
    interactions: createProjectionStore(transport, 'interactions'),
    // The agents with the read state of this window.
    agents: createProjectionStore(transport, `agents_for:${windowId}`),
    // The rate limits and the usage of today for each harness.
    limits: createProjectionStore(transport, 'limits'),
    health: createProjectionStore(transport, 'health'),
    // The link of this window to its machine. The host of the app serves it.
    link: createProjectionStore(transport, 'link'),
    settings: createProjectionStore(transport, 'settings'),
  };
  return {
    ...stores,
    /** Ends every subscription. */
    close(): void {
      for (const store of Object.values(stores)) store.close();
    },
  };
}

export type Stores = ReturnType<typeof createStores>;

/**
 * The stores of a phone for itself: its row of `phones` (the name and the push) and the
 * listener of the Mac (the VAPID key of the push). The desk on the phone has the stores of a window
 * too (`createStores`).
 */
export function createPhoneStores(transport: Transport) {
  const stores = {
    // The name of this phone and whether it has push.
    phones: createProjectionStore(transport, 'phones'),
    // The VAPID key of the core, for the push subscription.
    status: createProjectionStore(transport, 'phone_status'),
    // The machines of the Mac, which the phone reaches through it.
    machines: createProjectionStore(transport, 'machines'),
  };
  return {
    ...stores,
    /** Ends every subscription. */
    close(): void {
      for (const store of Object.values(stores)) store.close();
    },
  };
}

export type PhoneStores = ReturnType<typeof createPhoneStores>;
