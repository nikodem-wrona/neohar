// The machines of the Mac in the left column of a phone. The phone reaches them
// through the app of the Mac, and the desk shows one machine at a time. Each row has the state of
// its link from the projection `machines` of the Mac, and Show, Connect or Disconnect.

import { type MachineState, RpcError } from '@neohar/transport';
import { useState } from 'react';
import { useRows } from '../lib/hooks';
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
  const [error, setError] = useState<string | null>(null);
  const machines = useRows(phone.stores.machines);
  const act = (method: 'machine.connect' | 'machine.disconnect', machine: string): void => {
    setError(null);
    phone.transport
      .call(method, { machine })
      .catch((failure: unknown) =>
        setError(failure instanceof RpcError ? failure.message : String(failure)),
      );
  };
  return (
    <section className="phone-machines" aria-label="Machines">
      <div className="band">
        <span className="band-title">Machines</span>
      </div>
      <ul className="machine-list">
        {machines.map((row) => (
          <li
            key={row.label}
            className="machine-row"
            data-machine={row.label}
            aria-current={row.label === phone.machine ? 'true' : undefined}
          >
            <span className="machine-name">{row.label === 'local' ? 'This Mac' : row.label}</span>
            <span className="machine-state">{STATE_TEXT[row.state]}</span>
            {row.label !== phone.machine && (row.state === 'local' || row.state === 'connected') ? (
              <button
                type="button"
                className="card-button"
                onClick={() => phone.showMachine(row.label)}
              >
                Show
              </button>
            ) : null}
            {row.state === 'disconnected' ? (
              <button
                type="button"
                className="card-button"
                onClick={() => act('machine.connect', row.label)}
              >
                Connect
              </button>
            ) : null}
            {row.state === 'connected' || row.state === 'connecting' ? (
              <button
                type="button"
                className="card-button"
                onClick={() => act('machine.disconnect', row.label)}
              >
                Disconnect
              </button>
            ) : null}
          </li>
        ))}
      </ul>
      {error ? (
        <p className="card-error" role="alert">
          {error}
        </p>
      ) : null}
    </section>
  );
}
