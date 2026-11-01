// The view This phone: the name of the phone, a field to change it, the switch
// Notifications with "Send a test notification", and "Unpair this phone", which removes
// the phone from the Mac. The switch shows what the row of the phone in `phones` says: the core
// knows whether the phone has a push subscription.

import { RpcError } from '@neohar/transport';
import { useEffect, useRef, useState } from 'react';
import { useSelect } from '../lib/hooks';
import { usePhone } from './context';
import { turnOff, turnOn } from './push';

const message = (failure: unknown) =>
  failure instanceof RpcError || failure instanceof Error ? failure.message : String(failure);

export function ThisPhone(props: { restart: () => void }) {
  const { stores, transport, account, device, push } = usePhone();
  const row = useSelect(stores.phones, (state) => state.byKey[device.device]);
  const name = row?.name ?? device.name;
  const pushOn = row?.push === true;
  const [draft, setDraft] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [note, setNote] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [pushBusy, setPushBusy] = useState(false);
  // The switch is the native state of the box: a tap changes it at once, and the row of the core
  // sets it when it changes.
  const box = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (box.current) box.current.checked = pushOn;
  }, [pushOn]);

  // The switch shows the row of the core. A change that fails puts the switch back.
  const toggle = (input: HTMLInputElement): void => {
    setPushBusy(true);
    setError(null);
    setNote(null);
    const vapidKey = stores.status.get().byKey['_']?.vapid_key;
    const sent = input.checked ? turnOn(push, transport, vapidKey) : turnOff(push, transport);
    sent
      .catch((failure: unknown) => {
        setError(message(failure));
        input.checked = stores.phones.get().byKey[device.device]?.push === true;
      })
      .finally(() => setPushBusy(false));
  };
  const test = (): void => {
    setPushBusy(true);
    setError(null);
    setNote(null);
    transport
      .call('phone.push_test', { device: device.device })
      .then(() => setNote('The Mac sent a test notification.'))
      .catch((failure: unknown) => setError(message(failure)))
      .finally(() => setPushBusy(false));
  };

  const rename = (): void => {
    const value = draft;
    if (value === null || value.trim() === '' || value === name) return;
    setError(null);
    transport.call('phone.rename', { device: device.device, name: value }).then(
      () => setDraft(null),
      (failure: unknown) => setError(message(failure)),
    );
  };
  const unpair = (): void => {
    setBusy(true);
    setError(null);
    account.unpair().then(props.restart, (failure: unknown) => {
      setBusy(false);
      setError(message(failure));
    });
  };

  return (
    <section className="phone-section" aria-labelledby="phone-this">
      <h1 className="phone-title" id="phone-this">
        This phone
      </h1>
      <form
        className="phone-form"
        aria-label="Name of this phone"
        onSubmit={(event) => {
          event.preventDefault();
          rename();
        }}
      >
        <label className="phone-label" htmlFor="phone-name">
          Name
        </label>
        <input
          id="phone-name"
          className="phone-field"
          type="text"
          maxLength={64}
          value={draft ?? name}
          onChange={(event) => setDraft(event.currentTarget.value)}
        />
        <button
          type="submit"
          className="phone-button"
          disabled={draft === null || draft.trim() === '' || draft === name}
        >
          Rename
        </button>
      </form>
      <label className="phone-switch">
        <input
          type="checkbox"
          role="switch"
          ref={box}
          defaultChecked={pushOn}
          disabled={pushBusy}
          onChange={(event) => toggle(event.currentTarget)}
        />
        <span>Notifications</span>
      </label>
      <p className="phone-note">
        The Mac sends a notification when an agent needs you and no window of neohar has the focus.
        A notification names the agent and the reason, never the text of the agent.
      </p>
      {pushOn ? (
        <button type="button" className="phone-button" disabled={pushBusy} onClick={test}>
          Send a test notification
        </button>
      ) : null}
      {note ? (
        <p className="phone-note" role="status">
          {note}
        </p>
      ) : null}
      <p className="phone-note">
        A phone that the Mac knows can answer the agents and send them prompts. Unpair a phone that
        you do not use.
      </p>
      <button type="button" className="phone-button danger" disabled={busy} onClick={unpair}>
        Unpair this phone
      </button>
      {error ? (
        <p className="card-error" role="alert">
          {error}
        </p>
      ) : null}
    </section>
  );
}
