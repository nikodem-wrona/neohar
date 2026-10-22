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
import { createSignal, Match, onCleanup, Show, Switch } from 'solid-js';
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
  const [account, setAccount] = createSignal<Account>({ state: 'asking' });
  const ask = (): void => {
    setAccount({ state: 'asking' });
    props.account.me().then(
      (device) => setAccount(device ? { state: 'paired', device } : { state: 'unpaired' }),
      (failure: unknown) =>
        setAccount({
          state: 'unreachable',
          message: failure instanceof Error ? failure.message : String(failure),
        }),
    );
  };
  ask();
  const paired = () => {
    const current = account();
    return current.state === 'paired' ? current.device : undefined;
  };
  const unreachable = () => {
    const current = account();
    return current.state === 'unreachable' ? current.message : undefined;
  };

  // The views of a phone alone have the box `.phone`. The desk has the whole screen.
  return (
    <Switch>
      <Match when={account().state === 'asking'}>
        <div class="phone-page">
          <p class="phone-empty" role="status">
            Connecting to the Mac…
          </p>
        </div>
      </Match>
      <Match when={unreachable()}>
        {(message) => (
          <div class="phone-page">
            <section class="phone-section" aria-label="No connection">
              <p class="phone-banner" role="alert">
                The Mac does not answer: {message()}
              </p>
              <button type="button" class="phone-button" onClick={ask}>
                Try again
              </button>
            </section>
          </div>
        )}
      </Match>
      <Match when={account().state === 'unpaired'}>
        <div class="phone-page">
          <Pair
            account={props.account}
            search={props.search}
            standalone={props.standalone}
            onPaired={(device) => setAccount({ state: 'paired', device })}
          />
        </div>
      </Match>
      <Match when={paired()} keyed>
        {(device) => <Paired {...props} device={device} />}
      </Match>
    </Switch>
  );
}

/** The app of a paired phone: the banner of the Mac, and the desk of the machine that it shows. */
function Paired(props: PhoneAppProps & { device: PhoneDevice }) {
  // `#/machine/<label>` chooses the machine, for example from the push of a machine.
  const machineOf = (hash: string): string | null => {
    const route = routeOf(hash);
    return route.kind === 'machine' ? route.machine : null;
  };
  const [machine, setMachine] = createSignal(machineOf(location.hash) ?? 'local');
  const onHash = (): void => {
    const chosen = machineOf(location.hash);
    if (chosen === null) return;
    setMachine(chosen);
    history.replaceState(null, '', '#/');
  };
  if (machineOf(location.hash) !== null) history.replaceState(null, '', '#/');
  window.addEventListener('hashchange', onHash);
  onCleanup(() => window.removeEventListener('hashchange', onHash));

  const [lost, setLost] = createSignal(false);
  const endConnection = props.transport.onConnection((open) => {
    if (open) {
      setLost(false);
      return;
    }
    if (lost()) return;
    setLost(true);
    // The listener refuses the socket of a phone that the Mac removed.
    props.account.me().then(
      (device) => {
        if (!device) props.restart();
      },
      () => undefined,
    );
  });
  onCleanup(endConnection);

  return (
    <>
      <Show when={lost()}>
        <p class="phone-banner phone-banner-top" role="alert">
          The Mac does not answer. neohar connects again when it can.
        </p>
      </Show>
      <Show when={machine()} keyed>
        {(label) => (
          <MachineDesk {...props} machine={label} onMachine={(next) => setMachine(next)} />
        )}
      </Show>
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
  const transport = local
    ? props.transport
    : (props.machineTransport?.(props.machine) ?? props.transport);
  if (!local) onCleanup(() => transport.dispose?.());
  const name = local ? 'the Mac' : props.machine;

  const [windowId, setWindowId] = createSignal<string | null>(null);
  const [failed, setFailed] = createSignal<string | null>(null);
  const askWindow = (): void => {
    setFailed(null);
    transport.call('phone.window', {}).then(
      (found) => setWindowId(found.window),
      (failure: unknown) =>
        setFailed(failure instanceof RpcError ? failure.message : String(failure)),
    );
  };
  askWindow();
  // The socket of another machine: a banner while it is down, as for the Mac.
  const [lost, setLost] = createSignal(false);
  const endConnection = transport.onConnection((open) => {
    if (open && windowId() === null) askWindow();
    if (!local) setLost(!open);
  });
  onCleanup(endConnection);

  const back = (
    <Show when={!local}>
      <button type="button" class="phone-button" onClick={() => props.onMachine('local')}>
        Back to this Mac
      </button>
    </Show>
  );
  return (
    <>
      <Show when={lost() && windowId() !== null}>
        <p class="phone-banner phone-banner-top" role="alert">
          {name} does not answer. neohar connects again when it can.
        </p>
      </Show>
      <Switch>
        <Match when={windowId()} keyed>
          {(id) => (
            <PhoneDesk
              transport={transport}
              home={props.transport}
              account={props.account}
              device={props.device}
              windowId={id}
              machine={local ? 'local' : props.machine}
              onMachine={props.onMachine}
              restart={props.restart}
              {...(props.loadDecoder ? { loadDecoder: props.loadDecoder } : {})}
              {...(props.push ? { push: props.push } : {})}
            />
          )}
        </Match>
        <Match when={failed()}>
          {(message) => (
            <div class="phone-page">
              <section class="phone-section" aria-label="No window">
                <p class="phone-banner" role="alert">
                  {local ? 'The Mac' : props.machine} gave this phone no window: {message()}
                </p>
                {back}
              </section>
            </div>
          )}
        </Match>
        <Match when={true}>
          <div class="phone-page">
            <section class="phone-section" aria-label="Connecting">
              <p class="phone-empty" role="status">
                Connecting to {name}…
              </p>
              {back}
            </section>
          </div>
        </Match>
      </Switch>
    </>
  );
}
