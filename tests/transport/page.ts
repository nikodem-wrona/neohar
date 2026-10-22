// The page of the test: it opens the WebSocketTransport of the address and keeps what the core
// sends, so that the Playwright spec can read it.

import { WebSocketTransport } from '@neohar/transport';
import { realTimers } from '../../packages/ui/src/lib/timing';

interface Seen {
  kind: string;
  version: number;
  keys: string[];
}

const transport = WebSocketTransport.fromUrl(location.href, realTimers);
const seen: Seen[] = [];
const resyncs: number[] = [];

const subscription = transport.subscribe('tree', {
  onSnapshot: (data, version) =>
    seen.push({ kind: 'snapshot', version, keys: Object.keys(data.items) }),
  onPatch: (ops, version) =>
    seen.push({
      kind: 'patch',
      version,
      keys: ops.flatMap((op) => (op.op === 'set' || op.op === 'remove' ? [op.key] : [])),
    }),
  onResync: () => resyncs.push(Date.now()),
});

Object.assign(window, {
  nhTest: {
    seen: () => seen,
    resyncs: () => resyncs,
    call: (method: string, params: Record<string, unknown>) =>
      (transport as unknown as { call: (m: string, p: unknown) => Promise<unknown> }).call(
        method,
        params,
      ),
    close: () => subscription.close(),
  },
});
document.getElementById('log')!.textContent = 'ready';
