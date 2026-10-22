// The machines of the Mac in the left column of a phone. The phone reaches them
// through the app of the Mac, and the desk shows one machine at a time. Each row has the state of
// its link from the projection `machines` of the Mac, and Show, Connect or Disconnect.

import { type MachineState, RpcError } from '@neohar/transport';
import { createSignal, For, Show } from 'solid-js';
import { usePhone } from './context';

/** The words of the state of a link. */
const STATE_TEXT: Record<MachineState, string> = {
  local: '',
  connecting: 'Connecting',
  connected: 'Connected',
  disconnected: 'Not connected',
};

export function MachineList() {
  const phone = usePhone();
  const [error, setError] = createSignal<string | null>(null);
  const act = (method: 'machine.connect' | 'machine.disconnect', machine: string): void => {
    setError(null);
    phone.transport
      .call(method, { machine })
      .catch((failure: unknown) =>
        setError(failure instanceof RpcError ? failure.message : String(failure)),
      );
  };
  return (
    <section class="phone-machines" aria-label="Machines">
      <div class="band">
        <span class="band-title">Machines</span>
      </div>
      <ul class="machine-list">
        <For each={phone.stores.machines.rows()}>
          {(row) => (
            <li
              class="machine-row"
              data-machine={row.label}
              aria-current={row.label === phone.machine ? 'true' : undefined}
            >
              <span class="machine-name">{row.label === 'local' ? 'This Mac' : row.label}</span>
              <span class="machine-state">{STATE_TEXT[row.state]}</span>
              <Show
                when={
                  row.label !== phone.machine &&
                  (row.state === 'local' || row.state === 'connected')
                }
              >
                <button
                  type="button"
                  class="card-button"
                  onClick={() => phone.showMachine(row.label)}
                >
                  Show
                </button>
              </Show>
              <Show when={row.state === 'disconnected'}>
                <button
                  type="button"
                  class="card-button"
                  onClick={() => act('machine.connect', row.label)}
                >
                  Connect
                </button>
              </Show>
              <Show when={row.state === 'connected' || row.state === 'connecting'}>
                <button
                  type="button"
                  class="card-button"
                  onClick={() => act('machine.disconnect', row.label)}
                >
                  Disconnect
                </button>
              </Show>
            </li>
          )}
        </For>
      </ul>
      <Show when={error()}>
        {(text) => (
          <p class="card-error" role="alert">
            {text()}
          </p>
        )}
      </Show>
    </section>
  );
}
