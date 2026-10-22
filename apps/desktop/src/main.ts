// The entry of the app. A window that the core knows by a ULID (`?window=<id>`) gets the shell of
// the UI. The settings window (`?view=settings`) gets the settings page. A window without an id
// shows an error.

import { createDecoder } from '@neohar/proto-wasm';
import { TauriTransport } from '@neohar/transport';
import { ATLAS_SLOTS, mountApp, mountSettings, parseParams, realTimers } from '@neohar/ui';
import wasmUrl from '../../../packages/proto-wasm/pkg/proto_wasm_bg.wasm?url';

async function start(): Promise<void> {
  const params = parseParams(window.location.search);
  if (params.view === 'settings') {
    // The settings window of the app has no window of the layout.
    await import('@neohar/ui/style.css');
    const root = document.getElementById('root');
    if (!root) throw new Error('index.html has no root element');
    mountSettings(root, new TauriTransport({ timers: realTimers }), params.machine);
    return;
  }
  if (params.window === null) throw new Error('the address of this window has no window id');
  await import('@neohar/ui/style.css');
  // On macOS the window has no title bar, and its three buttons lie over the command line, which
  // leaves room for them.
  if (navigator.userAgent.includes('Mac')) document.documentElement.classList.add('title-overlay');
  const root = document.getElementById('root');
  if (!root) throw new Error('index.html has no root element');
  const decoder = await createDecoder(wasmUrl, ATLAS_SLOTS);
  mountApp(root, new TauriTransport({ timers: realTimers }), params.window, { decoder });
}

start().catch((error: unknown) => {
  console.error(error);
  document.body.textContent = `neohar could not start: ${
    error instanceof Error ? error.message : JSON.stringify(error)
  }`;
});
