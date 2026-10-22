// Mounts the app of one window. The page gives it the transport: the Tauri transport in the app,
// the WebSocket transport in the tests. The stores end with the mount.

import type { Decoder } from '@neohar/proto-wasm';
import type { Transport } from '@neohar/transport';
import { createRoot } from 'solid-js';
import { render } from 'solid-js/web';
import type { Platform } from '../input/clipboard';
import { CardHostProvider } from '../cards/host';
import { Keyboard } from '../input/keyboard';
import { InputSender } from '../input/sender';
import { pageFrames } from '../lib/frames';
import { RenderHost } from '../render';
import { SettingsView } from '../settings/SettingsView';
import { Shell } from '../shell/Shell';
import { createStores } from '../stores';
import { AppProvider } from './context';
import { createFailures, reportRejections } from './failures';
import { showPane } from './show';
import { createViewState } from './view';

export interface MountOptions {
  /** The frame decoder. With it the window paints the panes. Without it the window has no canvas. */
  decoder?: Decoder;
}

export interface MountedApp {
  /** Takes the window down and ends the subscriptions. */
  dispose(): void;
  /** The renderer of the window, when the page gave a decoder. */
  render?: RenderHost;
}

/** Draws the window into `root`. */
export function mountApp(
  root: HTMLElement,
  transport: Transport,
  windowId: string,
  options: MountOptions = {},
): MountedApp {
  return createRoot((dispose) => {
    const failures = createFailures(transport);
    // A promise of the page that nobody handled, and a subscription that the core refused, show in
    // the banner.
    const stopRejections = reportRejections(window, failures.report);
    const stopTransport = transport.onFailure?.(failures.report);
    const stores = createStores(transport, windowId);
    const view = createViewState();
    const host = options.decoder
      ? RenderHost.create({ transport, windowId, decoder: options.decoder })
      : undefined;
    host?.start();
    const platform: Platform = navigator.userAgent.includes('Mac') ? 'macos' : 'other';
    // Option is not Alt until the settings come: the shell applies `optionAsAlt` from them.
    const keyboard = new Keyboard({ platform, optionAsAlt: 'none' });
    const input = new InputSender(transport, pageFrames);
    const unmount = render(
      () => (
        <AppProvider
          value={{
            transport,
            run: failures.run,
            report: failures.report,
            failures,
            form: 'desk',
            stores,
            windowId,
            view,
            input,
            keyboard,
            platform,
            ...(host ? { render: host } : {}),
          }}
        >
          {/* A card answers with `via: card`, and its Open shows the pane in this window. */}
          <CardHostProvider
            value={{
              transport,
              via: 'card',
              report: failures.report,
              open: (pane) => {
                showPane(transport, windowId, pane).catch(failures.report);
              },
            }}
          >
            <Shell />
          </CardHostProvider>
        </AppProvider>
      ),
      root,
    );
    return {
      dispose: () => {
        unmount();
        host?.stop();
        stores.close();
        stopRejections();
        stopTransport?.();
        failures.dismiss();
        dispose();
      },
      ...(host ? { render: host } : {}),
    };
  });
}

/** Draws the settings window into `root`. It needs the transport and nothing else. */
export function mountSettings(
  root: HTMLElement,
  transport: Transport,
  machine?: string,
): () => void {
  document.title = machine ? `neohar settings · ${machine}` : 'neohar settings';
  return render(() => <SettingsView transport={transport} />, root);
}
