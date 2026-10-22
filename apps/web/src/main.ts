// The entry of the phone app. The listener of the Mac serves this page, and the page talks
// to the core through the WebSocket of its own origin. The cookie of the phone is the credential.
// The decoder of the frames is a chunk of its own: the desk loads it after its first paint, so a
// tap on a push shows its card before the panes paint.

import { PhoneAccount, WebSocketTransport } from '@neohar/transport';
import { ATLAS_SLOTS, mountPhone, realTimers } from '@neohar/ui/phone';
import wasmUrl from '../../../packages/proto-wasm/pkg/proto_wasm_bg.wasm?url';
import '@neohar/ui/style.css';
import '@neohar/ui/phone.css';

const root = document.getElementById('root');
if (!root) throw new Error('index.html has no root element');
mountPhone(root, WebSocketTransport.sameOrigin(realTimers), new PhoneAccount(), {
  loadDecoder: async () => (await import('@neohar/proto-wasm')).createDecoder(wasmUrl, ATLAS_SLOTS),
  // Another machine of the Mac: the same origin, with `machine=`.
  machineTransport: (machine) => WebSocketTransport.sameOrigin(realTimers, location, machine),
});

// The service worker gives the app of the home screen its scope, and it shows the push.
navigator.serviceWorker?.register('/sw.js', { scope: '/' }).catch((error: unknown) => {
  console.warn('the service worker did not register', error);
});
