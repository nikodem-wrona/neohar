// Mounts the phone app. The page of `apps/web` gives it the WebSocket transport of the
// origin of the page, the account of the phone, the loader of the decoder of the frames, which the
// desk calls after its first paint, and the transports of the other machines of the
// Mac.

import type { Decoder } from '@neohar/proto-wasm';
import type { PhoneAccount } from '@neohar/transport';
import { render } from 'solid-js/web';
import { PhoneApp, type PhoneTransport } from './PhoneApp';

/** Whether the page runs as the app of the home screen. */
function standalone(): boolean {
  const ios = (navigator as Navigator & { standalone?: boolean }).standalone === true;
  return ios || window.matchMedia('(display-mode: standalone)').matches;
}

/** Draws the phone app into `root`. Returns the function that takes it down. */
export function mountPhone(
  root: HTMLElement,
  transport: PhoneTransport,
  account: PhoneAccount,
  options: {
    loadDecoder?: () => Promise<Decoder>;
    /** The transport of another machine of the Mac. */
    machineTransport?: (machine: string) => PhoneTransport;
  } = {},
): () => void {
  return render(
    () => (
      <PhoneApp
        transport={transport}
        account={account}
        search={location.search}
        standalone={standalone()}
        restart={() => location.replace('/')}
        {...(options.loadDecoder ? { loadDecoder: options.loadDecoder } : {})}
        {...(options.machineTransport ? { machineTransport: options.machineTransport } : {})}
      />
    ),
    root,
  );
}
