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
import { useEffect, useMemo, useRef, useState } from 'react';
import { type App, AppProvider } from '../app/context';
import { createFailures, reportRejections } from '../app/failures';
import { createToasts } from '../app/toasts';
import { showPane } from '../app/show';
import { createViewState } from '../app/view';
import { watchDeliveries } from '../cards/deliveries';
import { type CardHost, CardHostProvider } from '../cards/host';
import { Keyboard } from '../input/keyboard';
import { InputSender } from '../input/sender';
import { pageFrames } from '../lib/frames';
import type { RenderHost } from '../render';
import { Shell } from '../shell/Shell';
import { createPhoneStores, createStores } from '../stores';
import { type Phone, PhoneProvider } from './context';
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

/**
 * What the desk keeps for its life: the stores, the state of the view, the failures, the toasts
 * and the input. The transport and the window of a desk do not change: another window is another
 * desk.
 */
function createDesk(transport: PhoneTransport, home: PhoneTransport, windowId: string) {
  const failures = createFailures(transport);
  const stores = createStores(transport, windowId);
  const toasts = createToasts();
  const deliveries = watchDeliveries(stores, toasts);
  const own = createPhoneStores(home);
  const view = createViewState();
  // A hardware keyboard of an iPhone has the chords of a Mac: `mod` is Command.
  const keyboard = new Keyboard({ platform: 'macos', optionAsAlt: 'none' });
  // A key that the socket lost goes again with its id after a reconnect.
  const input = new InputSender(transport, pageFrames, undefined, { outbox: true });
  return {
    failures,
    stores,
    toasts,
    deliveries,
    own,
    view,
    keyboard,
    input,
    close(): void {
      deliveries.stop();
      stores.close();
      own.close();
      failures.dismiss();
      toasts.clear();
    },
  };
}

export function PhoneDesk(props: PhoneDeskProps) {
  const { transport, windowId, home } = props;
  const [desk] = useState(() => createDesk(transport, home, windowId));
  const { failures, stores, toasts, deliveries, own, view, keyboard, input } = desk;
  useEffect(() => () => desk.close(), [desk]);
  useEffect(() => reportRejections(window, failures.report), [failures]);

  // A phone whose app the person sees gets no push. The Mac sends the push.
  useEffect(() => {
    const visible = startVisibleReport(home, document);
    const endHome = home.onConnection((open) => {
      if (open) visible.resend();
    });
    return () => {
      endHome();
      visible.stop();
    };
  }, [home]);

  // The single view follows the orientation. The core keeps it in memory only, so the page sends
  // it again after each new connection.
  useEffect(() => {
    const portrait = window.matchMedia(PORTRAIT);
    const sendSingle = (): void => {
      // A failed call goes again with the next connection.
      void transport
        .call('window.set_single', { window: windowId, single: portrait.matches })
        .catch(() => undefined);
    };
    portrait.addEventListener('change', sendSingle);
    sendSingle();
    const endConnection = transport.onConnection((open) => {
      if (!open) return;
      sendSingle();
      input.resend();
    });
    return () => {
      portrait.removeEventListener('change', sendSingle);
      endConnection();
    };
  }, [transport, windowId, input]);

  // The keyboard of the phone covers the bottom of the layout viewport. The shell has the height
  // of the visual viewport, so that the panes stay above the keyboard.
  useEffect(() => {
    const height = (): void => {
      const shown = window.visualViewport?.height ?? window.innerHeight;
      document.documentElement.style.setProperty('--phone-height', `${Math.round(shown)}px`);
    };
    height();
    window.visualViewport?.addEventListener('resize', height);
    window.addEventListener('resize', height);
    return () => {
      window.visualViewport?.removeEventListener('resize', height);
      window.removeEventListener('resize', height);
    };
  }, []);

  // The address chooses a view. A route that names a run waits for the agents.
  const [hash, setHash] = useState(location.hash);
  useEffect(() => {
    const onHash = (): void => {
      setHash(location.hash);
    };
    window.addEventListener('hashchange', onHash);
    return () => window.removeEventListener('hashchange', onHash);
  }, []);
  const handled = useRef<string | null>(null);
  useEffect(() => {
    const current = hash;
    const follow = (): void => {
      const route = routeOf(current);
      if (handled.current === current) return;
      switch (route.kind) {
        case 'desk':
          handled.current = current;
          return;
        case 'settings':
          handled.current = current;
          view.sheet.set({ kind: 'settings', section: route.section });
          return;
        case 'pane':
          handled.current = current;
          showPane(transport, windowId, route.pane).catch(failures.report);
          return;
        case 'interaction':
          handled.current = current;
          view.showDrawer('inbox');
          view.focusCard(route.interaction);
          return;
        case 'run': {
          const agents = stores.agents.get();
          if (agents.status === 'loading') return;
          handled.current = current;
          const run = agents.byKey[route.run];
          // The Inbox shows the card of the run, or the line of its turn with the box Reply.
          if (run && (run.facets.pending.length > 0 || stores.attention.get().byKey[route.run])) {
            view.showDrawer('inbox');
          }
          if (run?.pane) showPane(transport, windowId, run.pane).catch(failures.report);
          return;
        }
      }
    };
    follow();
    // A route that waits for the agents follows them until it is handled.
    const endAgents = stores.agents.subscribe(follow);
    const endAttention = stores.attention.subscribe(follow);
    return () => {
      endAgents();
      endAttention();
    };
  }, [hash, view, transport, windowId, failures, stores]);
  // A sheet that closes takes its address with it, so that a tap on the same push opens it again.
  useEffect(() => {
    const follow = (): void => {
      const sheet = view.sheet.get();
      const route = routeOf(location.hash);
      if (sheet?.kind === 'settings') {
        const next = hashOf({ kind: 'settings', section: sheet.section });
        if (location.hash !== next) {
          handled.current = next;
          history.replaceState(null, '', next);
        }
      } else if (route.kind === 'settings') {
        handled.current = '#/';
        history.replaceState(null, '', '#/');
      }
    };
    follow();
    return view.sheet.subscribe(follow);
  }, [view]);

  // The renderer and the decoder come after the first paint.
  const [host, setHost] = useState<RenderHost>();
  const loadDecoder = useRef(props.loadDecoder);
  useEffect(() => {
    const load = loadDecoder.current;
    if (!load) return;
    let created: RenderHost | undefined;
    let ended = false;
    void Promise.all([import('../render'), load()]).then(
      ([module, decoder]) => {
        if (ended) return;
        created = module.RenderHost.create({ transport, windowId, decoder });
        created.start();
        setHost(created);
      },
      (failure: unknown) => console.warn('the renderer did not load', failure),
    );
    return () => {
      ended = true;
      created?.stop();
    };
  }, [transport, windowId]);

  const { account, machine, restart, onMachine, device, push } = props;
  const app = useMemo<App>(
    () => ({
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
      upload: (file) => account.upload(file, file.name, machine === 'local' ? undefined : machine),
      ...(host ? { render: host } : {}),
    }),
    [transport, failures, toasts, stores, windowId, view, input, keyboard, account, machine, host],
  );
  // A card of the phone answers with `via: phone`, and its Open shows the pane.
  const cardHost = useMemo<CardHost>(
    () => ({
      transport,
      via: 'phone',
      answered: deliveries.expect,
      report: failures.report,
      open: (pane) => {
        view.drawerOpen.set(false);
        showPane(transport, windowId, pane).catch(failures.report);
      },
    }),
    [transport, deliveries, failures, view, windowId],
  );
  const phone = useMemo<Phone>(
    () => ({
      transport: home,
      stores: own,
      machine,
      showMachine: onMachine,
      account,
      device,
      push: push ?? realPushBrowser(),
    }),
    [home, own, machine, onMachine, account, device, push],
  );

  return (
    <PhoneProvider value={phone}>
      {/* The shell mounts again when the renderer comes. */}
      <AppProvider key={host ? 'render' : 'light'} value={app}>
        <CardHostProvider value={cardHost}>
          <Shell
            extraSettings={(section) =>
              section === 'phone' ? <ThisPhone restart={restart} /> : undefined
            }
            extraLeft={() => <MachineList />}
          />
        </CardHostProvider>
      </AppProvider>
    </PhoneProvider>
  );
}
