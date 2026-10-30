// The desk on a paired phone: the shell of a window of the desk with the form `phone`, on
// the phone window of this device. The core gives the window with `phone.window`.
//
// The light path: the shell, the stores, the cards and the Inbox come first. The
// renderer and the decoder of the frames come in a chunk of their own after the first paint, and
// the shell mounts again with them. The stores and the state of the view live outside the shell,
// so a card or a sheet that shows stays.
//
// The phone tells the core what a window of the desk tells it: the focus report of the shell, the
// single view of portrait (`window.set_single`), and also whether its app is visible
// (`phone.visible`). The hash of the address chooses a view.
//
// The desk can show another machine of the Mac: `transport` then goes to that machine,
// and `home` stays the transport of the Mac, for the stores of the phone and its machines.

import type { Decoder } from '@neohar/proto-wasm';
import type { PhoneAccount, PhoneDevice } from '@neohar/transport';
import { createEffect, createSignal, on, onCleanup, onMount, Show } from 'solid-js';
import { AppProvider } from '../app/context';
import { createFailures, reportRejections } from '../app/failures';
import { createToasts } from '../app/toasts';
import { showPane } from '../app/show';
import { createViewState } from '../app/view';
import { watchDeliveries } from '../cards/deliveries';
import { CardHostProvider } from '../cards/host';
import { Keyboard } from '../input/keyboard';
import { InputSender } from '../input/sender';
import { pageFrames } from '../lib/frames';
import type { RenderHost } from '../render';
import { Shell } from '../shell/Shell';
import { createPhoneStores, createStores } from '../stores';
import { PhoneProvider } from './context';
import { MachineList } from './Machines';
import { hashOf, routeOf } from './model';
import type { PhoneTransport } from './PhoneApp';
import { type PushBrowser, realPushBrowser } from './push';
import { ThisPhone } from './ThisPhone';
import { startVisibleReport } from './visible';

export interface PhoneDeskProps {
  /** The transport of the machine that the desk shows. */
  transport: PhoneTransport;
  /** The transport of the Mac, where the phone is paired. */
  home: PhoneTransport;
  /** The machine that the desk shows: `local` for the Mac. */
  machine: string;
  /** Shows the desk of another machine. */
  onMachine: (machine: string) => void;
  account: PhoneAccount;
  device: PhoneDevice;
  /** The phone window of this device, from `phone.window`. */
  windowId: string;
  /** Loads the decoder of the frames. Without it the desk paints no pane. */
  loadDecoder?: () => Promise<Decoder>;
  /** Loads the page again at `/`. */
  restart: () => void;
  push?: PushBrowser;
}

/** Portrait has the single view: one pane at a time. */
const PORTRAIT = '(orientation: portrait)';

export function PhoneDesk(props: PhoneDeskProps) {
  const { transport, windowId } = props;
  const failures = createFailures(transport);
  onCleanup(reportRejections(window, failures.report));
  const stores = createStores(transport, windowId);
  const toasts = createToasts();
  const deliveries = watchDeliveries(stores, toasts);
  const own = createPhoneStores(props.home);
  const view = createViewState();
  // A hardware keyboard of an iPhone has the chords of a Mac: `mod` is Command.
  const keyboard = new Keyboard({ platform: 'macos', optionAsAlt: 'none' });
  // A key that the socket lost goes again with its id after a reconnect.
  const input = new InputSender(transport, pageFrames, undefined, { outbox: true });
  onCleanup(() => {
    stores.close();
    own.close();
    failures.dismiss();
    toasts.clear();
  });

  // A phone whose app the person sees gets no push. The Mac sends the push.
  const visible = startVisibleReport(props.home, document);
  onCleanup(() => visible.stop());

  // The single view follows the orientation. The core keeps it in memory only, so the page sends
  // it again after each new connection.
  const portrait = window.matchMedia(PORTRAIT);
  const sendSingle = (): void => {
    // A failed call goes again with the next connection.
    void transport
      .call('window.set_single', { window: windowId, single: portrait.matches })
      .catch(() => undefined);
  };
  portrait.addEventListener('change', sendSingle);
  onCleanup(() => portrait.removeEventListener('change', sendSingle));
  sendSingle();
  const endConnection = transport.onConnection((open) => {
    if (!open) return;
    sendSingle();
    input.resend();
  });
  onCleanup(endConnection);
  const endHome = props.home.onConnection((open) => {
    if (open) visible.resend();
  });
  onCleanup(endHome);

  // The keyboard of the phone covers the bottom of the layout viewport. The shell has the height
  // of the visual viewport, so that the panes stay above the keyboard.
  const height = (): void => {
    const shown = window.visualViewport?.height ?? window.innerHeight;
    document.documentElement.style.setProperty('--phone-height', `${Math.round(shown)}px`);
  };
  height();
  window.visualViewport?.addEventListener('resize', height);
  window.addEventListener('resize', height);
  onCleanup(() => {
    window.visualViewport?.removeEventListener('resize', height);
    window.removeEventListener('resize', height);
  });

  // The address chooses a view. A route that names a run waits for the agents.
  const [hash, setHash] = createSignal(location.hash);
  const onHash = (): void => {
    setHash(location.hash);
  };
  window.addEventListener('hashchange', onHash);
  onCleanup(() => window.removeEventListener('hashchange', onHash));
  let handled: string | null = null;
  createEffect(() => {
    const current = hash();
    const route = routeOf(current);
    if (handled === current) return;
    switch (route.kind) {
      case 'desk':
        handled = current;
        return;
      case 'settings':
        handled = current;
        view.setSheet({ kind: 'settings', section: route.section });
        return;
      case 'pane':
        handled = current;
        showPane(transport, windowId, route.pane).catch(failures.report);
        return;
      case 'interaction':
        handled = current;
        view.showDrawer('inbox');
        view.focusCard(route.interaction);
        return;
      case 'run': {
        if (stores.agents.state.status === 'loading') return;
        handled = current;
        const run = stores.agents.state.byKey[route.run];
        // The Inbox shows the card of the run, or the line of its turn with the box Reply.
        if (run && (run.facets.pending.length > 0 || stores.attention.state.byKey[route.run])) {
          view.showDrawer('inbox');
        }
        if (run?.pane) showPane(transport, windowId, run.pane).catch(failures.report);
        return;
      }
    }
  });
  // A sheet that closes takes its address with it, so that a tap on the same push opens it again.
  createEffect(
    on(view.sheet, (sheet) => {
      const route = routeOf(location.hash);
      if (sheet?.kind === 'settings') {
        const next = hashOf({ kind: 'settings', section: sheet.section });
        if (location.hash !== next) {
          handled = next;
          history.replaceState(null, '', next);
        }
      } else if (route.kind === 'settings') {
        handled = '#/';
        history.replaceState(null, '', '#/');
      }
    }),
  );

  // The renderer and the decoder come after the first paint.
  const [host, setHost] = createSignal<RenderHost>();
  onMount(() => {
    const load = props.loadDecoder;
    if (!load) return;
    void Promise.all([import('../render'), load()]).then(
      ([module, decoder]) => {
        const created = module.RenderHost.create({ transport, windowId, decoder });
        created.start();
        setHost(created);
      },
      (failure: unknown) => console.warn('the renderer did not load', failure),
    );
  });
  onCleanup(() => host()?.stop());

  const Desk = (inner: { render?: RenderHost }) => (
    <AppProvider
      value={{
        transport,
        run: failures.run,
        report: failures.report,
        failures,
        toasts,
        form: 'phone',
        stores,
        windowId,
        view,
        input,
        keyboard,
        platform: 'macos',
        upload: (file) =>
          props.account.upload(
            file,
            file.name,
            props.machine === 'local' ? undefined : props.machine,
          ),
        ...(inner.render ? { render: inner.render } : {}),
      }}
    >
      {/* A card of the phone answers with `via: phone`, and its Open shows the pane. */}
      <CardHostProvider
        value={{
          transport,
          via: 'phone',
          answered: deliveries.expect,
          report: failures.report,
          open: (pane) => {
            view.setDrawerOpen(false);
            showPane(transport, windowId, pane).catch(failures.report);
          },
        }}
      >
        <Shell
          extraSettings={(section) =>
            section === 'phone' ? <ThisPhone restart={props.restart} /> : undefined
          }
          extraLeft={() => <MachineList />}
        />
      </CardHostProvider>
    </AppProvider>
  );

  return (
    <PhoneProvider
      value={{
        transport: props.home,
        stores: own,
        machine: props.machine,
        showMachine: props.onMachine,
        account: props.account,
        device: props.device,
        push: props.push ?? realPushBrowser(),
      }}
    >
      <Show when={host()} keyed fallback={<Desk />}>
        {(render) => <Desk render={render} />}
      </Show>
    </PhoneProvider>
  );
}
