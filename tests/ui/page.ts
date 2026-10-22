// The page of the test: the shell of the UI with the WebSocketTransport. The address names the
// core (`ws`, `token`) and the window (`window`), as the host names them in the app.

import { createDecoder } from '@neohar/proto-wasm';
import { WebSocketTransport } from '@neohar/transport';
import { ATLAS_SLOTS, mountApp, mountSettings, parseParams } from '@neohar/ui';
import { realTimers } from '@neohar/ui/phone';
import wasmUrl from '../../packages/proto-wasm/pkg/proto_wasm_bg.wasm?url';
import '@neohar/ui/style.css';

const params = parseParams(location.search);
const transport = WebSocketTransport.fromUrl(location.href, realTimers);
const root = document.getElementById('root');
if (!root) throw new Error('no root');

// The settings window has no window of the layout and no renderer.
if (params.view === 'settings') {
  const calls: { method: string; params: unknown }[] = [];
  const call = transport.call.bind(transport) as (
    method: string,
    params: unknown,
  ) => Promise<unknown>;
  (transport as unknown as { call: typeof call }).call = (method, args) => {
    calls.push({ method, params: args });
    return call(method, args);
  };
  mountSettings(root, transport);
  Object.assign(window, { nhTest: { calls: () => calls } });
} else {
  // The spec counts the calls that the UI sends, so it can tell that a drag sent one.
  const calls: { method: string; params: unknown; error?: string }[] = [];
  const call = transport.call.bind(transport) as (
    method: string,
    params: unknown,
  ) => Promise<unknown>;
  (transport as unknown as { call: typeof call }).call = (method, args) => {
    const entry: { method: string; params: unknown; error?: string } = { method, params: args };
    calls.push(entry);
    // A call that the core refuses keeps its error, so that a failing spec can say why.
    return call(method, args).catch((failure: unknown) => {
      entry.error = JSON.stringify(failure);
      throw failure;
    });
  };

  // The host opens the windows of the app. A test page without `window` opens one as a UI client
  // can (`window.open` is a method of the UI scope).
  const windowId = params.window ?? (await transport.call('window.open', {})).window;
  calls.length = 0;

  // The keys that reach the page. A reserved chord is taken before this listener sees it.
  const keys: { code: string; meta: boolean; alt: boolean; shift: boolean }[] = [];
  document.addEventListener('keydown', (e) =>
    keys.push({ code: e.code, meta: e.metaKey, alt: e.altKey, shift: e.shiftKey }),
  );

  const decoder = await createDecoder(wasmUrl, ATLAS_SLOTS);
  const app = mountApp(root, transport, windowId, { decoder });
  Object.assign(window, {
    nhTest: {
      calls: () => calls,
      keys: () => keys,
      // A call of the UI scope, for a method that the socket of the CLI cannot call.
      call: (method: string, args: unknown) => call(method, args),
      window: windowId,
      setFont: (px: number) => app.render?.setFont({ px }),
    },
  });
}
