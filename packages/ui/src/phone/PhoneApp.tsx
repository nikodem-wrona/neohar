// The phone app: the desk of `packages/ui` on the WebSocket of the origin of the page. It
// asks the listener who the phone is (`GET /api/me`). A phone that is not paired gets the view
// Pair. A paired phone asks the core for its window (`phone.window`) and gets the desk with the form
// `phone` (`PhoneDesk`).
//
// A socket that closes shows a banner until it opens again. The transport connects again by
// itself. When the socket closes, the app asks the listener once more who the phone is: a phone
// that the Mac removed starts again at the view Pair.
//
// The desk shows one machine at a time: the Mac, or another machine that the phone
// reaches through it. The desk of another machine has a transport of its own, and the phone has a
// window on that machine. The transport of the Mac stays for the account, the push and the list of
// the machines.

import type { Decoder } from '@neohar/proto-wasm';
import { type PhoneAccount, type PhoneDevice, RpcError, type Transport } from '@neohar/transport';
import { type ReactNode, useCallback, useEffect, useState } from 'react';
import { useLatest } from '../lib/hooks';
import { routeOf } from './model';
import { Pair } from './Pair';
import { PhoneDesk } from './PhoneDesk';
import type { PushBrowser } from './push';

/** A transport that says when its socket opens and closes: `WebSocketTransport`. */
export interface PhoneTransport extends Transport {
  onConnection(listener: (open: boolean) => void): () => void;
  /** Ends the transport: the transport of another machine ends with its desk. */
  dispose?(): void;
}

export interface PhoneAppProps {
  transport: PhoneTransport;
  account: PhoneAccount;
  /** The query of the address: the code of a pairing, if any. */
  search: string;
  /** The app runs from the home screen, apart from the browser. */
  standalone: boolean;
  /** Loads the page again at `/`. */
  restart: () => void;
  /** The push of the browser. The page of the app uses the real one. */
  push?: PushBrowser;
  /** Loads the decoder of the frames, after the first paint. */
  loadDecoder?: () => Promise<Decoder>;
  /** The transport of another machine of the Mac. Without it the desk is the Mac's. */
  machineTransport?: (machine: string) => PhoneTransport;
}

type Account =
  | { state: 'asking' }
  | { state: 'paired'; device: PhoneDevice }
  | { state: 'unpaired' }
  | { state: 'unreachable'; message: string };

export function PhoneApp(props: PhoneAppProps) {
  const [account, setAccount] = useState<Account>({ state: 'asking' });
  const latest = useLatest(props.account);
  const ask = useCallback((): void => {
    setAccount({ state: 'asking' });
    latest.current.me().then(
      (device) => setAccount(device ? { state: 'paired', device } : { state: 'unpaired' }),
      (failure: unknown) =>
        setAccount({
          state: 'unreachable',
          message: failure instanceof Error ? failure.message : String(failure),
        }),
    );
  }, [latest]);
  useEffect(ask, [ask]);

  // The views of a phone alone have the box `.phone`. The desk has the whole screen.
  switch (account.state) {
    case 'asking':
      return (
        <div className="phone-page">
          <p className="phone-empty" role="status">
            Connecting to the Mac…
          </p>
        </div>
      );
    case 'unreachable':
      return (
        <div className="phone-page">
          <section className="phone-section" aria-label="No connection">
            <p className="phone-banner" role="alert">
              The Mac does not answer: {account.message}
            </p>
            <button type="button" className="phone-button" onClick={ask}>
              Try again
            </button>
          </section>
        </div>
      );
    case 'unpaired':
      return (
        <div className="phone-page">
          <Pair
            account={props.account}
            search={props.search}
            standalone={props.standalone}
            onPaired={(device) => setAccount({ state: 'paired', device })}
          />
        </div>
      );
    case 'paired':
      return <Paired key={account.device.device} {...props} device={account.device} />;
  }
}

/** The machine that `#/machine/<label>` chooses, or none. */
function machineOf(hash: string): string | null {
  const route = routeOf(hash);
  return route.kind === 'machine' ? route.machine : null;
}

/** The app of a paired phone: the banner of the Mac, and the desk of the machine that it shows. */
function Paired(props: PhoneAppProps & { device: PhoneDevice }) {
  // `#/machine/<label>` chooses the machine, for example from the push of a machine.
  const [machine, setMachine] = useState(() => machineOf(location.hash) ?? 'local');
  useEffect(() => {
    const onHash = (): void => {
      const chosen = machineOf(location.hash);
      if (chosen === null) return;
      setMachine(chosen);
      history.replaceState(null, '', '#/');
    };
    if (machineOf(location.hash) !== null) history.replaceState(null, '', '#/');
    window.addEventListener('hashchange', onHash);
    return () => window.removeEventListener('hashchange', onHash);
  }, []);

  const [lost, setLost] = useState(false);
  const live = useLatest({ account: props.account, restart: props.restart });
  const { transport } = props;
  useEffect(() => {
    let down = false;
    return transport.onConnection((open) => {
      if (open) {
        down = false;
        setLost(false);
        return;
      }
      if (down) return;
      down = true;
      setLost(true);
      // The listener refuses the socket of a phone that the Mac removed.
      live.current.account.me().then(
        (device) => {
          if (!device) live.current.restart();
        },
        () => undefined,
      );
    });
  }, [transport, live]);

  return (
    <>
      {lost ? (
        <p className="phone-banner phone-banner-top" role="alert">
          The Mac does not answer. neohar connects again when it can.
        </p>
      ) : null}
      <MachineDesk key={machine} {...props} machine={machine} onMachine={setMachine} />
    </>
  );
}

/**
 * The desk of one machine: its transport, the window of the phone on it, and the desk. The Mac
 * (`local`) uses the transport of the page.
 */
function MachineDesk(
  props: PhoneAppProps & {
    device: PhoneDevice;
    machine: string;
    onMachine: (machine: string) => void;
  },
) {
  const local = props.machine === 'local' || !props.machineTransport;
  // The machine of a desk does not change: another machine is another desk.
  const [transport] = useState(() =>
    local ? props.transport : (props.machineTransport?.(props.machine) ?? props.transport),
  );
  useEffect(() => {
    if (!local) return () => transport.dispose?.();
  }, [local, transport]);
  const name = local ? 'the Mac' : props.machine;

  const [windowId, setWindowId] = useState<string | null>(null);
  const [failed, setFailed] = useState<string | null>(null);
  const asked = useLatest(windowId);
  const askWindow = useCallback((): void => {
    setFailed(null);
    transport.call('phone.window', {}).then(
      (found) => setWindowId(found.window),
      (failure: unknown) =>
        setFailed(failure instanceof RpcError ? failure.message : String(failure)),
    );
  }, [transport]);
  useEffect(askWindow, [askWindow]);
  // The socket of another machine: a banner while it is down, as for the Mac.
  const [lost, setLost] = useState(false);
  useEffect(
    () =>
      transport.onConnection((open) => {
        if (open && asked.current === null) askWindow();
        if (!local) setLost(!open);
      }),
    [transport, askWindow, local, asked],
  );

  const back: ReactNode = !local ? (
    <button type="button" className="phone-button" onClick={() => props.onMachine('local')}>
      Back to this Mac
    </button>
  ) : null;
  const body = (): ReactNode => {
    if (windowId !== null) {
      return (
        <PhoneDesk
          key={windowId}
          transport={transport}
          home={props.transport}
          account={props.account}
          device={props.device}
          windowId={windowId}
          machine={local ? 'local' : props.machine}
          onMachine={props.onMachine}
          restart={props.restart}
          {...(props.loadDecoder ? { loadDecoder: props.loadDecoder } : {})}
          {...(props.push ? { push: props.push } : {})}
        />
      );
    }
    if (failed) {
      return (
        <div className="phone-page">
          <section className="phone-section" aria-label="No window">
            <p className="phone-banner" role="alert">
              {local ? 'The Mac' : props.machine} gave this phone no window: {failed}
            </p>
            {back}
          </section>
        </div>
      );
    }
    return (
      <div className="phone-page">
        <section className="phone-section" aria-label="Connecting">
          <p className="phone-empty" role="status">
            Connecting to {name}…
          </p>
          {back}
        </section>
      </div>
    );
  };
  return (
    <>
      {lost && windowId !== null ? (
        <p className="phone-banner phone-banner-top" role="alert">
          {name} does not answer. neohar connects again when it can.
        </p>
      ) : null}
      {body()}
    </>
  );
}
